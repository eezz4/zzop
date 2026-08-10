//! `Matcher::LiteralScan` evaluation — a query over one file's projected `string_literals`.
//!
//! Per-file like `call_scan` (whose module shape this mirrors deliberately, arm for arm): every fact it
//! reads — the entries, the source text for marker/near-miss detection — belongs to a single file and
//! is already in hand inside the fused parse pass.
//!
//! The judgments here are the two a line scan structurally could not make, both answered WITHOUT the
//! value: `entropy_min` compares against the extraction-time entropy, and `skip_value_equals_name`
//! compares the entry's hash against the hash of its own name ([`zzop_core::value_hash_hex`] — the same
//! function every producer used, so the comparison cannot disagree with extraction). Unlike `call_scan`
//! the finding echoes NO snippet at all (see the `data` comment below): the source text is read only
//! for suppress markers and near-miss hints, and a line the text cannot supply still fires.

use crate::finding::Finding;
use crate::string_literals::value_hash_hex;

use super::def::{LiteralScan, RuleDef};
use super::diagnostics::RuleDiag;
use super::markers::{
    compile_marker_line_comment, marker_suppresses, message_with_near_miss, Leaders,
};
use super::source::RuleContext;

pub(super) fn eval_literal_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &LiteralScan,
    ctx: &RuleContext,
    out: &mut Vec<Finding>,
    // Rule-skip sink — same contract as every sibling matcher: a rule whose regex does not compile is
    // skipped, never fatal, and never silent.
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
    let Some(name_re) = diag.compile_opt("name_pattern", m.name_pattern.as_ref()) else {
        return;
    };
    let Some(name_exclude_re) =
        diag.compile_opt("name_exclude_pattern", m.name_exclude_pattern.as_ref())
    else {
        return;
    };
    let marker = rule.suppress_marker();
    // `//` OR `#` leaders, matching call-scan (not line-scan's `//`-only): this channel is
    // multi-language from its first wave — a Python `# zzop-<id>-ok` must suppress exactly like a
    // TypeScript `// zzop-<id>-ok`. Same `--` omission, same derived-marker note as `call_scan`'s twin.
    let Some(marker_re) = compile_marker_line_comment(&marker, diag.cache()) else {
        diag.malformed("its derived suppress marker does not compile as a regex");
        return;
    };

    for f in ctx.files {
        // Cheapest possible pre-skip — free on every file of every language with no producer.
        if f.string_literals.is_empty() {
            continue;
        }
        if !file_re.is_match(&f.rel) {
            continue;
        }
        if let Some(re) = &file_exclude_re {
            if re.is_match(&f.rel) {
                continue;
            }
        }
        let lines: Vec<&str> = f.text.lines().collect();
        // Source order — the producer's emission order IS the determinism contract, as for call sites.
        for site in &f.string_literals {
            if let Some(re) = &name_re {
                if !re.is_match(&site.name) {
                    continue;
                }
            }
            if let Some(re) = &name_exclude_re {
                if re.is_match(&site.name) {
                    continue;
                }
            }
            if let Some(min) = m.entropy_min {
                if site.entropy < min {
                    continue;
                }
            }
            if m.skip_value_equals_name && site.value_hash == value_hash_hex(&site.name) {
                // The sentinel shape (`refresh_token = "refresh_token"`) — a name/error code, not a
                // secret. Exact equality only; see the field doc for why nothing broader is honest.
                continue;
            }
            // 1-based by contract; a `0` anchors nowhere and is skipped defensively, as in call_scan.
            let Some(idx) = (site.line as usize).checked_sub(1) else {
                continue;
            };
            if marker_suppresses(&marker_re, &lines, idx) {
                continue;
            }
            let message =
                message_with_near_miss(Leaders::SlashOrHash, &marker, &lines, idx, &rule.message);
            out.push(Finding {
                rule_id: rule_id.clone(),
                severity: rule.severity,
                file: f.rel.clone(),
                line: site.line,
                message,
                evidence_paths: Vec::new(),
                // `name` and `entropy` ride the finding so a structured consumer can act on the
                // evidence without re-reading the line — and they are ALL that rides it. The
                // no-value contract (`zzop_core::string_literals`) is uniform across every surface
                // this evidence reaches, not a property of one channel: the sibling matchers' source
                // -line `snippet` would carry the candidate secret verbatim into the findings cache
                // (`.zzop/cache/findings/*.json`), stdout and MCP replies, and the value HASH would
                // carry it crackably (an unsalted 64-bit FNV of a real secret is
                // dictionary-crackable). So the finding names the evidence (`name`, `line`,
                // `entropy`) and never the value in any form; the reader opens the line.
                data: Some(serde_json::json!({
                    "name": site.name,
                    "entropy": site.entropy,
                })),
            });
        }
    }
}
