//! `analyze_repo` (and CLI `zzop analyze <path>`). Also hosts
//! `analyze_envelope_summary` (`analyze_envelope` tool / CLI `zzop analyze-envelope <file>`, Mode
//! A) — the two share one post-facade shaper (`shape_analyze_output`) so the same cap/disclosure/
//! config-warning contract holds for both entry points.

use crate::output::{FindingFilters, RunKnobs};

mod shape;
#[cfg(test)]
mod tests;

use shape::shape_analyze_output;

/// Turns rule-timing instrumentation on in an already-mapped request `Value`, for either request shape
/// the config loader produces: a single `AnalyzeRequest` object (`Method::Analyze`) or the
/// `{trees: [...]}` envelope (`Method::AnalyzeTrees`), where EVERY tree must be instrumented or the
/// per-tree reports would silently disagree about what they measured.
///
/// Written onto the mapped request rather than carried as a config key on purpose — see
/// `zzop_facade::AnalyzeRequest::profile_rules` for why this axis is a per-run switch and has no
/// `zzop.config.jsonc` spelling. A no-op when `profile_rules` is false, so an unprofiled run's request
/// is byte-identical to what the mapper produced.
pub(crate) fn apply_run_knobs(request: &mut serde_json::Value, knobs: RunKnobs) {
    if !knobs.profile_rules {
        return;
    }
    match request
        .get_mut("trees")
        .and_then(serde_json::Value::as_array_mut)
    {
        Some(trees) => {
            for tree in trees {
                tree["profileRules"] = serde_json::Value::Bool(true);
            }
        }
        None => request["profileRules"] = serde_json::Value::Bool(true),
    }
}

/// Analyze ONE tree, from EITHER source mode — exactly one of:
/// - `path` — a tree root, with `<path>/zzop.config.jsonc` auto-discovered when present
///   (`zzop_config::load_for_root`), the empty-config defaults otherwise.
/// - `config_path` — an explicit config file (or a directory holding one) at ANY location
///   (`zzop_config::load_config_file`), so a config that does not sit at the tree root is reachable.
///   Resolved exactly like the multi-tree siblings' own config mode, absolutization included.
///
/// Source-mode exclusivity is enforced HERE rather than in each host, for the same reason
/// `zzop_config::trees` enforces the multi-tree siblings' — without it a host passing both would get a
/// silently-narrowed answer. Execution goes through `zzop-facade` (one engine code path for every host)
/// and then this module's summary-first shaping. A config declaring multiple trees is a guided error:
/// that analysis is the cross-layer join's job, not this single-tree entry point's.
pub fn analyze_summary(
    path: Option<&str>,
    config_path: Option<&str>,
    filters: &FindingFilters,
) -> Result<String, String> {
    analyze_summary_with(path, config_path, filters, RunKnobs::default())
}

