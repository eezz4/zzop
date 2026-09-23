//! "Does this handler reach a guard?" — the whole evidence side of the rule, in one place.
//!
//! Two kinds of evidence answer that question and they must answer it with ONE vocabulary: a symbol id
//! the BFS visited (an edge the resolver drew) and the written name of a call it could not place. Keeping
//! both behind [`GuardReach`] is what makes a second spelling impossible — a guard name that clears a
//! route through an edge clears it through a dropped call too, and vice versa.

use std::cell::RefCell;
use std::collections::HashMap;

use zzop_core::callgraph::{bfs_reachable_in, Adjacency};

use super::vocab::vocab_re;
use super::{qualifier, ScanMutatingRouteNoAuthInput};

pub(super) struct GuardReach<'a> {
    input: &'a ScanMutatingRouteNoAuthInput<'a>,
    name_index: &'a HashMap<String, Vec<String>>,
    guard_re: Option<regex::Regex>,
    /// Memoizes the per-handler BFS across every mutating endpoint sharing a handler symbol.
    memo: RefCell<HashMap<String, bool>>,
    /// The graph's adjacency index, built once here rather than inside every traversal. The memo
    /// above already collapses REPEATED handlers; this collapses the setup cost for the DISTINCT
    /// ones, which the memo never could (review ledger V112).
    adjacency: Adjacency<'a>,
}

impl<'a> GuardReach<'a> {
    pub(super) fn new(
        input: &'a ScanMutatingRouteNoAuthInput<'a>,
        name_index: &'a HashMap<String, Vec<String>>,
    ) -> Self {
        Self {
            input,
            name_index,
            guard_re: vocab_re(input.auth_guard_pattern),
            memo: RefCell::new(HashMap::new()),
            adjacency: Adjacency::build(input.symbol_graph),
        }
    }

    /// True when the handler reaches auth evidence of either kind. The unresolved-name check rides the
    /// SAME traversal rather than a second one: `bfs_reachable` applies its predicate to every reachable
    /// node including the start, which is exactly the set whose dropped calls could be the guard.
    pub(super) fn reaches(&self, handler_symbol: &str) -> bool {
        if let Some(hit) = self.memo.borrow().get(handler_symbol) {
            return *hit;
        }
        let found = bfs_reachable_in(&self.adjacency, handler_symbol, |id| {
            self.is_guard_id(id) || self.calls_unresolved_guard(id)
        })
        .is_some();
        self.memo
            .borrow_mut()
            .insert(handler_symbol.to_string(), found);
        found
    }

    /// The two trailing segments of a visited id, each by its own matcher — module doc of the parent,
    /// "Match granularity".
    fn is_guard_id(&self, id: &str) -> bool {
        let mut seg = id.rsplit(['#', '.']);
        let tail = seg.next().unwrap_or(id);
        self.matches_guard_name(tail)
            || seg.next().is_some_and(|q| {
                qualifier::is_guard(q, self.name_index, self.input.qualifier_guard_tokens)
            })
    }

    /// A call the resolver dropped, judged by the name it is WRITTEN with. Only ever clears a finding;
    /// see [`ScanMutatingRouteNoAuthInput::unresolved_callees`].
    fn calls_unresolved_guard(&self, id: &str) -> bool {
        self.input
            .unresolved_callees
            .get(id)
            .is_some_and(|names| names.iter().any(|n| self.matches_guard_name(n)))
    }

    fn matches_guard_name(&self, name: &str) -> bool {
        self.guard_re.as_ref().is_some_and(|re| re.is_match(name))
    }

    /// The dropped names for one handler that did NOT match — the residue after [`Self::reaches`] has
    /// had its say. It rides the finding because it is the fastest way to dismiss a false positive this
    /// rule can still produce: a guard the project spells differently from its own declared pattern is
    /// visible here in one glance, where the reader would otherwise have to work out why the graph is
    /// short an edge.
    pub(super) fn unresolved_residue(&self, handler_symbol: &str) -> Vec<&'a str> {
        self.input
            .unresolved_callees
            .get(handler_symbol)
            .map(|names| names.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }
}
