//! `Matcher::MethodScan` evaluation — multi-pattern co-occurrence within a symbol's body span.

use crate::finding::Finding;

use super::def::{MethodScan, RuleDef};
use super::diagnostics::RuleDiag;
use super::markers::{
    compile_marker, compile_marker_sql, is_sql_file, marker_suppresses, message_with_near_miss,
    Leaders,
};
use super::source::RuleContext;

pub(super) fn eval_method_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &MethodScan,
    ctx: &RuleContext,
    out: &mut Vec<Finding>,
    // Rule-skip sink — see `diagnostics`'s module doc. Same contract as `line_scan`: skip, never fail,
    // but say so.
    diagnostics: &mut Vec<String>,
) {
    let rule_id = format!("{}/{}", pack_id, rule.id);
    let mut diag = RuleDiag::new(&rule_id, diagnostics);
    let Some(file_re) = diag.compile("file_pattern", &m.file_pattern) else {
        return;
    };
    // Path-negation escape hatch — see `LineScan::file_exclude_pattern` doc.
    let Some(file_exclude_re) =
        diag.compile_opt("file_exclude_pattern", m.file_exclude_pattern.as_ref())
    else {
        return;
    };
    let Some(require_re) = diag.compile_opt("require_file", m.require_file.as_ref()) else {
        return;
    };
    let Some(patterns) = diag.compile_labeled("patterns[].pattern", &m.patterns) else {
        return;
    };
    // The trigger label must be one of `patterns` — otherwise the DSL rule is malformed, skip it.
    let Some(trigger_idx) = patterns.iter().position(|(_, label)| *label == m.trigger) else {
        diag.malformed(&format!(
            "its `trigger` names label \"{}\", which no `patterns` entry declares",
            m.trigger
        ));
        return;
    };
    // Lexical-order gate — see `MethodScan::after`. Like `trigger`, naming a label no `patterns` entry
    // declares is malformed, not a silent no-op: it would otherwise degrade to plain co-occurrence, the
    // exact weakness `after` exists to remove.
    let after_idx = match &m.after {
        Some(label) => match patterns.iter().position(|(_, l)| l == label) {
            Some(i) => Some(i),
            None => {
                diag.malformed(&format!(
                    "its `after` names label \"{label}\", which no `patterns` entry declares"
                ));
                return;
            }
        },
        None => None,
    };
    // Same-function PAIRING gate on `after` — see `MethodScan::after_in_same_function`. Inert without
    // `after` (there is no pairing to scope), and a no-op on a file with no projected `function_spans`.
    let pair_scope = m.after_in_same_function && after_idx.is_some();
    // Veto patterns (guard present -> not a violation) — compiled like `patterns` above; labels unused.
    let Some(absent) = diag.compile_labeled("absent[].pattern", &m.absent) else {
        return;
    };
    // Whether a trigger match on the line being scanned satisfies `after`. No `after` -> always true
    // (byte-identical to the pre-`after` behaviour). Otherwise: true when the ordering label matched on an
    // earlier line, else only when it matches EARLIER ON THIS LINE by start offset — which is what makes a
    // one-liner continuation (`p.then(r => setX(r))`) count while `setX(v); await f();` does not.
    let order_ok = |after_seen_earlier: bool, scan: &str| -> bool {
        let Some(ai) = after_idx else { return true };
        if after_seen_earlier {
            return true;
        }
        match (
            patterns[ai].0.find(scan),
            patterns[trigger_idx].0.find(scan),
        ) {
            (Some(a), Some(t)) => a.start() < t.start(),
            _ => false,
        }
    };
    let marker = rule.suppress_marker();
    // Derived from the rule id (escaped) — a failure here is structural, see `line_scan`'s twin note.
    let Some(marker_re) = compile_marker(&marker) else {
        diag.malformed("its derived suppress marker does not compile as a regex");
        return;
    };
    // SQL-comment counterpart of `marker_re`, only ever consulted below when `is_sql_file(&f.rel)` — see
    // `compile_marker_sql`'s doc.
    let Some(marker_re_sql) = compile_marker_sql(&marker) else {
        diag.malformed("its derived suppress marker does not compile as a regex");
        return;
    };
    let Some(require_all) = diag.compile_all("require_file_all", &m.require_file_all) else {
        return;
    };
    // Negated mirror of require_file_all, see `MethodScan::require_file_absent` doc.
    let Some(require_absent) = diag.compile_all("require_file_absent", &m.require_file_absent)
    else {
        return;
    };

    for f in ctx.files {
        if !file_re.is_match(&f.rel) {
            continue;
        }
        if let Some(re) = &file_exclude_re {
            if re.is_match(&f.rel) {
                continue; // path-negation escape hatch, see field doc
            }
        }
        if let Some(req) = &require_re {
            if !req.is_match(&f.text) {
                continue;
            }
        }
        if !require_all.iter().all(|re| re.is_match(&f.text)) {
            continue; // short-circuits on the first miss
        }
        if require_absent.iter().any(|re| re.is_match(&f.text)) {
            continue; // ANY match anywhere in the file skips it (require_file_absent)
        }
        // Whole-file necessary-condition pre-skip: every `patterns` entry must match SOMEWHERE in the file,
        // a strict subsumption of the per-span check below, so findings stay identical.
        if !patterns.iter().all(|(re, _)| re.is_match(&f.text)) {
            continue;
        }
        let lines: Vec<&str> = f.text.lines().collect();
        let is_sql = is_sql_file(&f.rel);
        // Innermost-span priority: when spans overlap (a class symbol's span contains its methods' spans),
        // drop any symbol whose span strictly contains another candidate span — avoids double-counting.
        let spans: Vec<(usize, u32, u32)> = f
            .symbols
            .iter()
            .enumerate()
            .filter_map(|(idx, sym)| {
                let (Some(s), Some(e)) = (sym.body_start, sym.body_end) else {
                    return None;
                };
                (s != 0 && e >= s).then_some((idx, s, e))
            })
            .collect();
        let mut drop_symbol = vec![false; f.symbols.len()];
        for &(idx_a, s_a, e_a) in &spans {
            for &(idx_b, s_b, e_b) in &spans {
                if idx_a != idx_b && s_a <= s_b && e_a >= e_b && (s_a, e_a) != (s_b, e_b) {
                    drop_symbol[idx_a] = true;
                    break;
                }
            }
        }

        for (sym_idx, sym) in f.symbols.iter().enumerate() {
            if drop_symbol[sym_idx] {
                continue; // outer span strictly contains another candidate span — evaluate the leaf instead
            }
            let (Some(body_start), Some(body_end)) = (sym.body_start, sym.body_end) else {
                continue; // no body span (type/interface, or parser couldn't project one) -> not scannable
            };
            if body_start == 0 || body_end < body_start {
                continue; // malformed span, defensively skip
            }
            let start_idx = (body_start - 1) as usize;
            if start_idx >= lines.len() {
                continue;
            }
            let end_idx = (body_end as usize).min(lines.len()); // exclusive; body_end is 1-based inclusive
            let span = &lines[start_idx..end_idx];

            let mut satisfied = vec![false; patterns.len()];
            let mut trigger_hit: Option<(usize, &str)> = None; // (index within span, line text)
            let mut vetoed = false;
            // Latest line the ordering label matched on so far. Only tracked under `pair_scope`;
            // otherwise `satisfied[after_idx]` (a plain "matched on some earlier line" bit) decides
            // ordering exactly as before.
            let mut last_after_line: Option<u32> = None;
            for (i, line) in span.iter().enumerate() {
                if m.skip_comment_lines {
                    let t = line.trim_start();
                    if t.starts_with("//") || t.starts_with('*') || t.starts_with("/*") {
                        continue;
                    }
                }
                // Pattern/absent regexes test `scan` (string interiors masked when opted in); the ORIGINAL
                // `line` still supplies the finding's snippet and `marker_suppresses` context below.
                let scan: std::borrow::Cow<'_, str> = if m.strip_string_literals {
                    std::borrow::Cow::Owned(crate::dsl::string_mask::mask_string_literals(line))
                } else {
                    std::borrow::Cow::Borrowed(line)
                };
                let abs_line = body_start + i as u32;
                // Snapshot BEFORE this line's matching mutates `satisfied`: the ordering label counts as
                // "already seen" only from a strictly EARLIER line. Same-line ordering is decided by
                // offset below, so a label first matched on this very line cannot vacuously satisfy it.
                // Under `pair_scope` that earlier match must additionally still be INSIDE this line's own
                // innermost function — i.e. at or after that function's start line, since it is already
                // before this line. A sibling closure's `await` therefore no longer pairs with this
                // line's setter, while the ENCLOSING function's own `await` still does. Same-LINE pairs
                // are unaffected (that branch lives in `order_ok`).
                let after_seen_earlier = match after_idx {
                    None => true,
                    Some(ai) if !pair_scope => satisfied[ai],
                    Some(_) => {
                        // `unwrap_or(0)` is the CONTRACT (see `MethodScan::after_in_same_function`),
                        // not a fallback: a trigger line inside NO projected function span reads as
                        // "no gate on this line", never as "no pair" — floor 0 readmits every earlier
                        // `after` match in the symbol span, i.e. the pre-gate scope. Per LINE, not
                        // only per file: a file WITH spans still has lines outside all of them.
                        let floor = f.innermost_function_start(abs_line).unwrap_or(0);
                        last_after_line.is_some_and(|a| a >= floor)
                    }
                };
                for (pi, (re, _)) in patterns.iter().enumerate() {
                    if !satisfied[pi] && re.is_match(&scan) {
                        // Lexical-order gate — see `MethodScan::after`. A trigger that does not follow the
                        // ordering label is not a hit at all: it neither satisfies nor anchors, so a later
                        // trigger that DOES follow becomes the finding's line.
                        if pi == trigger_idx && !order_ok(after_seen_earlier, &scan) {
                            continue;
                        }
                        if pi == trigger_idx && m.trigger_in_loop {
                            // Structural containment gate: this trigger match only counts if the
                            // line is textually inside a loop statement or array-iteration
                            // callback body — see `MethodScan::trigger_in_loop` and
                            // `SourceFile::loop_spans` docs. A match outside every loop span is a
                            // plain co-occurrence and neither satisfies the trigger nor can supply
                            // the finding's line.
                            if !f
                                .loop_spans
                                .iter()
                                .any(|&(s, e)| s <= abs_line && abs_line <= e)
                            {
                                continue;
                            }
                        }
                        satisfied[pi] = true;
                        if pi == trigger_idx && trigger_hit.is_none() {
                            trigger_hit = Some((i, line));
                        }
                    }
                }
                // Record this line's ordering-label match AFTER the trigger decision above, so a label
                // first seen on this very line still cannot vacuously satisfy the "earlier line" branch.
                // The LATEST such line is all the gate needs: the test is "at or after the trigger
                // function's start", and if the latest match fails that, no earlier one can pass it.
                if let Some(ai) = after_idx {
                    if pair_scope && patterns[ai].0.is_match(&scan) {
                        last_after_line = Some(abs_line);
                    }
                }
                if !vetoed && absent.iter().any(|(re, _)| re.is_match(&scan)) {
                    vetoed = true;
                }
            }
            if vetoed || !satisfied.iter().all(|&b| b) {
                continue;
            }
            let Some((i, line)) = trigger_hit else {
                continue; // unreachable: satisfied[trigger_idx] implies trigger_hit is Some
            };
            if marker_suppresses(&marker_re, &lines, start_idx + i) {
                continue;
            }
            if is_sql && marker_suppresses(&marker_re_sql, &lines, start_idx + i) {
                continue;
            }
            // Marker-shaped comment that is NOT this rule's marker -> disclose it in the message (the
            // finding still fires; see `message_with_near_miss`). Message-only: no gate above changes.
            let leaders = if is_sql {
                Leaders::SlashOrSql
            } else {
                Leaders::Slash
            };
            let message =
                message_with_near_miss(leaders, &marker, &lines, start_idx + i, &rule.message);
            let snippet: String = line.trim().chars().take(m.snippet_max).collect();
            out.push(Finding {
                rule_id: rule_id.clone(),
                severity: rule.severity,
                file: f.rel.clone(),
                line: body_start + i as u32,
                message,
                data: Some(serde_json::json!({ "snippet": snippet, "method": sym.name })),
            });
        }
    }
}
