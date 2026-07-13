//! Cross-layer IO — joins what one tree CONSUMES to what another tree PROVIDES, on a normalized contract key.
//! Not AST matching: a `(kind, key)` exact join, with three integrity gates layered on top of the raw join
//! (each verified against real false-positive vectors, not speculative):
//! - **Ambiguity**: a consume whose key is provided by 2+ DISTINCT source trees is not auto-linked — it goes
//!   to [`CrossLayerResult::ambiguous_consumes`] with every candidate listed, rather than emitting a many-to-many
//!   edge fan-out that silently picks a "winner". Providers all within ONE tree are unaffected (legal
//!   multi-provider case, e.g. one tree exposing a topic twice) and still join exactly like before.
//! - **External egress**: a consume whose key carries a host (`"://"` present — an absolute URL an adapter
//!   preserved) never cross-tree joins; it goes to [`CrossLayerResult::external_consumes`] instead of
//!   `unprovidedConsumes`, since it is third-party egress, not drift.
//! - **Low confidence**: an edge whose key matches an injected pattern (generic paths like `/health` that many
//!   unrelated services legitimately share) is still emitted, but tagged with
//!   [`CrossLayerEdge::low_confidence_reason`] so a consumer can discount it. The pattern table itself is
//!   injected via [`LinkOptions`] — core carries no default vocabulary (see `zzop_metrics::
//!   default_generic_interface_key_patterns` for the shipped table).
//!
//! Each adapter projects its IO to a normalized key with full local context; the linker is then a dumb exact
//! join (no AST-level heuristics). Even a crude parser (e.g. JSP) joins as a first-class citizen as long as it
//! emits accurate IoFacts.

use std::collections::HashMap;

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
    /// `"oazapfts"`, `"angular"`) — provenance for CLIENT-SCOPED normalization seams, e.g. an
    /// `axios.defaults.baseURL` path prefix must apply to axios call sites only, never to a fetch
    /// call in the same tree. `None` = producer doesn't tag (older envelopes, non-egress kinds);
    /// a client-scoped seam then leaves the consume untouched (never guessed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
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
/// lands in exactly one of them (see [`link_cross_layer_io`]'s doc for how the ambiguous-candidate exclusion
/// keeps `unconsumed_provides` honest).
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

/// Injectable options for [`link_cross_layer_io`]. Mirrors `zzop_git::CollectOptions::commit_type_patterns`'s
/// mechanism/vocabulary split: this crate owns the injectable mechanism (matching a compiled pattern against
/// an edge's key), never the default pattern table itself — that vocabulary (which paths count as "generic")
/// lives in `zzop_metrics::default_generic_interface_key_patterns`, injected by the engine call site.
#[derive(Debug, Clone, Default)]
pub struct LinkOptions {
    /// `(pattern, reason)` pairs, checked in order; the first pattern whose regex matches an edge's key
    /// sets that edge's `low_confidence_reason` to the paired reason string. Empty by default — no edge is
    /// ever marked low-confidence unless a caller injects a table.
    pub low_confidence_key_patterns: Vec<(regex::Regex, String)>,
}

