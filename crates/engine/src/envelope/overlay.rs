//! Mode B — adapter overlays folded onto native per-file artifacts: `apply_adapter_overlays`, the
//! shared fact-census predicate `overlay_file_carries_facts`, and the io `file`-field normalizer the
//! two merge branches (see `merge`) share.

use zzop_core::{IoFacts, NormalizedEnvelope};

use super::merge::{merge_projection_onto_artifact, synthetic_artifact_from_projection};
use super::reserved::{
    drop_reserved_io, is_reserved_consume_kind, is_reserved_provide_kind, reserved_drop_warning,
};

/// Merges each of `overlays` onto `artifacts` in place — the Mode B counterpart of `analyze_envelope`
/// (Mode A): a partial envelope (typically just `io` + fragment channels for a handful of files) folded
/// onto the native per-file artifacts a real `analyze_tree` run already produced, rather than an
/// envelope standing in for the entire tree. This is how an external framework adapter participates in
/// a native run without reimplementing a full parser (`EngineConfig::adapter_overlays`; empty = the
/// pre-overlay path, byte-for-byte).
///
/// Overlays are processed in `parser`-sorted order (deterministic regardless of assembly order) and
/// each is re-validated via `zzop_core::validate_envelope` first — a malformed overlay degrades to one
/// `warnings` entry naming its `parser` id and first few issues, then is skipped entirely.
///
/// Three additional self-reports fire per ACCEPTED overlay (each an aggregate over the whole overlay,
/// never per file):
/// - **Source mismatch** (G3): a `NormalizedEnvelope` self-declares `source` as the cross-layer join's
///   per-tree tag, but `apply_adapter_overlays` unconditionally merges every projection onto THIS tree's
///   artifacts regardless of what `source` says — an overlay whose `source` differs from `source_id`
///   still has its facts join as if they belonged to `source_id`. A non-empty, differing `source` is a
///   structural fact worth surfacing (a cross-source join silently becoming intra-source); an empty
///   `source` makes no claim at all, so it is never flagged.
/// - **Synthetic-entry census** (G8a): how many of an overlay's declared `files[].path` matched no file
///   in this tree at all (the push-new branch below) — a declared path with a typo silently inflates
///   `fileCount` via a synthetic entry with zero warnings otherwise; this names the count and a sample.
/// - **Zero-fact coverage** (G8b): an overlay whose every entry carries no extracted fact at all (see
///   [`overlay_file_carries_facts`]) still used to count as "adapter coverage" for the per-extension
///   "no native parser" disclosure (`analyze::assemble`'s overlay-exclusion set) — a bad/empty adapter
///   masked the very loss disclosure it should have triggered. This warning names that emptiness
///   directly; `analyze::assemble` separately stops excluding such a file from the disclosure.
///
/// Per `FileProjection`: if `path` matches an existing artifact's `rel`, it's merged in place — `io`
/// entries appended minus exact-duplicate `(kind, key, file, line)` tuples (`file` normalized to
/// `projection.path` first), fragments appended with no dedup (composition dedups later), and
/// `const_map_fragment` native-first (existing key wins); the native artifact's own
/// `imports`/`re_exports`/`dynamic_imports` are left untouched (native dep-graph facts stay
/// authoritative — see `merge_projection_onto_artifact`'s doc). If `path` names no existing artifact
/// (e.g. a `.py`/`.jsp`/`.svelte` sibling the native dispatch table doesn't recognize), it's pushed as a
/// synthetic `FileArtifact` carrying the projection's OWN `imports`/`re_exports`/`dynamic_imports` (so
/// it contributes real dep-graph fan-in edges too — see `synthetic_artifact_from_projection`'s doc) with
/// every other native-only field (symbols, wrapper/query/store/field-usage fragments) at its
/// empty/default value. A `FileProjection` additionally marked `is_entry: true` has its `path` unioned
/// into `dead_candidate_findings`'s `extra_entries` set in `analyze::assemble`, exempting it from
/// `dead-candidates` the same way a package.json manifest entry is exempt.
///
/// `artifacts` is re-sorted by `rel` before returning — `analyze::assemble` relies on that order for
/// `ir.ir.symbols`'s determinism.
///
/// Before either merge branch, every reserved engine-internal sentinel `IoProvide`/`IoConsume` (kinds
/// `nest-global-prefix`/`client-base-prefix`, see [`is_reserved_provide_kind`]/[`is_reserved_consume_kind`])
/// is dropped from the projection's `io` — a producer-forbidden pair only the native TS parser may emit
/// and only the native `analyze::assemble` seams (`apply_and_strip_global_prefix`/
/// `apply_client_base_prefixes`) may consume+strip. Those seams run later over the WHOLE tree's merged
/// `io_provides`/`io_consumes`, so an overlay sentinel that survived the merge would get re-applied
/// project-wide (every native route re-prefixed), not scoped to the overlay's own files. Each overlay with
/// any drops gets one aggregate `warnings` entry naming its `parser`, the dropped count, and the reserved
/// kinds (built by [`reserved_drop_warning`], shared with `analyze_envelope`'s Mode A counterpart so the
/// two modes' wording can't drift) — a partial drop, so (unlike a validation failure) the overlay's other
/// io/fragments still merge.
pub(crate) fn apply_adapter_overlays(
    artifacts: &mut Vec<crate::pipeline::FileArtifact>,
    overlays: &[NormalizedEnvelope],
    source_id: &str,
    warnings: &mut Vec<String>,
) {
    let mut ordered: Vec<&NormalizedEnvelope> = overlays.iter().collect();
    ordered.sort_by(|a, b| a.parser.cmp(&b.parser));

    for overlay in ordered {
        let json = match serde_json::to_string(overlay) {
            Ok(j) => j,
            Err(e) => {
                warnings.push(format!(
                    "adapter overlay '{}' skipped: failed to serialize for validation: {e}",
                    overlay.parser
                ));
                continue;
            }
        };
        if let Err(issues) = zzop_core::validate_envelope(&json) {
            let detail = issues
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            warnings.push(format!(
                "adapter overlay '{}' skipped: {detail}",
                overlay.parser
            ));
            continue;
        }

        // G3 — source-mismatch self-report: `overlay.source` is this envelope's own declared cross-layer
        // join tag, but every projection below merges onto THIS tree's artifacts (tagged `source_id`)
        // regardless of what `overlay.source` says — a non-empty, differing value means the overlay's
        // facts silently relabel to `source_id`, turning what looks like a cross-source join into an
        // intra-source one. An empty `source` makes no claim, so it is never flagged. Additionally gated
        // on the overlay carrying join-relevant io: io is the only channel that participates in
        // cross-source joins, so for an attributes-/is_entry-only overlay (e.g. the auth-overlay-adapter
        // example, whose consumers read the config directly and are source-agnostic) a differing `source`
        // is inert and the warning's "joins read as intra-source" claim would be false noise.
        let carries_join_io = overlay.files.iter().any(|f| {
            f.io.provides
                .iter()
                .any(|p| !is_reserved_provide_kind(&p.kind))
                || f.io
                    .consumes
                    .iter()
                    .any(|c| !is_reserved_consume_kind(&c.kind))
        });
        if carries_join_io && !overlay.source.is_empty() && overlay.source != source_id {
            warnings.push(format!(
                "adapter overlay \"{src}\" (parser {parser}) declares a different source than the tree \
                 \"{tree}\" it's attached to — its files and io merge into \"{tree}\" regardless, so joins \
                 against its facts read as intra-source, not cross-source. If it belongs to a different \
                 tree, move it to that tree's `overlays: [...]` in zzop.config.jsonc (embedders: \
                 `adapterOverlays`).",
                src = overlay.source,
                parser = overlay.parser,
                tree = source_id,
            ));
        }

        // Reserved engine-internal sentinel kinds (see `is_reserved_provide_kind`/`is_reserved_consume_kind`)
        // are producer-forbidden: dropped from every projection's `io` BEFORE the merge/synthetic
        // branch below, so neither path can hand one to `apply_and_strip_global_prefix`/
        // `apply_client_base_prefixes` (which run later, inside `analyze::assemble`, over the WHOLE native
        // tree's `io_provides`/`io_consumes` — an overlay sentinel surviving to there would get
        // re-interpreted as a real project-wide setting and re-prefix every native route, not just this
        // overlay's own). Dropped counts are aggregated across the WHOLE overlay (every projection), then
        // reported as one warning per overlay — a partial drop, not a skip, so processing continues.
        //
        // G8 — alongside the drop, this same loop tallies `declared_n` (every projection), `synthetic`
        // (pushed as a brand-new artifact — no existing `rel` matched) with up to 3 sample paths, and
        // `fact_carrying` (how many CLEANED projections — i.e. post reserved-io-drop, matching what
        // actually merges — carry at least one real fact per [`overlay_file_carries_facts`]).
        let mut reserved_dropped = 0usize;
        let declared_n = overlay.files.len();
        let mut fact_carrying = 0usize;
        let mut synthetic_count = 0usize;
        let mut synthetic_samples: Vec<String> = Vec::new();
        for projection in &overlay.files {
            let (cleaned, dropped) = drop_reserved_io(projection);
            reserved_dropped += dropped;
            if overlay_file_carries_facts(&cleaned) {
                fact_carrying += 1;
            }
            if let Some(artifact) = artifacts.iter_mut().find(|a| a.rel == cleaned.path) {
                merge_projection_onto_artifact(artifact, &cleaned);
            } else {
                synthetic_count += 1;
                if synthetic_samples.len() < 3 {
                    synthetic_samples.push(cleaned.path.clone());
                }
                artifacts.push(synthetic_artifact_from_projection(&cleaned));
            }
        }
        if let Some(w) = reserved_drop_warning("adapter overlay", &overlay.parser, reserved_dropped)
        {
            warnings.push(w);
        }
        if synthetic_count > 0 {
            let mut sample_str = synthetic_samples.join(", ");
            if synthetic_count > synthetic_samples.len() {
                sample_str.push_str(&format!(
                    ", +{} more",
                    synthetic_count - synthetic_samples.len()
                ));
            }
            warnings.push(format!(
                "adapter overlay \"{}\" (parser {}): {synthetic_count} of {declared_n} declared file(s) \
                 matched no file in this tree and were added as synthetic entries: {sample_str} — their \
                 io still merges and joins under the declared path (check for path typos), and they \
                 count in coverage.files. If these paths are typos, fix the overlay's files[].path \
                 (tree-root-relative) so its facts merge onto the real files.",
                overlay.source, overlay.parser
            ));
        }
        if fact_carrying == 0 && declared_n > 0 {
            let plural = if declared_n == 1 { "y" } else { "ies" };
            warnings.push(format!(
                "adapter overlay \"{}\" (parser {}): none of its {declared_n} file entr{plural} carries \
                 any extracted facts (io/imports/fragments/attributes) — the adapter may have \
                 produced an empty envelope. Empty entries do not count as parser coverage, so the \
                 per-extension \"no native parser\" diagnostic still applies to their files.",
                overlay.source, overlay.parser
            ));
        }
    }

    artifacts.sort_by(|a, b| a.rel.cmp(&b.rel));
}

