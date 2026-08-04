//! REQUEST ASSEMBLY — "which trees is this call actually about?". Lives in this crate for the same
//! reason the mapper does: "what is a valid config" and "what request does that config become" are one
//! owner's question, so no host and no entry point can drift on source-mode exclusivity or on whether a
//! single-tree config is a join.
//!
//! Multi-path tree building for "paths mode" ([`zero_config_trees`], reached from an MCP tool's `paths`
//! argument and the equivalent trailing CLI paths — each path is loaded from its OWN config since
//! 2026-07-27; the function keeps its old name and says why in its own doc). Because this helper is
//! shared, any error text it produces must take the calling OPERATION's own name as a parameter rather
//! than hardcoding one sibling's (a live-fire misfire: the endpoint query with a single `paths` entry
//! reported "cross_repo needs at least 2 paths").
//!
//! WIRE NEUTRALITY — what that `operation` parameter may be spelled. The parameter used to be called
//! `tool_name` and the two twin-surface callers passed their MCP TOOL name (`cross_repo`,
//! `check_endpoint`), which put one host's vocabulary in front of the other host's user: the same defect
//! the parameter exists to prevent, one level up. The callers now pass a surface-neutral OPERATION name
//! ("the cross-layer join", "the endpoint query"); the CLI-only lanes (`manifest`/`facts`/`graph`) pass
//! their lane name, which is neutral because those lanes have exactly one host. Every sentence built here
//! is likewise spelling-free — no `configPath`, no tool name — because each product's own usage text owns
//! its own words. Machine-pinned by `crates/engine/tests/rule_contracts/host_vocabulary.rs`.
//!
//! Two loaders sit above it, one per source-mode contract, both ending at an `analyzeTrees` request
//! (`zzop-summary`'s shaping entry points are the callers named in each):
//! - [`load_trees_request`] — the JOIN-shaped entry points (`cross_summary`, `manifest_json`): exactly
//!   one source (paths XOR config), 2+ paths, and a single-tree config REFUSED.
//! - [`resolve_trees_request`] — the entry points that are meaningful over ONE tree too
//!   (`endpoint_summary`, `facts_json`): `path` XOR `paths` XOR `configPath`, with a single-tree
//!   config WRAPPED into a one-entry `analyzeTrees` request rather than refused.
//!
//! Both are shared rather than copied so no two entry points can drift on source-mode exclusivity or
//! config-method gating.

use crate::paths;

/// The multi-tree request loader: config-first mode (`config_path` — the config's `trees` define the
/// join) or multi-path paths mode ([`zero_config_trees`]). Source-mode exclusivity is enforced HERE,
/// not (only) in the hosts — without it a future host passing both would get a silently-narrowed join
/// (config wins, paths ignored), exactly the per-host-drift class this crate exists to close.
///
/// Three refusals, one per way a call can name no usable source: BOTH sources, NEITHER source, and a
/// config that resolves to one tree. `operation` names the CALLER in every one of them, for the same
/// reason `zero_config_trees` takes it (see the module doc's misattribution incident).
pub fn load_trees_request(
    operation: &str,
    paths: &[String],
    config_path: Option<&str>,
) -> Result<crate::LoadedRequest, String> {
    if config_path.is_some() && !paths.is_empty() {
        return Err(format!(
            "{operation} takes either tree paths or a config file, not both — pass exactly one source"
        ));
    }
    match config_path {
        Some(cp) => {
            // Absolutized like every path argument (see `crate::paths`), so a relative `--config` works
            // from any cwd and the config's own directory resolves absolute for the mapper.
            let loaded =
                crate::load_config_file(&paths::absolutize(cp)).map_err(|e| e.to_string())?;
            if loaded.method != crate::Method::AnalyzeTrees {
                // Spelling-free (see the module doc's WIRE NEUTRALITY note): "use analyze_repo for it"
                // named one host's tool and left the other host's user with advice they cannot take.
                return Err(format!(
                    "the config at {} defines a single tree — analyze it as ONE tree instead, or declare `trees` (2+, or \"auto\") for a cross-layer join",
                    loaded
                        .config_path
                        .as_deref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| cp.to_string())
                ));
            }
            Ok(loaded)
        }
        // NEITHER source given — the sibling's explicit neither-branch, which this lane lacked until
        // 2026-08-02. Without it the call fell through to `zero_config_trees`, whose sentence is about
        // paths mode alone ("needs at least 2 paths") and therefore never mentioned that a config file
        // is an equally valid source here: a caller who had one was told to go find a second directory.
        // The host-local copy this shared layer replaced (`packages/mcp/src/tools.rs`'s `cross_repo`
        // arm, deleted 2026-08-01) was the more complete of the two, and that is what this closes.
        //
        // It names TWO sources where the sibling names three, and the difference is the contract, not a
        // wording slip: `resolve_trees_request` accepts ONE tree root because its callers' questions are
        // meaningful over one tree, and this lane's are not — a join needs two sides. Copying the
        // sibling's sentence verbatim would offer a source this function refuses one branch later.
        None if paths.is_empty() => Err(format!(
            "{operation} needs a source: pass 2+ tree roots, or a config file (a zzop.config.jsonc) \
             whose `trees` define the join — one tree root is not a join"
        )),
        None => zero_config_trees(operation, paths),
    }
}

