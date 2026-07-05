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