/// Exact join of trees' IO on (kind, key), with the ambiguity/external/low-confidence gates documented in
/// this module's doc. Pure function (given `opts`).
pub fn link_cross_layer_io(trees: &[SourceIo], opts: &LinkOptions) -> CrossLayerResult {
    // Index providers by (kind, key). Multiple providers for one key is legal (e.g. two services expose one topic).
    let mut providers_by_key: HashMap<String, Vec<TaggedProvide>> = HashMap::new();
    for SourceIo { source, io } in trees {
        for p in &io.provides {
            providers_by_key
                .entry(id_key(&p.kind, &p.key))
                .or_default()
                .push(TaggedProvide {
                    source: source.clone(),
                    provide: p.clone(),
                });
        }
    }

    // Keys whose providers span 2+ DISTINCT source trees — ambiguous, never auto-linked. Computed once
    // over `providers_by_key` (source-tree spread is a property of the key's provider set, independent of
    // which consume is looking it up), then consulted per-consume below. NOTE: this set alone must NOT
    // drive the `unconsumed_provides` exclusion — a multi-tree key nobody consumes is still dead; only keys
    // an actual consume referenced ambiguously are exempt (tracked separately in `ambiguously_consumed_keys`).
    let ambiguous_keys: std::collections::HashSet<String> = providers_by_key
        .iter()
        .filter(|(_, providers)| {
            providers
                .iter()
                .map(|p| p.source.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len()
                >= 2
        })
        .map(|(k, _)| k.clone())
        .collect();

    let mut edges = Vec::new();
    let mut unprovided_consumes = Vec::new();
    let mut unresolved_consumes = Vec::new();
    let mut external_consumes = Vec::new();
    let mut ambiguous_consumes = Vec::new();
    let mut consumed_keys = std::collections::HashSet::new();
    let mut ambiguously_consumed_keys = std::collections::HashSet::new();

    for SourceIo { source, io } in trees {
        for c in &io.consumes {
            let Some(key) = &c.key else {
                unresolved_consumes.push(TaggedConsume {
                    source: source.clone(),
                    consume: c.clone(),
                });
                continue;
            };
            if key.contains("://") {
                // A host-carrying key is third-party egress — never cross-tree joined, never
                // `unprovidedConsumes`.
                external_consumes.push(TaggedConsume {
                    source: source.clone(),
                    consume: c.clone(),
                });
                continue;
            }
            let k = id_key(&c.kind, key);
            let Some(providers) = providers_by_key.get(&k) else {
                unprovided_consumes.push(TaggedConsume {
                    source: source.clone(),
                    consume: c.clone(),
                });
                continue;
            };
            if ambiguous_keys.contains(&k) {
                ambiguously_consumed_keys.insert(k.clone());
                let mut candidates = providers.clone();
                candidates.sort_by(|a, b| {
                    a.source
                        .cmp(&b.source)
                        .then(a.provide.file.cmp(&b.provide.file))
                        .then(a.provide.line.cmp(&b.provide.line))
                });
                ambiguous_consumes.push(AmbiguousConsume {
                    source: source.clone(),
                    consume: c.clone(),
                    candidates,
                });
                continue;
            }
            consumed_keys.insert(k.clone());
            let low_confidence_reason = opts
                .low_confidence_key_patterns
                .iter()
                .find(|(re, _)| re.is_match(key))
                .map(|(_, reason)| reason.clone());
            for p in providers {
                edges.push(CrossLayerEdge {
                    kind: c.kind.clone(),
                    key: key.clone(),
                    from: EdgeFrom {
                        source: source.clone(),
                        file: c.file.clone(),
                        line: c.line,
                    },
                    to: EdgeTo {
                        source: p.source.clone(),
                        file: p.provide.file.clone(),
                        line: p.provide.line,
                        symbol: p.provide.symbol.clone(),
                    },
                    cross_source: *source != p.source,
                    low_confidence_reason: low_confidence_reason.clone(),
                });
            }
        }
    }

    // A provide that was referenced ambiguously (it IS a candidate some consume saw, just not
    // unambiguously linkable) is not dead — but a multi-tree-provided key NOBODY consumes is exactly as
    // dead as a single-tree one, so the exclusion is keyed on `ambiguously_consumed_keys` (keys that
    // actually produced an `ambiguous_consumes` entry), never on provider-set shape alone — see
    // `CrossLayerResult::unconsumed_provides`'s doc.
    let mut unconsumed_provides = Vec::new();
    for (k, providers) in providers_by_key {
        if !consumed_keys.contains(&k) && !ambiguously_consumed_keys.contains(&k) {
            unconsumed_provides.extend(providers);
        }
    }
    // `providers_by_key` is a HashMap — sort so the serialized `unconsumedProvides` order is stable
    // run-to-run (deterministic-output contract; every other bucket is already ordered).
    unconsumed_provides.sort_by(|a, b| {
        a.provide
            .key
            .cmp(&b.provide.key)
            .then(a.source.cmp(&b.source))
            .then(a.provide.file.cmp(&b.provide.file))
            .then(a.provide.line.cmp(&b.provide.line))
    });

    edges.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then(a.from.file.cmp(&b.from.file))
            .then(a.from.line.cmp(&b.from.line))
    });
    ambiguous_consumes.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.consume.file.cmp(&b.consume.file))
            .then(a.consume.line.cmp(&b.consume.line))
    });
    external_consumes.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.consume.file.cmp(&b.consume.file))
            .then(a.consume.line.cmp(&b.consume.line))
    });

    CrossLayerResult {
        edges,
        unconsumed_provides,
        unprovided_consumes,
        unresolved_consumes,
        external_consumes,
        ambiguous_consumes,
    }
}

