//! `zzop graph` — the cross-layer JOIN, serialized as a mermaid flowchart. The one rendering surface
//! this repo owns, and deliberately the thinnest possible one: zzop emits a standard graph FORMAT and
//! an external renderer (mermaid.js, Claude's inline renderer, mermaid-cli, any viewer) draws it. The
//! engine renders zero pixels and stays pure/Node-free/IO-free; nothing here reaches past the same
//! `analyzeTrees` output every sibling projection already reads.
//!
//! ## Why mermaid is the default, and what the second format cost
//! The format decision was left open when this was designed, with mermaid / DOT / graph-JSON as the
//! candidates. Mermaid wins on immediate utility (it renders inline in the main consumer's chat surface
//! with no toolchain at all) and DOT was REJECTED rather than deferred: a second format is not free —
//! it costs a `--format` flag on the CLI surface, a second emitter's tests, and a second row in every
//! doc that names this lane — while buying nothing the mermaid text cannot already do FOR THE JOIN, the
//! case that motivated the feature. That reasoning is unchanged and DOT is still rejected.
//!
//! What it did NOT settle is a domain whose graph does not fit in a drawn picture at all, and [`dep`]'s
//! module doc reserved exactly that: a mid-size repo's import graph "produces a black square, which is
//! worse than drawing nothing because it looks like information". Measured on zzop's own tree, mermaid
//! draws 40 of 1073 files. So [`cosmograph`] ships as a second serialization for that ONE domain — the
//! reserved branch arriving, not the DOT question reopening — and it pays the price listed above
//! knowingly. The bar a third format has to clear is the same one: name the case the existing formats
//! structurally cannot serve, measured rather than asserted. There is still no format-plugin framework.
//!
//! ## What a node IS (and what it is not)
//! A node is a `(sourceId, side, kind, key)` tuple — side being `provide` or `consume` — NOT a call
//! site. Twelve fetch calls to the same route in one tree collapse into ONE consume node. That is the
//! whole point of a picture, but it means **file and line are not in this output at all**; a reader who
//! needs them uses `zzop facts` (uncapped, per-site) or `zzop cross`'s `distinctBucketKeyFirstSites`
//! (one site per distinct key — the first, which is what that name now says out loud).
//!
//! ## What this CANNOT render, disclosed in the output itself
//! The graph carries the six *graph-shaped* join buckets and nothing else. Four things are structurally
//! absent, and [`mermaid::render`] prints each as a `%%` header line rather than letting a reader infer
//! completeness from a picture:
//! - **`crossLayerFindings`** — zzop's drift VERDICTS (route shadowing, body-field drift, near-miss
//!   reasons, ...). They are findings about the join, not members of it; a finding has no node identity
//!   here, and inventing one would be inventing facts the IR does not have.
//! - **`hostRekeyCounts`** — a per-host counter, not an edge.
//! - **`warnings` / `configWarnings` / `disclosure`** — prose channels; a diagram cannot carry them,
//!   and `zzop cross`/`zzop facts` already do.
//! - **items with neither `key` nor `raw`** — nothing to label a node with, so they are counted and
//!   disclosed, never guessed (the same "never guessed" rule `manifest` follows for the same case).
//!
//! ## Scoping and truncation
//! A large join makes an unreadable picture, so this surface is SCOPED by construction: `--top` caps
//! DRAWN RELATIONS per bucket (default [`DEFAULT_GRAPH_TOP`] — deduped first, see `collect`'s module
//! doc for why capping raw rows described a picture the reader was not looking at) and `--scope` keeps
//! only rows whose source id or one of whose site paths starts with a given prefix. Per-bucket rather
//! than per-document on purpose — one global cap would let a whole bucket vanish behind a big `edges`
//! list, and a diagram that silently omits a bucket is exactly the failure this project forbids. Every
//! cap and filter announces itself twice: in the `%%` header (drawn / in-scope / total relations plus
//! the call-site count they aggregate, per bucket) and as a VISIBLE note node, so the disclosure
//! survives into the rendered picture where a comment does not.
//!
//! ## Determinism
//! Byte-stable for the same input and options. Nodes live in a `BTreeMap` and edges in a `BTreeSet`
//! keyed by their full identity, so emission order is the sorted order; node ids (`n0`, `n1`, ...) are
//! assigned AFTER that sort, which is also what keeps arbitrary key text out of mermaid identifiers.
//! No `HashMap` iteration reaches this output.

pub mod cochange;
mod collect;
mod cosmograph;
pub mod dep;
mod fold;
mod mermaid;
mod model;
pub mod posture;
pub mod risk;
#[cfg(test)]
mod tests;
mod vocabulary;

use model::Graph;
pub use vocabulary::{GraphDomain, GraphFormat};

