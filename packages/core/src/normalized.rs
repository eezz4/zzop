//! External-parser protocol receiver — the engine-side deserialization + validation of the
//! "Normalized AST" envelope an external/custom parser (Java, Python, JSP, anything the engine does
//! not parse natively) emits per source tree, frozen v1 by `docs/NORMALIZED_AST.md`. These types
//! mirror that document's Envelope/FileProjection JSON shapes field-for-field and reuse the SAME
//! `zpz_core` serde types (`SourceSymbol`/`ImportBinding`/`ReExport`/`IoFacts`) native parsers already
//! project into — an external parser is first-class regardless of how crude it is, as long as its
//! projection round-trips through these exact structs (see the doc's Validation section).

use serde::{Deserialize, Serialize};

use crate::io::IoFacts;
use crate::ir::{ImportMap, ReExport, SourceSymbol};

/// The exact `format` string every conforming envelope must carry (`docs/NORMALIZED_AST.md`'s Envelope
/// section).
pub const NORMALIZED_AST_FORMAT: &str = "zpz-normalized-ast";

/// The highest `NormalizedEnvelope::version` this engine build understands — same "reject newer, never
/// guess" policy as `pack_loader::SUPPORTED_DSL_SCHEMA_VERSION` (see `docs/NORMALIZED_AST.md`'s "a
/// consumer rejects `version` greater than it supports" line).
pub const SUPPORTED_NORMALIZED_AST_VERSION: u32 = 1;

/// One external-parser invocation's output for one source tree (`docs/NORMALIZED_AST.md`'s Envelope
/// section, v1 freeze). `format`/`version` are plain fields here (not enforced at the type level) so a
/// deserialization failure can never hide a "wrong format string"/"future version" mismatch behind a
/// generic serde error — [`validate_envelope`] is what turns those into structured, actionable errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedEnvelope {
    pub format: String,
    pub version: u32,
    /// `"<parser id>/<impl version>"` — doubles as the cache fingerprint segment (bump the impl
    /// version whenever the projection changes for identical input).
    pub parser: String,
    /// Tree/source id — the cross-layer join's per-tree tag (see `crate::io`'s module doc).
    pub source: String,
    pub files: Vec<FileProjection>,
}

/// One file's projection (`docs/NORMALIZED_AST.md`'s FileProjection section) — every field mirrors a
/// reused `zpz_core` serde type; see that document for the authoritative semantics of each
/// (loc/symbols/imports/re_exports/used_names/io/degraded). Every optional-in-practice field defaults
/// to its empty value when a producer omits it (a minimal/degraded parser may legitimately have nothing
/// to say about, say, `re_exports`), matching the doc's "graceful degrade, never an error" convention.
///
/// IMPORTANT — cross-file specifier resolution for the fragment-channel fields below
/// (`const_map_fragment`/`trpc_router_fragments`/`router_mount_fragments`): a `TrpcRouterEntry::Ref`'s
/// `specifier` or a `RouterMountEntry::Mount`'s `specifier` MUST resolve to either (a) another file's
/// `path` exactly as that file emits it in this SAME envelope's `files[]` (an exact repo-relative
/// string match), or (b) a `./`- or `../`-relative path resolved from the EMITTING file's own
/// directory. An external adapter controls both sides of this reference — it emits both the fragment
/// and every file's `path` — so a full-envelope analysis (Mode A, `analyze_envelope`) never applies
/// tsconfig/workspace-alias resolution to fragments. Adapter OVERLAYS (Mode B) compose alongside the
/// native tree and inherit its alias-aware resolver — a superset; producers should rely only on the
/// exact/relative contract above so the same envelope behaves identically in both modes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileProjection {
    /// Relative, forward-slash path.
    pub path: String,
    /// Raw physical line count (`text.split('\n').length` semantics).
    pub loc: u32,
    #[serde(default)]
    pub symbols: Vec<SourceSymbol>,
    #[serde(default)]
    pub imports: ImportMap,
    #[serde(default)]
    pub re_exports: Vec<ReExport>,
    #[serde(default)]
    pub used_names: Vec<String>,
    /// Producer FRAGMENT CHANNELS for cross-file composition — the envelope equivalent of what
    /// `zpz_engine::analyze`'s `compose_trpc_provides` / `compose_router_mount_provides` already
    /// compose from native in-process adapters' per-file fragments. An external adapter that only
    /// knows plain io facts may omit all three entirely (default = empty, and that is a fully valid,
    /// non-degraded projection); one that additionally understands a router framework (tRPC,
    /// Hono-style code-registered mounting, etc.) can emit fragments here and have them fold into the
    /// SAME whole-tree composition pass native parsers' fragments go through — the engine does not
    /// care which side (native or external) produced a given fragment. `const_map_fragment` is a
    /// simpler same-shaped channel: `identifier -> literal string value` for this file's top-level
    /// `const` string bindings, used to resolve identifier-valued route/table arguments elsewhere in
    /// composition without a producer having to do that substitution itself.
    #[serde(default)]
    pub const_map_fragment: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub trpc_router_fragments: Vec<crate::TrpcRouterFragment>,
    #[serde(default)]
    pub router_mount_fragments: Vec<crate::RouterMountFragment>,
    #[serde(default)]
    pub io: IoFacts,
    /// The parser could not fully process this file (size cap, syntax failure) — `loc` must still be
    /// present regardless.
    #[serde(default)]
    pub degraded: bool,
}