fn id_key(kind: &str, key: &str) -> String {
    format!("{kind} {key}")
}

/// The verb-shaped-NAME vocabulary: every place a cross-layer HTTP verb is inferred from an
/// identifier's NAME — a member callee (`axios.get`, Angular `this.http.get`), a computed-member
/// string literal (`axios['post']`), a hono `$get`-style terminal, an Express-style `.get(path, h)`
/// registration, a Spring `@GetMapping` annotation — draws from this ONE set (single definition;
/// per-vocabulary spellings stay at their call sites but are pinned to this const by tests, so a verb
/// added here is added everywhere deliberately, never by drift). Deliberately NOT a filter on keys
/// overall: extractors that read the verb from an EXPLICIT attribute (`fetch(url, { method: 'HEAD' })`,
/// Spring `method = RequestMethod.HEAD`) pass their literal through verbatim — an explicit spelling is
/// a visible fact, while a name-shaped match outside this set would be a guess.
pub const HTTP_KEY_VERBS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH"];

/// The canonical `http` interface key both sides must produce so the join is exact.
/// Path params (`{x}` or `:x`) -> `{}`; duplicate slashes collapsed; trailing slash dropped; method upper-cased.
/// Keeping this in core (not per-adapter) guarantees an FE-emitted key and a BE-emitted key are byte-identical.
pub fn http_interface_key(method: &str, raw_path: &str) -> String {
    let with_slash = format!("/{raw_path}");
    let collapsed = re_multi_slash().replace_all(&with_slash, "/");
    let params = re_param().replace_all(&collapsed, "{}");
    let trimmed = re_trailing().replace(&params, "$1");
    format!("{} {}", method.to_uppercase(), trimmed)
}

/// [`http_interface_key`] for a CONSUME-side URL: drops the query/fragment suffix (`?...`/`#...`)
/// before normalization. A call-site URL's `?` is always a query separator (`axios.get('articles?limit=10')`,
/// `` `articles?${qs}` `` -> `articles?{}`), and a route provide's key never carries one, so a
/// query-suffixed consume key is structurally guaranteed to miss the exact join AND the near-miss
/// segment comparison — the same reasoning that already drops oazapfts's `${QS...}` suffix at
/// extraction. Provide-side keying must NOT use this: in a route PATTERN a `?` is not a query
/// separator (e.g. Spring's `?` single-character wildcard), so provides keep [`http_interface_key`].
pub fn http_consume_interface_key(method: &str, raw_url: &str) -> String {
    let path = raw_url.split(['?', '#']).next().unwrap_or(raw_url);
    http_interface_key(method, path)
}

fn re_multi_slash() -> &'static regex::Regex {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"/+").unwrap())
}
fn re_param() -> &'static regex::Regex {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"\{[^}]+\}|:[A-Za-z_][A-Za-z0-9_]*").unwrap())
}
fn re_trailing() -> &'static regex::Regex {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"(.)/+$").unwrap())
}

#[cfg(test)]
mod tests {
    //! Exercises `link_cross_layer_io`: consume-to-provide joins across trees, dead provides (nothing
    //! consumes them), dangling consumes (nothing provides them), unresolved dynamic consumes are never
    //! force-matched, provider symbols carry onto the edge, `http_interface_key` normalization (params,
    //! slashes, method), and same-tree matches are not flagged cross-source.
    use super::*;

    fn provide(kind: &str, key: &str, file: &str, line: u32, symbol: Option<&str>) -> IoProvide {
        IoProvide {
            body: None,
            kind: kind.into(),
            key: key.into(),
            file: file.into(),
            line,
            symbol: symbol.map(Into::into),
        }
    }
    fn consume(
        kind: &str,
        key: Option<&str>,
        file: &str,
        line: u32,
        raw: Option<&str>,
    ) -> IoConsume {
        IoConsume {
            client: None,
            body: None,
            kind: kind.into(),
            key: key.map(Into::into),
            file: file.into(),
            line,
            raw: raw.map(Into::into),
            method: None,
        }
    }

