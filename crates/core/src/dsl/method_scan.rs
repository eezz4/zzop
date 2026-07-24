//! `Matcher::MethodScan` evaluation — multi-pattern co-occurrence within a symbol's body span.

use crate::finding::Finding;

use super::def::{MethodScan, RuleDef};
use super::diagnostics::RuleDiag;
use super::markers::{compile_marker, compile_marker_sql, is_sql_file, marker_suppresses};
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
    // Veto patterns (guard present -> not a violation) — compiled like `patterns` above; labels unused.
    let Some(absent) = diag.compile_labeled("absent[].pattern", &m.absent) else {
        return;
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
                for (pi, (re, _)) in patterns.iter().enumerate() {
                    if !satisfied[pi] && re.is_match(&scan) {
                        if pi == trigger_idx && m.trigger_in_loop {
                            // Structural containment gate: this trigger match only counts if the
                            // line is textually inside a loop statement or array-iteration
                            // callback body — see `MethodScan::trigger_in_loop` and
                            // `SourceFile::loop_spans` docs. A match outside every loop span is a
                            // plain co-occurrence and neither satisfies the trigger nor can supply
                            // the finding's line.
                            let abs_line = body_start + i as u32;
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
            let snippet: String = line.trim().chars().take(m.snippet_max).collect();
            out.push(Finding {
                rule_id: rule_id.clone(),
                severity: rule.severity,
                file: f.rel.clone(),
                line: body_start + i as u32,
                message: rule.message.clone(),
                data: Some(serde_json::json!({ "snippet": snippet, "method": sym.name })),
            });
        }
    }
}
