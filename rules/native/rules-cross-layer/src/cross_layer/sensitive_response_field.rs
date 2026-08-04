//! `cross-layer/sensitive-response-field` (warning; critical when consumed) — a route handler's
//! DECLARED response shape (`response-shape-v1`: the return-type annotation's class/interface,
//! resolved at assemble time) contains a field whose NAME is sensitive-shaped (`password`/`token`/
//! `secret`/hash-family). The first consumer of the response-shape fact, landed WITH it (the "a
//! field lands together with its consuming rule" wave rule — no reader means speculative field).
//!
//! ## Why provide-side only, and what the join adds
//! The base finding needs NO cross-layer edge — the declaration alone is the evidence, so the rule
//! fires on a corpus with zero resolved edges (unlike the drift rules, which compare two sides).
//! The join is the ESCALATION evidence: an `http` edge landing on the route proves an in-analysis
//! consumer receives the declared shape — critical, with the consumer count named. No edge means
//! "no consumer WITNESSED", never "unexposed" — the message says so.
//!
//! ## Substrate boundary vs the DSL `security` pack (deliberate, in the message too)
//! The `security` pack's secret rules (`hardcoded-secret`, `jwt-sign-literal-secret`,
//! `vendor-token-committed`, ...) match literal secret VALUES written in source. This rule reads
//! declared response field NAMES — no value ever appears in the declaration. Different substrate,
//! different defect (a leak of stored data through an API contract, not a committed credential);
//! the two can legitimately co-fire on one file and neither subsumes the other.
//!
//! ## Honesty bounds (each named in the finding message)
//! - NAME evidence only: a field named `token` may carry a public identifier, and an auth route
//!   returning a token IS its design — the finding asks for a review, and severity never exceeds
//!   what the name + the join can prove.
//! - DECLARATION evidence only: runtime serialization (`class-transformer` `@Exclude` decorators, `toJSON` methods,
//!   interceptors) may strip the field at runtime — not read, by the projection contract (input =
//!   declarations, flow tracing out of scope).

use std::collections::BTreeMap;

use zzop_core::{disable_hint, CrossLayerEdge, Finding, ProvideResponseShape, Severity};

/// Sensitive-name SUBSTRING tokens, matched against a lowercased, `_`/`-`-stripped field name
/// (`passwordHash` -> `passwordhash` contains `password`). Substring matching is reserved for
/// tokens long/specific enough that no benign field name embeds them by accident.
///
/// ENTRY CONTRACT (all three lists, declared or default): the field-name side is normalized but
/// entries are compared as written — entries must be lowercase and `_`/`-`-free (`api_key` never
/// matches), deliberately UNLIKE `secretParamNames` where `api_key`/`api-key` are real entries.
///
/// This is the DEFAULT behind the declarable config key `vocabulary.sensitiveResponseFieldSubstrings`
/// (what a project calls its own sensitive fields is a name it picks) — `zzop_engine::VocabularyConfig`
/// references THIS symbol rather than restating the list, and a declared list REPLACES it whole
/// (the vocabulary contract's per-key whole replacement, never an element-wise merge). Undeclared =
/// this axis makes no judgment, same as every declarable vocabulary since 2026-07-27.
pub const SENSITIVE_RESPONSE_FIELD_SUBSTRINGS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "apikey",
    "privatekey",
    "credential",
    "passphrase",
    "salt",
];

/// Sensitive-name EXACT tokens — short words whose substring form would drown in false positives
/// (`token` ⊂ `tokenCount`, `hash` ⊂ `contentHash`), so they must BE the whole normalized name.
/// Default behind `vocabulary.sensitiveResponseFieldExactNames` — same contract as
/// [`SENSITIVE_RESPONSE_FIELD_SUBSTRINGS`].
pub const SENSITIVE_RESPONSE_FIELD_EXACT: &[&str] = &["token", "jwt", "hash", "pwd", "ssn", "otp"];

/// Sensitive-name SUFFIX tokens: `accessToken`/`refreshToken`/`sessionToken`/`idToken` all end in
/// `token` while `tokenCount`/`tokenizer` do not — a suffix is the credential-naming convention's
/// shape. Only `token` today; `hash` is deliberately NOT here (`contentHash`/`commitHash` are
/// integrity fields, not secrets). Default behind `vocabulary.sensitiveResponseFieldSuffixes` —
/// same contract as [`SENSITIVE_RESPONSE_FIELD_SUBSTRINGS`].
pub const SENSITIVE_RESPONSE_FIELD_SUFFIXES: &[&str] = &["token"];

