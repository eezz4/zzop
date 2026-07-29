//! Per-tree deployment topology — the `trees[].topology` object's read + validation.
//!
//! Split out of `mapper.rs` on 2026-07-28 (line cap) when the three keys moved under one roof. The
//! seam is honest rather than size-driven: everything here answers "where does this tree sit behind a
//! gateway", which is the one question the `topology` grouping exists to make askable in one place.

use serde_json::{Map, Value};

use super::validation::{validate_hosts_array, validate_mount_at, validate_mounts_array};
use crate::ConfigError;

/// Reads `trees[i].topology` onto the tree REQUEST, which keeps the three flat — `topology` is a
/// config-file grouping, not a wire change, exactly as `packs.extraDirs` maps onto a flat `packsDir`.
pub(super) fn apply_topology(
    tree_obj: &Map<String, Value>,
    tree_request: &mut Map<String, Value>,
    i: usize,
) -> Result<(), ConfigError> {
    // Connection topology — `trees[].topology`, `trees[]` entries only (the `roots` shorthand
    // below never reads these keys at all).
    //
    // The three used to sit flat on the tree entry as `mountedAt`/`mounts`/`hosts`, and they
    // read as three unrelated keys: a past participle plus a preposition, and two plural nouns.
    // They are one family — where this tree sits behind a gateway — and a reader could not tell
    // from the names. Grouping them says it, exactly as `vocabulary` groups the name axis, and
    // once grouped `topology.mountedAt` (the whole tree's `at`) and `topology.mounts[].at` (one
    // directory's) read as the same idea at two scopes rather than as a coincidence.
    //
    // Replacement granularity is unchanged and is the LEAF, the same rule `packs` and `git`
    // already follow: declaring `topology.hosts` leaves `topology.mounts` exactly as it was.
    //
    // The OLD flat spelling is a hard `ConfigError` naming the new path, not a warning and not a
    // silent honoring. Two reasons, and the first is the load-bearing one: every other topology
    // mistake here is already a load-time error (a `mountedAt` without a leading `/`, a `dir`
    // that escapes the tree), because a wrong prefix does not fail loudly at analysis time — it
    // mis-keys the join and reports confidently. A dropped-but-warned mount is the same failure
    // with a line of text over it. Second, config became mandatory in this same release, so every
    // user is already editing this file once; making them find one more moved key while they are
    // in there is cheaper than leaving two spellings live and letting them diverge.
    for legacy in ["mountedAt", "mounts", "hosts"] {
        if tree_obj.get(legacy).is_some() {
            return Err(ConfigError(format!(
                "trees[{i}].{legacy} moved to trees[{i}].topology.{legacy} — the three \
                 deployment-topology keys now sit under one `topology` object so they read as \
                 the one family they are. Nest it and the value is unchanged."
            )));
        }
    }
    if let Some(topo) = tree_obj.get("topology") {
        let topo = topo.as_object().ok_or_else(|| {
            ConfigError(format!(
                "trees[{i}].topology must be an object ({{ \"mountedAt\": ..., \"clientBase\": \
                 ..., \"mounts\": [...], \"hosts\": [...] }})."
            ))
        })?;
        if let Some(v) = topo.get("mountedAt") {
            let s = validate_mount_at(v, &format!("trees[{i}].topology.mountedAt"))?;
            tree_request.insert("mountedAt".to_string(), Value::String(s));
        }
        // The CALLING side's mirror of `mountedAt` (2026-07-29). `mountedAt`/`mounts`/`hosts` are all
        // about where this tree is SERVED; `clientBase` is the prefix this tree's own outbound calls
        // carry — the base an engine that refuses to guess cannot read when it is assigned from a
        // cross-file constant (`axios.defaults.baseURL = settings.baseApiUrl`). Same validation as
        // `mountedAt` because it is the same kind of value: a leading-slash path prefix, no scheme, no
        // `{}` placeholder. Belongs in `topology` for the reason the grouping exists — it answers the
        // same "where does this tree sit relative to the gateway" question, from the other end.
        if let Some(v) = topo.get("clientBase") {
            let s = validate_mount_at(v, &format!("trees[{i}].topology.clientBase"))?;
            tree_request.insert("clientBase".to_string(), Value::String(s));
        }
        if let Some(v) = topo.get("mounts") {
            let arr = validate_mounts_array(v, &format!("trees[{i}].topology.mounts"))?;
            if !arr.is_empty() {
                tree_request.insert("mounts".to_string(), Value::Array(arr));
            }
        }
        if let Some(v) = topo.get("hosts") {
            let arr = validate_hosts_array(v, &format!("trees[{i}].topology.hosts"))?;
            if !arr.is_empty() {
                tree_request.insert("hosts".to_string(), Value::Array(arr));
            }
        }
    }
    Ok(())
}
