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
//! - **Only what an envelope run actually USES is taken**: the convention vocabulary, and the PACK
//!   AXIS. Every other key configures a TREE analysis — a walker skip list, a cache directory, a git
//!   window — and an envelope run walks nothing, caches nothing and has no working tree. Forwarding
//!   those would be a knob accepted and wired nowhere, which is the defect class this file exists to
//!   close, not repeat.
//!
//! # Why the pack axis belongs here (2026-08-17, correcting this file's own premise)
//! Until this date the rule above read "only the convention vocabulary", and listed "pack directories
//! on disk" among the keys that configure a tree walk. That was wrong on the one key it mattered for:
//! `packs.extraDirs`/`disabled`/`only` configure the RULE SET, and an envelope run evaluates rules —
//! `symbol-scan` and `io-scan` ones, which is exactly what Mode A exists to make possible for a
//! language zzop has no parser for.
//!
//! What the omission cost is measurable. Every bundled rule gates on a `file_pattern` listing zzop's
//! OWN native extensions, so a complete, valid envelope for a Ruby tree — routes, symbols, io — draws
//! zero DSL findings, and the same envelope with `.rb` rewritten to `.py` and nothing else changed
//! draws three. The run says so honestly (`no_applicable_dsl_rule_warning` fires and names the scope),
//! but the ONE lever that answers it — ship a pack whose rules target your extension — was reachable
//! from an embedder's request object and from no CLI or MCP caller. The escape hatch was documented,
//! honest about being shut, and had no handle on the inside.
//!
//! The forwarded keys stay a strict subset of what the envelope wire already accepts
//! (`zzop_facade::EnvelopeAnalyzeRequest`): this file adds no capability, it connects one that existed
//! to the callers that could not reach it.
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
    /// The mapped PACK AXIS — `packsDir`, `disabledRules`, `packsOnly` — each forwarded onto the
    /// envelope request field of the same name, and each ABSENT from this list when the config did not
    /// produce it, so a config that declares no packs sends the byte-identical request it always did.
    ///
    /// Taken from the mapper's output rather than re-read off the raw config: `packs.extraDirs` is
    /// resolved to absolute paths against the config's own directory there, and `packs.disabled` is
    /// folded into `disabledRules` together with every rule set to severity `"off"`. Re-deriving either
    /// here would be a second mapper, and a relative pack path would then mean two different
    /// directories depending on which lane read the file.
    pub(super) packs: serde_json::Map<String, serde_json::Value>,
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
        packs: mapped_packs(&loaded.request),
        disclosure: applied_disclosure(&candidate),
        warnings: loaded.warnings,
    }))
}

/// The pack-axis keys the mapper emitted, read off the same two request shapes [`mapped_vocabulary`]
/// reads. Only keys the mapper actually produced are carried: an absent key means the config said
/// nothing about that axis, which on this lane must leave the facade's own defaults (the bundled-pack
/// seed, no disabled rules, no allowlist) exactly as they were.
///
/// Note what a PRESENT `packsDir` can also mean: the mapper's `zzop/rules/` discovery. An adapter author
/// who drops a pack in the conventional authored location beside the envelope gets it loaded without
/// declaring anything, the same way a tree author does — one on-disk convention, not two.
fn mapped_packs(request: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    const PACK_AXIS_KEYS: [&str; 3] = ["packsDir", "disabledRules", "packsOnly"];
    let mut out = serde_json::Map::new();
    for key in PACK_AXIS_KEYS {
        if let Some(value) = request
            .get(key)
            .or_else(|| request.pointer(&format!("/trees/0/{key}")))
        {
            out.insert(key.to_string(), value.clone());
        }
    }
    out
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
        "applied the convention vocabulary and the rule-pack selection (`packs.extraDirs`, \
         `packs.disabled`, `packs.only`) declared in {} — the zzop.config.jsonc sitting next to the \
         analyzed envelope file. Those two are what an envelope run takes from an adjacent config: \
         every other key there configures a tree analysis (walking, caching, git history), none of \
         which an envelope run does. The pack axis is here because Mode A DOES evaluate rules, and the \
         bundled ones gate on zzop's own native extensions — so a pack targeting your envelope's \
         filetypes is the way an unsupported language gets DSL findings at all. Without this file the \
         run would have used the built-in convention vocabulary and the bundled packs alone, so the \
         findings below can differ from a run made anywhere else.",
        config_path.display()
    )
}