    fn fixture() -> Vec<SourceIo> {
        let fe = SourceIo {
            source: "fe".into(),
            io: IoFacts {
                provides: vec![],
                consumes: vec![
                    consume("http", Some("GET /authen/getUserInfo"), "Ctx.tsx", 37, None),
                    consume("http", Some("GET /authen/getSignout"), "Ctx.tsx", 68, None),
                    consume("http", Some("GET /missing/route"), "Ctx.tsx", 99, None), // dangling
                    consume("http", None, "Dyn.tsx", 5, Some("axios.get(url)")),      // unresolved
                ],
            },
        };
        let be = SourceIo {
            source: "be".into(),
            io: IoFacts {
                provides: vec![
                    provide(
                        "http",
                        "GET /authen/getUserInfo",
                        "CtrlAuthen.java",
                        40,
                        Some("getUserInfo"),
                    ),
                    provide(
                        "http",
                        "GET /authen/getSignout",
                        "CtrlAuthen.java",
                        56,
                        None,
                    ),
                    provide(
                        "http",
                        "GET /authen/getGoogleRedirect",
                        "CtrlAuthen.java",
                        25,
                        None,
                    ), // dead
                ],
                consumes: vec![consume(
                    "db-table",
                    Some("table:users"),
                    "RepoAuthen.java",
                    12,
                    None,
                )],
            },
        };
        let db = SourceIo {
            source: "db".into(),
            io: IoFacts {
                provides: vec![provide("db-table", "table:users", "schema.sql", 1, None)],
                consumes: vec![],
            },
        };
        vec![fe, be, db]
    }

    #[test]
    fn joins_consume_to_provide_across_trees() {
        let r = link_cross_layer_io(&fixture(), &LinkOptions::default());
        let http: Vec<_> = r.edges.iter().filter(|e| e.kind == "http").collect();
        assert_eq!(http.len(), 2);
        // sorted by key -> getSignout first
        assert_eq!(http[0].key, "GET /authen/getSignout");
        assert_eq!(http[0].from.source, "fe");
        assert_eq!(http[0].from.line, 68);
        assert_eq!(http[0].to.source, "be");
        assert_eq!(http[0].to.line, 56);
        assert!(http[0].cross_source);
        // BE->DB edge also resolves (different kind / layer)
        let dbe = r.edges.iter().find(|e| e.kind == "db-table").unwrap();
        assert_eq!(dbe.from.source, "be");
        assert_eq!(dbe.to.source, "db");
        assert_eq!(dbe.to.file, "schema.sql");
        assert_eq!(r.edges.len(), 3);
    }

    #[test]
    fn provide_nothing_consumes_is_dead_code() {
        let r = link_cross_layer_io(&fixture(), &LinkOptions::default());
        assert_eq!(r.unconsumed_provides.len(), 1);
        assert_eq!(
            r.unconsumed_provides[0].provide.key,
            "GET /authen/getGoogleRedirect"
        );
        assert_eq!(r.unconsumed_provides[0].source, "be");
    }

    #[test]
    fn consume_nothing_provides_is_dangling() {
        let r = link_cross_layer_io(&fixture(), &LinkOptions::default());
        assert_eq!(r.unprovided_consumes.len(), 1);
        assert_eq!(
            r.unprovided_consumes[0].consume.key.as_deref(),
            Some("GET /missing/route")
        );
        assert_eq!(r.unprovided_consumes[0].source, "fe");
    }

    #[test]
    fn unresolvable_dynamic_consume_never_force_matched() {
        let r = link_cross_layer_io(&fixture(), &LinkOptions::default());
        assert_eq!(r.unresolved_consumes.len(), 1);
        assert_eq!(
            r.unresolved_consumes[0].consume.raw.as_deref(),
            Some("axios.get(url)")
        );
        assert_eq!(r.unresolved_consumes[0].source, "fe");
    }

    #[test]
    fn carries_provider_symbol_onto_edge() {
        let r = link_cross_layer_io(&fixture(), &LinkOptions::default());
        let e = r
            .edges
            .iter()
            .find(|x| x.key == "GET /authen/getUserInfo")
            .unwrap();
        assert_eq!(e.to.symbol.as_deref(), Some("getUserInfo"));
    }

    #[test]
    fn http_key_normalizes_params_slashes_and_method() {
        assert_eq!(
            http_interface_key("get", "authen/getUserInfo"),
            "GET /authen/getUserInfo"
        );
        assert_eq!(http_interface_key("get", "/users/{id}"), "GET /users/{}");
        assert_eq!(http_interface_key("get", "/users/:id"), "GET /users/{}");
        // duplicate slashes collapsed, trailing slash dropped, method upper-cased
        assert_eq!(http_interface_key("post", "//a//b/"), "POST /a/b");
        // root path preserved (single slash, no trailing-drop)
        assert_eq!(http_interface_key("get", ""), "GET /");
    }

