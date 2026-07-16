use std::collections::{BTreeMap, BTreeSet, HashSet};

use zzop_core::IoProvide;

/// Resolves `IoProvide::body`'s `dto_ref` (`body-shape-v1`) against the tree-wide merged class-shape map
/// (`zzop_core::ClassShapeFragment`) — the assemble-time counterpart of
/// [`compose_controller_prefix_provides`]'s constant-ref resolution, but for request-body DTO classes: a
/// `@Body() dto: CreateUserDto` provide only names the DTO class by its identifier; the class declaration
/// usually lives in another file, so a single-file scan can't resolve it (see `ProvideBodyShape`'s own doc).
///
/// ## Merge (never guess)
/// `class_shapes` is sorted by file path first for a deterministic scan order (mirrors
/// [`merge_const_map_fragments`]'s determinism rationale), then folded into one `name -> ClassShapeFragment`
/// map:
/// - A name declared identically (same `fields` + `complete`) in one file, or repeated identically across
///   2+ files, resolves normally.
/// - A name declared with CONFLICTING shapes (different `fields` or `complete`) across 2+ files is
///   POISONED — it resolves to nothing for every provide referencing it, and ONE aggregated warning names
///   the class and every file that declared it, rather than guessing which declaration is authoritative.
///
/// ## Provide resolution
/// Every provide whose `body` is `Some(shape)` with `shape.dto_ref == Some(name)`:
/// - `name` resolved (found, not poisoned): `fields`/`complete` are copied from the merged shape and
///   `dto_ref` is cleared to `None` — fully resolved, matching `ProvideBodyShape`'s own doc.
/// - `name` absent from the merge, or poisoned: the WHOLE `body` is dropped to `None` (never guessed,
///   same policy as an unresolved `prefix_ref`) — one aggregated warning per distinct `(file, dto_ref)`
///   pair, naming the ref, the file, and how many provides in that file lost their body contract, mirroring
///   [`compose_controller_prefix_provides`]'s aggregation style.
///
/// Must run AFTER every provide-composition pass (`compose_controller_prefix_provides`, the global-prefix
/// seam, `compose_trpc_provides`, `compose_router_mount_provides`, file-convention routes) so a
/// prefix-ref-composed provide's body also gets resolved here — see `zzop_engine::analyze::mod`'s call site.
pub(crate) fn resolve_provide_body_refs(
    io_provides: &mut [IoProvide],
    class_shapes: Vec<(String, Vec<zzop_core::ClassShapeFragment>)>,
    warnings: &mut Vec<String>,
) {
    let mut class_shapes = class_shapes;
    class_shapes.sort_by(|a, b| a.0.cmp(&b.0));

    let mut shapes_by_name: BTreeMap<String, zzop_core::ClassShapeFragment> = BTreeMap::new();
    let mut files_by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut poisoned: HashSet<String> = HashSet::new();

    for (file, frags) in &class_shapes {
        for frag in frags {
            files_by_name
                .entry(frag.name.clone())
                .or_default()
                .insert(file.clone());
            match shapes_by_name.get(&frag.name) {
                None => {
                    shapes_by_name.insert(frag.name.clone(), frag.clone());
                }
                Some(existing) => {
                    if existing.fields != frag.fields || existing.complete != frag.complete {
                        poisoned.insert(frag.name.clone());
                    }
                }
            }
        }
    }

    // Only disclose a poisoned name some provide's `dto_ref` actually references: class-shape
    // fragments are emitted for EVERY class declaration, so same-name/different-shape non-DTO
    // classes (`Config`, `Options`, feature-local types) are common and legitimate — warning on an
    // unreferenced collision would disclose a drop that never happened (a phantom disclosure, the
    // same stance `unmatched_suppression_warnings` codifies).
    let referenced: HashSet<&str> = io_provides
        .iter()
        .filter_map(|p| p.body.as_ref().and_then(|b| b.dto_ref.as_deref()))
        .collect();
    let mut poisoned_names: Vec<&String> = poisoned
        .iter()
        .filter(|n| referenced.contains(n.as_str()))
        .collect();
    poisoned_names.sort();
    for name in poisoned_names {
        let files: Vec<&str> = files_by_name[name].iter().map(String::as_str).collect();
        warnings.push(format!(
            "class `{name}` is declared with conflicting field shapes across {} files ({}) — request-body \
             resolution for `{name}` is dropped, never guessed",
            files.len(),
            files.join(", ")
        ));
    }

    // One aggregated warning per (file, dto_ref) whose ref could not be resolved — count of provides
    // dropped, mirroring `compose_controller_prefix_provides`'s aggregation style.
    let mut unresolved: BTreeMap<(String, String), u32> = BTreeMap::new();

    for provide in io_provides.iter_mut() {
        let Some(dto_ref) = provide.body.as_ref().and_then(|b| b.dto_ref.clone()) else {
            continue;
        };
        if poisoned.contains(&dto_ref) {
            provide.body = None;
            *unresolved
                .entry((provide.file.clone(), dto_ref))
                .or_insert(0) += 1;
            continue;
        }
        match shapes_by_name.get(&dto_ref) {
            Some(frag) => {
                if let Some(shape) = provide.body.as_mut() {
                    shape.fields = frag.fields.clone();
                    shape.complete = frag.complete;
                    shape.dto_ref = None;
                }
            }
            None => {
                provide.body = None;
                *unresolved
                    .entry((provide.file.clone(), dto_ref))
                    .or_insert(0) += 1;
            }
        }
    }

    for ((file, dto_ref), count) in unresolved {
        let provide_word = if count == 1 { "provide" } else { "provides" };
        warnings.push(format!(
            "could not resolve request-body DTO `{dto_ref}` ({file}) to a known class shape — its {count} \
             {provide_word} keep no body contract; the DTO class may live in an unanalyzed file"
        ));
    }
}

#[cfg(test)]
mod tests;
