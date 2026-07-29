//! What ONE file in the dependency graph carries, and where each axis comes from.
//!
//! Split out of `dep.rs` when the node stopped being a `(sourceId, rel)` tuple: the cosmograph lane
//! styles nodes by MEASURED axes — size, then history — and a tuple has nowhere to put them, so a
//! reader of a thousand-node picture was getting topology and nothing else. Reading those axes out of
//! the run's own output is a different job from assembling the graph, and it is the job with all the
//! "is this number real?" reasoning in it, so it lives here.
//!
//! # The rule every axis in this file obeys
//! An axis this run did not measure is `None`, and `None` is NOT `0`. The emitter omits the column
//! entirely for `None` rather than writing a zero, because `loc: 0` spells "this file is empty" in the
//! same bytes as "nothing measured this file" and `churn: 0` spells "never changed" in the same bytes
//! as "nobody looked" — and a viewer styling by either would draw the second as the first. That is the
//! silent-failure shape this repo refuses everywhere else (a parser that produces no fact is a matrix
//! blank, not a `0`).

use std::collections::BTreeMap;

use serde_json::Value;

/// One file in the graph.
#[derive(Clone)]
pub(in crate::graph) struct DepNode {
    pub(in crate::graph) source: String,
    /// Tree-relative path.
    pub(in crate::graph) rel: String,
    /// Lines of code, from the run's own `ir.loc`. `None` when this run carried no measurement for the
    /// file — which happens for real: `ir.dep` names every import TARGET, including ones outside the
    /// scanned file set, and those have no LOC to report.
    pub(in crate::graph) loc: Option<u32>,
    /// This file's git history, from the run's own `output.nodes[]`. `None` when git collection did not
    /// run for this tree, and ALSO when it ran but produced no row for this file — two different
    /// reasons, one honest outcome.
    pub(in crate::graph) git: Option<GitAxes>,
}

/// The history axes a viewer colours by. Grouped rather than flattened into [`DepNode`] because they
/// arrive and vanish TOGETHER: git either answered for a file or it did not, and four independently
/// nullable fields would let a row claim a churn without a change count.
#[derive(Clone)]
pub(in crate::graph) struct GitAxes {
    pub(in crate::graph) change_count: u32,
    pub(in crate::graph) churn: u32,
    pub(in crate::graph) author_count: u32,
    /// ISO date, `None` when git tracked the file but reported no date for it. A null date is not a
    /// date, so it is omitted rather than emitted as `null` — same rule as the axes above.
    pub(in crate::graph) last_modified: Option<String>,
}

/// One tree's measured axes, read once and then asked per file. Per TREE because both gates are: a run
/// can analyze several trees and collect git for only some of them.
pub(in crate::graph) struct TreeAxes<'a> {
    /// `ir.loc` — a rel -> lines object. A file it does not name is one this run did not measure.
    loc: &'a Value,
    git: BTreeMap<&'a str, GitAxes>,
}

impl<'a> TreeAxes<'a> {
    pub(in crate::graph) fn of(tree: &'a Value) -> Self {
        TreeAxes {
            loc: &tree["output"]["ir"]["loc"],
            git: git_axes_by_path(tree),
        }
    }

    /// One file's node — `source` is the tree's `sourceId`, `rel` its tree-relative path.
    pub(in crate::graph) fn node(&self, source: &str, rel: &str) -> DepNode {
        DepNode {
            source: source.to_string(),
            rel: rel.to_string(),
            loc: self.loc.get(rel).and_then(Value::as_u64).map(|n| n as u32),
            git: self.git.get(rel).cloned(),
        }
    }
}

/// One tree's per-file git history, keyed by tree-relative path — EMPTY when git collection did not run
/// for that tree.
///
/// The gate is `output.gitWindow`, and it has to be. `output.nodes[]` is built unconditionally, and its
/// `changeCount`/`churn`/`authorCount` are plain `u32`s that default to `0` when there is no history to
/// read (`zzop_core::file_nodes`'s `build_one`). So on a run with git off, every node on the wire
/// carries a perfectly-formed `changeCount: 0` — indistinguishable, field by field, from a file nobody
/// has ever touched. `gitWindow` is `null` exactly when that phase did not run — `zzop_facade`'s
/// `AnalyzeOutputView` calls that null "the honest 'git didn't run' signal" — which makes it the only
/// place the distinction survives. Reading the fields without the gate would paint an unmeasured repo
/// as a frozen one.
fn git_axes_by_path(tree: &Value) -> BTreeMap<&str, GitAxes> {
    if tree["output"]["gitWindow"].is_null() {
        return BTreeMap::new();
    }
    // A `&'static []` rather than a local `Vec`: the keys returned borrow from `tree`, so the fallback
    // must not be owned by this frame.
    let nodes: &[Value] = tree["output"]["nodes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut out = BTreeMap::new();
    for n in nodes {
        let Some(path) = n["path"].as_str() else {
            continue;
        };
        let u32_at = |k: &str| n[k].as_u64().unwrap_or(0) as u32;
        out.insert(
            path,
            GitAxes {
                change_count: u32_at("changeCount"),
                churn: u32_at("churn"),
                author_count: u32_at("authorCount"),
                last_modified: n["lastModified"].as_str().map(str::to_string),
            },
        );
    }
    out
}