    #[test]
    fn http_consume_key_drops_query_and_fragment_suffix() {
        // Literal query — the fe-axios RealWorld corpus shape (`axios.get('articles?limit=10')`)
        // whose keyed form could never join `GET /articles` (dogfood round 6, 2026-07-10).
        assert_eq!(
            http_consume_interface_key("get", "articles?limit=10"),
            "GET /articles"
        );
        // Interpolated query is normalized to `?{}` upstream — same drop.
        assert_eq!(
            http_consume_interface_key("get", "/articles?{}"),
            "GET /articles"
        );
        // Query after a path param, and fragment suffix.
        assert_eq!(
            http_consume_interface_key("get", "/articles/{slug}?include=author"),
            "GET /articles/{}"
        );
        assert_eq!(
            http_consume_interface_key("get", "/docs#anchor"),
            "GET /docs"
        );
        // No suffix -> identical to http_interface_key.
        assert_eq!(
            http_consume_interface_key("post", "//a//b/"),
            http_interface_key("post", "//a//b/")
        );
        // Query-only URL degrades to the root path (the egress extractor vetoes this shape
        // earlier — see `base_relative_path` — so it only arises from an explicit `/?x=1`).
        assert_eq!(http_consume_interface_key("get", "/?page=2"), "GET /");
    }

    #[test]
    fn intra_tree_match_is_not_cross_source() {
        let one = SourceIo {
            source: "be".into(),
            io: IoFacts {
                provides: vec![provide("queue", "topic:jobs", "Producer.java", 3, None)],
                consumes: vec![consume(
                    "queue",
                    Some("topic:jobs"),
                    "Consumer.java",
                    9,
                    None,
                )],
            },
        };
        let r = link_cross_layer_io(&[one], &LinkOptions::default());
        assert_eq!(r.edges.len(), 1);
        assert!(!r.edges[0].cross_source);
        assert_eq!(r.unconsumed_provides.len(), 0);
    }

    #[test]
    fn key_provided_by_two_distinct_trees_is_ambiguous_not_edged() {
        // Same key ("GET /health") provided by TWO different source trees — a many-to-many join across
        // trees would silently pick one; instead this must land in `ambiguousConsumes` with both candidates,
        // emit no edges for it, and NOT appear in `unconsumedProvides` either (it IS referenced, just
        // ambiguously).
        let a = SourceIo {
            source: "svc-a".into(),
            io: IoFacts {
                provides: vec![provide("http", "GET /health", "svc-a/health.ts", 3, None)],
                consumes: vec![],
            },
        };
        let b = SourceIo {
            source: "svc-b".into(),
            io: IoFacts {
                provides: vec![provide("http", "GET /health", "svc-b/health.ts", 7, None)],
                consumes: vec![],
            },
        };
        let caller = SourceIo {
            source: "gateway".into(),
            io: IoFacts {
                provides: vec![],
                consumes: vec![consume("http", Some("GET /health"), "gw.ts", 1, None)],
            },
        };
        let r = link_cross_layer_io(&[a, b, caller], &LinkOptions::default());

        assert!(
            r.edges.iter().all(|e| e.key != "GET /health"),
            "ambiguous key must not produce edges: {:?}",
            r.edges
        );
        assert_eq!(r.ambiguous_consumes.len(), 1);
        assert_eq!(r.ambiguous_consumes[0].source, "gateway");
        assert_eq!(
            r.ambiguous_consumes[0].consume.key.as_deref(),
            Some("GET /health")
        );
        assert_eq!(r.ambiguous_consumes[0].candidates.len(), 2);
        // deterministically sorted by (source, file, line)
        assert_eq!(r.ambiguous_consumes[0].candidates[0].source, "svc-a");
        assert_eq!(r.ambiguous_consumes[0].candidates[1].source, "svc-b");

        assert!(
            r.unconsumed_provides
                .iter()
                .all(|p| p.provide.key != "GET /health"),
            "ambiguous-candidate provides must not be counted dead: {:?}",
            r.unconsumed_provides
        );
    }