/// Default per-bucket row cap. Small on purpose: the deliverable is a picture a human reads at a
/// glance, and the disclosure tells them exactly how much was left out and how to see more. There is
/// no UPPER bound on `--top` — this is a file/pipe surface like `facts` and `manifest`, not the
/// cap-governed MCP wire.
pub const DEFAULT_GRAPH_TOP: usize = 25;

/// `zzop graph <path>... | zzop graph --config <path>` — same three source modes as `facts`/`endpoint`
/// (one path, 2+ paths, or a config), because the join is meaningful over a single tree too.
pub fn graph_mermaid(
    paths: &[String],
    config_path: Option<&str>,
    scope: Option<&str>,
    top: Option<usize>,
    domain: GraphDomain,
    fold: Option<usize>,
) -> Result<String, String> {
    let v = analyze(paths, config_path)?;
    // Each domain owns its own default cap — see `GraphDomain::default_top`, which is also what the
    // help text reads, so the number a caller is told is the number they get.
    let top = top.unwrap_or(domain.default_top());
    // A judgment domain never reaches a fold: the CLI refuses the flag for it (see
    // [`GraphDomain::accepts_fold`]), and quietly honouring it here would recreate the silently-ignored
    // knob that refusal exists to prevent.
    let fold = fold::Fold::of(fold.filter(|_| domain.accepts_fold()));
    Ok(match domain {
        GraphDomain::Join => project(&v, scope, top),
        GraphDomain::Dep => dep::project(&v, scope, top, fold),
        GraphDomain::Risk => risk::project(&v, scope, top),
        GraphDomain::Posture => posture::project(&v, scope, top),
        GraphDomain::CoChange => cochange::project(&v, scope, top, fold),
    })
}

/// The analysis half, shared by every format so a second serialization cannot become a second answer:
/// both lanes read the SAME `analyzeTrees` output through the same three source modes.
fn analyze(paths: &[String], config_path: Option<&str>) -> Result<serde_json::Value, String> {
    let (path, rest) = match paths {
        [one] => (Some(one.as_str()), &paths[..0]),
        many => (None, many),
    };
    let loaded = zzop_config::trees::resolve_trees_request("graph", path, rest, config_path)?;
    let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
    serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())
}

/// A cosmograph table plus the census the CLI prints on stderr. Two fields rather than one string
/// because they go to two different streams ON PURPOSE — stdout is a data table a viewer parses, and a
/// prose line in it would be a corrupt row. See [`cosmograph`]'s module doc.
pub struct CosmographOutput {
    /// NDJSON, one record per line. stdout.
    pub data: String,
    /// One human-readable line. stderr.
    pub census: String,
}

/// `zzop graph --format cosmograph-nodes|cosmograph-links` — the DEPENDENCY domain as NDJSON tables for
/// an interactive viewer.
///
/// Takes no `GraphDomain` and no `top`, both deliberately. The domain is fixed because `dep` is the only
/// one whose picture mermaid actually fails to draw (its own module doc: a mid-size repo's import graph
/// "produces a black square, which is worse than drawing nothing"), and the other three are tens of
/// nodes where a flowchart is strictly better. Building the other three now would be speculative — the
/// bar `projection-contract.md` sets for a new fact, applied to a new format. There is no `top` because
/// this lane is uncapped: capping exists to keep a DRAWN picture readable and a viewer with zoom does
/// that itself. Both omissions are structural rather than validated, so neither can be got wrong by a
/// caller.
pub fn graph_cosmograph(
    paths: &[String],
    config_path: Option<&str>,
    scope: Option<&str>,
    links: bool,
) -> Result<CosmographOutput, String> {
    let v = analyze(paths, config_path)?;
    let universe = dep::collect(&v);
    let (data, census) = if links {
        cosmograph::links_ndjson(&universe, scope)
    } else {
        cosmograph::nodes_ndjson(&universe, scope)
    };
    Ok(CosmographOutput {
        data,
        census: census.render(),
    })
}

/// The pure projection `analyzeTrees` output -> mermaid text. Split out from the analysis call so the
/// whole format is unit-testable from a literal engine output with no filesystem, exactly like
/// `facts`/`manifest` do it.
fn project(v: &serde_json::Value, scope: Option<&str>, top: usize) -> String {
    let mut g = Graph::default();
    let empty = Vec::new();
    for t in v["trees"].as_array().unwrap_or(&empty) {
        let Some(source) = t["sourceId"].as_str() else {
            continue;
        };
        let zero = t["output"]["coverage"]["joinContributionZero"]
            .as_bool()
            .unwrap_or(false);
        g.sources.insert(source.to_string(), zero);
    }

    let cl = &v["crossLayer"];
    let mut counts = vec![collect::collect_edges(&mut g, &cl["edges"], scope, top)];
    for bucket in crate::output::KEY_BUCKETS {
        counts.push(collect::collect_bucket(
            &mut g,
            bucket,
            &cl[bucket],
            scope,
            top,
        ));
    }
    mermaid::render(&g, &counts, scope, top)
}