/// The single-tree-tolerant loader, shared by `endpoint_summary` and `facts_json` — same vocabulary
/// as the sibling summary functions:
/// - `path` — one tree, resolved exactly like `analyze_summary` (`crate::load_for_root`:
///   `<path>/zzop.config.jsonc` required, as for every analysis lane). A single-tree request is
///   wrapped into `{trees: [request]}` by [`wrap_single_tree`].
/// - `paths` — 2+ tree roots, each carrying its own config, via [`zero_config_trees`] (identical to
///   `cross_summary`'s paths mode, disclosure warnings included).
/// - `configPath` — an explicit config file/directory (`crate::load_config_file`); unlike
///   [`load_trees_request`], a single-tree config is NOT an error here — it wraps like `path` does,
///   since both callers' questions are meaningful over one tree (an endpoint query and a facts dump
///   both read cross-layer JOIN facts, and the join runs fine over one tree, intra-tree edges
///   included).
///
/// `operation` names the CALLER in the paths-mode error, for the same reason [`zero_config_trees`]
/// takes it (see the module doc's misattribution incident).
pub fn resolve_trees_request(
    operation: &str,
    path: Option<&str>,
    paths: &[String],
    config_path: Option<&str>,
) -> Result<crate::LoadedRequest, String> {
    match (path, paths.is_empty(), config_path) {
        (Some(p), true, None) => {
            // Absolutized at the host boundary (see the sibling `crate::paths`) — required by
            // `zzop-config`'s absolute-root contract, and it makes the facade's dir-name sourceId
            // default real for a relative argument (`.` has no `file_name` until absolutized): an
            // unnamed single tree is named after its root's basename at the shared facade
            // chokepoint (`zzop_facade`'s `apply_source_id_default` — formerly a local default
            // here, hoisted so every host and entry point shares one naming rule).
            let root = paths::absolutize(p);
            if !root.exists() {
                return Err(format!("path does not exist: {p}"));
            }
            let loaded = crate::load_for_root(&root).map_err(|e| e.to_string())?;
            Ok(wrap_single_tree(loaded))
        }
        (None, false, None) => zero_config_trees(operation, paths),
        (None, true, Some(cp)) => {
            // Absolutized like `cross --config` (see there) — a relative configPath works from
            // any cwd. A single-root config with no explicit sourceId is named after the TREE
            // root's basename (not the config file's own directory) by the same facade default —
            // the mapper resolves `root` to absolute against the config's directory before the
            // request reaches the facade.
            let loaded =
                crate::load_config_file(&paths::absolutize(cp)).map_err(|e| e.to_string())?;
            Ok(wrap_single_tree(loaded))
        }
        (None, true, None) => Err(
            "pass one tree root, 2+ tree roots, or a config file (a zzop.config.jsonc)".to_string(),
        ),
        _ => Err(
            "pass exactly ONE source: one tree root, 2+ tree roots, or a config file".to_string(),
        ),
    }
}

/// A `Method::Analyze` request becomes a one-entry `analyzeTrees` request; an `AnalyzeTrees`
/// request passes through untouched.
fn wrap_single_tree(mut loaded: crate::LoadedRequest) -> crate::LoadedRequest {
    if loaded.method == crate::Method::Analyze {
        loaded.request = serde_json::json!({ "trees": [loaded.request] });
        loaded.method = crate::Method::AnalyzeTrees;
    }
    loaded
}

