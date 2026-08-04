//! Finding / Severity / RuleExplain — rule output contract.
//! Whether a finding comes from a native rule or a DSL pack, it is normalized and merged here.

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
/// (`rules: { "<id>": "off" }`), the embedder-facing field name in the parenthetical (`disabledRules`,
/// the WIRE spelling — every embedder request surface serializes with
/// `#[serde(rename_all = "camelCase")]` and deliberately without `deny_unknown_fields`, so the
/// snake_case `disabled_rules` this hint printed until 2026-08-02 was a spelling the wire silently
/// ignores: a user who typed the hint verbatim into a JSON request got a no-op, not an error) —
/// one shared builder so this fragment cannot drift per call site the way it did before this function
/// existed. A 2026-07-10 audit swept 31 native message sites that had each hand-written a slightly
/// different rendering of this same fragment, plus one plain-string (non-`format!`) site that shipped a
/// literal `{{` in its output because a mechanical format!-escaping sweep assumed every site was inside a
/// `format!` call — both defect classes are structurally impossible once every site calls this instead of
/// hand-writing the text. See `docs/rules/authoring-guide.md`'s "Message triple" / native-rule-message
/// contract bullets, and `crates/engine/tests/rule_contracts/`'s
/// `native_rule_files_that_build_findings_mention_disabled_rules` test, for the "how to exclude" leg every
/// native finding message must carry.
pub fn disable_hint(id: &str) -> String {
    format!("Disable via config `rules: {{ \"{id}\": \"off\" }}` (embedders: `disabledRules`)")
}

#[cfg(test)]
mod disable_hint_tests {
    // Deliberately synthetic, made-up ids (not a real registered native analysis id) — this module lives
    // in `crates/core/src`, which `crates/engine/tests/rule_contracts/`'s
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
    fn renders_the_wire_spelled_embedder_field_name() {
        let hint = disable_hint("example-rule");
        assert!(
            hint.contains("disabledRules"),
            "disable_hint must name the disabledRules embedder field in its WIRE (camelCase) spelling — \
             the request surfaces are `#[serde(rename_all = \"camelCase\")]` without \
             `deny_unknown_fields`, so a snake_case `disabled_rules` in a JSON request is a silent \
             no-op: {hint:?}"
        );
        assert!(
            !hint.contains("disabled_rules"),
            "disable_hint must not print the snake_case spelling the wire silently ignores: {hint:?}"
        );
    }

    #[test]
    fn renders_the_exact_known_shape() {
        // Pins the exact rendering (config dialect first, embedder field in the parenthetical, single
        // braces — never `{{`/`}}`) so a future edit to this function's format! string is a loud,
        // intentional diff here too.
        assert_eq!(
            disable_hint("example-rule"),
            "Disable via config `rules: { \"example-rule\": \"off\" }` (embedders: `disabledRules`)"
        );
    }
}

/// Normalized rule output. Narrow on `rule_id` to recover the concrete shape (a pack's native shape lives in `data`).
/// `#[serde(rename_all = "camelCase")]`: this is an output-only type (never deserialized from an external
/// input contract), so its JSON shape is free to use the same camelCase convention as every other
/// wire-boundary output type — see `crates/facade/src/lib.rs`'s `AnalyzeOutputView` doc for the full
/// casing-unification rationale. Only `rule_id` -> `ruleId` actually changes; every other field is a
/// single word.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Rule id (e.g. "security/sql-string-concat", "graph/circular").
    pub rule_id: String,
    pub severity: Severity,
    pub file: String,
    pub line: u32,
    /// One-line message / snippet.
    pub message: String,
    /// Every OTHER tree-relative path this finding names — the paths it prints in `message`/`data` that
    /// are not [`file`](Finding::file). Empty for the great majority of findings, which speak about one
    /// place; populated by the relational rules that necessarily name two (a consume site and the
    /// provide it mismatched, an N-source collision's sibling sites).
    ///
    /// EXISTS SO `exclude` CAN MEAN WHAT IT SAYS. The filter used to read the anchor alone, so the
    /// contract it actually enforced was "do not ANCHOR a finding here" while every document promised
    /// "do not NAME this path to me": a finding anchored on the consume side sailed through the provide
    /// side's `exclude` and printed that path in its message anyway. The two roles get different
    /// treatment, from ONE config key — see `registry::merge_findings`: excluded ANCHOR drops the
    /// finding, excluded EVIDENCE redacts just that path. "My folder is the subject of the problem, I
    /// don't look at it; my folder is evidence in someone else's problem, I see the problem but my paths
    /// are not named."
    ///
    /// A TYPED FIELD RATHER THAN A PER-RULE TABLE OF `data` KEYS. The table version — "which `data` key
    /// holds a path, for each of the twelve rules that carry one" — is the shape this repo refuses
    /// elsewhere for the reason it would fail here: a thirteenth rule, or a renamed key, leaves it
    /// silently short, and the failure looks exactly like "that path was not excluded".
    ///
    /// Paths are tree-relative and in the same spelling `file` uses, because the exclude filter compares
    /// them with the identical substring/glob matcher.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_paths: Vec<String>,
    /// Pack-native finding shape — opaque at the engine boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
