//! F4 declared-import denominator — per extension, how many import-shaped declarations the parsers
//! actually SAW, counted BEFORE dep resolution drops the ones that fail. The dep channel carries
//! resolved in-tree edges only ([`zzop_core::DEP_GRAPH_RESOLVED_ONLY`]), so pre-adapter blindness
//! (a 91-file tree sitting at 3 edges) used to surface as a bare number with no baseline to read it
//! against. This module supplies the baseline: `declared N` next to `resolved 0` is the
//! import-resolution-blindness signal made visible.
//!
//! CACHE AXIS (judged for F4): this is a NON-CACHED assemble-time recomputation. Every input below
//! (`imports` / `re_exports` / `dynamic_imports`) already rides the per-file cache slice
//! (`zzop_cache::FileIrSlice`) — the count is derived from those cache-carried fields on every run,
//! is never itself cached, and adds no `FileIrSlice` field, so no `CACHE_SCHEMA_VERSION` roll is
//! needed. A `PARSER_FINGERPRINT` roll is not "needed" in the sense of REMEMBERED — it happens anyway,
//! because `crates/engine/build.rs` derives `FP_ENGINE` from these bytes and that token suffixes every
//! arm. Shape, not invalidation; the distinction matters now that nobody bumps by hand. Same convention
//! as the F5 census drains beside it in `collect`, which re-resolve cached specifiers per run.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::{ImportMap, ReExport};

/// Sums, per extension, each parsed file's DISTINCT declared import specifiers — the union across the
/// three channels that can become that file's own outgoing dep edges (import bindings, re-exports,
/// dynamic `import()`), all counted pre-resolution.
///
/// MEASURED/UNMEASURED contract (never-guess): a file appears in `ts_import_pairs` exactly when its
/// parser projects an import channel at all (`FileArtifact::imports` is `Some` — TS/Python/Rust/Go/
/// Java/C#), so an extension with only channel-less files (prisma, sql, lexical-only, `.vue`/`.svelte`)
/// contributes NO key here — absence of a key means "never measured", never 0, and the facade coverage
/// table renders it as an unmeasured cell. A parsed file with zero imports contributes a real 0 to its
/// extension's sum (key present — measured zero). `.vue`/`.svelte` are deliberately OUT even though
/// `prescan::collect_prescan_import_pairs` extracts their imports: a pre-scanned file never becomes a
/// dep-graph SOURCE (`dep_graph::merge_prescan_fan_in` is target-fan-in-only), so a declared count there
/// would read as resolver blindness on a channel that structurally has no resolved side.
///
/// NOT 1:1 with `CoverageCensus::resolved_import_edges`, in either direction: a declaration is a
/// SPECIFIER and an edge is a resolved `(importer, imported file)` PAIR — package imports and
/// unresolvable specifiers are declared but never edges, while one Java glob import
/// (`import com.foo.*`) can fan out to several edges. The facade legend states this yardstick.
///
/// Extension grain: [`ext_of`], the same lowercased-tail grain the facade coverage table groups by.
pub(super) fn by_ext(
    ts_import_pairs: &[(String, ImportMap)],
    ts_re_export_pairs: &[(String, Vec<ReExport>)],
    ts_dynamic_import_pairs: &[(String, Vec<String>)],
) -> BTreeMap<String, usize> {
    // rel -> distinct declared specifiers. Seeded from `ts_import_pairs` (one entry per import-channel
    // file, empty maps included — that is what makes "measured 0" representable); the re-export and
    // dynamic-import pairs are collected only for files already in that set (`collect`'s
    // `if let Some(imports)` block gates all three), so no phantom file is ever added here.
    let mut per_file: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (rel, imports) in ts_import_pairs {
        let specs = per_file.entry(rel).or_default();
        for binding in imports.values() {
            specs.insert(binding.specifier.as_str());
        }
    }
    for (rel, re_exports) in ts_re_export_pairs {
        let specs = per_file.entry(rel).or_default();
        for re_export in re_exports {
            specs.insert(re_export.specifier.as_str());
        }
    }
    for (rel, dynamic) in ts_dynamic_import_pairs {
        let specs = per_file.entry(rel).or_default();
        for specifier in dynamic {
            specs.insert(specifier.as_str());
        }
    }

    let mut by_ext: BTreeMap<String, usize> = BTreeMap::new();
    for (rel, specs) in per_file {
        *by_ext.entry(ext_of(rel)).or_default() += specs.len();
    }
    by_ext
}

/// Lowercased tail after the last `.` of the last path segment; the whole name (lowercased) when there
/// is no dot. It must agree byte-for-byte with every other reader of this key or a measured extension
/// would miss its table row and read as unmeasured.
///
/// It USED to be justified as a deliberate duplicate of the facade's own copy, on the grounds that the
/// facade keeps no engine dependency in that pure-JSON post-processor. That justification stopped being
/// true on 2026-08-21: `crates/facade/src/query_coverage.rs`'s `ext_of` now delegates to
/// `zzop_engine::zero_extraction::ext_of`, so the facade holds an engine dependency here and there is
/// one owner on that side. This copy remains because it is in the engine's own assemble pass, which the
/// shared recognizer sits downstream of — a fact of the path string with no dispatch table to drift.
/// If a third reader ever appears, it delegates rather than copies; two is already one more than the
/// argument above can carry.
fn ext_of(rel: &str) -> String {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    match base.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => base.to_ascii_lowercase(),
    }
}
