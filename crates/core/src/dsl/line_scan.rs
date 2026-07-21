//! `Matcher::LineScan` evaluation — per-line regex scan.

use crate::finding::Finding;

use super::def::{LineScan, RuleDef};
use super::markers::{
    compile_marker, compile_marker_sql, compile_require_all, is_sql_file, marker_suppresses,
};
use super::source::RuleContext;

/// A compiled per-line matcher — single or labeled alternatives.
enum LineMatch {
    Single(regex::Regex),
    Any(Vec<(regex::Regex, String)>),
}

pub(super) fn eval_line_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &LineScan,
    ctx: &RuleContext,
    // `Some(cand)` is the RegexSet pre-filter's per-file candidacy for this rule; `None` means the
    // pre-filter is disabled (scan every file).
    file_candidates: Option<&[bool]>,
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
    let Some(require_all) = compile_require_all(&m.require_file_all) else {
        return;
    };
    // Negated mirror of require_file_all — see `LineScan::require_file_absent` doc. Reuses
    // `compile_require_all`; ANY-vs-ALL semantics are applied by the caller below.
    let Some(require_absent) = compile_require_all(&m.require_file_absent) else {
        return;
    };
    let exclude_re = match &m.exclude_pattern {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
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
    // `any` (labeled alternatives) takes precedence, else `line_pattern` (single). Neither -> invalid DSL -> skip.
    let matcher = match (&m.any, &m.line_pattern) {
        (Some(alts), _) => {
            let mut v = Vec::with_capacity(alts.len());
            for lp in alts {
                let Ok(re) = regex::Regex::new(&lp.pattern) else {
                    return;
                };
                v.push((re, lp.label.clone()));
            }
            LineMatch::Any(v)
        }
        (None, Some(p)) => match regex::Regex::new(p) {
            Ok(re) => LineMatch::Single(re),
            Err(_) => return,
        },
        (None, None) => return,
    };
    let rule_id = format!("{}/{}", pack_id, rule.id);

    for (file_idx, f) in ctx.files.iter().enumerate() {
        if let Some(cand) = file_candidates {
            if !cand[file_idx] {
                continue; // RegexSet proved zero pattern hits in this file — see fn doc
            }
        }
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
        let lines: Vec<&str> = f.text.lines().collect();
        let is_sql = is_sql_file(&f.rel);
        for (i, line) in lines.iter().enumerate() {
            if m.skip_comment_lines {
                let t = line.trim_start();
                if t.starts_with("//") || t.starts_with('*') || t.starts_with("/*") {
                    continue;
                }
            }
            // `exclude`/`line_pattern`/`any` regexes test `scan` (string interiors masked when opted in);
            // the ORIGINAL `line` still supplies the snippet and `marker_suppresses` context below.
            let scan: std::borrow::Cow<'_, str> = if m.strip_string_literals {
                std::borrow::Cow::Owned(crate::dsl::string_mask::mask_string_literals(line))
            } else {
                std::borrow::Cow::Borrowed(line)
            };
            if let Some(re) = &exclude_re {
                if re.is_match(&scan) {
                    continue;
                }
            }
            let label: Option<&str> = match &matcher {
                LineMatch::Single(re) => {
                    if re.is_match(&scan) {
                        Some("")
                    } else {
                        None
                    }
                }
                LineMatch::Any(alts) => alts
                    .iter()
                    .find(|(re, _)| re.is_match(&scan))
                    .map(|(_, label)| label.as_str()),
            };
            let Some(label) = label else { continue };
            if let Some(re) = &marker_re {
                if marker_suppresses(re, &lines, i) {
                    continue;
                }
            }
            if is_sql {
                if let Some(re) = &marker_re_sql {
                    if marker_suppresses(re, &lines, i) {
                        continue;
                    }
                }
            }
            let snippet: String = line.trim().chars().take(m.snippet_max).collect();
            let data = if label.is_empty() {
                serde_json::json!({ "snippet": snippet })
            } else {
                serde_json::json!({ "snippet": snippet, "label": label })
            };
            out.push(Finding {
                rule_id: rule_id.clone(),
                severity: rule.severity,
                file: f.rel.clone(),
                line: (i + 1) as u32,
                message: rule.message.clone(),
                data: Some(data),
            });
        }
    }
}
