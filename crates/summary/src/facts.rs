//! `zzop facts` — the POST-ASSEMBLY FACT DUMP: everything the engine knows about a run, projected for
//! a program that is not zzop. This is the consumer half of the custom-rule extension point, and it is
//! deliberately only that half: zzop **emits** facts here. It never executes a user's program and never
//! ingests a user's findings — see [`facts_json`]'s own doc for the two forks that are out of scope.
//!
//! ## Why the fact dump is a separate surface instead of a flag on `cross`
//! Every shipped reply is a CAPPED summary (`crossLayer.edges` <= `DEFAULT_EDGES_LIMIT`, findings <=
//! `DEFAULT_FINDINGS_LIMIT`, ...). A rule author needs the opposite: the whole substrate, uncapped, or
//! their rule's verdict is computed over a truncated world and silently wrong. Those two contracts
//! cannot share one output shape, so they do not — exactly the split `manifest_json` already makes for
//! the drift lane.
//!
//! ## Stage: post-assembly, never per-file
//! The facts here are the tree-wide `CommonIr` AFTER assembly and the cross-layer join — router mounts,
//! controller prefixes and tRPC composition are already applied (`zzop_engine`'s `analyze::assemble`).
//! This is the only stage with an honest cache story: per-file results participate in the engine's
//! `ruleset_fingerprint`, and there is no honest fingerprint for a user's own rule program (its mtime?
//! its bytes? its transitive deps?) — every answer is a stale-result generator. Post-assembly needs no
//! fingerprint at all, which is why the io-scan phase and the adapter overlays already live there.
//!
//! ## What is NOT here, and why
//! - **`AttributeStore`** (`zzop_core::AttributeStore`) — the one post-assembly fact whose CONTAINER is
//!   not already a serialized wire shape (its element `Attribute` is). Emitting it would freeze a NEW
//!   wire shape, and this repo's projection-contract bar requires a consuming rule to exist before a new
//!   shape ships. Deliberately absent; it is not an oversight.
//! - **`findings` / `crossLayerFindings`** — zzop's own verdicts, not facts. A rule author computes
//!   their own; carrying ours would put the same data on two surfaces under two different caps, the
//!   exact drift class `docs/contracts/surface-parity.json` exists to prevent. Every input our
//!   cross-layer rules read IS here, so any of them can be re-implemented (see the `crossLayer` note).
//!
//! ## Key naming: `commonIr`, not `ir`
//! The per-tree IR rides under `commonIr` rather than the engine's own field name — a design choice, and
//! recorded so it does not look accidental. It was ORIGINALLY a workaround: the meta-test behind
//! `docs/contracts/surface-parity.json`'s `omit` status for `ir`
//! (`crates/engine/tests/rule_contracts/surface_parity.rs`) asserted that field's JSON key literal
//! appeared in no host/summary/CLI source at all, which a CLI-only lane could not satisfy. That test was
//! rescoped on 2026-07-26 to scan MCP-reachable emission sources only, so both spellings now pass and
//! the name had to be re-decided on its merits. `commonIr` won on two: it camel-cases the exact type a
//! reader must look up to interpret the block (`zzop_core::CommonIr`), and it stays greppable inside a
//! consumer's own codebase, where the two letters `ir` are a substring of `circular`, `directory` and
//! `require`. The cost, accepted: a direct `zzop-facade` embedding spells the same block `ir`, so a
//! reader moving between the two surfaces has one mapping to carry — stated in that registry's `ir` row.

#[cfg(test)]
mod tests;

