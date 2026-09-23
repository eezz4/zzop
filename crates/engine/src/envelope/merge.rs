//! Mode B's two per-`FileProjection` merge branches — onto an existing native artifact
//! (`merge_projection_onto_artifact`, here) or as a brand-new synthetic one
//! (`synthetic_artifact_from_projection`, in the `synthetic` child module). Called only from
//! `overlay::apply_adapter_overlays`.

use zzop_core::IoFacts;

use super::overlay::normalize_io_file_field;

mod collisions;
mod synthetic;

pub(super) use collisions::{record_fragment_collisions, FragmentCollision, MergeLosses};
pub(super) use synthetic::synthetic_artifact_from_projection;

/// One native fact an overlay DISPLACED via a declared `overrides` entry, carried back to
/// `apply_adapter_overlays` so the run can say so.
///
/// The engine refuses to lose a fact silently, and an override is the one overlay operation that
/// otherwise would: the native binding is simply gone from the output with nothing recording that it was
/// ever extracted. This carries enough to re-derive the judgment — which file, which local name, what we
/// had, what replaced it — for the same reason `zzop_core::registry::redact` marks an excluded evidence
/// path instead of dropping it: a reader must be able to tell "displaced here" from "never known".
pub(super) struct Tombstone {
    pub(super) path: String,
    pub(super) local_name: String,
    pub(super) native_specifier: String,
    pub(super) overlay_specifier: String,
}

/// The mirror of [`Tombstone`]: a binding the OVERLAY offered that the engine dropped, because the
/// native pass had already bound that local name to something different and the overlay declared no
/// override for it.
///
/// Native-first is the right default — an adapter must not overwrite parsed facts by accident — but a
/// silent drop of the adapter's side is the same defect as a silent drop of ours, just pointing the
/// other way. Without this line an author who misspells an override declaration, or forgets one, sees a
/// run indistinguishable from success: the corrected binding is absent, the wrong native one is still
/// there, and nothing separates "I was overruled" from "I was applied". Measured on
/// `examples/adapters/override-required`, this is easy to hit — Python binds `import util.config` under
/// the local name `util`, so an adapter naming the key `util.config` misses entirely while one naming
/// `util` collides.
///
/// Only a DIFFERING value is recorded. An overlay restating a binding the native pass already holds is
/// agreement, not a loss — that is `java-imports-adapter`'s whole situation, and warning about it would
/// be noise on every run.
pub(super) struct DroppedOverlayBinding {
    pub(super) path: String,
    pub(super) local_name: String,
    pub(super) native_specifier: String,
    pub(super) overlay_specifier: String,
}

