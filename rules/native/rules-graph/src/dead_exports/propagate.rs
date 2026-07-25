//! Re-export chain fixpoint for `find_dead_exports` — see the parent module doc's "What counts as a
//! use" section for what a chain hop means.

use std::collections::{HashMap, HashSet};

/// When `barrel#X` is imported, the source it re-exports is alive too — a fixpoint loop resolves
/// multi-hop chains. `chain[barrel_file] = [(local_alias, target_file, original_name)]`.
pub(super) fn propagate_re_exports(
    imported_keys: &mut HashSet<String>,
    wildcard_files: &mut HashSet<String>,
    chain: &HashMap<String, Vec<(String, String, String)>>,
) {
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = imported_keys.iter().cloned().collect();
    while let Some(key) = queue.pop() {
        if !visited.insert(key.clone()) {
            continue;
        }
        let Some(hash_idx) = key.rfind('#') else {
            continue;
        };
        let file = &key[..hash_idx];
        let name = &key[hash_idx + 1..];
        let Some(edges) = chain.get(file) else {
            continue;
        };
        for (local_alias, target_file, original_name) in edges {
            if local_alias != name {
                continue;
            }
            let next_key = format!("{target_file}#{original_name}");
            if imported_keys.contains(&next_key) {
                continue;
            }
            imported_keys.insert(next_key.clone());
            queue.push(next_key);
        }
    }
    // wildcard_files propagate through the chain too, via the same fixpoint, to reach further hops.
    let mut changed = true;
    while changed {
        changed = false;
        let current: Vec<String> = wildcard_files.iter().cloned().collect();
        for file in current {
            let Some(edges) = chain.get(&file) else {
                continue;
            };
            for (_, target_file, _) in edges {
                if wildcard_files.insert(target_file.clone()) {
                    changed = true;
                }
            }
        }
    }
}