/// The whole post-assembly fact substrate for a run — same two-plus-one source modes and the same
/// single `analyzeTrees` engine path as `endpoint_summary` (`zzop_config::trees::resolve_trees_request`:
/// one `path`, 2+ `paths`, or a `configPath`, a single-tree config wrapped rather than refused),
/// projected as facts instead of a verdict.
///
/// ## Scope (adjudicated, recorded so it is not re-litigated by accident)
/// This function is the EMIT half and nothing else. Ingesting a user's findings back into zzop
/// (so `rules: {off}` could silence one) is deliberately not built: freezing an ingest contract
/// before anyone has used the emit half means freezing it without knowing what it is for. zzop
/// SPAWNING the user's program is rejected outright — it would be the workspace's first non-git
/// process spawn and would move the trust boundary from our binary into the analyzed repo's own
/// config ("I scanned a repo and it executed code", worse over MCP). A WASM sandbox waits for a
/// demand of that shape, which has not arrived.
pub fn facts_json(paths: &[String], config_path: Option<&str>) -> Result<String, String> {
    // One path is the single-tree mode (`resolve_trees_request`'s `path`), 2+ is multi-root paths
    // mode — the same split `zzop endpoint`'s argv already makes, so a rule author can dump facts for
    // one repo without inventing a second tree.
    let (path, rest) = match paths {
        [one] => (Some(one.as_str()), &paths[..0]),
        many => (None, many),
    };
    let loaded = zzop_config::trees::resolve_trees_request("facts", path, rest, config_path)?;
    let config = loaded
        .config_path
        .as_deref()
        .map(|p| p.display().to_string());
    let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
    let v = serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())?;
    // Config-loader warnings first, then each tree output's facade-level `configWarnings` entries
    // (tree order) — merged into the one config-honesty channel, exactly like `cross_summary`.
    let mut config_warnings: Vec<serde_json::Value> = loaded
        .warnings
        .into_iter()
        .map(serde_json::Value::String)
        .collect();
    let empty = Vec::new();
    for t in v["trees"].as_array().unwrap_or(&empty) {
        config_warnings.extend(crate::warnings::facade_config_warnings(&t["output"]));
    }
    Ok(project(&v, config.as_deref(), config_warnings))
}

