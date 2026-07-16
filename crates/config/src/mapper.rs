//! The config→facade-request mapper — the port of `packages/cli/lib/mapper.js`'s `configToRequest`
//! (plus `collectConfigWarnings`), working on `serde_json::Value` exactly as the JS works on plain
//! objects (no config structs: unknown keys must flow through to the warning walk, not fail serde).
//!
//! Contract anchors (verbatim behaviors the port must keep — see the JS source for the full set):
//! - Severity aliases: off/none/disable/disabled; critical/error/err/high; warning/warn/medium;
//!   info/information/note/low. `"off"` routes to `disabledRules`, everything else to
//!   `severityOverrides` with the engine's lowercase values.
//! - `isGlobPattern` = the exact character class `[*?{}]` — `[`/`]` deliberately excluded so
//!   Next.js-style `app/[locale]/` paths stay substring matches (`path` key), not globs (`glob`).
//! - `trees` wins over `roots` silently when both are set; single `roots` entry with no `trees` key
//!   selects `analyze`, everything else `analyzeTrees` (a single-entry `trees` array included).
//! - `mountedAt`/`mounts`/`hosts` fail-fast validation gates (leading `/`, no scheme, no `{}`
//!   placeholder, no whitespace; `dir` tree-relative, forward slashes; hosts bare, no `://`, no `/`).
//! - Overlay files are read HERE (tree-root-relative), read/parse failures become warnings that skip
//!   the overlay, never errors.
//! - Unknown keys at every scoped level warn (never reject) with the `config-surface.json`
//!   vocabulary (`crate::CONFIG_SURFACE_JSON`).
//! - The `withDefaults` layer folds in here for native hosts: bundled packs are injected as inline
//!   `packDefs` (`crate::BUNDLED_PACK_SOURCES`), and `git: {}` is injected when the config has no
//!   `git` key, so zero-config still collects git signals (30-day default) exactly like the JS CLI.
//! - CLI-presentation keys (`failOn`/`format`/`report.*`) are NOT forwarded into the request.
//!
//! Deliberate deviation (documented in the crate doc): `root`/`cacheDir`/`packsDir` resolve against
//! `base_dir` (the config file's directory) instead of the process cwd.
//!
//! ## Implementation notes beyond the JS source
//! - JS's `collectConfigWarnings` and `configToRequest` are two independently-called functions (the
//!   CLI calls both, and `configToRequest`'s own overlay resolution throws its warnings away since
//!   `collectConfigWarnings` recomputes them). This port threads ONE warnings `Vec` end to end (see
//!   `MappedRequest`), so overlay read/parse warnings are collected exactly once, at the same real
//!   resolution call that builds `adapterOverlays` — not recomputed by a second pass.
//! - `serde_json::Value::Object` iterates its keys in sorted (`BTreeMap`) order, not JS's
//!   source-text insertion order (`serde_json`'s `preserve_order` feature is not enabled here).
//!   This only ever affects the ORDER of generated warning/array entries whose own content is a set
//!   (unknown-key warnings, `disabledRules`), never their content — deep-equal comparisons (as JSON
//!   values, not raw strings) are unaffected either way.
//! - `JSON.stringify(value)` (used in JS's severity error text) is mirrored with
//!   `serde_json::to_string`, which matches byte-for-byte for every primitive JSON value; object/array
//!   key order can differ for the same `BTreeMap`-vs-insertion-order reason above, an irrelevant edge
//!   case for a severity value (which is never itself an object/array in a well-formed config).

mod options;
mod paths;
mod severity;
mod validation;
mod warnings;

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::{ConfigError, Method};

use options::build_shared_options;
use paths::{path_to_string, resolve_path};
use validation::{
    resolve_overlays_for_root, validate_hosts_array, validate_mount_at, validate_mounts_array,
    validate_overlays_array,
};
use warnings::{collect_config_warnings, parse_pack_defs};

/// The result of mapping one config object: the facade request value (with defaults injected),
/// the method it targets, and every non-fatal warning collected along the way.
#[derive(Debug)]
pub struct MappedRequest {
    pub method: Method,
    pub request: serde_json::Value,
    pub warnings: Vec<String>,
}