/// [`analyze_summary`] plus the per-invocation [`RunKnobs`] — today, rule timing.
///
/// A separate entry point rather than a fourth argument on the original because the knob is CLI-only:
/// `--profile-rules` has no MCP tool argument (a timing report is a question about a local run, not an
/// answer an agent asked for), so every other host keeps calling the 3-argument form and cannot
/// accidentally be handed a profiler it never asked to enable. If an MCP argument is ever added, the
/// two collapse into one — the wrapper exists to avoid churning hosts for a knob they do not offer,
/// not to fork the lane.
pub fn analyze_summary_with(
    path: Option<&str>,
    config_path: Option<&str>,
    filters: &FindingFilters,
    knobs: RunKnobs,
) -> Result<String, String> {
    // Absolutized at the host boundary (see `paths`): `zzop-config` requires an absolute root.
    let (root, mut loaded) = match (path, config_path) {
        (Some(_), Some(_)) => {
            return Err(
                "pass either a tree root or a config file, not both — pass exactly one source"
                    .to_string(),
            )
        }
        (None, None) => {
            return Err("pass a tree root, or a config file (a zzop.config.jsonc)".to_string())
        }
        (Some(p), None) => {
            if p.trim().is_empty() {
                return Err("path is empty — pass the tree's root directory".to_string());
            }
            let root = zzop_config::paths::absolutize(p);
            if !root.exists() {
                return Err(format!("path does not exist: {p}"));
            }
            let loaded = zzop_config::load_for_root(&root).map_err(|e| e.to_string())?;
            (root, loaded)
        }
        (None, Some(cp)) => {
            let resolved = zzop_config::paths::absolutize(cp);
            let loaded = zzop_config::load_config_file(&resolved).map_err(|e| e.to_string())?;
            (resolved, loaded)
        }
    };
    // Instrumentation is set on the MAPPED request, after the config loader has produced it and before
    // either method sends it — the one place both `Analyze` and `AnalyzeTrees` pass through, so neither
    // branch can be wired and the other forgotten.
    apply_run_knobs(&mut loaded.request, knobs);
    // `disclosure` is the facade's run-global blindness-class registry (which failure classes zzop
    // does/does NOT detect) — the meta-honesty channel an AI consumer needs alongside the active
    // `warnings`; it rides at the top level of every facade output. Carried here, then FOLDED to its
    // counts plus a pointer by the shaper (`crate::output::disclosure`) — never dropped, and never
    // shipped as the ~10.6KB of run-invariant prose it is at the facade boundary.
    let (output_view, disclosure) = match loaded.method {
        zzop_config::Method::Analyze => {
            let out = zzop_facade::analyze_json(&loaded.request.to_string())?;
            let v = serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())?;
            let disclosure = v["disclosure"].clone();
            (v, disclosure)
        }
        zzop_config::Method::AnalyzeTrees => {
            let tree_count = loaded.request["trees"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);
            if tree_count > 1 {
                // SPELLING-FREE by contract: this crate is shared by every host, so naming one host's
                // word for the cross-layer join here ("the cross_repo tool", which this message used
                // to say) is advice a `zzop` CLI user cannot take. Each product's own usage text owns
                // its own words — the same remedy `zzop_config::load_config_file`'s missing-config
                // error already carries, pinned in `crates/config/src/lib_tests.rs`.
                return Err(format!(
                    "the config at {} defines {tree_count} trees — run the CROSS-LAYER JOIN over this config instead, or point this single-tree analysis at one tree root directly",
                    loaded
                        .config_path
                        .as_deref()
                        .unwrap_or(&root)
                        .display()
                ));
            }
            let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
            let v = serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())?;
            (v["trees"][0]["output"].clone(), v["disclosure"].clone())
        }
    };
    // Config-loader warnings, collected first; the facade-level `configWarnings` entries riding the
    // tree output (engine-side config diagnostics, e.g. unknown-rule-id overrides) are merged onto
    // these later, in `shape_analyze_output` (see `crate::warnings::facade_config_warnings`).
    let config_warnings: Vec<serde_json::Value> = loaded
        .warnings
        .into_iter()
        .map(serde_json::Value::String)
        .collect();
    // The RESOLVED absolute TREE ROOT, never the raw argument — a live-fire gap: `path: "."` echoed back
    // the literal `.` with the actual analyzed directory never disclosed anywhere in the reply. Read out
    // of the request the mapper built (`root`, or the single tree's own `root`) rather than off the
    // argument, because in config mode the argument names the CONFIG FILE and echoing it would report a
    // path that was never analyzed; the resolved argument stays the fallback for a request shape that
    // somehow carries no root at all.
    let mut leading = serde_json::Map::new();
    let analyzed_root = loaded.request["root"]
        .as_str()
        .or_else(|| loaded.request["trees"][0]["root"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| root.display().to_string());
    leading.insert("path".to_string(), serde_json::json!(analyzed_root));
    leading.insert(
        "config".to_string(),
        serde_json::json!(loaded
            .config_path
            .as_deref()
            .map(|p| p.display().to_string())),
    );
    let summary = shape_analyze_output(leading, &output_view, disclosure, config_warnings, filters);
    serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
}