/// The pure projection `analyzeTrees` output -> facts JSON. Split out from the analysis call so the
/// whole shape is unit-testable from a literal engine output with no filesystem.
///
/// ## §0 disclosure: every key is always present
/// A capability that can silently produce nothing must positively confirm it ran, so NOTHING here is
/// skip-if-empty — the `packsLoaded` convention ("an empty array is the honest zero signal") applied to
/// a whole document, and the `capability-absent-vs-empty` disclosure class ("a present output field
/// means the capability ran"). Concretely: `tool` names the build that produced the facts; every tree
/// carries its `coverage` census (whose `joinContributionZero` is the positive "this tree extracted
/// nothing joinable" fact) and its own `warnings` (the framework-silence self-reports); `commonIr.io`
/// is materialized to `{provides:[],consumes:[]}` where the engine omitted the optional field; and all
/// seven `crossLayer` buckets are materialized to `[]`. A rule author must never have to read an absent
/// key as either "zero" or "did not run".
///
/// ## Determinism
/// Byte-stable for the same input. Everything set-shaped is ALREADY ordered upstream — `ir.dep`/`ir.loc`
/// serialize through `zzop_core::serde_util::sorted_map`, `ir.symbols` follows the file pass's
/// sorted-by-`rel` invariant, `ir.io` is `(kind, key, file, line)`-sorted by assemble, and every
/// `crossLayer` bucket is sorted by the linker. `trees` deliberately keeps REQUEST order rather than
/// being re-sorted by `sourceId`: the `crossLayer` buckets are themselves accumulated in tree order, so
/// sorting only the tree array would publish two contradictory orders inside one document.
fn project(
    v: &serde_json::Value,
    config: Option<&str>,
    config_warnings: Vec<serde_json::Value>,
) -> String {
    let empty = Vec::new();
    let trees: Vec<serde_json::Value> = v["trees"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .map(|t| {
            let output = &t["output"];
            serde_json::json!({
                "sourceId": t["sourceId"],
                // The two honesty channels a fact consumer needs BEFORE trusting a zero: the coverage
                // census (files/parserDispatched/symbols/io counts + `joinContributionZero`) and this tree's
                // own engine self-reports (framework silence, a topology host with no effect, the tRPC
                // mount-route suppression note).
                "coverage": output["coverage"],
                "warnings": array_or_empty(&output["warnings"]),
                "commonIr": common_ir(&output["ir"]),
            })
        })
        .collect();

    serde_json::to_string_pretty(&serde_json::json!({
        // Which build produced these facts — a rule keyed on a fact shape needs it for the same reason
        // `manifest` does (our own extraction improvement can move keys with no change to the code).
        "tool": zzop_facade::version_string(),
        "config": config,
        "configWarnings": config_warnings,
        "trees": trees,
        "crossLayer": cross_layer(&v["crossLayer"]),
        // Run-level self-reports that belong to the JOIN itself, not any one tree (the
        // parallel-implementation tripwire). Always an array here, unlike `cross_summary`'s
        // presence-gated forward — this surface has no token budget to protect.
        "warnings": array_or_empty(&v["warnings"]),
        // The run-global blindness-class registry, VERBATIM — deliberately un-folded, unlike the
        // analyze/cross/endpoint replies, which since 2026-07-29 carry its counts plus a pointer to
        // `zzop contract disclosure-classes` (see `crate::output::disclosure`). Carried whole even
        // though it is a build-time constant, and even though it is the largest block on this surface:
        // this is the one lane where the reader writes their own verdicts and the doc above already
        // says it has no token budget to protect, so what zzop is structurally blind to belongs next to
        // the facts, not one command away.
        "disclosure": array_or_empty(&v["disclosure"]),
    }))
    .expect("facts is plain JSON values")
}

/// One tree's `CommonIr` verbatim (`source`, `parser`, `dep`, `symbols`, `loc`, `io`), with the ONE
/// absent-vs-empty hazard closed: `MinimalIr::io` is `skip_serializing_if = "Option::is_none"`, so a
/// tree that extracted no io emits no `io` key at all and a rule author would have to guess whether the
/// io channel was empty or never ran. Materialized to the empty `IoFacts` shape instead. A non-object
/// `ir` (a malformed/older output) degrades to the empty IR skeleton for the same reason — never `null`.
fn common_ir(ir: &serde_json::Value) -> serde_json::Value {
    let mut map = ir.as_object().cloned().unwrap_or_default();
    if !map.get("io").is_some_and(serde_json::Value::is_object) {
        map.insert(
            "io".to_string(),
            serde_json::json!({ "provides": [], "consumes": [] }),
        );
    }
    serde_json::Value::Object(map)
}

/// The whole `CrossLayerResult` verbatim, with every bucket materialized (see [`project`]'s §0 note).
/// `hostRekeyCounts` and `wildcardRoutePartitions` are the genuinely skip-if-empty fields on the type;
/// the other six are materialized defensively so the guarantee is "all eight, always", not "six always
/// and two usually".
///
/// This is what makes the surface adequate: `unconsumedProvides` + `unresolvedConsumes` + `edges` are
/// exactly the inputs `cross-layer/unconsumed-endpoint` reads, so that rule (and its siblings) can be
/// re-implemented outside zzop rather than only inspected. `wildcardRoutePartitions` is part of that
/// adequacy and not an extra: a re-implementer who cannot see which routes left the join would rebuild
/// the exact three false findings the partition removed — an empty array says "no pattern route here",
/// where an absent key would have said nothing at all.
fn cross_layer(cl: &serde_json::Value) -> serde_json::Value {
    let mut map = cl.as_object().cloned().unwrap_or_default();
    for bucket in std::iter::once("edges")
        .chain(crate::output::KEY_BUCKETS)
        .chain(["hostRekeyCounts", "wildcardRoutePartitions"])
    {
        if !map.get(bucket).is_some_and(serde_json::Value::is_array) {
            map.insert(bucket.to_string(), serde_json::json!([]));
        }
    }
    serde_json::Value::Object(map)
}

/// A JSON array field forwarded verbatim, degrading an absent/non-array value to `[]` — never `null`,
/// never a missing key (see [`project`]'s §0 note).
fn array_or_empty(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Array(_) => v.clone(),
        _ => serde_json::json!([]),
    }
}