/// Extensions whose files the TypeScript parser — the one BUILT-IN producer of
/// `IoProvide::response` (`response-shape-v1` Nest return-type capture; a Mode B overlay can
/// supply the same fact for any language) — is dispatched for.
///
/// POLICY VALUE, T2: duplicates `zzop_engine`'s TypeScript dispatch arm (this crate depends on
/// `zzop_core` only, so it cannot import the predicate) — pinned both directions by
/// `zzop_engine::sightlines`' `ts_witness_extension_lists_match_the_dispatch_table`, the same
/// arrangement as [`super::retrying_write_no_idempotency::RETRY_WITNESS_EXTENSIONS`].
pub const RESPONSE_WITNESS_EXTENSIONS: &[&str] =
    &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"];

/// This rule's machine-readable sightline declaration (`zzop_core::RuleSightline`): one BUILT-IN
/// recognizer today (parser-typescript's controller-decorator capture; a Mode B overlay can supply
/// the fact for any language), and that fact is this rule's only trigger — the SILENCE class.
pub(crate) fn sightlines() -> Vec<zzop_core::RuleSightline> {
    vec![zzop_core::RuleSightline {
        rule_id: "cross-layer/sensitive-response-field",
        trigger_extensions: RESPONSE_WITNESS_EXTENSIONS,
        outside_meaning: "a declared response shape (`IoProvide::response`) has one built-in producer \
                          today — the TypeScript parser's Nest controller-decorator return-type \
                          capture — while a Mode B adapter overlay can supply it for routes in any \
                          language, and that fact is this rule's only trigger — ZERO findings on a \
                          tree whose routes live outside these extensions (or in any non-decorator \
                          registration shape), with no overlay supplying the fact, means the response \
                          side was NOT ANALYZED, never \"no sensitive response field\" (and even \
                          inside them, a handler that declares no return type is skipped and \
                          disclosed via the tree's warnings, never guessed)"
            .to_string(),
        assert_when_blind: false,
    }]
}

/// One `http` provide site carrying a RESOLVED declared-response shape, tagged with its source tree —
/// engine-derived from the same `SourceIo` list the join consumes (same rationale as
/// [`super::HttpProvideSite`]: the kernel's join result deliberately does not thread rule inputs).
/// A plain struct rather than a site-keyed map: one line can carry several routes (an array-path
/// decorator emits one provide per path), which a site-keyed map would collapse.
#[derive(Debug, Clone)]
pub struct ResponseProvideSite {
    pub source: String,
    /// The full normalized `"METHOD /path"` key.
    pub key: String,
    pub file: String,
    pub line: u32,
    pub response: ProvideResponseShape,
}

/// Lowercase + strip `_`/`-` so `password_hash`, `PASSWORD-HASH` and `passwordHash` all normalize
/// to `passwordhash` — one spelling for the vocabulary to judge.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| *c != '_' && *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

/// The run's declared sensitive-name vocabulary (three matching axes over one normalized name) —
/// each list is the corresponding `vocabulary.sensitiveResponseField*` declaration (already
/// lowercased entries), or empty when undeclared (that axis then makes no judgment). The shipped
/// defaults are the three `SENSITIVE_RESPONSE_FIELD_*` consts above, reaching a run only through
/// the user's own config (template default), never as an engine assumption behind it.
#[derive(Debug, Clone, Copy)]
pub struct SensitiveResponseVocab<'a> {
    pub substrings: &'a [&'a str],
    pub exact_names: &'a [&'a str],
    pub suffixes: &'a [&'a str],
}

impl SensitiveResponseVocab<'_> {
    /// The built-in defaults — what `zzop init`'s template declares. Test/fixture convenience.
    pub fn built_in() -> Self {
        SensitiveResponseVocab {
            substrings: SENSITIVE_RESPONSE_FIELD_SUBSTRINGS,
            exact_names: SENSITIVE_RESPONSE_FIELD_EXACT,
            suffixes: SENSITIVE_RESPONSE_FIELD_SUFFIXES,
        }
    }

    /// The sensitive-vocabulary verdict for one declared field name.
    fn matches(&self, name: &str) -> bool {
        let norm = normalize(name);
        self.substrings.iter().any(|t| norm.contains(t))
            || self.exact_names.iter().any(|t| norm == *t)
            || self.suffixes.iter().any(|t| norm.ends_with(t))
    }
}