/// `analyze_envelope` (and CLI `zzop analyze-envelope <envelope.json>`) — Mode A: a full
/// Normalized-AST envelope REPLACES native parsing entirely (contrast `validate_envelope`, which only
/// checks the envelope's shape and never runs analysis; and Mode B mount/overlay requests, which merge
/// external symbols ON TOP of a natively-parsed tree). Runs via `zzop_facade::analyze_envelope_json` —
/// the SAME `AnalyzeOutputView`-shaped output `analyze_json`/`analyze_trees_json` produce, so it goes
/// through the identical [`shape_analyze_output`] this module's tree-mode path uses: a shaping fix
/// (a cap, a warning merge) lands for both entry points at once instead of drifting per host.
pub fn analyze_envelope_summary(
    envelope_json: &str,
    filters: &FindingFilters,
) -> Result<String, String> {
    analyze_envelope_summary_with(envelope_json, filters, RunKnobs::default())
}

/// [`analyze_envelope_summary`] plus the per-invocation [`RunKnobs`] — the envelope-lane twin of
/// [`analyze_summary_with`], and CLI-only for the same reason (no MCP tool argument turns profiling
/// on). This twin did NOT exist until 2026-08-03, deliberately: `zzop_engine::analyze_envelope` used
/// to construct `rule_timings: None` unconditionally (its pack evaluation ran outside the timing
/// accumulator), so the CLI refused `--profile-rules` on this lane by name rather than accept a knob
/// nothing reads. Mode A's pack evaluation now feeds the same accumulator the native path does
/// (`crates/engine/src/envelope/file_pass.rs`/`ingest.rs`), so the knob is real here too.
pub fn analyze_envelope_summary_with(
    envelope_json: &str,
    filters: &FindingFilters,
    knobs: RunKnobs,
) -> Result<String, String> {
    if envelope_json.trim().is_empty() {
        // Spelling-free: `envelopeJson` is the MCP argument name, but the CLI twin passes a FILE whose
        // text landed here — naming the wire argument told half the callers about a knob they do not have.
        return Err(
            "the envelope document is empty — pass a Normalized AST envelope JSON document"
                .to_string(),
        );
    }
    // Envelope mode has no filesystem root/config file to auto-discover (unlike `analyze_summary`'s
    // `zzop_config::load_for_root`) — an envelope carries no location the engine can re-read
    // (`docs/NORMALIZED_AST.md`). `"{}"` is the SAME "an empty config still means full analysis" default
    // `analyze_envelope_json` itself documents at the facade layer (bundled packs injected as inline
    // seeds, no disabledRules/severityOverrides/suppressions/mounts) — the MCP surface takes
    // `envelopeJson` only, so this is the minimal valid `EnvelopeAnalyzeRequest` construction, not a
    // shortcut around it. `profileRules` is the one per-invocation addition, written only when asked
    // for — an unprofiled request stays byte-identical to the pre-knob `"{}"`. Declaring no
    // `vocabulary` here is likewise deliberate: the facade assigns the product default
    // (`VocabularyConfig::built_in()`) to an undeclared envelope request, so both hosts run the same
    // built-in convention vocabulary — declaring one is a facade-request capability neither host
    // exposes as a knob today (`EnvelopeAnalyzeRequest::vocabulary`).
    let config = if knobs.profile_rules {
        r#"{"profileRules":true}"#
    } else {
        "{}"
    };
    let out = zzop_facade::analyze_envelope_json(envelope_json, config)?;
    let output_view: serde_json::Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
    let disclosure = output_view["disclosure"].clone();
    let summary = shape_analyze_output(
        serde_json::Map::new(),
        &output_view,
        disclosure,
        Vec::new(),
        filters,
    );
    serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
}
