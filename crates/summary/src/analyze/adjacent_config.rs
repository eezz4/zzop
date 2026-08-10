//! ADJACENT-CONFIG DISCOVERY for the envelope lane — the answer to "an envelope run cannot declare a
//! convention vocabulary, yet zzop's own finding messages tell its user to declare one".
//!
//! The tree lane already auto-discovers a `zzop.config.jsonc` at the analyzed root
//! (`zzop_config::load_for_root`). A caller that names an envelope FILE has the symmetric location: the
//! directory that file sits in. So this reads the config sitting NEXT TO the envelope, through the same
//! loader the tree lane uses — JSONC stripping, unknown-key warnings, retired-key notices and
//! vocabulary normalization therefore behave identically on both lanes, because they are the same code.
//!
//! Two boundaries, both load-bearing:
//! - **Only a caller that HAS a location gets this.** An envelope document is JSON text; a caller that
//!   passes text alone has nothing adjacent to discover, and no location is invented for it (see
//!   [`super::analyze_envelope_summary`]). That asymmetry is real, not an oversight, so it is disclosed
//!   where each audience reads its own surface rather than papered over here.
//! - **Only the convention vocabulary is taken.** Every other key in a config configures a TREE
//!   analysis — a walker skip list, a cache directory, a git window, pack directories on disk — and an
//!   envelope run walks nothing, caches nothing and has no working tree. Forwarding those would be a
//!   knob accepted and wired nowhere, which is the defect class this file exists to close, not repeat.
//!
//! Never-guess: a config that is present but unreadable/invalid is an ERROR (the loader's own), never a
//! silent fall back to the built-in vocabulary; and a config that is absent leaves the request
//! byte-identical to the one this lane sent before discovery existed.

use std::path::Path;

/// What a discovered config contributes to an envelope run: the declaration itself, the sentence that
/// says it was applied, and the loader's own notes about that file.
pub(super) struct AdjacentConfig {
    /// The mapped `vocabulary` object, exactly as the tree lane's request carries it — passed WHOLE
    /// onto the facade's `EnvelopeAnalyzeRequest::vocabulary`.
    pub(super) vocabulary: serde_json::Value,
    /// The one disclosure that fires when — and only when — a config was found and applied.
    pub(super) disclosure: String,
    /// The config loader's own warnings (unknown keys, retired keys, overlay notes), forwarded so a
    /// typo'd `vocabulary` key is reported here exactly as it would be on the tree lane.
    pub(super) warnings: Vec<String>,
}

/// Reads the `zzop.config.jsonc` sitting next to `envelope_path`, or reports that there is none.
///
/// `Ok(None)` means "no config file there", and the caller must then behave exactly as it did before
/// this module existed. `Err` means a config IS there and could not be honoured — never downgraded to
/// `Ok(None)`, because silently analysing with the built-in vocabulary after the author declared their
/// own is the failure this whole lane is being repaired for.
pub(super) fn discover(envelope_path: &str) -> Result<Option<AdjacentConfig>, String> {
    if envelope_path.trim().is_empty() {
        return Ok(None);
    }
    // Absolutized at the host boundary, like every other path argument (`zzop_config::paths`): the
    // loader requires an absolute root, and a bare `envelope.json` has no parent until it has one.
    let envelope = zzop_config::paths::absolutize(envelope_path);
    let Some(dir) = envelope.parent() else {
        return Ok(None);
    };
    let candidate = dir.join(zzop_config::DEFAULT_CONFIG_FILENAME);
    // The same existence test `load_for_root` makes, made here first so its absence is an ANSWER
    // ("nothing to apply") rather than that function's refusal error — an envelope run without a
    // config is a supported run, unlike a tree analysis, which has no vocabulary at all without one.
    if !candidate.is_file() {
        return Ok(None);
    }
    let loaded = zzop_config::load_for_root_vocabulary_only(dir).map_err(|e| e.to_string())?;
    Ok(Some(AdjacentConfig {
        vocabulary: mapped_vocabulary(&loaded.request),
        disclosure: applied_disclosure(&candidate),
        warnings: loaded.warnings,
    }))
}

/// The `vocabulary` object out of a mapped request, whichever shape the config produced: a single
/// `AnalyzeRequest` object, or the `{trees: [...]}` envelope where every tree carries the same
/// config-global vocabulary (`zzop_config::mapper`'s shared options are merged into each tree).
///
/// An absent key yields an EMPTY object rather than "leave the built-in default in place", and that is
/// the point rather than an edge case: a config that declares no vocabulary declares no vocabulary, on
/// this lane exactly as on the tree lane, where the built-in fallback was removed on 2026-07-27
/// precisely so a run can never judge by values its author never saw. Falling back here would give one
/// config file two meanings depending on which lane read it.
fn mapped_vocabulary(request: &serde_json::Value) -> serde_json::Value {
    request
        .get("vocabulary")
        .or_else(|| request.pointer("/trees/0/vocabulary"))
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()))
}

/// The applied-disclosure sentence. Names the FILE (the thing a reader can open), states the one thing
/// taken from it, and states what the run would otherwise have used — so a reader who did not expect a
/// config to be in play can tell exactly what changed and why.
///
/// Spelling-free, like every other message in this shared crate: both a terminal caller and a tool
/// caller can reach this lane, so a sentence naming either one's dialect would be advice half its
/// audience cannot take (pinned by `crates/engine/tests/rule_contracts/host_vocabulary.rs`).
fn applied_disclosure(config_path: &Path) -> String {
    format!(
        "applied the convention vocabulary declared in {} — the zzop.config.jsonc sitting next to the \
         analyzed envelope file. That declaration is the ONLY thing an envelope run takes from an \
         adjacent config: every other key there configures a tree analysis (walking, caching, git \
         history, pack directories), none of which an envelope run does. Without that file this run \
         would have used the built-in convention vocabulary instead, so the findings below can differ \
         from a run made anywhere else.",
        config_path.display()
    )
}
