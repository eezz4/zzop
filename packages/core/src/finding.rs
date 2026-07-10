//! Finding / Severity / RuleExplain — rule output contract.
//! Whether a finding comes from a native rule, a DSL pack, or a JS quick-rule, it is normalized and merged here.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

/// Human-facing guidance a rule pack carries with its findings (why / what to check / what breaks / how to fix).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleExplain {
    /// Root cause.
    pub cause: String,
    /// How to confirm a true positive.
    pub review: String,
    /// Runtime symptom / failure mode if left unaddressed.
    pub symptom: String,
    /// The concrete shape of the fix.
    pub fix: String,
}

/// The canonical disable-hint fragment every native finding message embeds: config-file dialect first
/// (`rules: { "<id>": "off" }`), the embedder-facing field name in the parenthetical (`disabled_rules`) —
/// one shared builder so this fragment cannot drift per call site the way it did before this function
/// existed. A 2026-07-10 audit swept 31 native message sites that had each hand-written a slightly
/// different rendering of this same fragment, plus one plain-string (non-`format!`) site that shipped a
/// literal `{{` in its output because a mechanical format!-escaping sweep assumed every site was inside a
/// `format!` call — both defect classes are structurally impossible once every site calls this instead of
/// hand-writing the text. See `docs/rules/authoring-guide.md`'s "Message triple" / native-rule-message
/// contract bullets, and `packages/engine/tests/rule_contracts.rs`'s
/// `native_rule_files_that_build_findings_mention_disabled_rules` test, for the "how to exclude" leg every
/// native finding message must carry.
pub fn disable_hint(id: &str) -> String {
    format!("Disable via config `rules: {{ \"{id}\": \"off\" }}` (embedders: `disabled_rules`)")
}

#[cfg(test)]
mod disable_hint_tests {
    // Deliberately synthetic, made-up ids (not a real registered native analysis id) — this module lives
    // in `packages/core/src`, which `packages/engine/tests/rule_contracts.rs`'s
    // `kernel_core_carries_no_native_analysis_id_string_literal` contract forbids from quoting any REAL
    // native analysis id as a literal (the kernel must stay rule-vocabulary-free); only `registry.rs` and
    // `dsl.rs` are exempt from that check, and this file is neither.
    use super::disable_hint;

    #[test]
    fn renders_no_escaped_braces() {
        let hint = disable_hint("example-rule");
        assert!(
            !hint.contains("{{") && !hint.contains("}}"),
            "disable_hint must render literal single braces, not leftover format!-escape sequences: {hint:?}"
        );
    }

    #[test]
    fn renders_the_id() {
        let hint = disable_hint("example-family/example-rule");
        assert!(
            hint.contains("example-family/example-rule"),
            "disable_hint must embed the id it was called with: {hint:?}"
        );
    }

    #[test]
    fn renders_the_embedder_field_name() {
        let hint = disable_hint("example-rule");
        assert!(
            hint.contains("disabled_rules"),
            "disable_hint must name the disabled_rules embedder field: {hint:?}"
        );
    }

    #[test]
    fn renders_the_exact_known_shape() {
        // Pins the exact rendering (config dialect first, embedder field in the parenthetical, single
        // braces — never `{{`/`}}`) so a future edit to this function's format! string is a loud,
        // intentional diff here too.
        assert_eq!(
            disable_hint("example-rule"),
            "Disable via config `rules: { \"example-rule\": \"off\" }` (embedders: `disabled_rules`)"
        );
    }
}

/// Normalized rule output. Narrow on `rule_id` to recover the concrete shape (a pack's native shape lives in `data`).
/// `#[serde(rename_all = "camelCase")]`: this is an output-only type (never deserialized from an external
/// input contract), so its JSON shape is free to use the same camelCase convention as every other
/// napi-boundary output type — see `packages/napi/src/api.rs`'s `AnalyzeOutputView` doc for the full
/// casing-unification rationale. Only `rule_id` -> `ruleId` actually changes; every other field is a
/// single word.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Rule id (e.g. "java-security/taint", "graph/circular").
    pub rule_id: String,
    pub severity: Severity,
    pub file: String,
    pub line: u32,
    /// One-line message / snippet.
    pub message: String,
    /// Pack-native finding shape — opaque at the engine boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