/// The "found" branch of `apply_adapter_overlays`'s per-`FileProjection` merge (see that function's doc
/// for the dedup/native-first semantics per channel). A TypeScript artifact the native pass parsed keeps
/// every dep-graph fact it extracted itself — an overlay never overrides a parsed one — but it no longer
/// SILENCES the overlay's other facts: the three dep-graph channels merge ADDITIVELY, exactly like `io`
/// and the fragment channels already do. Also covers the case the native pass walks but cannot parse
/// (e.g. a `.svelte` component lands here as a degraded artifact with `imports: None`), where the
/// overlay supplies the whole channel and its imports become real fan-in edges to their TS targets.
///
/// The previous rule was all-or-nothing at the FILE level: one native binding discarded the overlay's
/// entire dep-graph contribution for that file. That made an adapter's worth a function of what the
/// native parser happened to leave empty rather than of what the adapter knows — `examples/adapters/
/// java-imports-adapter` became a total no-op the day the native Java parser started emitting imports,
/// and a partially-better adapter had the same fate (native at 60% discarded the adapter's other 40%).
/// Additive merging is what lets native and injected extraction COMBINE on one file.
///
/// Native-first is enforced per KEY, not per file: `imports` is a `key -> ImportBinding` map — the key
/// being the local name for most front ends, but not for all of them (`crates/core/src/ir/imports.rs`
/// owns the per-language table; C# keys a plain `using` by its full specifier) — so a key the native
/// pass already bound keeps its native binding (`or_insert_with`, the same rule
/// `const_map_fragment` uses below) and only keys it never bound are added. `re_exports` and
/// `dynamic_imports` are sequences with no key, so they append minus exact duplicates — `ReExport` is
/// compared by value, which keeps a type-only re-export distinct from an otherwise identical runtime one
/// (only the latter is a dep-graph edge).
///
/// A DECLARED override (`FileProjection::overrides`) is the single exception to native-first, and it
/// runs BEFORE the additive pass — see the loop's own comment for why the order is load-bearing. Every
/// direction of loss is reported out through `losses`: displacements, overlay bindings the native side
/// outranked, and — the one loss additive merging can cause all by itself — files whose router-composition
/// fragments now come from two producers at once (see [`collisions`]).
pub(super) fn merge_projection_onto_artifact(
    artifact: &mut crate::pipeline::FileArtifact,
    projection: &zzop_core::FileProjection,
    losses: &mut MergeLosses,
) {
    // Split up front so the two dep-graph passes below read as they always did; both borrows end
    // before `record_fragment_collisions` needs `losses` whole again.
    let (tombstones, dropped) = (&mut losses.tombstones, &mut losses.dropped);
    // DISPLACEMENT first. The additive pass below is native-first (`or_insert_with`), so a declared
    // override must remove the native binding here or the replacement would lose its own collision.
    // Every removal is recorded; validation (`zzop_core`'s structural pass) guarantees a declaration
    // always carries its replacement, so a displaced fact is always swapped, never deleted.
    for local_name in &projection.overrides.imports {
        let Some(replacement) = projection.imports.get(local_name) else {
            continue; // validation rejects this shape, and a rejected overlay never reaches here.
        };
        if let Some(existing) = artifact.imports.as_mut() {
            if let Some(displaced) = existing.remove(local_name) {
                tombstones.push(Tombstone {
                    path: projection.path.clone(),
                    local_name: local_name.clone(),
                    native_specifier: displaced.specifier,
                    overlay_specifier: replacement.specifier.clone(),
                });
            }
        }
    }

    // Gated on the projection actually carrying dep-graph data, for the same reason
    // `synthetic_artifact_from_projection` gates on it below: `analyze::assemble` folds an artifact into
    // `ts_import_pairs`/`ts_paths`/`package_import_files` inside `if let Some(imports) = artifact.imports`,
    // so flipping a `None` to `Some(empty)` would enter a no-data file into those sets. With data present
    // the flip is exactly what a degraded/non-TS file needs.
    let has_dep_graph_data = !projection.imports.is_empty()
        || !projection.re_exports.is_empty()
        || !projection.dynamic_imports.is_empty();
    if has_dep_graph_data {
        let existing = artifact
            .imports
            .get_or_insert_with(zzop_core::ImportMap::default);
        for (local_name, binding) in &projection.imports {
            match existing.get(local_name) {
                // Native already bound this name to something ELSE and no override was declared for it.
                // Native still wins, but the overlay's side is not lost quietly — see
                // `DroppedOverlayBinding` for why the silent version of this is a defect.
                Some(native) if native.specifier != binding.specifier => {
                    dropped.push(DroppedOverlayBinding {
                        path: projection.path.clone(),
                        local_name: local_name.clone(),
                        native_specifier: native.specifier.clone(),
                        overlay_specifier: binding.specifier.clone(),
                    });
                }
                // Same specifier: agreement, nothing lost, nothing to say.
                Some(_) => {}
                None => {
                    existing.insert(local_name.clone(), binding.clone());
                }
            }
        }
        for re_export in &projection.re_exports {
            if !artifact.re_exports.contains(re_export) {
                artifact.re_exports.push(re_export.clone());
            }
        }
        for specifier in &projection.dynamic_imports {
            if !artifact.dynamic_imports.contains(specifier) {
                artifact.dynamic_imports.push(specifier.clone());
            }
        }
    }

    let mut incoming_io = projection.io.clone();
    normalize_io_file_field(&mut incoming_io, &projection.path);

    let existing = artifact.io.get_or_insert_with(IoFacts::default);
    for provide in incoming_io.provides {
        let dup = existing.provides.iter().any(|p| {
            p.kind == provide.kind
                && p.key == provide.key
                && p.file == provide.file
                && p.line == provide.line
        });
        if !dup {
            existing.provides.push(provide);
        }
    }
    for consume in incoming_io.consumes {
        let dup = existing.consumes.iter().any(|c| {
            c.kind == consume.kind
                && c.key == consume.key
                && c.file == consume.file
                && c.line == consume.line
        });
        if !dup {
            existing.consumes.push(consume);
        }
    }

    // BEFORE the three `extend`s below — they are the exact point at which "which producer said this"
    // stops being answerable, and the by-name composers two phases later can subtract because of it.
    record_fragment_collisions(artifact, projection, losses);

    artifact
        .procedure_router_fragments
        .extend(projection.procedure_router_fragments.iter().cloned());
    artifact
        .router_mount_fragments
        .extend(projection.router_mount_fragments.iter().cloned());
    artifact
        .class_shape_fragments
        .extend(projection.class_shape_fragments.iter().cloned());
    #[allow(
        clippy::iter_over_hash_type,
        reason = "iteration order cannot reach the result: one projection's `const_map_fragment` has unique keys, so the first-writer-wins `or_insert_with` never resolves a collision produced by this loop"
    )]
    for (key, value) in &projection.const_map_fragment {
        artifact
            .const_map_fragment
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}
