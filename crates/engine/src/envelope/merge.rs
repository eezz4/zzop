//! Mode B's two per-`FileProjection` merge branches — onto an existing native artifact
//! (`merge_projection_onto_artifact`) or as a brand-new synthetic one
//! (`synthetic_artifact_from_projection`). Called only from `overlay::apply_adapter_overlays`.

use zzop_core::IoFacts;

use super::overlay::normalize_io_file_field;

/// The "found" branch of `apply_adapter_overlays`'s per-`FileProjection` merge (see that function's doc
/// for the dedup/native-first semantics per channel). A TypeScript artifact the native pass parsed keeps
/// its own authoritative `imports`/`re_exports`/`dynamic_imports` (an overlay never overrides parsed
/// facts). But the native pass walks EVERY file, so a non-TS file type the engine can't parse (e.g. a
/// `.svelte` component) lands here too as a degraded artifact with `imports: None` — nothing to preserve
/// — and an overlay carrying dep-graph data then fills it, letting an adapter complete the graph for that
/// file type (its imports become real fan-in edges to their TS targets).
pub(super) fn merge_projection_onto_artifact(
    artifact: &mut crate::pipeline::FileArtifact,
    projection: &zzop_core::FileProjection,
) {
    // Dep-graph facts: adopt the overlay's only when the native artifact has none of its own (a
    // degraded/non-TS file), so parsed TS imports always win over an overlay.
    if artifact.imports.is_none()
        && (!projection.imports.is_empty()
            || !projection.re_exports.is_empty()
            || !projection.dynamic_imports.is_empty())
    {
        artifact.imports = Some(projection.imports.clone());
        artifact.re_exports = projection.re_exports.clone();
        artifact.dynamic_imports = projection.dynamic_imports.clone();
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

    artifact
        .procedure_router_fragments
        .extend(projection.procedure_router_fragments.iter().cloned());
    artifact
        .router_mount_fragments
        .extend(projection.router_mount_fragments.iter().cloned());
    artifact
        .class_shape_fragments
        .extend(projection.class_shape_fragments.iter().cloned());
    for (key, value) in &projection.const_map_fragment {
        artifact
            .const_map_fragment
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}

/// The "not found" branch of `apply_adapter_overlays`'s per-`FileProjection` merge — builds a brand-new
/// `FileArtifact` for a `path` the native pass never dispatched at all.
pub(super) fn synthetic_artifact_from_projection(
    projection: &zzop_core::FileProjection,
) -> crate::pipeline::FileArtifact {
    let mut io = projection.io.clone();
    normalize_io_file_field(&mut io, &projection.path);
    let io = if io.provides.is_empty() && io.consumes.is_empty() {
        None
    } else {
        Some(io)
    };

    // Per the Mode B dep-graph-completion contract (the injection contract extends past io/fragments to
    // dep-graph facts, so any non-TS adapter can complete the graph while the engine stays
    // framework-neutral): `analyze::assemble` only ever folds an artifact's `imports`/`re_exports`/
    // `dynamic_imports` into `ts_import_pairs`/`ts_re_export_pairs`/`ts_dynamic_import_pairs` (-> real
    // dep-graph edges, via `build_dep_with_workspace`) inside its `if let Some(imports) = artifact.imports`
    // branch — so `imports` must be `Some` whenever ANY of the three carries data, not just when `imports`
    // itself is non-empty (a bare re-export or a dynamic-only file can have an empty `imports` map and
    // still need graph participation, mirroring `analyze_envelope`'s own Defect-A/2 handling in
    // `file_pass`). Truly empty (none of the three populated) keeps `imports: None` so a no-data overlay
    // file doesn't needlessly enter `ts_import_pairs`/`ts_paths`/`package_import_files`.
    let has_dep_graph_data = !projection.imports.is_empty()
        || !projection.re_exports.is_empty()
        || !projection.dynamic_imports.is_empty();

    crate::pipeline::FileArtifact {
        rel: projection.path.clone(),
        symbols: Vec::new(),
        // Was unconditionally `None` ("dead data" by design) — now carries the projection's own imports
        // whenever there is dep-graph data to contribute, so an injected non-TS file (`.svelte`/`.vue`/
        // `.astro`) gives its imported native TS targets real fan-in, exactly like a native TS importer
        // would. This is the synthetic-artifact half of the injection contract's dep-graph completion;
        // `merge_projection_onto_artifact` (the onto-an-EXISTING-native-artifact branch, above) is
        // deliberately NOT touched here — native imports stay authoritative there, a separate concern.
        imports: has_dep_graph_data.then(|| projection.imports.clone()),
        // Now carried through (previously always `Vec::new()` — see the superseded comment this
        // replaces) via the SAME `if let Some(imports)` branch in `analyze::assemble` as `imports` right
        // above: a synthetic overlay file's bare re-export or dynamic `import()` now gives its target
        // real fan-in too. (Mode A's `analyze_envelope` is unaffected either way: it builds `dep` by hand
        // straight from `FileProjection`, per the re-export/dynamic-import merge in `file_pass`, never
        // through this struct.)
        re_exports: projection.re_exports.clone(),
        dynamic_imports: projection.dynamic_imports.clone(),
        loc: projection.loc,
        findings: Vec::new(),
        degraded: false,
        minified_or_generated: false,
        io,
        rule_timings: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: projection.const_map_fragment.clone(),
        procedure_router_fragments: projection.procedure_router_fragments.clone(),
        router_mount_fragments: projection.router_mount_fragments.clone(),
        // Wrapper resolution, query-call-site recognition, store-binding recognition, and field-usage-
        // token scanning are all native-TS-source concerns; an external adapter emits final io/router
        // fragments instead, so a synthetic overlay artifact never carries these. Controller-prefix
        // route fragments are the same native-TS-only concern (envelope module doc): an external adapter
        // already resolves its own controller prefixes before emitting `IoProvide`s, so it never has one
        // of these to carry either.
        wrapper_def_fragments: Vec::new(),
        wrapper_call_fragments: Vec::new(),
        controller_prefix_route_fragments: Vec::new(),
        // Class shapes ARE plumbed from the projection (unlike the native-TS-only concerns above):
        // an adapter may emit `IoProvide::body.dto_ref` and rely on the same assemble-time resolver
        // native controllers use, feeding it shapes for classes its own language declares.
        class_shape_fragments: projection.class_shape_fragments.clone(),
        query_call_sites: Vec::new(),
        field_usage_tokens: Vec::new(),
        // Plumbed straight from the projection (empty when absent) — same "carry the real fact, never a
        // placeholder" reasoning as the Mode A `SourceFile` in `file_pass`, even though no DSL rule pass
        // runs over a synthetic overlay artifact today (`findings: Vec::new()` above).
        loop_spans: projection.loop_spans.clone(),
    }
}
