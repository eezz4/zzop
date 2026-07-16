//! S3: committed-spec io-silence tripwire (consume side).

use std::fs;
use std::path::Path;

use super::controller_silence::MIN_PROVIDES_FLOOR;

/// Both-direction io-near-zero floor for the committed-spec tripwire. Its own constant (rather than
/// reusing `MIN_PROVIDES_FLOOR` by name) since S2 and S3 gate on different substrates — `http`-only
/// extracted provides vs. total io provides + keyed consumes — and may need to diverge independently
/// later, even though both currently carry the same round-9-derived near-zero rationale and value.
/// `pub(crate)` so `analyze::assemble` can precheck it before building the sorted walked-rel list this
/// function's candidate scan needs — the same "cheap on the success path" convention `controller_silence_warning`'s
/// own doc describes, extended past disk IO to the (much cheaper, but non-zero on a huge tree) rel-list
/// sort itself.
pub(crate) const IO_NEAR_ZERO_FLOOR: usize = MIN_PROVIDES_FLOOR;

/// Cap on how many spec-shaped candidate files get a real disk read (the content probe) — bounds
/// worst-case IO even on a tree with several oddly-named `openapi`/`swagger` json/yaml files, without
/// requiring the caller to pre-filter beyond the walked-file list it already has.
const MAX_SPEC_PROBES: usize = 5;

/// Whether `rel`'s basename looks like a committed OpenAPI/Swagger spec: extension json/yaml/yml AND the
/// basename contains "openapi" or "swagger" (case-insensitive). Cheap, no disk IO — the caller filters the
/// full walked-rel list with this before any probe read happens.
fn is_spec_candidate_rel(rel: &str) -> bool {
    let path = Path::new(rel);
    let ext_ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        e.eq_ignore_ascii_case("json")
            || e.eq_ignore_ascii_case("yaml")
            || e.eq_ignore_ascii_case("yml")
    });
    if !ext_ok {
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower.contains("openapi") || lower.contains("swagger")
}

/// Returns a ready-to-push `warnings` entry when a committed OpenAPI/Swagger spec file exists in the tree
/// while `io_provides_count`/`io_consumes_keyed_count` are BOTH below `IO_NEAR_ZERO_FLOOR` — the signature
/// of a tree that talks through a GENERATED client (SDK class/methods) built from that spec, which the
/// literal-call-site consume extractor cannot see into.
///
/// Gated before any disk IO: returns `None` immediately when either io count already clears the floor (a
/// server tree with real provides, or an FE with healthy keyed consumes, pays zero probe cost). Only then
/// does it filter `all_walked_rels` for spec-shaped candidates and read up to `MAX_SPEC_PROBES` of them,
/// requiring a `"paths"` (json) or `paths:` (yaml) marker before firing — belt-and-braces against a
/// coincidentally named file (e.g. `swagger-ui.css`, already excluded by extension, or a `swagger-theme.json`
/// asset that never mentions `paths`).
///
/// Determinism: `all_walked_rels` must already be sorted by the caller (`analyze::assemble`, the same
/// convention `controller_silence_warning`'s `candidate_rels` relies on) — the first matching candidate
/// probed/reported is therefore deterministic without any extra sort here.
pub fn committed_spec_io_silence_warning(
    root: &Path,
    all_walked_rels: &[String],
    io_provides_count: usize,
    io_consumes_keyed_count: usize,
) -> Option<String> {
    if io_provides_count >= IO_NEAR_ZERO_FLOOR || io_consumes_keyed_count >= IO_NEAR_ZERO_FLOOR {
        return None;
    }
    for rel in all_walked_rels
        .iter()
        .filter(|rel| is_spec_candidate_rel(rel))
        .take(MAX_SPEC_PROBES)
    {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        if !(text.contains("\"paths\"") || text.contains("paths:")) {
            continue;
        }
        return Some(format!(
            "a committed OpenAPI/Swagger spec exists at {rel} but this tree contributed almost no \
joinable io ({io_provides_count} provide(s) / {io_consumes_keyed_count} keyed consume(s)) — if the app \
talks through a GENERATED client (SDK class/methods) rather than direct calls, native extraction cannot \
see those calls; project the generated client with the Mode B openapi-sdk-adapter (see the adapter \
examples for its generated class-method client support) to restore cross-layer visibility: a partial \
envelope covering just the missing io channel is enough; contract: `zzop-mcp contract envelope-guide` \
on MCP hosts, docs/NORMALIZED_AST.md in the repo."
        ));
    }
    None
}
