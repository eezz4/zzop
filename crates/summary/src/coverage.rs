//! The `zzop coverage` query core (`coverage_summary`) — the host-layer half of the aggregate
//! visibility surface (H5's landing; the three-value-cell ruling lives in
//! `zzop_facade::query_coverage_json`'s module doc).
//!
//! Exactly the shape `file_summary` established: resolve trees through the shared
//! `zzop_config::trees` front end, run the SAME `analyzeTrees` engine path, hand the output to the
//! pure facade core, then stamp the two host-layer honesty channels on top — `config` and
//! `configWarnings`. The core stays pure and never sees the config front end. Argv split is
//! `facts_json`'s: one path = single-tree mode, 2+ = paths mode (each root loads its own config).
//!
//! # What this lane reads, and why the list lives here now
//! `trees[].sourceId`; `output.ir.loc` (extension grouping + `.git`-segment walk detection);
//! `output.ir.symbols` + `output.ir.dep` (structural membership — non-empty dep source entries also
//! feed the per-extension `inDepGraph` count); `output.ir.io` (so a file whose parser projects io and
//! nothing else — `.sql`, `.prisma` — counts as structurally read); `output.degraded`;
//! `output.coverage`, `output.warnings` and `output.nativeAnalyses` forwarded verbatim; plus the two
//! CAPABILITY inputs `zzop_engine::rule_sightlines` and `zzop_engine::framework_recognizers`, which
//! are code rather than run output. That list used to sit in `docs/contracts/surface-parity.json`
//! under `_cliOnlyLanes["zzop coverage"]`, and it left with the row on 2026-09-04: this lane STOPPED
//! being CLI-only when the `check_coverage` MCP tool shipped, and a lane declared in that object is
//! SUBTRACTED from the registry's leak scan — so keeping the row would have muted the guard over
//! exactly the file that had just gained a second wire.
/// WHICH convention keys this run's config declared, and which it left silent — the other half of
/// the question this lane answers. Split into its own file because the ARGUMENT is the substance:
/// silence is not neutral, and for the auth keys it is not even quiet.
mod vocabulary_declared;

/// Answers "how much of this tree does zzop actually see?" — see
/// [`zzop_facade::query_coverage_json`] for the three-value cell rule and why there is no score.
pub fn coverage_summary(paths: &[String], config_path: Option<&str>) -> Result<String, String> {
    let (path, rest) = match paths {
        [one] => (Some(one.as_str()), &paths[..0]),
        many => (None, many),
    };
    // Surface-neutral operation name — `zzop_config::trees`' WIRE NEUTRALITY note owns that rule.
    let loaded =
        zzop_config::trees::resolve_trees_request("the coverage query", path, rest, config_path)?;
    let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
    let result = zzop_facade::query_coverage_json(&out)?;
    let mut v: serde_json::Value = serde_json::from_str(&result).map_err(|e| e.to_string())?;
    // Same two host-layer channels every sibling stamps, in the same order.
    v["config"] = loaded
        .config_path
        .as_deref()
        .map(|p| serde_json::Value::String(p.display().to_string()))
        .unwrap_or(serde_json::Value::Null);
    // Config-loader warnings first, then the engine-side config diagnostics from EVERY tree — see
    // `crate::warnings`. This lane used to read `configWarnings` off the multi-tree document's TOP
    // LEVEL, where `MultiAnalyzeOutputView` has no such field at all, so the merge contributed nothing
    // on every run and a typo'd `disabledRules` id was reported by `analyze`/`cross`/`file` and silently
    // swallowed here. The published `[]` was not "nothing to report" — it was "never asked".
    let mut warnings: Vec<serde_json::Value> = loaded
        .warnings
        .iter()
        .cloned()
        .map(serde_json::Value::String)
        .collect();
    let analysis: serde_json::Value = serde_json::from_str(&out).unwrap_or(serde_json::Value::Null);
    warnings.extend(crate::warnings::tree_config_warnings(&analysis));
    v["configWarnings"] = serde_json::json!(warnings);
    // WHICH conventions this config actually put to work — see `vocabulary_declared`'s module doc for
    // why the answer needs no guessing, and for the measured asymmetry that makes it worth printing.
    // It reads the MAPPED REQUEST rather than the analysis output, because the analysis output does not
    // carry `vocabulary` at all; the config front end above has already produced everything this needs.
    v["vocabularyDeclared"] =
        vocabulary_declared::rows(&loaded.request, &loaded.declared_vocabulary);
    v["vocabularyDeclaredMeaning"] = serde_json::json!(vocabulary_declared::MEANING);
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}
