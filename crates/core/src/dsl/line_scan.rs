//! `Matcher::LineScan` evaluation — per-line regex scan.

use crate::finding::Finding;

use super::def::{LineScan, RuleDef};
use super::diagnostics::RuleDiag;
use super::markers::{
    compile_markers, is_comment_line, leaders_for_path, marker_leaders_for_path,
    message_with_near_miss,
};
use super::source::RuleContext;

/// A compiled per-line matcher — single or labeled alternatives.
enum LineMatch {
    Single(regex::Regex),
    Any(Vec<(regex::Regex, String)>),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn eval_line_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &LineScan,
    ctx: &RuleContext,
    // `Some(cand)` is the RegexSet pre-filter's per-file candidacy for this rule; `None` means the
    // pre-filter is disabled (scan every file).
    file_candidates: Option<&[bool]>,
    out: &mut Vec<Finding>,
    // Rule-skip sink — see `diagnostics`'s module doc. A rule that cannot compile is skipped, never
    // fatal (analysis is best-effort), but the skip is REPORTED so its silence can't read as "clean".
    diagnostics: &mut Vec<String>,
    // The owning pack's compiled-regex memo — see `crate::dsl::RegexCache`.
    cache: &crate::dsl::RegexCache,
) {
    let rule_id = format!("{}/{}", pack_id, rule.id);
    let mut diag = RuleDiag::new(&rule_id, diagnostics, cache);
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
    let Some(require_all) = diag.compile_all("require_file_all", &m.require_file_all) else {
        return;
    };
    // Negated mirror of require_file_all — see `LineScan::require_file_absent` doc. Compiled the same
    // way; ANY-vs-ALL semantics are applied by the caller below.
    let Some(require_absent) = diag.compile_all("require_file_absent", &m.require_file_absent)
    else {
        return;
    };
    let Some(exclude_re) = diag.compile_opt("exclude_pattern", m.exclude_pattern.as_ref()) else {
        return;
    };
    // One-line-lookback veto — see `LineScan::prev_line_exclude_pattern` doc.
    let Some(prev_exclude_re) = diag.compile_opt(
        "prev_line_exclude_pattern",
        m.prev_line_exclude_pattern.as_ref(),
    ) else {
        return;
    };
    let marker = rule.suppress_marker();
    // The marker regexes are built from the rule id (escaped), so failure here means the id itself is
    // unusable — structural, not a pattern the author wrote wrong, but just as fatal to the rule. All
    // three leader forms are compiled at once and `markers::MarkerRegexes` owns which one a given file
    // licenses, so this matcher and its method-scan twin cannot disagree about that.
    let Some(marker_res) = compile_markers(&marker, diag.cache()) else {
        diag.malformed("its derived suppress marker does not compile as a regex");
        return;
    };

    // `any` (labeled alternatives) takes precedence, else `line_pattern` (single). Neither -> invalid DSL -> skip.
    let matcher = match (&m.any, &m.line_pattern) {
        (Some(alts), _) => match diag.compile_labeled("any[].pattern", alts) {
            Some(v) => LineMatch::Any(v),
            None => return,
        },
        (None, Some(p)) => match diag.compile("line_pattern", p) {
            Some(re) => LineMatch::Single(re),
            None => return,
        },
        (None, None) => {
            diag.malformed("it declares neither `line_pattern` nor `any`");
            return;
        }
    };

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
        // TWO axes, deliberately not one lookup (`markers::path` says why): `leaders` is the SKIP axis
        // (`skip_comment_lines` — a `#`-commented secret is still committed, so `#` is NOT in it),
        // `marker_leaders` is the MARKER axis (a `.env` file's only real comment leader IS `#`).
        let leaders = leaders_for_path(&f.rel);
        let marker_leaders = marker_leaders_for_path(&f.rel);
        for (i, line) in lines.iter().enumerate() {
            // Comment-line gate — the leader set is the file's own (`markers::Leaders`), so a
            // `--` line in a `.sql` migration is skipped exactly like a `//` line in a `.ts` file.
            if m.skip_comment_lines && is_comment_line(leaders, line) {
                continue;
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
            // One-line-lookback veto — the immediately preceding line (and ONLY that line — the same
            // 1-line window as marker suppression) is tested under the same string masking as every
            // other line regex. Checked after the positive match so the previous line is only masked
            // for candidate lines. Line 0 has no predecessor and can never be vetoed here.
            if let Some(re) = &prev_exclude_re {
                if i > 0 {
                    let prev: std::borrow::Cow<'_, str> = if m.strip_string_literals {
                        std::borrow::Cow::Owned(crate::dsl::string_mask::mask_string_literals(
                            lines[i - 1],
                        ))
                    } else {
                        std::borrow::Cow::Borrowed(lines[i - 1])
                    };
                    if re.is_match(&prev) {
                        continue;
                    }
                }
            }
            // Structural line gate over the call-site channel — see `LineScan::line_call_kind`. The
            // regex said the SHAPE is on this line; the gate asks the parser whether the line really
            // CALLS the named family. Evidence-allowing: an empty channel silences, never widens.
            if let Some(kind) = &m.line_call_kind {
                let line_no = (i + 1) as u32;
                if !f
                    .call_sites
                    .iter()
                    .any(|s| s.kind == *kind && s.line == line_no)
                {
                    continue;
                }
            }
            if marker_res.suppresses(marker_leaders, &lines, i) {
                continue;
            }
            // Marker-shaped comment that is NOT this rule's marker -> disclose it in the message (the
            // finding still fires; see `message_with_near_miss`). Message-only: no gate above changes.
            // Reads the MARKER leaders, never the skip leaders: near-miss must mirror suppression
            // exactly, or a `#` comment that WOULD have suppressed goes unmentioned in a `.env` file.
            let message = message_with_near_miss(marker_leaders, &marker, &lines, i, &rule.message);
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
                message,
                evidence_paths: Vec::new(),
                data: Some(data),
            });
        }
    }
}
