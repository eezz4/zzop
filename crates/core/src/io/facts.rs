//! The IO fact types an adapter emits ([`IoFacts`]: provides/consumes, with witnessed body shapes)
//! and the cross-layer result/bucket types the linker produces. Serde wire contract: these types
//! serialize into cache artifacts and JSON output — attributes are frozen, do not reshape.

use serde::{Deserialize, Serialize};

/// The boundary an interface crosses. Open-ended (String) so an adapter may introduce its own kind.
pub type IoKind = String;

/// The statically witnessed shape of a request-body object literal at an HTTP consume site.
/// Extraction is evidence-only: keys are recorded exactly as written (dotted paths, depth <= 2 —
/// one level under each top-level key, which is all the DTO comparison needs), and NOTHING is
/// inferred about parts the literal does not show. A body passed as an identifier/expression is
/// not represented at all (`IoConsume::body: None`), never approximated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumeBodyShape {
    /// Dotted key paths witnessed in the literal (e.g. `"user"`, `"user.email"`). A shorthand
    /// property (`{ user }`) contributes its key; its children stay unwitnessed.
    pub keys: Vec<String>,
    /// Paths whose DIRECT children are exhaustively listed in `keys` — `""` for the top level.
    /// A level containing a spread, computed key, getter, or non-literal nested value is omitted,
    /// which suppresses any "missing field" comparison at that level (incomplete evidence stays
    /// silent). "Extra key" comparisons only need the witnessed key itself, so they survive.
    pub complete_at: Vec<String>,
}

/// One declared field of a request-body DTO class (name + whether the contract requires it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvideBodyField {
    pub name: String,
    /// `true` when the field is `?`-optional or carries an `@IsOptional()` decorator.
    pub optional: bool,
}

/// The request-body contract a route handler declares (`@Body() dto: CreateUserDto`).
/// Emitted by the parser with only `dto_ref` set (the DTO class usually lives in another file);
/// assemble resolves the ref against the tree-wide merged class-shape map and fills `fields`.
/// An unresolvable or ambiguous ref drops the whole shape (never guessed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvideBodyShape {
    /// `@Body('user')` sub-key — the DTO describes `body.user`, not the body root. `None` = root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_key: Option<String>,
    /// Unresolved DTO class name as written in the parameter type annotation. Present on parser
    /// emit; cleared by assemble once `fields` is materialized (an adapter overlay may instead
    /// supply `fields` directly and leave this `None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dto_ref: Option<String>,
    /// Resolved DTO fields (empty until assemble resolves `dto_ref`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ProvideBodyField>,
    /// `false` when the DTO's field list may be partial (an `extends` clause, constructor
    /// parameter properties, an index signature, or computed keys) — suppresses "extra key"
    /// claims, since the unseen parent may declare the key.
    #[serde(default)]
    pub complete: bool,
}

/// An ingress a tree PROVIDES. `key` is the adapter-normalized interface identity.
/// `#[serde(rename_all = "camelCase")]` is a no-op today (every field is one word) — applied for
/// future-proofing/consistency; this type is shared with `docs/NORMALIZED_AST.md`'s frozen v1 envelope
/// input contract (via `FileProjection.io`), but since no field name actually changes there is no casing
/// conflict to resolve (unlike `SourceSymbol` — see that type's doc).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IoProvide {
    pub kind: IoKind,
    pub key: String,
    pub file: String,
    pub line: u32,
    /// Handler/owner symbol id (e.g. the controller method) for richer edges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Request-body contract the handler declares, when statically visible (`@Body()` param with
    /// a class DTO type). See `ProvideBodyShape` — additive/optional, absent everywhere it does
    /// not apply, so the frozen v1 envelope contract is untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<ProvideBodyShape>,
}

/// An egress a tree CONSUMES. `key` = None when the adapter could not statically resolve a dynamic target
/// (it is reported, never force-matched). See `IoProvide`'s doc re: `rename_all` being a no-op here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IoConsume {
    pub kind: IoKind,
    pub key: Option<String>,
    pub file: String,
    pub line: u32,
    /// The raw expression text as written at the call site. Set on every consume the adapter could not
    /// resolve immediately (`key: None`) — and deliberately KEPT when the engine's late cross-file
    /// resolution fills `key` in afterwards, as provenance that the key came from a constant lookup
    /// rather than a literal. `raw` present therefore does NOT imply unresolved; `key: None` alone does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    /// HTTP method of the call site, kept for late re-resolution of an unresolved consume (the joinable
    /// key is `"METHOD /path"` — without the method a late-resolved path could not be keyed). Only set
    /// when `key` is `None` at extraction time; a resolved consume already encodes its method inside
    /// `key` (late resolution fills `key` but leaves this field in place, like `raw`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// Statically witnessed request-body literal shape at the call site, when one is visible.
    /// See `ConsumeBodyShape` — additive/optional, same envelope-compat note as `IoProvide::body`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<ConsumeBodyShape>,
    /// Which client recognizer produced this consume (`"axios"`, `"ky"`, `"fetch"`, `"$fetch"`,
    /// `"angular"`) — provenance for CLIENT-SCOPED normalization seams, e.g. an
    /// `axios.defaults.baseURL` path prefix must apply to axios call sites only, never to a fetch
    /// call in the same tree. `None` = producer doesn't tag (older envelopes, non-egress kinds);
    /// a client-scoped seam then leaves the consume untouched (never guessed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// `Some(true)` when this egress call site is statically witnessed to run under an automatic-retry
    /// mechanism (an `axios-retry`-wired file, or a lexical retry wrapper like `pRetry(...)` /
    /// `backOff(...)`) AND the call is a WRITE verb (POST/PUT/PATCH/DELETE) — i.e. a non-idempotent
    /// request that may be replayed. Read verbs are never tagged (replaying a GET is safe), so the
    /// field is a *risk* marker, present only where it means something. Producer: the parser-typescript
    /// egress COLLECTOR only (TS-only — see the projection-contract language-coverage matrix); the sibling
    /// TS consume producers (hono-client, trpc, and the engine's intra-file fetch-wrapper synthesis) leave
    /// it `None`, so a write reached only through those paths is not yet tagged (an increment-2 gap). `None`
    /// everywhere else too (other languages, non-egress kinds, read verbs, no retry witnessed) — absence is
    /// a graceful skip, never a claim of "no retry". Consumed cross-tree by the
    /// `cross-layer/retrying-write-no-idempotency` rule, which flags a retried write that resolves to a real
    /// provider route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_configured: Option<bool>,
}

