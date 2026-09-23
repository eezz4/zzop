//! `module_map` — "what IS this codebase?", as DATA rather than as a picture.
//!
//! # Why this exists, measured
//! 📏 External review round 20 (ledger V226), on the `grafana` corpus checkout: `zzop graph . --domain
//! dep --fold 1` is **2,183 bytes** — 24 boxes and 16 edges — and it answers "what is this codebase"
//! better than the default analyze reply does. That reply is **33,410 bytes** over MCP for the same
//! tree and carries no module map, no entry points and no route table. Round 21 measured the gap directly: sweeping an MCP
//! reply for `dep|import|module|graph` finds `coverage.declaredImportsByExt.*` and
//! `coverage.resolvedImportEdges` — per-extension COUNTS — and nothing that says what the modules are
//! or which depends on which.
//!
//! So the closest answer to the headline question was reachable only from a terminal, and this
//! product's named audience is an agent.
//!
//! # Why a shaper and not an MCP twin of the graph lane
//! The obvious move — give `zzop graph` an MCP tool — was costed and refused (2026-09-15). Un-declaring
//! that lane in `_cliOnlyLanes` is mandatory when it gains a twin, and doing it turns
//! `host_vocabulary::shared_crate_user_facing_messages_carry_no_cli_only_vocabulary` red at 📏 **25
//! sites across 8 files**: the renderers embed `--domain`, `--top` and `--fold` in their census
//! comments, which is correct for a terminal reader and unusable for a client with no argv.
//!
//! The requirement never asked for the lane. V226 asks for the MODULE MAP, and a map delivered as data
//! passes through no renderer — so the 25 sites stay true, the picture lane stays CLI-only with its
//! recorded reason intact, and an agent gets the shape it can actually use.
//!
//! # What it deliberately does NOT do
//! No mermaid, no `--top` cap and no second granularity. The fold depth is the one knob, because the
//! map's whole job is to be small enough to read: a cap would answer "which boxes" — a question only a
//! reader with the whole map can ask — and this reply IS the whole map at its own grain.
//!
//! # Where this file lives, and why it is not under `graph/`
//! Every non-test `.rs` inside a `_cliOnlyLanes` module tree is declared in
//! `docs/contracts/surface-parity.json`, and a declaration SUBTRACTS that file from the guard that
//! keeps CLI vocabulary (the dash-prefixed `--domain`, `--top`, `--fold`) off the MCP wire. This lane
//! is ON that wire, so it sits outside the tree and reads `graph`'s fold through a prose-free data
//! seam ([`crate::graph::module_fold`]) instead.
//!
//! The wire argument spelled `fold` is NOT that vocabulary and does not collide with it: the guard's
//! needle is the argv token `--fold`, and this reply's key carries no dashes. Sharing the word is
//! deliberate — `graph --domain dep --fold <n>` performs the identical collapse, and one operation
//! gets one name. It was `depth` for part of a day, until that token turned out to be GIT's flag in
//! `crates/metrics`' thin-history warning, where contract 16 cannot tell two tools apart by token.

use serde_json::{json, Value};

use crate::graph::{self, ModuleBox};

