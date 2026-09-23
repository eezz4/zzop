//! The "not found" branch of Mode B's per-`FileProjection` merge: a path the native pass never
//! dispatched at all, so there is no artifact to merge ONTO and one has to be built.
//!
//! Split out of `merge.rs` when that file hit the 300-line cap. The seam is the one its own module
//! doc already named — found vs not-found — and the two branches share nothing but the projection they
//! read: this one decides what a file with NO native extraction is allowed to contribute, which is a
//! different question from what an overlay may displace.

use crate::envelope::overlay::normalize_io_file_field;

/// The "not found" branch of `apply_adapter_overlays`'s per-`FileProjection` merge — builds a brand-new
/// `FileArtifact` for a `path` the native pass never dispatched at all.
pub(in crate::envelope) fn synthetic_artifact_from_projection(
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
        suppress_markers: Vec::new(),
        rel: projection.path.clone(),
        symbols: Vec::new(),
        // Was unconditionally `None` ("dead data" by design) — now carries the projection's own imports
        // whenever there is dep-graph data to contribute, so an injected non-TS file (`.svelte`/`.vue`/
        // `.astro`) gives its imported native TS targets real fan-in, exactly like a native TS importer
        // would. This is the synthetic-artifact half of the injection contract's dep-graph completion;
        // `merge_projection_onto_artifact` (the onto-an-EXISTING-native-artifact branch, above) reaches
        // the same outcome by a different rule — additive per key, native binding wins on a collision
        // unless the overlay declared an override for that name. Nothing to displace here: a synthetic
        // artifact has no native facts, so `overrides` on such a projection is inert by construction.
        imports: has_dep_graph_data.then(|| projection.imports.clone()),
        // Now carried through (previously always `Vec::new()` — see the superseded comment this
        // replaces) via the SAME `if let Some(imports)` branch in `analyze::assemble` as `imports` right
        // above: a synthetic overlay file's bare re-export or dynamic `import()` now gives its target
        // real fan-in too. (Mode A's `analyze_envelope` is unaffected either way: it builds `dep` by hand
        // straight from `FileProjection`, per the re-export/dynamic-import merge in `file_pass`, never
        // through this struct.)
        re_exports: projection.re_exports.clone(),
        dynamic_imports: projection.dynamic_imports.clone(),
        // Envelope/overlay-projected files carry no natively-captured runtime asset refs (the
        // `parse_asset_refs` capture runs only in the fresh native pass) — always empty here.
        asset_refs: Vec::new(),
        loc: projection.loc,
        findings: Vec::new(),
        // Never degraded, and unchanged by the cause split: the three causes are all verdicts about a
        // read/parse this lane never performs — the facts here came from the overlay, already extracted.
        degrade_cause: None,
        minified_or_generated: false,
        io,
        rule_timings: Vec::new(),
        used_names: Vec::new(),
        // An external projection carries no signature evidence (the envelope has no such channel),
        // so an overlay-only file simply gets no `unimported-export` exemptions — same graceful degrade
        // as `used_names` directly above.
        exported_signature_names: Vec::new(),
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
        function_spans: projection.function_spans.clone(),
        test_spans: projection.test_spans.clone(),
        // No wire counterpart to plumb: `FileProjection` carries no call-site channel — see the identical
        // note at the Mode A `SourceFile` in `file_pass` for why that boundary is deliberate.
        call_sites: Vec::new(),
        // No wire counterpart either, and deliberately so for a second reason beyond `call_sites`':
        // the channel carries hashes of candidate secrets, which must not ride an external
        // submission — `file_pass`'s note at its `string_literals` owns the privacy argument.
        string_literals: Vec::new(),
        // Same boundary a third time, and this one has a wire counterpart that is deliberately NOT read:
        // `FileProjection` carries `calls`, but only the Mode A envelope lane consumes it
        // (`envelope::callgraph`). Feeding it in here would turn a refactor into a new capability for
        // Mode B overlays, which `FileProjection::calls`' own doc says is not offered today.
        call_graph: Default::default(),
        // No wire counterpart, same boundary as the three fields above.
        export_aliases: Vec::new(),
        has_generated_banner: false,
    }
}