/// The IO an adapter emits for one tree (alongside MinimalIR dep/symbols/loc). See `IoProvide`'s doc re:
/// `rename_all` being a no-op here.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IoFacts {
    pub provides: Vec<IoProvide>,
    pub consumes: Vec<IoConsume>,
}

/// One source tree's IO, tagged with the source id it came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceIo {
    pub source: String,
    pub io: IoFacts,
}

/// A resolved cross-layer edge: a consumer site depends on a provider site, joined on (kind, key).
/// Output-only (reached via `analyzeTrees`'s `crossLayer` field) — no input-contract sharing, so
/// `cross_source` -> `crossSource` renames freely.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossLayerEdge {
    pub kind: IoKind,
    pub key: String,
    pub from: EdgeFrom,
    pub to: EdgeTo,
    /// True when consumer and provider are different source trees — the cross-repo/layer case.
    pub cross_source: bool,
    /// Set when `key` matched one of `LinkOptions::low_confidence_key_patterns` — the edge is still
    /// emitted (exactly one provider tree, so it is not ambiguous), but the key shape itself is generic
    /// enough (e.g. `/health`) that many unrelated services could share it, so the match is lower
    /// confidence than a distinctively-named route. `None` when no pattern matched (the common case).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low_confidence_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeFrom {
    pub source: String,
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeTo {
    pub source: String,
    pub file: String,
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

/// Output-only (`analyzeTrees`'s `crossLayer` field) — `unconsumed_provides` -> `unconsumedProvides` renames
/// freely.
/// The five buckets besides `edges` (`unconsumed_provides`, `unprovided_consumes`, `unresolved_consumes`,
/// `external_consumes`, `ambiguous_consumes`'s candidates) are disjoint by construction: a provide/consume
/// lands in exactly one of them (see [`link_cross_layer_io`](crate::io::link_cross_layer_io)'s doc for how
/// the ambiguous-candidate exclusion keeps `unconsumed_provides` honest).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossLayerResult {
    /// consume -> provide matches (the cross-layer dependency edges).
    pub edges: Vec<CrossLayerEdge>,
    /// A provide nothing consumes — cross-layer dead code. Never includes a provide that is an
    /// [`ambiguous_consumes`](Self::ambiguous_consumes) candidate: that provide IS referenced, just not
    /// unambiguously, so calling it dead would be misleading.
    pub unconsumed_provides: Vec<TaggedProvide>,
    /// A consume whose key nothing provides — drift/bug.
    pub unprovided_consumes: Vec<TaggedConsume>,
    /// A consume the adapter could not resolve (key=None) — marked, never matched.
    pub unresolved_consumes: Vec<TaggedConsume>,
    /// A consume whose key carries a host (`"://"` present, e.g. `GET https://vendor.com/api/users`) —
    /// third-party egress. Never cross-tree joined and never counted as `unprovidedConsumes`, since an
    /// unmatched absolute-URL consume is expected (nothing in THIS analysis should provide someone else's
    /// API), not drift.
    pub external_consumes: Vec<TaggedConsume>,
    /// A consume whose key is provided by 2+ DISTINCT source trees — not auto-linked (no edges emitted for
    /// it), since picking one provider over another would be a guess. Every candidate provider is listed
    /// so a caller can resolve the ambiguity by hand.
    pub ambiguous_consumes: Vec<AmbiguousConsume>,
    /// Per DISTINCT host declared in [`LinkOptions::internal_hosts`](crate::io::LinkOptions::internal_hosts)
    /// (input order preserved, after
    /// dedup): how many absolute-URL consumes were re-keyed to internal and joined via the normal path
    /// (edge/ambiguous/unprovided — see that field's doc for the re-keying gate itself, applied BEFORE
    /// the `"://"` external-egress gate). Empty when no hosts were declared. Effect-counting substrate
    /// for the engine's zero-effect tripwire (a declared host with count 0 is stale, or its consumers use
    /// relative paths — see `zzop_engine::analyze_trees`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_rekey_counts: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaggedProvide {
    pub source: String,
    #[serde(flatten)]
    pub provide: IoProvide,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaggedConsume {
    pub source: String,
    #[serde(flatten)]
    pub consume: IoConsume,
}

/// A consume whose key matched providers spanning 2+ distinct source trees — see
/// [`CrossLayerResult::ambiguous_consumes`]. `candidates` is sorted deterministically by `(source, file, line)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmbiguousConsume {
    pub source: String,
    #[serde(flatten)]
    pub consume: IoConsume,
    pub candidates: Vec<TaggedProvide>,
}
