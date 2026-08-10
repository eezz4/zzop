//! Matcher shapes — the `Matcher` enum with one struct per variant (the enum below is the list; this
//! header does not keep a second copy) plus the shared `LabeledPattern`. Split out of `def/mod.rs` (which keeps the
//! pack/rule envelope types and the fragment-expansion logic) purely to stay under the repo's per-file
//! line cap; `def/mod.rs` re-exports every type here so external paths (`zzop_core::dsl::def::Matcher`,
//! `…::LineScan`, …) are unchanged.
//!
//! `LineScan`, `MethodScan` and `CallScan` live one level down ([`line_scan`], [`method_scan`],
//! [`call_scan`]) for that same line-cap reason and are re-exported here, so nothing outside this file
//! can tell the difference — see those modules' own headers.

use serde::Deserialize;

use crate::{io::IoKind, ir::SourceSymbolKind};

mod call_scan;
mod line_scan;
mod literal_scan;
mod method_scan;

pub use call_scan::CallScan;
pub use line_scan::LineScan;
pub use literal_scan::LiteralScan;
pub use method_scan::MethodScan;

/// Matcher — dispatched on the `type` tag. v0 was lexical line-scan + method-scan; symbol-scan and io-scan
/// (below) are the first IR-query matchers. Whole-graph queries (cross-file/cross-layer) still stay native.
///
/// The wire tag is the variant name in kebab-case, so a pack writes `"type": "call-scan"`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Matcher {
    LineScan(LineScan),
    MethodScan(MethodScan),
    SymbolScan(SymbolScan),
    IoScan(IoScan),
    /// Query over a file's projected `call_sites` — see [`CallScan`].
    CallScan(CallScan),
    /// Query over a file's projected `string_literals` (name + value hash + value entropy, never the
    /// value) — see [`LiteralScan`].
    LiteralScan(LiteralScan),
}

/// A regex + classification label (becomes a finding's `data.label` on first match).
#[derive(Debug, Clone, Deserialize)]
pub struct LabeledPattern {
    pub pattern: String,
    pub label: String,
}

pub(super) fn default_snippet_max() -> usize {
    160
}

/// Query over a file's `SourceSymbol` list (declarations the parser projected), for naming-convention /
/// banned-export style rules line-scan can't express reliably (e.g. "every exported React component must
/// be PascalCase"). Filters combine with AND: `file_pattern` narrows the file set; `kind`/`name_pattern`/
/// `exported` narrow the symbols within it.
///
/// `negate` flips what `name_pattern` means rather than negating the whole matcher: `false` (default) fires
/// on a symbol matching it; `true` fires on a symbol NOT matching it. `negate: true` with no `name_pattern`
/// has nothing to negate against, so every symbol passes — equivalent to a plain `kind`/`exported` query.
#[derive(Debug, Clone, Deserialize)]
pub struct SymbolScan {
    /// Target file-path regex (e.g. `(?i)\.tsx?$`).
    pub file_pattern: String,
    /// Restrict to one `SourceSymbolKind` (function/class/const/type/interface).
    #[serde(default)]
    pub kind: Option<SourceSymbolKind>,
    /// Regex on the symbol name — meaning flips under `negate` (see struct doc).
    #[serde(default)]
    pub name_pattern: Option<String>,
    /// Restrict to exported (`true`) or non-exported (`false`) symbols.
    #[serde(default)]
    pub exported: Option<bool>,
    /// See struct doc — flips `name_pattern`'s role from "must match" to "must not match".
    #[serde(default)]
    pub negate: bool,
}

/// Which side(s) of a file's `IoFacts` an `IoScan` rule queries.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IoDirection {
    Provides,
    Consumes,
    Any,
}

