//! External-parser protocol receiver — the engine-side deserialization + validation of the
//! "Normalized AST" envelope an external/custom parser (Java, Python, JSP, anything the engine does
//! not parse natively) emits per source tree, frozen v1 by `docs/NORMALIZED_AST.md`. These types
//! mirror that document's Envelope/FileProjection JSON shapes field-for-field and reuse the SAME
//! `zzop_core` serde types (`SourceSymbol`/`ImportBinding`/`ReExport`/`IoFacts`) native parsers already
//! project into — an external parser is first-class regardless of how crude it is, as long as its
//! projection round-trips through these exact structs (see the doc's Validation section).

use serde::{Deserialize, Serialize};

use crate::io::IoFacts;
use crate::ir::{ImportMap, ReExport, SourceSymbol};

/// The exact `format` string every conforming envelope must carry (`docs/NORMALIZED_AST.md`'s Envelope
/// section).
pub const NORMALIZED_AST_FORMAT: &str = "zzop-normalized-ast";

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
/// reused `zzop_core` serde type; see that document for the authoritative semantics of each
/// (loc/symbols/imports/re_exports/used_names/io/degraded). Every optional-in-practice field defaults
/// to its empty value when a producer omits it (a minimal/degraded parser may legitimately have nothing
/// to say about, say, `re_exports`), matching the doc's "graceful degrade, never an error" convention.
///
/// IMPORTANT — cross-file specifier resolution for the fragment-channel fields below
/// (`const_map_fragment`/`procedure_router_fragments`/`router_mount_fragments`): a `ProcedureRouterEntry::Ref`'s
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
    /// This file's dynamic-`import()` specifiers (external-parser contract; optional). Mirrors the native
    /// `FileArtifact::dynamic_imports` — folded into the envelope dep graph as real (circular-excluded)
    /// edges so a code-split-only module keeps its fan-in on the envelope path too.
    #[serde(default)]
    pub dynamic_imports: Vec<String>,
    #[serde(default)]
    pub used_names: Vec<String>,
    /// Producer FRAGMENT CHANNELS for cross-file composition — the envelope equivalent of what
    /// `zzop_engine::analyze`'s `compose_trpc_provides` / `compose_router_mount_provides` already
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
    pub procedure_router_fragments: Vec<crate::ProcedureRouterFragment>,
    #[serde(default)]
    pub router_mount_fragments: Vec<crate::RouterMountFragment>,
    /// Class field-shape fragments (`body-shape-v1`) — the DTO-resolution substrate for
    /// `IoProvide::body.dto_ref` (see `zzop_core::ClassShapeFragment`). Same optional/graceful
    /// posture as the other fragment channels: absent = this producer doesn't extract class shapes.
    #[serde(default)]
    pub class_shape_fragments: Vec<crate::ClassShapeFragment>,
    #[serde(default)]
    pub io: IoFacts,
    /// Generic entity-attribute annotations (`attributes-v1`) — the open-vocab injection channel for
    /// cross-cutting facts a producer attaches to entities (routes/symbols/files/path-scopes) that
    /// per-file extraction can't see, e.g. `{ "target": { "pathScope": { "prefix": "/admin" } },
    /// "key": "auth-guarded", "value": true }` to mark a middleware-guarded router. Consumed BY KEY by
    /// rules (see `zzop_core::AttributeStore`); the contract/kernel is agnostic to `key`. OPTIONAL
    /// (`#[serde(default)]`; absent = no annotations = every attribute-aware rule keeps its native
    /// behavior). Collected tree-wide at assemble regardless of which file emits them.
    #[serde(default)]
    pub attributes: Vec<crate::Attribute>,
    /// Per-file loop-body line spans (1-based, inclusive) — external-parser counterpart of
    /// `zzop_core::dsl::SourceFile::loop_spans` (see that field's doc for the exact span contract: each
    /// loop statement's full span, plus array-iteration callback ARGUMENT spans only, never the whole
    /// call). OPTIONAL (`#[serde(default)]`; absent = empty = `MethodScan::trigger_in_loop` silently
    /// skips this file, same graceful-degrade policy as `symbols`/`io`). Serialized snake_case
    /// (`loop_spans`), consistent with `FileProjection`'s other fields — this struct has no
    /// `rename_all`, so `SourceSymbol`'s dual-casing convention (a `rename_all = "camelCase"` type)
    /// does not apply here; `loopSpans` is still tolerated on INPUT for camelCase emitters.
    #[serde(default, alias = "loopSpans")]
    pub loop_spans: Vec<(u32, u32)>,
    /// The parser could not fully process this file (size cap, syntax failure) — `loc` must still be
    /// present regardless.
    #[serde(default)]
    pub degraded: bool,
    /// Adapter-declared framework ENTRY / reachable-root: a file loaded by the framework/runtime by
    /// convention (e.g. SvelteKit `hooks.*`/`+page`, a `.vue` route) rather than imported, so its
    /// `fan_in == 0` is expected — exempts it from `dead-candidates`/`unreachable`, the overlay
    /// counterpart of package.json manifest entries (`pipeline::package_json_entries`'s
    /// `extra_entries`). Meaningful in Mode B (`apply_adapter_overlays` unions every `is_entry` path
    /// across `EngineConfig::adapter_overlays` into `dead_candidate_findings`'s `extra_entries`); Mode A
    /// (`analyze_envelope`) does not read this field at all today (no filesystem-manifest concept to
    /// union it against). Default `false`.
    #[serde(default)]
    pub is_entry: bool,
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
// Note: `const_map_fragment`/`procedure_router_fragments`/`router_mount_fragments` presence is never
// validated here — any fragment content a producer emits is accepted as-is (empty is always valid,
// per their `#[serde(default)]`). An unresolvable `Ref`/`Mount` specifier is a composition-time
// concern, silently skipped by the engine's assembly pass, not a validation-time rejection — "never
// guessed" per this crate's convention, but also never a hard error for a shape this validator cannot
// know is wrong.
pub fn validate_envelope(json: &str) -> Result<NormalizedEnvelope, Vec<String>> {
    // A JSON ARRAY root is a special case: serde's derived `Deserialize` for a struct accepts a
    // sequence as well as a map (the positional-fields fallback other serde formats rely on), so a
    // top-level array is NOT rejected as "wrong shape" the way a string/number/bool/null root already
    // is (those hit the ordinary "invalid type: X, expected struct NormalizedEnvelope" branch, which is
    // clear on its own). Instead each array element gets deserialized against the next declared field
    // in turn, so `["a"]` against a `format: String` first field fails with a field-level type mismatch
    // ("invalid type: integer `1`, expected a string") that reads like ONE field is wrong rather than
    // "this isn't an envelope at all" — a blind field test hit exactly this passing a JSON array as
    // `envelopeJson`. Caught here, before the struct deserialize, with the honest diagnosis.
    if matches!(
        serde_json::from_str::<serde_json::Value>(json),
        Ok(serde_json::Value::Array(_))
    ) {
        return Err(vec![
            "expected a JSON object envelope, got an array".to_string()
        ]);
    }
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
mod tests;
