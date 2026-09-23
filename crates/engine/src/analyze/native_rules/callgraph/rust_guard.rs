//! The Rust arm of `run_callgraph_rules`' second pass — the fourth language to feed the shared
//! `SymbolGraph`, and the one whose guard evidence arrives as graph EDGES rather than a side-channel.
//!
//! ## Two producers, one merge — both now in the per-file lane
//! Every `.rs` file contributes twice:
//! - `parse_calls` — real call sites, exactly like the TS/Java/Python loops.
//! - `parse_extractor_guards` — one edge per handler-parameter TYPE, because that is how Rust web
//!   frameworks express auth (`async fn create(user: AuthUser, ..)`). That producer's own doc carries the
//!   corpus measurement behind the claim and the veto it applies; here the only thing worth repeating is
//!   WHY it needs no `decorator_guarded` entry: the evidence already sits on the handler symbol, so the
//!   BFS walks to it without help.
//!
//! ## Resolution: real, but crate-local
//! `zzop_parser_rust::rust_import_candidates` is the same in-tree module-path resolver the dep graph
//! uses, so `crate::`/`super::`/`self::` specifiers resolve to real files — better than Java's
//! opaque-specifier stand-in. `RustWorkspaceMap` extends it across same-workspace crates
//! (`zzop_core::x` -> `crates/core/src/lib.rs`), which is what makes zzop's own tree — the anchor corpus
//! D18 chose — resolve as one graph rather than sixteen disconnected ones.
//!
//! An EXTERNAL crate head (`serde::`, `tokio::`) resolves to nothing and its edge is dropped, never
//! guessed. Same single-hop limitation the sibling loops declare: a target id nothing else has outgoing
//! edges from ends the walk there.
//!
//! ## Nothing is read from disk here any more
//! Rust rides the shared, TS-named `ts_paths` set and its `ImportMap` already rides `ts_import_pairs`
//! (`pipeline::fresh`'s `ts_slot` accepts `Language::Rust`), so imports never needed a re-parse. The
//! CALL SITES did, on this module's own loop, until 2026-09-08 — and that loop turned out to be the
//! single largest term in the whole call-graph pass: **4.4-5.8s of a ~7.5s warm run** on a repository
//! with 1,395 `.rs` files, paid again on every warm run (review ledger V111). Both producers moved
//! to `pipeline::fresh::call_graph`, where the parse has already happened and the result is cached.
//!
//! What is left in this module is the half that CANNOT move: `resolve_rust_call_target` answers a
//! whole-tree question (which file does this path name, across workspace members), and a per-file lane
//! has neither the workspace map nor the path set to answer it.

use std::collections::HashSet;

use crate::pipeline::RustWorkspaceMap;

/// A Rust call's cross-file target file — see this module's "Resolution" doc. `None` (edge dropped) for
/// an external-crate head that no workspace member answers to.
pub(super) fn resolve_rust_call_target(
    specifier: &str,
    from_file: &str,
    ts_paths: &HashSet<String>,
    workspace: &RustWorkspaceMap,
) -> Option<String> {
    crate::analyze::assemble::helpers::resolve_rust_import(
        specifier, from_file, ts_paths, workspace,
    )
}
