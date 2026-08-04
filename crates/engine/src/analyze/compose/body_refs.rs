use std::collections::{BTreeMap, HashSet};

use zzop_core::IoProvide;

use super::shape_merge::ShapeMerge;

/// Resolves `IoProvide::body`'s `dto_ref` (`body-shape-v1`) against the tree-wide merged class-shape map
/// ([`ShapeMerge`], built once and shared with the response pass) — the assemble-time counterpart of
/// [`compose_controller_prefix_provides`]'s constant-ref resolution, but for request-body DTO types: a
/// `@Body() dto: CreateUserDto` provide only names the DTO by its identifier; the declaration
/// usually lives in another file, so a single-file scan can't resolve it (see `ProvideBodyShape`'s own doc).
///
/// ## Provide resolution (merge/poisoning semantics live in [`ShapeMerge`]'s doc)
/// Every provide whose `body` is `Some(shape)` with `shape.dto_ref == Some(name)`:
/// - `name` resolved (found, not poisoned): `fields`/`complete` are copied from the merged shape and
///   `dto_ref` is cleared to `None` — fully resolved, matching `ProvideBodyShape`'s own doc.
/// - `name` absent from the merge, or poisoned: the WHOLE `body` is dropped to `None` (never guessed,
///   same policy as an unresolved `prefix_ref`) — one aggregated warning per distinct `(file, dto_ref)`
///   pair, naming the ref, the file, and how many provides in that file lost their body contract, mirroring
///   [`compose_controller_prefix_provides`]'s aggregation style. Referenced poisoned names additionally
///   get [`ShapeMerge::poisoned_disclosures`]'s own conflicting-shape warning.
///
/// Must run AFTER every provide-composition pass (`compose_controller_prefix_provides`, the global-prefix
/// seam, `compose_trpc_provides`, `compose_router_mount_provides`, file-convention routes) so a
/// prefix-ref-composed provide's body also gets resolved here — see `zzop_engine::analyze::mod`'s call site.
pub(crate) fn resolve_provide_body_refs(
    io_provides: &mut [IoProvide],
    merge: &ShapeMerge,
    warnings: &mut Vec<String>,
) {
    let referenced: HashSet<&str> = io_provides
        .iter()
        .filter_map(|p| p.body.as_ref().and_then(|b| b.dto_ref.as_deref()))
        .collect();
    warnings.extend(merge.poisoned_disclosures(&referenced, "request-body"));

    // One aggregated warning per (file, dto_ref) whose ref could not be resolved — count of provides
    // dropped, mirroring `compose_controller_prefix_provides`'s aggregation style.
    let mut unresolved: BTreeMap<(String, String), u32> = BTreeMap::new();

    for provide in io_provides.iter_mut() {
        let Some(dto_ref) = provide.body.as_ref().and_then(|b| b.dto_ref.clone()) else {
            continue;
        };
        if merge.is_poisoned(&dto_ref) {
            provide.body = None;
            *unresolved
                .entry((provide.file.clone(), dto_ref))
                .or_insert(0) += 1;
            continue;
        }
        match merge.get(&dto_ref) {
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
