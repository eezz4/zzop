//! The Rust arm of `run_callgraph_rules`' second pass — the fourth language to feed the shared
//! `SymbolGraph`, and the one whose guard evidence arrives as graph EDGES rather than a side-channel.
//!
//! ## Two producers, one merge
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
//! ## Imports come from the shared pairs, not a re-parse
//! Rust rides the shared, TS-named `ts_paths` set and its `ImportMap` already rides `ts_import_pairs`
//! (`pipeline::fresh`'s `ts_slot` accepts `Language::Rust`), so — like Python and unlike Java — only the
//! call sites need re-reading here.

use std::collections::HashSet;

use zzop_core::callgraph::RawCall;

use crate::pipeline::RustWorkspaceMap;

/// Re-parses every Rust-dispatched file's call sites AND handler signatures off disk, extending
/// `raw_calls` in place. `rels` is sorted so the merge is independent of `ts_paths`' hash iteration
/// order.
pub(super) fn parse_calls_and_guards(
    root: &std::path::Path,
    ts_paths: &HashSet<String>,
    vocab: &zzop_parser_rust::RustGuardVocab<'_>,
    raw_calls: &mut Vec<RawCall>,
) {
    let mut rels: Vec<&String> = ts_paths
        .iter()
        .filter(|rel| crate::analyze::assemble::helpers::is_rust_source_ext(rel))
        .collect();
    rels.sort();
    for rel in rels {
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            raw_calls.extend(zzop_parser_rust::parse_calls(rel, &text));
            raw_calls.extend(zzop_parser_rust::parse_extractor_guards(rel, &text, vocab));
        }
    }
}

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
