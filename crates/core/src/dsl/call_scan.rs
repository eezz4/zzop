//! `Matcher::CallScan` evaluation — a query over one file's projected `call_sites`.
//!
//! Per-file like `line_scan`/`method_scan` (and unlike `ir_scan`'s whole-tree io pass): every fact it
//! reads — the sites, the loop spans, the source text — belongs to a single file and is already in hand
//! inside the fused parse pass.
//!
//! ## Anchor text is a COURTESY here, not a precondition
//!
//! The site itself decides whether a rule fires; the file's text is consulted only for the snippet and the
//! suppress-marker window. A site whose line the text cannot supply (envelope mode carries no source
//! lines; a producer could also emit a line past the end) therefore still FIRES, with an empty snippet and
//! nothing suppressing it — the same "honestly absent, never a guessed match" treatment
//! `IoScan::anchor_exclude_pattern` gives an unreachable anchor line. Silently dropping such a site would
//! turn a missing convenience into a missing finding.

use crate::finding::Finding;

use super::def::{CallScan, RuleDef};
use super::diagnostics::RuleDiag;
use super::markers::{
    compile_marker_line_comment, marker_suppresses, message_with_near_miss, Leaders,
};
use super::source::RuleContext;

pub(super) fn eval_call_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &CallScan,
    ctx: &RuleContext,
    out: &mut Vec<Finding>,
    // Rule-skip sink — see `diagnostics`'s module doc. Same contract as every sibling matcher: a rule
    // whose regex does not compile is skipped, never fatal, and never silent.
    diagnostics: &mut Vec<String>,
    // The owning pack's compiled-regex memo — see `crate::dsl::RegexCache`.
    cache: &crate::dsl::RegexCache,
) {
    let rule_id = format!("{}/{}", pack_id, rule.id);
    let mut diag = RuleDiag::new(&rule_id, diagnostics, cache);
    let Some(file_re) = diag.compile("file_pattern", &m.file_pattern) else {
        return;
    };
    let Some(file_exclude_re) =
        diag.compile_opt("file_exclude_pattern", m.file_exclude_pattern.as_ref())
    else {
        return;
    };
    let Some(callee_re) = diag.compile_opt("callee_pattern", m.callee_pattern.as_ref()) else {
        return;
    };
    let Some(algorithm_re) = diag.compile_opt("algorithm_pattern", m.algorithm_pattern.as_ref())
    else {
        return;
    };
    let Some(line_re) = diag.compile_opt("line_pattern", m.line_pattern.as_ref()) else {
        return;
    };
    let marker = rule.suppress_marker();
    // `//` OR `#` leaders, matching the io-scan pass rather than line-scan's `//`-only: this channel is
    // multi-language from its first wave (a Python `# zzop-<id>-ok` must suppress exactly like a
    // TypeScript `// zzop-<id>-ok`), and unlike line-scan there is no per-language rule copy whose own
    // `file_pattern` would have narrowed the comment syntax. `--` is deliberately absent — SQL has no call
    // sites, and `--x` is a decrement in JS/TS (see `markers::compile_marker_sql`'s isolation note).
    //
    // Derived from the rule id (escaped), so a failure here is structural rather than an author's typo —
    // same note as `line_scan`'s twin.
    let Some(marker_re) = compile_marker_line_comment(&marker, diag.cache()) else {
        diag.malformed("its derived suppress marker does not compile as a regex");
        return;
    };

    for f in ctx.files {
        // Cheapest possible pre-skip, and the one that makes this matcher free on every file of every
        // language with no producer: no sites, no work, no text split.
        if f.call_sites.is_empty() {
            continue;
        }
        if !file_re.is_match(&f.rel) {
            continue;
        }
        if let Some(re) = &file_exclude_re {
            if re.is_match(&f.rel) {
                continue; // path-negation escape hatch, see field doc
            }
        }
        let lines: Vec<&str> = f.text.lines().collect();
        // Source order — the producer's emission order IS the determinism contract for this channel, the
        // same one `IoFacts`' vecs carry.
        for site in &f.call_sites {
            if let Some(k) = &m.kind {
                if site.kind != *k {
                    continue;
                }
            }
            if let Some(re) = &callee_re {
                if !re.is_match(&site.callee) {
                    continue;
                }
            }
            // `algorithm_pattern` — never-guess on both sides (see the field's doc): a site whose
            // `algorithm` is `None` never matches a rule that filters on it.
            if let Some(re) = &algorithm_re {
                match &site.algorithm {
                    Some(algo) if re.is_match(algo) => {}
                    _ => continue,
                }
            }
            // `line_pattern` — the lexical residual. A line the text cannot supply matches nothing
            // when this field is set, which deliberately INVERTS this module's "anchor text is a
            // courtesy" note for the rules that opt in — the field's doc owns why.
            if let Some(re) = &line_re {
                let line_text = (site.line as usize)
                    .checked_sub(1)
                    .and_then(|i| lines.get(i));
                match line_text {
                    Some(text) if re.is_match(text) => {}
                    _ => continue,
                }
            }
            if m.in_loop
                && !f
                    .loop_spans
                    .iter()
                    .any(|&(s, e)| s <= site.line && site.line <= e)
            {
                // Outside every projected loop span — the per-iteration claim has no evidence, so the site
                // is not a hit at all. A file with NO loop spans (language without the projection, degraded
                // parse) therefore goes silent under this gate, which is `trigger_in_loop`'s contract too.
                continue;
            }
            // 1-based by contract. A `0` anchors a finding nowhere, so it is skipped defensively rather
            // than clamped — the same treatment `method_scan` gives a `body_start == 0` span.
            let Some(idx) = (site.line as usize).checked_sub(1) else {
                continue;
            };
            // `marker_suppresses` / `message_with_near_miss` both index `lines` with `.get()`, so a line
            // the text cannot supply contributes nothing rather than panicking — see this module's doc for
            // why that is a fire-anyway, not a skip.
            if marker_suppresses(&marker_re, &lines, idx) {
                continue;
            }
            // Marker-SHAPED comment that is not this rule's marker -> disclosed in the message; the finding
            // still fires. Message-only, no gate above changes.
            let message =
                message_with_near_miss(Leaders::SlashOrHash, &marker, &lines, idx, &rule.message);
            let snippet: String = lines
                .get(idx)
                .map(|l| l.trim().chars().take(m.snippet_max).collect())
                .unwrap_or_default();
            out.push(Finding {
                rule_id: rule_id.clone(),
                severity: rule.severity,
                file: f.rel.clone(),
                line: site.line,
                message,
                evidence_paths: Vec::new(),
                // `callee` and `kind` ride the finding so a structured consumer can act on the evidence
                // without re-reading the line — the same reason `ir_scan` puts `kind` on an io finding.
                // `algorithm` rides only when the site carries one, mirroring the field's own
                // absent-is-honest serialization.
                data: Some(match &site.algorithm {
                    Some(algo) => serde_json::json!({
                        "snippet": snippet,
                        "callee": site.callee,
                        "kind": site.kind,
                        "algorithm": algo,
                    }),
                    None => serde_json::json!({
                        "snippet": snippet,
                        "callee": site.callee,
                        "kind": site.kind,
                    }),
                }),
            });
        }
    }
}
