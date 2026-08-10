//! Rust import-specifier resolution — the crate-name/module-path half of `super`.
//!
//! Split out on 2026-07-29 (line cap) as one topic: everything here answers "what file does this Rust
//! `use` name?", including the cross-crate re-anchoring D46 added, which is the part with real design
//! content rather than a one-line predicate.

use std::collections::HashSet;

use crate::pipeline::RustWorkspaceMap;

/// Rust standard/compiler-provided crate family — never a genuinely external (third-party) package, so a
/// `use std::...`/`use core::...`/... head is excluded from the package-import census entirely (task 4's
/// "exclude the std family from the census" requirement), the same way a Python relative specifier
/// (`starts_with('.')`) never even reaches `resolve_python_import`.
pub(in crate::analyze) const RUST_STD_CRATE_FAMILY: &[&str] =
    &["std", "core", "alloc", "proc_macro", "test"];

/// The first `::`-segment of a Rust import specifier (`"crate::a::b"` -> `"crate"`, `"serde::Deserialize"`
/// -> `"serde"`, a bare single-segment specifier -> itself unchanged).
pub(in crate::analyze) fn rust_head(specifier: &str) -> &str {
    specifier.split("::").next().unwrap_or(specifier)
}

/// Rust import-specifier resolution glue — the Rust-side counterpart of `resolve_python_import`, unifying
/// TWO resolution paths behind one call: `zzop_parser_rust::rust_import_candidates` (pure, in-tree
/// `crate::`/`super::`/`self::` module-path resolution — returns an empty candidate list for any other
/// head, including a bare external one) tried first, then — only reached when the first path yields
/// nothing, which is always true for an external head since `rust_import_candidates` itself never
/// resolves one — a same-workspace crate lookup via `workspace` (the "dogfooding payoff": an external head
/// like `zzop_core` resolving into `crates/core/`). Both candidate lists are checked against `all_paths`,
/// first-present-wins, mirroring `resolve_python_import`'s own convention. Called from BOTH
/// [`super::dep_graph::merge_rust_dep_edges`] (dep-graph edges) and the router-mount compose resolver
/// closure in `super::provides` (cross-file `.nest()`/`.merge()` mounts) — same dual-call shape
/// `resolve_python_import`'s own doc describes for its two call sites.
///
/// ## The cross-crate edge lands on the MODULE, not the crate root (2026-07-29)
/// A same-workspace import used to resolve to the target crate's `src/lib.rs` and stop there, so every
/// boundary-crossing edge in a workspace pointed at one file per crate. Measured on this repo: 279
/// boundary edges, module-file targets ZERO. The edges existed — the earlier claim that they did not was
/// stale — but they pointed at the wrong place, and `circular`/`unreachable`/blast-radius are all computed
/// over that graph, so the numbers were coarse rather than absent.
///
/// The fix reuses the resolver already here instead of adding a second one: `zzop_core::io::link` is
/// rewritten to `crate::io::link` and re-resolved with the target crate's own root as `from_file`, which
/// is exactly what `rust_import_candidates` does for an intra-crate path. It handles the item-vs-module
/// tail on its own (`zzop_core::Finding` finds no `src/Finding.rs`, falls to the crate root, and is
/// correct there).
///
/// **Residual, deliberate: a RE-EXPORTED symbol still lands on the crate root.** `use zzop_core::Finding`
/// resolves to `crates/core/src/lib.rs` even though the type is defined in `src/finding.rs`, because
/// following it would mean reading the target crate's `pub use` graph — a second resolver over facts this
/// pass does not hold. The edge is not wrong (the root really does re-export it), it is coarse, and a
/// crate whose public surface is entirely re-exported is unchanged by this work. Chasing it properly needs
/// item-level resolution, which is rust-analyzer territory and is refused for the same isolation/cost
/// reason the parser layer refuses every other compiler front end.
pub(in crate::analyze) fn resolve_rust_import(
    specifier: &str,
    from_file: &str,
    all_paths: &HashSet<String>,
    workspace: &RustWorkspaceMap,
) -> Option<String> {
    let candidates =
        zzop_parser_rust::rust_import_candidates(specifier, from_file, workspace.target_roots());
    if let Some(hit) = candidates.into_iter().find(|c| all_paths.contains(c)) {
        return Some(hit);
    }
    let head = rust_head(specifier);
    // `#path` joins the three keyword heads here for the same reason they are here: all four name a
    // location INSIDE this tree, so when the candidate check above found nothing, the honest answer is
    // "no edge" — not "then it must be an external crate". Without this arm a `#[path]` whose target is
    // outside the walked tree would fall through and resolve to a workspace crate root, inventing an
    // edge to a file the declaration never named.
    if matches!(head, "crate" | "super" | "self") || head == zzop_parser_rust::PATH_ATTR_HEAD {
        return None;
    }
    let crate_root = workspace
        .crate_roots(head)?
        .iter()
        .find(|c| all_paths.contains(c.as_str()))?;

    // Re-anchor at the target crate and resolve the REST of the path there. `crate::` is the same head
    // `rust_import_candidates` already understands, and its `from_file` only matters through the rightmost
    // `/src/` segment — which `crate_root` carries by construction (`scan_rust_workspace` builds
    // `<dir>/src/lib.rs`).
    let rest = specifier.strip_prefix(head)?.trim_start_matches(':');
    if !rest.is_empty() {
        let inner = zzop_parser_rust::rust_import_candidates(
            &format!("crate::{rest}"),
            crate_root,
            workspace.target_roots(),
        );
        if let Some(hit) = inner.into_iter().find(|c| all_paths.contains(c)) {
            return Some(hit);
        }
    }
    // A bare `use some_crate;`, or a path whose tail names a re-exported item — the crate root is the
    // honest answer for both. See the residual note above.
    Some(crate_root.clone())
}