pub fn sensitive_response_field_findings(
    sites: &[ResponseProvideSite],
    edges: &[CrossLayerEdge],
    vocab: SensitiveResponseVocab<'_>,
) -> Vec<Finding> {
    // Consumer count per (site, route key), `http` edges only. A site-only key would let an
    // array-path decorator's sibling route (one provide per path from ONE line) inherit consumers
    // it never had; `edge.key == provide.key` is a linker exact-equality invariant, so keying on
    // it loses no real consumer.
    let mut consumers: BTreeMap<(&str, &str, u32, &str), u32> = BTreeMap::new();
    for e in edges.iter().filter(|e| e.kind == "http") {
        *consumers
            .entry((
                e.to.source.as_str(),
                e.to.file.as_str(),
                e.to.line,
                e.key.as_str(),
            ))
            .or_insert(0) += 1;
    }

    let mut out = Vec::new();
    for site in sites {
        // An unresolved `dto_ref` is a leak from assemble-time resolution (should not normally
        // reach a rule, but never guess if it does) — same stance as `body_field_drift`.
        if site.response.dto_ref.is_some() {
            continue;
        }
        let mut sensitive: Vec<&str> = site
            .response
            .fields
            .iter()
            .filter(|f| vocab.matches(&f.name))
            .map(|f| f.name.as_str())
            .collect();
        if sensitive.is_empty() {
            continue;
        }
        sensitive.sort_unstable();
        sensitive.dedup();
        let fields_list = sensitive.join(", ");
        let field_word = if sensitive.len() == 1 {
            "field"
        } else {
            "fields"
        };

        let consumer_count = consumers
            .get(&(
                site.source.as_str(),
                site.file.as_str(),
                site.line,
                site.key.as_str(),
            ))
            .copied()
            .unwrap_or(0);
        let consumed = consumer_count > 0;
        let (severity, exposure) = if consumed {
            let site_word = if consumer_count == 1 {
                "call site"
            } else {
                "call sites"
            };
            (
                Severity::Critical,
                format!(
                    "and the cross-layer join shows it consumed by {consumer_count} {site_word} in \
                     this analysis — the declared shape is live response surface"
                ),
            )
        } else {
            (
                Severity::Warning,
                "no in-analysis consumer was witnessed (which never proves the route unexposed — \
                 callers outside this analysis stay invisible)"
                    .to_string(),
            )
        };

        let message = format!(
            "route `{}` declares a response shape containing sensitive-named {field_word} \
             `{fields_list}`, {exposure}. Evidence is the declared field NAME only — the value may \
             be benign (a public id) and an auth route returning a token can be by design — and the \
             DECLARATION only: runtime serialization (`@Exclude` decorators, `toJSON` methods, interceptors) is not \
             read. Verify the field belongs in the wire contract; if not, remove it from the \
             response DTO or strip it before serialization. Distinct from the `security` pack's \
             secret rules, which match literal secret VALUES in source — this reads declared \
             response field names, so both can legitimately fire on one file. {}",
            site.key,
            disable_hint("cross-layer/sensitive-response-field"),
        );

        let mut data = serde_json::json!({
            // The ANCHOR's tree — `Finding::file` is tree-relative, so this is what makes
            // `<source>/<file>:<line>` a unique key (and what benchmark attribution reads).
            "source": site.source,
            "key": site.key,
            "sensitiveFields": sensitive,
            "consumed": consumed,
        });
        if consumed {
            data["consumerCount"] = serde_json::json!(consumer_count);
        }

        out.push(Finding {
            rule_id: "cross-layer/sensitive-response-field".to_string(),
            severity,
            file: site.file.clone(),
            line: site.line,
            message,
            evidence_paths: Vec::new(),
            data: Some(data),
        });
    }

    // Deterministic order + dedupe: one finding per (source, file, line, key) — `sites` is
    // engine-derived from per-tree io in input order, and a tree could legitimately register the
    // same key twice at one line only through producer duplication, which must not double-report.
    out.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then_with(|| data_str(a, "source").cmp(data_str(b, "source")))
            .then_with(|| data_str(a, "key").cmp(data_str(b, "key")))
    });
    out.dedup_by(|a, b| {
        a.file == b.file
            && a.line == b.line
            && data_str(a, "source") == data_str(b, "source")
            && data_str(a, "key") == data_str(b, "key")
    });
    out
}

fn data_str<'a>(f: &'a Finding, key: &str) -> &'a str {
    f.data
        .as_ref()
        .and_then(|d| d.get(key))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