/// Paths mode: one tree request per path, each loaded from THAT path's own `zzop.config.jsonc`
/// ([`crate::load_for_root`]), `sourceId` = the directory name.
///
/// # It used to build these config-free (reversed 2026-07-27)
///
/// This mode deliberately did NOT load a config sitting inside a path: it mapped an empty `{}` against
/// each root and disclosed the ignored file in `configWarnings`. That was defensible while an empty
/// config still meant "every default applies". It stopped being defensible the day an undeclared
/// convention vocabulary became a judgment NOT MADE (`zzop_engine::vocabulary`) — paths mode would then
/// have been the one lane that quietly analyzed less, WITHOUT the refusal every sibling lane gives,
/// which is the silent-blindness failure the config requirement exists to remove. So each path is loaded
/// exactly like a single-tree run of that path, and a path with no config is refused by the same
/// message, naming the path that lacks one.
///
/// A path whose config declares its own multi-tree set is refused rather than flattened: that config's
/// answer to "which trees?" and this call's `paths` are two different answers, and picking one silently
/// would make the analyzed set unreadable from either.
///
/// `operation` is the CALLER's own surface-neutral operation name ("the cross-layer join", "the endpoint
/// query", `manifest`, ...) — this helper is shared, so the "at least 2 paths" error must name whichever
/// operation the caller actually is, never a hardcoded sibling (see the module doc for the live-fire
/// misattribution this parameter fixes, and for why it may not be one host's tool spelling).
pub(crate) fn zero_config_trees(
    operation: &str,
    paths: &[String],
) -> Result<crate::LoadedRequest, String> {
    if paths.len() < 2 {
        return Err(format!(
            "{operation} needs at least 2 paths (e.g. the frontend and the backend)"
        ));
    }
    let mut trees: Vec<serde_json::Value> = Vec::with_capacity(paths.len());
    let mut warnings: Vec<String> = Vec::new();
    let mut honored: Vec<String> = Vec::with_capacity(paths.len());
    for p in paths {
        // Absolutized at the host boundary (see `paths`) — this also makes the dir-name `sourceId`
        // below real for a relative argument (`.` has no `file_name` until it is absolutized).
        let root = paths::absolutize(p);
        if !root.exists() {
            return Err(format!("path does not exist: {p}"));
        }
        let loaded = crate::load_for_root(&root).map_err(|e| e.to_string())?;
        let loaded_path = root
            .join(crate::DEFAULT_CONFIG_FILENAME)
            .display()
            .to_string();
        // The loader's own warnings must survive this mode too (e.g. a bundled pack that failed to
        // parse) — dropping them here would make paths mode the one silent sibling.
        warnings.extend(loaded.warnings);
        let mut req = match loaded.method {
            crate::Method::Analyze => loaded.request,
            crate::Method::AnalyzeTrees => {
                return Err(format!(
                    "{p} has a {} declaring its own tree set, which this call's path list also \
                     answers — run {operation} in CONFIG MODE over that config instead, or point \
                     these paths at trees whose configs declare one tree each",
                    crate::DEFAULT_CONFIG_FILENAME
                ))
            }
        };
        let source_id = root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(p.as_str())
            .to_string();
        req["sourceId"] = serde_json::Value::String(source_id);
        trees.push(req);
        honored.push(loaded_path);
    }
    // `config_path` stays `None` and the reply's `config` field with it: there is no ONE config this run
    // honored, there are N. That is exactly why the fact has to be said out loud instead — a reader who
    // sees `config: null` would otherwise conclude, correctly under the old behaviour and wrongly under
    // this one, that no config file was read at all. The old sentence here said the opposite ("paths mode
    // does NOT load it"); it is replaced rather than deleted, because the honesty need did not change,
    // only the answer.
    warnings.push(format!(
        "paths mode loaded each tree's own {}: {}. The reply's `config` field stays null because no \
         single config governs this run — run it in CONFIG MODE over one config to have that field \
         name the file that did.",
        crate::DEFAULT_CONFIG_FILENAME,
        honored.join(", ")
    ));
    Ok(crate::LoadedRequest {
        method: crate::Method::AnalyzeTrees,
        request: serde_json::json!({ "trees": trees }),
        warnings,
        config_path: None,
    })
}