/// True iff `file` contributes at least one extraction FACT that an overlay merge actually acts on —
/// non-empty non-reserved `io.provides`/`io.consumes`, `imports`, `re_exports`, `dynamic_imports`, any
/// fragment channel (`const_map_fragment`, `procedure_router_fragments`, `router_mount_fragments`,
/// `class_shape_fragments`), a non-empty per-file `attributes` (the channel already lives on
/// `FileProjection` itself, one array per file — not envelope-level — so "this projection's own
/// `attributes` is non-empty" is already the precise per-file rule, no cross-referencing an `EntityRef`
/// target needed), or `is_entry == true`. `path`/`loc`/`degraded` are metadata, not facts.
///
/// The io checks skip reserved engine-internal sentinel kinds (the same set `drop_reserved_io` strips
/// before the merge), so the predicate judges a RAW projection and a cleaned one identically — the two
/// call sites can safely feed it different pre-processing stages without drifting.
///
/// `symbols` is deliberately EXCLUDED: Mode B's merge never consumes overlay symbols
/// (`merge_projection_onto_artifact` does not touch the field and `synthetic_artifact_from_projection`
/// sets it empty), so counting it would call a file "covered" for data the engine silently drops — a
/// symbols-only overlay must instead trip the zero-fact census and keep the "no native parser"
/// disclosure alive. `used_names` and `loop_spans` are excluded for the same reason: neither is read by
/// either merge branch in a way that reaches an actual consumer today.
///
/// Shared by [`apply_adapter_overlays`]'s own per-overlay zero-fact census (G8b) and
/// `analyze::assemble`'s overlay-exclusion set for the "no native parser" per-extension disclosure (G8's
/// unmasking half) — one rule, two call sites, so they can never drift apart.
pub(crate) fn overlay_file_carries_facts(file: &zzop_core::FileProjection) -> bool {
    file.io
        .provides
        .iter()
        .any(|p| !is_reserved_provide_kind(&p.kind))
        || file
            .io
            .consumes
            .iter()
            .any(|c| !is_reserved_consume_kind(&c.kind))
        || !file.imports.is_empty()
        || !file.re_exports.is_empty()
        || !file.dynamic_imports.is_empty()
        || !file.const_map_fragment.is_empty()
        || !file.procedure_router_fragments.is_empty()
        || !file.router_mount_fragments.is_empty()
        || !file.class_shape_fragments.is_empty()
        || !file.attributes.is_empty()
        || file.is_entry
}

/// Overwrites every `IoProvide`/`IoConsume` in `io`'s `file` field to `path` — the defensive
/// normalization `apply_adapter_overlays` describes: an overlay is not trusted to already have set
/// `file` to match its own `FileProjection::path`.
pub(super) fn normalize_io_file_field(io: &mut IoFacts, path: &str) {
    for provide in &mut io.provides {
        provide.file = path.to_string();
    }
    for consume in &mut io.consumes {
        consume.file = path.to_string();
    }
}