/// Maps a parsed config object (post-JSONC, post-`trees:"auto"`-expansion) to a facade request.
/// `base_dir` is the config file's directory — the resolution base for `root`/`cacheDir`/`packsDir`
/// (deviation from JS cwd semantics, see module doc); overlays stay tree-root-relative (JS parity).
pub fn config_to_request(
    config: &serde_json::Value,
    base_dir: &Path,
) -> Result<MappedRequest, ConfigError> {
    use serde_json::{Map, Value};

    if !config.is_object() {
        return Err(ConfigError("Config must be a JSON object.".to_string()));
    }

    let mut warnings = collect_config_warnings(config);

    let shared = build_shared_options(config, base_dir)?;

    // Top-level `overlays` (shared across every tree) — shape-validated here, resolved per tree below.
    let shared_overlay_paths = match config.get("overlays") {
        Some(v) => validate_overlays_array(v, "overlays")?,
        None => Vec::new(),
    };

    let trees_key_present = config.get("trees").is_some();
    let mut trees: Vec<Map<String, Value>> = Vec::new();

    if let Some(trees_value) = config.get("trees") {
        // `trees: "auto"` must already be expanded (by `workspaces::expand_auto_trees`, called from
        // `load_for_root`/`load_config_file` right after parsing) before it reaches this pure mapper.
        // A caller invoking `config_to_request` directly with the raw sentinel gets an actionable
        // error instead of the generic "must be a non-empty array" below.
        if trees_value.as_str() == Some("auto") {
            return Err(ConfigError(
                "trees: \"auto\" must be expanded before config_to_request — call \
                 workspaces::expand_auto_trees(config, base_dir) first (load_for_root/load_config_file \
                 do this automatically)."
                    .to_string(),
            ));
        }

        let arr = trees_value.as_array().filter(|a| !a.is_empty());
        let Some(arr) = arr else {
            return Err(ConfigError(
                "trees, when present, must be a non-empty array of { root, sourceId }.".to_string(),
            ));
        };

        for (i, tree_val) in arr.iter().enumerate() {
            let tree_obj = tree_val.as_object();
            let raw_root = tree_obj
                .and_then(|o| o.get("root"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            let Some(raw_root) = raw_root else {
                return Err(ConfigError(format!(
                    "trees[{i}] must be an object with a non-empty \"root\" string."
                )));
            };
            let tree_obj = tree_obj.expect("raw_root extraction requires an object");

            let tree_overlay_paths = match tree_obj.get("overlays") {
                Some(v) => validate_overlays_array(v, &format!("trees[{i}].overlays"))?,
                None => Vec::new(),
            };

            let resolved_root = resolve_path(base_dir, raw_root);
            let source_id = tree_obj
                .get("sourceId")
                .and_then(Value::as_str)
                .unwrap_or(raw_root);

            let mut tree_request = shared.clone();
            tree_request.insert(
                "root".to_string(),
                Value::String(path_to_string(&resolved_root)),
            );
            tree_request.insert("sourceId".to_string(), Value::String(source_id.to_string()));

            let (overlays, overlay_warnings) = resolve_overlays_for_root(
                &resolved_root,
                raw_root,
                &shared_overlay_paths,
                &tree_overlay_paths,
            );
            warnings.extend(overlay_warnings);
            if !overlays.is_empty() {
                tree_request.insert("adapterOverlays".to_string(), Value::Array(overlays));
            }

            // Connection topology — `trees[]` entries only (the `roots` shorthand below never reads
            // these keys at all).
            if let Some(v) = tree_obj.get("mountedAt") {
                let s = validate_mount_at(v, &format!("trees[{i}].mountedAt"))?;
                tree_request.insert("mountedAt".to_string(), Value::String(s));
            }
            if let Some(v) = tree_obj.get("mounts") {
                let arr = validate_mounts_array(v, &format!("trees[{i}].mounts"))?;
                if !arr.is_empty() {
                    tree_request.insert("mounts".to_string(), Value::Array(arr));
                }
            }
            if let Some(v) = tree_obj.get("hosts") {
                let arr = validate_hosts_array(v, &format!("trees[{i}].hosts"))?;
                if !arr.is_empty() {
                    tree_request.insert("hosts".to_string(), Value::Array(arr));
                }
            }

            trees.push(tree_request);
        }
    } else {
        let roots: Vec<String> = match config.get("roots") {
            None => vec![".".to_string()],
            Some(v) => {
                let arr = v.as_array().filter(|a| !a.is_empty());
                let Some(arr) = arr else {
                    return Err(ConfigError(
                        "roots must be a non-empty array of directory paths.".to_string(),
                    ));
                };
                let mut out = Vec::with_capacity(arr.len());
                for r in arr {
                    match r.as_str().filter(|s| !s.is_empty()) {
                        Some(s) => out.push(s.to_string()),
                        None => {
                            return Err(ConfigError(
                                "roots entries must be non-empty strings.".to_string(),
                            ))
                        }
                    }
                }
                out
            }
        };

        // Multiple roots => give each tree a distinct sourceId (its raw root string) so cross-source
        // analysis works; a single root needs no source tag (it takes the single-tree `Analyze` path).
        let multiple = roots.len() > 1;
        for root in &roots {
            let resolved_root = resolve_path(base_dir, root);
            let mut tree_request = shared.clone();
            tree_request.insert(
                "root".to_string(),
                Value::String(path_to_string(&resolved_root)),
            );
            if multiple {
                tree_request.insert("sourceId".to_string(), Value::String(root.clone()));
            }

            let (overlays, overlay_warnings) =
                resolve_overlays_for_root(&resolved_root, root, &shared_overlay_paths, &[]);
            warnings.extend(overlay_warnings);
            if !overlays.is_empty() {
                tree_request.insert("adapterOverlays".to_string(), Value::Array(overlays));
            }

            trees.push(tree_request);
        }
    }

    // `withDefaults` analog (packages/native/index.js): every tree request gets a `git: {}` default
    // when the config named no `git` key at all, and the bundled DSL packs inline as `packDefs` — a
    // native host has no sidecar `rules/` directory to point a `packsDir` string at, so the packs
    // themselves ride inside the request instead.
    let pack_defs = parse_pack_defs(crate::BUNDLED_PACK_SOURCES, &mut warnings);
    for tree in &mut trees {
        tree.entry("git".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        tree.insert("packDefs".to_string(), Value::Array(pack_defs.clone()));
    }

    let method = if trees.len() == 1 && !trees_key_present {
        Method::Analyze
    } else {
        Method::AnalyzeTrees
    };

    let request = match method {
        Method::Analyze => Value::Object(trees.into_iter().next().expect("exactly one tree")),
        Method::AnalyzeTrees => {
            let mut top = Map::new();
            top.insert(
                "trees".to_string(),
                Value::Array(trees.into_iter().map(Value::Object).collect()),
            );
            Value::Object(top)
        }
    };

    Ok(MappedRequest {
        method,
        request,
        warnings,
    })
}
