//! `Matcher::MethodScan` evaluation — multi-pattern co-occurrence within a symbol's body span.

use crate::finding::Finding;

use super::def::{MethodScan, RuleDef};
use super::markers::{
    compile_marker, compile_marker_sql, compile_require_all, is_sql_file, marker_suppresses,
};
use super::source::RuleContext;

pub(super) fn eval_method_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &MethodScan,
    ctx: &RuleContext,
    out: &mut Vec<Finding>,
) {
    // Skip the rule if a regex fails to compile (TODO: report via diagnostics).
    let Ok(file_re) = regex::Regex::new(&m.file_pattern) else {
        return;
    };
    // Path-negation escape hatch — see `LineScan::file_exclude_pattern` doc.
    let file_exclude_re = match &m.file_exclude_pattern {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
    let require_re = match &m.require_file {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
    let mut patterns = Vec::with_capacity(m.patterns.len());
    for lp in &m.patterns {
        let Ok(re) = regex::Regex::new(&lp.pattern) else {
            return;
        };
        patterns.push((re, lp.label.clone()));
    }
    // The trigger label must be one of `patterns` — otherwise the DSL rule is malformed, skip it.
    let Some(trigger_idx) = patterns.iter().position(|(_, label)| *label == m.trigger) else {
        return;
    };
    // Veto patterns (guard present -> not a violation) — compiled like `patterns` above.
    let mut absent = Vec::with_capacity(m.absent.len());
    for lp in &m.absent {
        let Ok(re) = regex::Regex::new(&lp.pattern) else {
            return;
        };
        absent.push(re);
    }
    let marker_re = match &rule.suppress_marker {
        Some(marker) => match compile_marker(marker) {
            Some(r) => Some(r),
            None => return,
        },
        None => None,
    };
    // SQL-comment counterpart of `marker_re`, only ever consulted below when `is_sql_file(&f.rel)` — see
    // `compile_marker_sql`'s doc.
    let marker_re_sql = match &rule.suppress_marker {
        Some(marker) => match compile_marker_sql(marker) {
            Some(r) => Some(r),
            None => return,
        },
        None => None,
    };
    let Some(require_all) = compile_require_all(&m.require_file_all) else {
        return;
    };
    // Negated mirror of require_file_all, see `MethodScan::require_file_absent` doc.
    let Some(require_absent) = compile_require_all(&m.require_file_absent) else {
        return;
    };
    let rule_id = format!("{}/{}", pack_id, rule.id);

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
                for (pi, (re, _)) in patterns.iter().enumerate() {
                    if !satisfied[pi] && re.is_match(line) {
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
                if !vetoed && absent.iter().any(|re| re.is_match(line)) {
                    vetoed = true;
                }
            }
            if vetoed || !satisfied.iter().all(|&b| b) {
                continue;
            }
            let Some((i, line)) = trigger_hit else {
                continue; // unreachable: satisfied[trigger_idx] implies trigger_hit is Some
            };
            if let Some(re) = &marker_re {
                if marker_suppresses(re, &lines, start_idx + i) {
                    continue;
                }
            }
            if is_sql {
                if let Some(re) = &marker_re_sql {
                    if marker_suppresses(re, &lines, start_idx + i) {
                        continue;
                    }
                }
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