/// Validates `json` against the v1 Normalized AST contract (`docs/NORMALIZED_AST.md`) beyond what plain
/// `serde_json` deserialization alone checks — a wrong `format` string or an out-of-range `version`
/// still deserializes fine as plain data (both are ordinary `String`/`u32` fields), so only this
/// function's semantic pass rejects them. Also rejected: an empty `path`, a duplicate `path` across
/// `files`, and a symbol whose `body_end` is less than its `body_start`.
///
/// Collects every applicable issue rather than stopping at the first — a producer fixing its output
/// against one `validate_envelope` call should see every problem at once, not one round-trip per bug
/// (the same "structured, list of issues" shape `pack_loader::load_dsl_packs`'s `LoadResult::errors`
/// uses for a directory of packs). Returns `Ok(envelope)` only when the JSON parses AND every semantic
/// check passes; a JSON parse failure short-circuits with a single-element `Vec` (there is no partial
/// envelope to inspect for further issues in that case).
// Note: `const_map_fragment`/`trpc_router_fragments`/`router_mount_fragments` presence is never
// validated here — any fragment content a producer emits is accepted as-is (empty is always valid,
// per their `#[serde(default)]`). An unresolvable `Ref`/`Mount` specifier is a composition-time
// concern, silently skipped by the engine's assembly pass, not a validation-time rejection — "never
// guessed" per this crate's convention, but also never a hard error for a shape this validator cannot
// know is wrong.
pub fn validate_envelope(json: &str) -> Result<NormalizedEnvelope, Vec<String>> {
    let envelope: NormalizedEnvelope =
        serde_json::from_str(json).map_err(|e| vec![format!("invalid JSON: {e}")])?;

    let mut errors = Vec::new();

    if envelope.format != NORMALIZED_AST_FORMAT {
        errors.push(format!(
            "unknown format: '{}' (expected '{NORMALIZED_AST_FORMAT}')",
            envelope.format
        ));
    }
    if envelope.version > SUPPORTED_NORMALIZED_AST_VERSION {
        errors.push(format!(
            "unsupported version: {} (this engine supports up to {SUPPORTED_NORMALIZED_AST_VERSION})",
            envelope.version
        ));
    }

    let mut seen_paths: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (idx, file) in envelope.files.iter().enumerate() {
        if file.path.is_empty() {
            errors.push(format!("files[{idx}]: empty path"));
        } else if !seen_paths.insert(file.path.as_str()) {
            errors.push(format!("files[{idx}] ('{}'): duplicate path", file.path));
        }
        for sym in &file.symbols {
            if let (Some(start), Some(end)) = (sym.body_start, sym.body_end) {
                if end < start {
                    errors.push(format!(
                        "files[{idx}] ('{}') symbol '{}': body_end ({end}) < body_start ({start})",
                        file.path, sym.name
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(envelope)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{SourceSymbol, SourceSymbolKind};

    fn symbol(name: &str, body_start: Option<u32>, body_end: Option<u32>) -> SourceSymbol {
        SourceSymbol {
            id: format!("f.ext#{name}"),
            file: "f.ext".into(),
            name: name.into(),
            kind: SourceSymbolKind::Function,
            line: 1,
            exported: true,
            is_default: false,
            body_start,
            body_end,
        }
    }

    fn valid_envelope_json() -> String {
        r#"{
            "format": "zpz-normalized-ast",
            "version": 1,
            "parser": "jsp-lexical/1",
            "source": "legacy",
            "files": [
                {
                    "path": "legacy/user.jsp",
                    "loc": 42,
                    "symbols": [],
                    "imports": {},
                    "re_exports": [],
                    "used_names": [],
                    "io": { "provides": [], "consumes": [] },
                    "degraded": false
                }
            ]
        }"#
        .to_string()
    }

    #[test]
    fn valid_envelope_round_trips() {
        let envelope = validate_envelope(&valid_envelope_json()).expect("should validate");
        assert_eq!(envelope.format, NORMALIZED_AST_FORMAT);
        assert_eq!(envelope.version, 1);
        assert_eq!(envelope.files.len(), 1);
        assert_eq!(envelope.files[0].path, "legacy/user.jsp");
    }

    #[test]
    fn minimal_envelope_with_defaulted_fields_round_trips() {
        // A minimal/degraded producer omits every optional field.
        let json = r#"{
            "format": "zpz-normalized-ast",
            "version": 1,
            "parser": "min/1",
            "source": "s",
            "files": [ { "path": "a.ext", "loc": 1 } ]
        }"#;
        let envelope = validate_envelope(json).expect("should validate");
        let file = &envelope.files[0];
        assert!(file.symbols.is_empty());
        assert!(file.imports.is_empty());
        assert!(file.re_exports.is_empty());
        assert!(file.used_names.is_empty());
        assert!(file.io.provides.is_empty());
        assert!(file.io.consumes.is_empty());
        assert!(!file.degraded);
        // A producer that only knows plain io facts may omit the fragment channels entirely — absent
        // still means empty, and this remains a fully valid, non-degraded projection.
        assert!(file.const_map_fragment.is_empty());
        assert!(file.trpc_router_fragments.is_empty());
        assert!(file.router_mount_fragments.is_empty());
    }

    #[test]
    fn fragment_channels_round_trip_when_present() {
        use crate::{RouterMountEntry, RouterMountFragment, TrpcRouterEntry, TrpcRouterFragment};

        let mut const_map_fragment = std::collections::HashMap::new();
        const_map_fragment.insert("USERS_TABLE".to_string(), "users".to_string());

        let envelope = NormalizedEnvelope {
            format: NORMALIZED_AST_FORMAT.to_string(),
            version: 1,
            parser: "custom-router/1".to_string(),
            source: "s".to_string(),
            files: vec![FileProjection {
                path: "a.ext".to_string(),
                loc: 10,
                symbols: vec![],
                imports: ImportMap::new(),
                re_exports: vec![],
                used_names: vec![],
                const_map_fragment,
                trpc_router_fragments: vec![TrpcRouterFragment {
                    name: "viewerRouter".to_string(),
                    entries: vec![TrpcRouterEntry::Leaf {
                        key: "get".to_string(),
                        verb: "QUERY".to_string(),
                        line: 3,
                    }],
                }],
                router_mount_fragments: vec![RouterMountFragment {
                    name: "auth".to_string(),
                    entries: vec![RouterMountEntry::Verb {
                        method: "POST".to_string(),
                        path: "/setup".to_string(),
                        handler: Some("handler".to_string()),
                        line: 7,
                    }],
                }],
                io: IoFacts::default(),
                degraded: false,
            }],
        };
        let json = serde_json::to_string(&envelope).unwrap();

        let round_tripped = validate_envelope(&json).expect("should validate");
        let file = &round_tripped.files[0];
        assert_eq!(
            file.const_map_fragment.get("USERS_TABLE"),
            Some(&"users".to_string())
        );
        assert_eq!(file.trpc_router_fragments.len(), 1);
        assert_eq!(file.trpc_router_fragments[0].name, "viewerRouter");
        assert_eq!(file.router_mount_fragments.len(), 1);
        assert_eq!(file.router_mount_fragments[0].name, "auth");

        // Re-serializing keeps the fragment channels present (not silently dropped on round-trip).
        assert!(json.contains("const_map_fragment"));
        assert!(json.contains("trpc_router_fragments"));
        assert!(json.contains("router_mount_fragments"));
    }

    #[test]
    fn rejects_invalid_json() {
        let errors = validate_envelope("not json").unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("invalid JSON"));
    }

    #[test]
    fn rejects_unknown_format() {
        let json = valid_envelope_json().replace("zpz-normalized-ast", "some-other-format");
        let errors = validate_envelope(&json).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("unknown format")));
    }

    #[test]
    fn rejects_version_greater_than_supported() {
        let json = valid_envelope_json().replace("\"version\": 1", "\"version\": 2");
        let errors = validate_envelope(&json).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("unsupported version")));
    }

    #[test]
    fn accepts_version_equal_to_supported() {
        assert!(validate_envelope(&valid_envelope_json()).is_ok());
    }

    #[test]
    fn rejects_empty_path() {
        let json = valid_envelope_json().replace("legacy/user.jsp", "");
        let errors = validate_envelope(&json).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("empty path")));
    }

    #[test]
    fn rejects_duplicate_paths() {
        let envelope = NormalizedEnvelope {
            format: NORMALIZED_AST_FORMAT.to_string(),
            version: 1,
            parser: "p/1".to_string(),
            source: "s".to_string(),
            files: vec![
                FileProjection {
                    path: "a.ext".to_string(),
                    loc: 1,
                    symbols: vec![],
                    imports: ImportMap::new(),
                    re_exports: vec![],
                    used_names: vec![],
                    const_map_fragment: std::collections::HashMap::new(),
                    trpc_router_fragments: vec![],
                    router_mount_fragments: vec![],
                    io: IoFacts::default(),
                    degraded: false,
                },
                FileProjection {
                    path: "a.ext".to_string(),
                    loc: 2,
                    symbols: vec![],
                    imports: ImportMap::new(),
                    re_exports: vec![],
                    used_names: vec![],
                    const_map_fragment: std::collections::HashMap::new(),
                    trpc_router_fragments: vec![],
                    router_mount_fragments: vec![],
                    io: IoFacts::default(),
                    degraded: false,
                },
            ],
        };
        let json = serde_json::to_string(&envelope).unwrap();
        let errors = validate_envelope(&json).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("duplicate path")));
    }

    #[test]
    fn rejects_body_end_less_than_body_start() {
        let mut envelope: NormalizedEnvelope =
            serde_json::from_str(&valid_envelope_json()).unwrap();
        envelope.files[0]
            .symbols
            .push(symbol("m", Some(10), Some(5)));
        let json = serde_json::to_string(&envelope).unwrap();
        let errors = validate_envelope(&json).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("body_end")));
    }

    #[test]
    fn body_end_equal_to_body_start_is_accepted() {
        let mut envelope: NormalizedEnvelope =
            serde_json::from_str(&valid_envelope_json()).unwrap();
        envelope.files[0]
            .symbols
            .push(symbol("m", Some(5), Some(5)));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(validate_envelope(&json).is_ok());
    }

    #[test]
    fn symbol_with_no_body_span_is_never_flagged() {
        let mut envelope: NormalizedEnvelope =
            serde_json::from_str(&valid_envelope_json()).unwrap();
        envelope.files[0]
            .symbols
            .push(symbol("no-span", None, None));
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(validate_envelope(&json).is_ok());
    }

    /// `docs/examples/jsp-envelope.example.json` — the JSP contract example `docs/NORMALIZED_AST.md`'s
    /// Validation section points at: a hand-written, crude-parser-shaped envelope (symbols with no body
    /// spans, one `http` provide + one `db-table` consume, no imports) that must still validate cleanly
    /// against this exact contract.
    #[test]
    fn jsp_contract_example_validates() {
        let json = include_str!("../../../docs/examples/jsp-envelope.example.json");
        let envelope = validate_envelope(json).expect("jsp-envelope.example.json should validate");
        assert_eq!(envelope.parser, "jsp-lexical/1");
        assert_eq!(envelope.files.len(), 1);
        let file = &envelope.files[0];
        assert_eq!(file.path, "webapp/legacy/user.jsp");
        assert_eq!(file.symbols.len(), 2);
        assert!(file.symbols.iter().all(|s| s.body_start.is_none()));
        assert_eq!(file.io.provides.len(), 1);
        assert_eq!(file.io.provides[0].key, "GET /legacy/user.jsp");
        assert_eq!(file.io.consumes.len(), 1);
        assert_eq!(file.io.consumes[0].key.as_deref(), Some("table:users"));
    }

    #[test]
    fn collects_multiple_errors_at_once() {
        let json = valid_envelope_json()
            .replace("zpz-normalized-ast", "bogus")
            .replace("\"version\": 1", "\"version\": 99");
        let errors = validate_envelope(&json).unwrap_err();
        assert_eq!(errors.len(), 2);
    }
}
