//! `validateRulePackOnly` — the pre-load, structure-only DSL rule-pack check behind the
//! `validate_rule_pack` MCP tool and `zzop validate-rule-pack <path>`.
//!
//! Scope: this surfaces only judgments the engine itself already makes — the loader's load-time
//! verdicts plus one eval-time check (the regex compile below; such a pack LOADS clean but the
//! affected rule never fires). Concretely, `issues` can only contain:
//! - a serde deserialization message (bad JSON, missing field, wrong type) or the schema-version
//!   gate, both via `zzop_core::parse_dsl_pack` — the exact per-file verdict `load_dsl_packs`
//!   applies to every `rules/dsl/*.json` (one path, no forked logic);
//! - `zzop_core::pack_regex_issues` — every regex-typed matcher field that fails to compile, the
//!   judgment the DSL interpreter applies at eval time by silently no-oping the affected rule.
//!
//! It NEVER judges rule quality or semantics ("is this a good rule", "will this pattern over-match")
//! — a structurally valid pack with a useless rule reports `valid: true`.

use crate::envelope::ValidateReport;

/// `validateRulePackOnly(packJson)`: reports `{"valid": bool, "issues": ["..."]}` for one DSL
/// rule-pack JSON text (the content of a `rules/dsl/*.json` / `packsDir` file, or one `packDefs`
/// entry). `valid: true` means the pack would load AND every matcher regex compiles (a rule with a
/// non-compiling regex loads but can never fire — named here so an author learns it BEFORE
/// shipping). Mirrors [`crate::validate_envelope_only_json`]'s contract exactly: unlike the
/// `analyze*` entry points this NEVER fails — unparseable input is an ordinary
/// `{"valid": false, ...}` report, not an `Err`, since a validity CHECK cannot itself be "wrong"
/// the way a malformed analyze request can.
pub fn validate_rule_pack_json(pack_json: &str) -> String {
    let issues = match zzop_core::parse_dsl_pack(pack_json) {
        Ok(pack) => zzop_core::pack_regex_issues(&pack),
        Err(message) => vec![message],
    };
    let report = ValidateReport {
        valid: issues.is_empty(),
        issues,
    };

    serde_json::to_string(&report).unwrap_or_else(|e| {
        format!(
            r#"{{"valid":false,"issues":["zzop-facade: failed to serialize validate report: {e}"]}}"#
        )
    })
}