/// The module map for 1+ trees, at `fold` path segments.
///
/// `fold` is the number of leading path segments that make one module: `1` is the top-level map, `2`
/// the next grain down. It is required rather than defaulted, because an unfolded "map" is the file
/// graph, which is the thing this exists to collapse.
pub fn module_map(
    paths: &[String],
    config_path: Option<&str>,
    depth: usize,
) -> Result<String, String> {
    let folded = graph::module_fold(paths, config_path, depth)?;

    let modules: Vec<Value> = folded
        .modules
        .iter()
        .map(|m| {
            let ModuleBox {
                id,
                files,
                loc,
                in_cycle,
            } = m;
            let mut row = json!({ "id": id, "files": files, "inCycle": in_cycle });
            // `lines` ships only when something was measured, and never without saying over how many
            // of the module's files — a sum alone would be read as the module's size.
            if let Some((lines, measured)) = loc {
                row["lines"] = json!(lines);
                row["linesMeasuredOver"] = json!(measured);
            }
            row
        })
        .collect();

    let edges: Vec<Value> = folded
        .edges
        .iter()
        .map(|(from, to, file_edges)| json!({ "from": from, "to": to, "fileEdges": file_edges }))
        .collect();

    let out = json!({
        "fold": depth,
        "modules": modules,
        "edges": edges,
        "census": {
            "files": folded.files,
            "modules": folded.modules.len(),
            "fileImports": folded.file_edges,
            "moduleEdges": folded.edges.len(),
            "unfoldableModules": folded.unfoldable,
            "fileCycles": folded.file_cycles,
        },
        "meaning": crate::output::legends::folded_string("module_map.meaning"),
    });
    serde_json::to_string_pretty(&out).map_err(|e| e.to_string())
}

/// The legend, FOLDED: this constant is what the reply-legends contract document renders, and the
/// reply itself carries `legends::module_map_note` plus a pointer here.
///
/// It qualifies on the fold's own stated terms — run-invariant, interpolating nothing, and large
/// enough to profit. Measured 2026-09-15 on this engine's tree: the full text is 2,243 bytes, the
/// `fold`-1 reply is 3,032 bytes with it folded, and 4,248 with it inline — so inline it would be
/// more than half the answer. Shipping it that way would have been the exact failure `output::legends`' doc records
/// about `architecture`'s two legends, which sat unfolded for thirteen days after they qualified —
/// with the twist that the guard could not have caught it, since its population was one reply.
/// `crates/summary/tests/legend_fold.rs` now sweeps this one too.
pub(crate) const MEANING: &str =
    "The import graph of this run, collapsed so that one MODULE is the first `fold` segments of a \
     file path. This is a TOPOLOGY, never a verdict: no row says a module is badly placed, and the \
     map carries no severity, no score and no ranking.\n\n\
     READ THE TWO EDGE NUMBERS AS A PAIR, because they count different populations and the smaller \
     one is not a subset failure. `edges[]` holds only edges BETWEEN modules — an import from one \
     file to another inside the SAME module is what a module IS, not a relation between modules, so \
     it is dropped here. `census.fileImports` counts ALL file-level imports between analyzed files, \
     the dropped ones included. Measured on this engine's own tree at `fold` 1 on 2026-09-15, \
     those two were 310 and 3322 — illustrative of the RATIO rather than a pinned constant, since \
     both move with the repo. The difference is not loss; it is the imports that stayed inside a \
     module. `edges[].fileEdges` \
     is how many file-level imports collapsed into that one module edge, so a thick edge is many \
     files agreeing rather than one strong dependency.\n\n\
     `modules[].lines` is a SUM over the module's files, and it never ships alone: \
     `linesMeasuredOver` says how many of the module's `files` that sum covers. When they differ, \
     the rest were named as import TARGETS without being analyzed, so the sum is over part of the \
     module. When nothing in the module was measured, both keys are ABSENT rather than written as \
     `0` — an unmeasured module and an empty one are different facts and must not share bytes.\n\n\
     `inCycle` marks a module holding at least one file that sits in an import cycle. The cycle may \
     be entirely INSIDE the module, which is not the same defect as a cycle between modules, and \
     this field does not tell them apart; `census.fileCycles` counts the cycles themselves. \
     `census.unfoldableModules` counts modules that are a single file shallower than `fold` — real \
     rows that could not be folded further, not rows that were lost.\n\n\
     NOTHING IS CAPPED. Every module and every edge at this depth is present, because a map with \
     rows missing is not a map, and no argument can remove one. If the result is too large to read, \
     raise `fold`: that makes the boxes bigger rather than hiding some of them, and it is the only \
     knob this answer has.";