    #[test]
    fn multi_tree_provided_key_nobody_consumes_is_still_dead() {
        // Two trees provide the same key and NO consume references it at all — the provider-set being
        // multi-tree must not exempt it from `unconsumedProvides` (that exemption is only for keys an
        // actual consume referenced ambiguously).
        let a = SourceIo {
            source: "svc-a".into(),
            io: IoFacts {
                provides: vec![provide("http", "DELETE /api/me", "svc-a/me.ts", 3, None)],
                consumes: vec![],
            },
        };
        let b = SourceIo {
            source: "svc-b".into(),
            io: IoFacts {
                provides: vec![provide("http", "DELETE /api/me", "svc-b/me.ts", 9, None)],
                consumes: vec![],
            },
        };
        let r = link_cross_layer_io(&[a, b], &LinkOptions::default());
        assert!(r.edges.is_empty());
        assert!(r.ambiguous_consumes.is_empty());
        assert_eq!(
            r.unconsumed_provides.len(),
            2,
            "both unconsumed provider entries must be reported dead: {:?}",
            r.unconsumed_provides
        );
    }

    #[test]
    fn multi_provider_within_one_tree_is_unaffected_by_ambiguity_gate() {
        // Two providers for the same key, but BOTH from the same source tree — legal multi-provider case
        // (e.g. a tree exposing a topic twice), unaffected by the cross-tree ambiguity gate: edges to each.
        let one = SourceIo {
            source: "be".into(),
            io: IoFacts {
                provides: vec![
                    provide("http", "GET /ping", "a.ts", 1, None),
                    provide("http", "GET /ping", "b.ts", 2, None),
                ],
                consumes: vec![consume("http", Some("GET /ping"), "c.ts", 3, None)],
            },
        };
        let r = link_cross_layer_io(&[one], &LinkOptions::default());
        assert_eq!(r.edges.len(), 2);
        assert!(r.ambiguous_consumes.is_empty());
    }

    #[test]
    fn host_carrying_consume_key_is_external_never_dangling_even_with_a_matching_internal_provide()
    {
        // "GET https://vendor.com/api/users" must route to `external`, never join even though an
        // internal "GET /api/users" provide exists in the same analysis — the host makes it egress, not
        // an internal route reference.
        let fe = SourceIo {
            source: "fe".into(),
            io: IoFacts {
                provides: vec![],
                consumes: vec![consume(
                    "http",
                    Some("GET https://vendor.com/api/users"),
                    "Client.tsx",
                    10,
                    None,
                )],
            },
        };
        let be = SourceIo {
            source: "be".into(),
            io: IoFacts {
                provides: vec![provide("http", "GET /api/users", "Api.java", 5, None)],
                consumes: vec![],
            },
        };
        let r = link_cross_layer_io(&[fe, be], &LinkOptions::default());

        assert_eq!(r.external_consumes.len(), 1);
        assert_eq!(
            r.external_consumes[0].consume.key.as_deref(),
            Some("GET https://vendor.com/api/users")
        );
        assert_eq!(r.external_consumes[0].source, "fe");
        assert!(r.unprovided_consumes.is_empty());
        assert!(r.edges.is_empty());
        // The internal BE provide is untouched — nothing consumed it, so it's dead, unrelated to external.
        assert_eq!(r.unconsumed_provides.len(), 1);
        assert_eq!(r.unconsumed_provides[0].provide.key, "GET /api/users");
    }

    #[test]
    fn edge_key_matching_a_low_confidence_pattern_carries_the_reason() {
        let fe = SourceIo {
            source: "fe".into(),
            io: IoFacts {
                provides: vec![],
                consumes: vec![
                    consume("http", Some("GET /health"), "Client.tsx", 1, None),
                    consume("http", Some("GET /orders"), "Client.tsx", 2, None),
                ],
            },
        };
        let be = SourceIo {
            source: "be".into(),
            io: IoFacts {
                provides: vec![
                    provide("http", "GET /health", "Api.java", 1, None),
                    provide("http", "GET /orders", "Api.java", 9, None),
                ],
                consumes: vec![],
            },
        };
        let opts = LinkOptions {
            low_confidence_key_patterns: vec![(
                regex::Regex::new(r"^GET /health$").unwrap(),
                "generic path shared by many services".to_string(),
            )],
        };
        let r = link_cross_layer_io(&[fe, be], &opts);

        let health = r.edges.iter().find(|e| e.key == "GET /health").unwrap();
        assert_eq!(
            health.low_confidence_reason.as_deref(),
            Some("generic path shared by many services")
        );
        let orders = r.edges.iter().find(|e| e.key == "GET /orders").unwrap();
        assert_eq!(orders.low_confidence_reason, None);
    }
}