/// Query over a tree's cross-layer IO (every `IoProvide`/`IoConsume`, projected whole-tree since the 2026
/// projection redesign — see `crate::dsl::eval_pack_io_scan`), for boundary-convention rules
/// line-scan/method-scan can't express (e.g. "every HTTP endpoint must be versioned under
/// `/api/v[0-9]+/`", or "every mutating route must carry an `auth-guarded` attribute"). Filters combine
/// with AND, evaluated cheap-first: `file_exclude_pattern` right after `file_pattern`, then `direction`
/// selects `provides`/`consumes`/`any`, `kind` is an exact match, then `key_pattern`/`negate` (below), then
/// the four additive gates below (`symbol_pattern`, `attr_present`, `attr_absent`,
/// `anchor_exclude_pattern`) — each a plain conjunctive filter evaluated AFTER `negate` has already
/// resolved `key_pattern`'s role; `negate` itself only ever flips `key_pattern`, never these newer fields.
/// `key_pattern` + `negate` work like `SymbolScan`'s. An entry with `key: None` (unresolved) never matches
/// `key_pattern` — under `negate: true` that makes it a hit.
#[derive(Debug, Clone, Deserialize)]
pub struct IoScan {
    /// Target file-path regex — see struct doc for why this field is required.
    pub file_pattern: String,
    /// Optional path regex — an entry whose `file` matches this is skipped entirely, evaluated right
    /// after `file_pattern` (cheapest gate first). Same rationale and shape as `LineScan`'s field of the
    /// same name (e.g. excluding `${test-paths-stories}` so a composed whole-tree provide/consume from a
    /// test/story file never reaches the rule) — fragment-expanded by `RulePackDef::expand_fragments`
    /// exactly like `LineScan::file_exclude_pattern` is.
    #[serde(default)]
    pub file_exclude_pattern: Option<String>,
    pub direction: IoDirection,
    /// Exact match against `IoProvide`/`IoConsume`'s `kind` string (e.g. `"http"`, `"db-table"`).
    #[serde(default)]
    pub kind: Option<IoKind>,
    /// Regex on the entry's normalized key — meaning flips under `negate` (see struct doc).
    #[serde(default)]
    pub key_pattern: Option<String>,
    /// See struct doc — flips `key_pattern`'s role from "must match" to "must not match".
    #[serde(default)]
    pub negate: bool,
    /// Regex on `IoProvide::symbol` — PROVIDES-ONLY evidence: a consume never carries a symbol, so when
    /// this is set a consume entry never matches, and a provide whose `symbol` is `None` never matches
    /// either (never-guess). Unlike `key_pattern`, `negate` does NOT flip this field's role — it is a
    /// plain "must match" gate evaluated after `negate` has already resolved `key_pattern` (see struct
    /// doc).
    #[serde(default)]
    pub symbol_pattern: Option<String>,
    /// Entry matches only when the tree's `AttributeStore` has NO truthy value for
    /// `route_attr(entry.kind, entry.key, attr_absent)` (see `crate::attributes::AttributeStore::route_attr`
    /// — exact `IoKey` wins, else the longest covering `PathScope`; truthiness via `attr_is_truthy`). An
    /// entry with no resolved key (an unresolved consume) has nothing to look up, so it always satisfies
    /// this gate. A plain string, not a regex — never regex-checked by `pack_regex_issues`.
    #[serde(default)]
    pub attr_absent: Option<String>,
    /// Entry matches only when that same `route_attr` lookup IS truthy. An entry with no resolved key
    /// never satisfies this gate (nothing to look up). A plain string, not a regex — never regex-checked
    /// by `pack_regex_issues`.
    #[serde(default)]
    pub attr_present: Option<String>,
    /// Regex applied to the ANCHOR LINE's own text (the provide/consume's own source line, fetched via
    /// `IoScanTreeContext::anchor_line`). When the callback returns `None` (no source text reachable —
    /// e.g. envelope mode with no native source), the exclusion simply does not apply: lexical carve-outs
    /// are a native-tree convenience, honestly absent without source text, never a guessed match.
    #[serde(default)]
    pub anchor_exclude_pattern: Option<String>,
}
