//! The JOIN domain's node/edge model — the shape `collect` fills and `mermaid` draws. Split out of
//! `mod.rs` for the 300-line source cap when the cosmograph lane pushed it over; the three files stay
//! one module in every other sense, which is why these items are private to `graph` rather than public.
//!
//! Note the asymmetry with EVERY other domain (`GraphDomain`'s non-`Join` variants — today
//! `dep`/`risk`/`posture`/`cochange`): each owns its own model because its nodes are files, folders,
//! routes or co-changed pairs rather than io keys. This file is the JOIN's model specifically, not a
//! shared one — a shared graph type across domains whose nodes mean different things would be a type
//! that means nothing. The domain COUNT is not written here: this doc said "the other three" and "four
//! domains" after a fifth variant landed (2026-09-12, ledger V168), and the enum is the only roster
//! that cannot fall behind itself.

use std::collections::{BTreeMap, BTreeSet};

/// Node side. Part of the node's identity, so a tree that both provides and consumes the same key gets
/// two nodes and a visible self-edge instead of one node whose meaning depends on which arrow you read.
pub(super) const PROVIDE: &str = "provide";
pub(super) const CONSUME: &str = "consume";

/// A node's identity: `(sourceId, side, kind, key)`. Sorted as a tuple, which is the whole ordering
/// contract — see the module doc's determinism note.
pub(super) type NodeKey = (String, &'static str, String, String);

/// One rendered relation. `dotted` marks a NON-authoritative arrow (an ambiguous candidate, or a
/// low-confidence key match) so the picture never draws a guess the same way it draws a resolved join.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct GraphEdge {
    pub(super) from: NodeKey,
    pub(super) to: NodeKey,
    pub(super) dotted: bool,
    pub(super) label: Option<String>,
}

/// Per-bucket honesty row. `rows` is the bucket's raw length (CALL SITES); the other three count
/// distinct drawable RELATIONS — how many exist, how many survived `--scope`, how many were drawn.
/// Both scales are published, because a node aggregates sites and a census that reported only one of
/// the two would misdescribe the picture (see `collect`'s own module doc for the measurement that
/// forced this). `unlabelable` is the remainder that could not be labelled at all.
pub(super) struct BucketCount {
    pub(super) bucket: &'static str,
    pub(super) rows: usize,
    pub(super) total: usize,
    pub(super) in_scope: usize,
    pub(super) shown: usize,
    pub(super) unlabelable: usize,
}

/// The whole drawable graph plus its disclosure. Deliberately not public: the mermaid text is this
/// module's product, and a second consumer of the model would be the beginning of the format-plugin
/// framework the module doc rejects.
#[derive(Default)]
pub(super) struct Graph {
    /// `sourceId -> joinContributionZero`. Every analyzed tree appears, including one that contributed
    /// nothing — an absent subgraph would read as "this tree is fine", when the honest reading is "this
    /// tree extracted no joinable io and is invisible to the join".
    pub(super) sources: BTreeMap<String, bool>,
    /// Node -> role tag. Role is also the bucket name, so the label carries the verdict in text and the
    /// `classDef` carries it in colour; a renderer that ignores styling still tells the truth.
    pub(super) nodes: BTreeMap<NodeKey, &'static str>,
    pub(super) edges: BTreeSet<GraphEdge>,
}

impl Graph {
    /// Records a node, keeping the STRONGEST role when one node is reached twice. The five non-edge
    /// buckets are disjoint by construction (`CrossLayerResult`'s own contract), so this is a
    /// tie-breaker for a state the engine does not produce today — present so the output stays
    /// deterministic if it ever does, rather than depending on visit order.
    pub(super) fn node(
        &mut self,
        source: &str,
        side: &'static str,
        kind: &str,
        key: &str,
        role: &'static str,
    ) {
        let node = (source.to_string(), side, kind.to_string(), key.to_string());
        let entry = self.nodes.entry(node).or_insert(role);
        if role_rank(role) < role_rank(entry) {
            *entry = role;
        }
    }
}

/// Role precedence, lowest = strongest. Only reachable through [`Graph::node`]'s tie-breaker.
pub(super) fn role_rank(role: &str) -> u8 {
    match role {
        "linked" => 0,
        "candidate" => 1,
        "unconsumed" => 2,
        "unprovided" => 3,
        "ambiguous" => 4,
        "unresolved" => 5,
        _ => 6,
    }
}
