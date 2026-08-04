//! The Mode A call-graph pass's disclosure trio — every way `run_envelope_callgraph` can see less
//! than a producer would assume, each named on `warnings` (recall-direction degrades must never be
//! silent — the projection contract's own stance). Split from `callgraph.rs` for the repo line cap;
//! same wording, same callers.
//!
//! - [`absence_warning`] — the channel itself is missing while http routes exist.
//! - [`dropped_calls_warning`] — the channel was supplied but resolution dropped edges (the drop is
//!   the resolver's contract — never guess — the DISCLOSURE is this function's job; before it, "we
//!   analyzed the graph" and "the whole graph evaporated" were indistinguishable in the output).
//! - [`uncovered_extension_warning`] — edges exist but `mutating-route-no-auth`'s covered-extension
//!   gate exempts some routes regardless.

use std::collections::BTreeMap;

/// The three call-graph-BFS rule ids this pass can light up, spelled once — the disclosures quote
/// them, and `tests` pins each against the shipped registry so a disclosure can never name a ghost.
pub(super) const CALL_GRAPH_RULE_IDS: [&str; 3] = [
    "mutating-route-no-auth",
    "unsafe-read-endpoint",
    "non-idempotent-write",
];

/// Cap on example files named per drop disclosure — the same "up to 3 example paths" convention the
/// sibling tripwires (`framework_silence`, the overlay censuses) use.
const MAX_DROP_EXAMPLES: usize = 3;

/// `Some(warning)` when this envelope carries http route provides but an EMPTY `calls` channel — the
/// recall-direction degrade the projection contract requires to be named: the call-graph rules did not
/// look, and a zero from them means NOT ANALYZED, never "no risky route". `None` when there are no
/// http routes at all (nothing those rules would have judged) — an unconditional warning would train
/// readers to ignore it.
pub(super) fn absence_warning(io_provides: &[zzop_core::IoProvide]) -> Option<String> {
    let http_routes = io_provides.iter().filter(|p| p.kind == "http").count();
    if http_routes == 0 {
        return None;
    }
    Some(format!(
        "Envelope call-graph gap: this envelope carries {http_routes} http route(s) but no `calls` \
         channel, so the call-graph rules ({}) have no edges to walk and stay silent — recall, not \
         cleanliness. To turn them on, have the producer emit `files[].calls` (call sites attributed \
         to their enclosing symbol — see docs/NORMALIZED_AST.md's calls section); `unsafe-read-endpoint`/\
         `non-idempotent-write` additionally need `symbols[].writeSites` evidence. Route-level guard \
         knowledge the graph can't express can be injected as an `auth-guarded` attribute instead.",
        CALL_GRAPH_RULE_IDS.join(", "),
    ))
}

/// `Some(warning)` when resolution dropped supplied `calls` edges — counted per file as
/// `supplied - resolved`, which is exact because `resolve_calls_for_file` maps each `RawCall` to 0
/// or 1 edge and the boundary validator rejects a `#` in a calls-carrying path (so an edge's
/// `from`-file prefix always names its supplying projection). There is one drop reason by the
/// resolver's contract: the callee resolved through neither the file's `imports` nor any declared
/// symbol, so the edge was dropped, never guessed. `None` when every supplied edge resolved. Total
/// evaporation (supplied > 0, resolved == 0) gets the gap-grade phrasing: the rules then walked an
/// EMPTY graph, and their zero is recall, not cleanliness.
pub(super) fn dropped_calls_warning(
    files: &[&zzop_core::FileProjection],
    symbol_graph: &zzop_core::callgraph::SymbolGraph,
) -> Option<String> {
    let mut supplied_by_file: BTreeMap<&str, usize> = BTreeMap::new();
    for f in files.iter().filter(|f| !f.calls.is_empty()) {
        supplied_by_file.insert(f.path.as_str(), f.calls.len());
    }
    let mut resolved_by_file: BTreeMap<&str, usize> = BTreeMap::new();
    for edge in symbol_graph {
        let file = edge.from.split('#').next().unwrap_or(edge.from.as_str());
        *resolved_by_file.entry(file).or_insert(0) += 1;
    }
    let supplied: usize = supplied_by_file.values().sum();
    let resolved: usize = symbol_graph.len();
    let dropped = supplied.saturating_sub(resolved);
    if dropped == 0 {
        return None;
    }

    // (file, dropped-of-supplied) for every file that lost at least one edge, in path order.
    let per_file: Vec<(&str, usize, usize)> = supplied_by_file
        .iter()
        .filter_map(|(file, &n)| {
            let kept = resolved_by_file.get(file).copied().unwrap_or(0);
            (kept < n).then_some((*file, n - kept, n))
        })
        .collect();
    let examples: Vec<String> = per_file
        .iter()
        .take(MAX_DROP_EXAMPLES)
        .map(|(file, d, n)| format!("{file} ({d} of {n})"))
        .collect();
    let more = per_file.len().saturating_sub(examples.len());
    let more_note = if more > 0 {
        format!(", +{more} more file(s)")
    } else {
        String::new()
    };

    let tail = if resolved == 0 {
        format!(
            "Every supplied edge evaporated: the call-graph rules ({}) walked an EMPTY graph, so \
             their zeroes here are recall, not cleanliness — fix the callee names/imports, or \
             declare the callee symbols, to give them real edges.",
            CALL_GRAPH_RULE_IDS.join(", "),
        )
    } else {
        format!("The rules walked the {resolved} edge(s) that did resolve.")
    };
    Some(format!(
        "Envelope call-graph drop: {dropped} of {supplied} supplied `calls` edge(s) resolved to \
         nothing — each dropped callee matched neither its file's `imports` nor any symbol declared \
         in this envelope, so its edge was dropped, never guessed (the resolver's contract). \
         Dropped in: {}{more_note}. {tail}",
        examples.join(", "),
    ))
}

/// `Some(warning)` when calls WERE supplied but some http routes live in files whose extension is
/// outside `zzop_rules_http`'s `CALL_GRAPH_COVERED_EXTENSIONS` — `mutating-route-no-auth`'s own
/// candidate gate exempts those routes today regardless of supplied edges (the gate predates this
/// channel), so a producer must not read that rule's silence on them as a verdict. The covered set is
/// rendered from the rules crate's constant, never a copy. The two write-site scanners are NOT
/// extension-gated and do honor supplied calls for any language.
pub(super) fn uncovered_extension_warning(io_provides: &[zzop_core::IoProvide]) -> Option<String> {
    let covered = zzop_rules_http::CALL_GRAPH_COVERED_EXTENSIONS;
    let mut uncovered: std::collections::BTreeSet<String> = Default::default();
    for p in io_provides.iter().filter(|p| p.kind == "http") {
        let Some(ext) = std::path::Path::new(&p.file)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
        else {
            continue;
        };
        if !covered.contains(&ext.as_str()) {
            uncovered.insert(ext);
        }
    }
    if uncovered.is_empty() {
        return None;
    }
    Some(format!(
        "Envelope call-graph residual: `calls` were supplied, but http routes in .{} sit outside \
         `mutating-route-no-auth`'s covered-extension set ({}), whose candidate gate exempts them \
         even with supplied edges — that rule's silence on those routes is an exemption, not a \
         verdict. `unsafe-read-endpoint`/`non-idempotent-write` are not extension-gated and do walk \
         the supplied graph. An `auth-guarded` attribute injection covers the guard question for the \
         exempted routes today.",
        uncovered.into_iter().collect::<Vec<_>>().join("/."),
        covered.join("/"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seals that every rule id the disclosures name is a rule the engine actually ships — the same
    /// "never name a ghost" pin `framework_silence::call_graph_language` keeps for S8.
    #[test]
    fn disclosed_rule_ids_are_real_shipped_rules() {
        let mut registry = zzop_core::RuleRegistry::new();
        crate::register_all_native(&mut registry);
        let ids = registry.ids();
        for id in CALL_GRAPH_RULE_IDS {
            assert!(
                ids.iter().any(|shipped| shipped == id),
                "{id} is not a shipped native rule id: {ids:?}"
            );
        }
    }

    fn provide(file: &str, key: &str) -> zzop_core::IoProvide {
        zzop_core::IoProvide {
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line: 1,
            symbol: None,
            body: None,
            response: None,
        }
    }

    /// Both sides of the absence disclosure: http routes + empty channel warns and names the channel;
    /// no routes stays silent (the same command, a non-zero and a zero).
    #[test]
    fn absence_is_disclosed_exactly_when_routes_exist() {
        let w = absence_warning(&[provide("app/routes.rb", "POST /users")])
            .expect("routes + no calls must be disclosed");
        assert!(w.contains("no `calls` channel"), "{w}");
        assert!(w.contains("mutating-route-no-auth"), "{w}");
        assert!(w.contains("files[].calls"), "{w}");
        assert!(absence_warning(&[]).is_none());
    }

    /// Both sides of the residual disclosure: an uncovered-extension route is named (set rendered from
    /// the rules crate's constant), a covered-extension route is not.
    #[test]
    fn uncovered_extension_residual_is_disclosed_exactly_when_present() {
        let w = uncovered_extension_warning(&[provide("app/routes.rb", "POST /users")])
            .expect("uncovered-extension route must be disclosed");
        assert!(w.contains(".rb"), "{w}");
        assert!(
            w.contains(&zzop_rules_http::CALL_GRAPH_COVERED_EXTENSIONS.join("/")),
            "{w}"
        );
        assert!(uncovered_extension_warning(&[provide("src/routes.ts", "POST /users")]).is_none());
    }

    fn file_with_calls(path: &str, callees: &[&str]) -> zzop_core::FileProjection {
        let mut file: zzop_core::FileProjection =
            serde_json::from_value(serde_json::json!({ "path": path, "loc": 1 }))
                .expect("minimal projection deserializes");
        for (i, callee) in callees.iter().enumerate() {
            file.calls.push(zzop_core::callgraph::RawCall {
                from_symbol: format!("{path}#handler"),
                callee_name: callee.to_string(),
                line: i as u32 + 1,
                receiver_type: None,
                is_heritage: false,
            });
        }
        file
    }

    /// The drop disclosure's three pinned states in one place: drop > 0 warns with per-file counts,
    /// drop == 0 stays silent, and total evaporation (supplied > 0, resolved == 0) carries the
    /// gap-grade EMPTY-graph phrasing that a partial drop must NOT carry.
    #[test]
    fn dropped_calls_are_disclosed_with_counts_and_evaporation_grade() {
        let a = file_with_calls("app/a.py", &["ghost", "phantom"]);
        let b = file_with_calls("app/b.py", &["ghost"]);
        let files = [&a, &b];

        // Total evaporation: nothing resolved.
        let w = dropped_calls_warning(&files, &Vec::new()).expect("total drop must be disclosed");
        assert!(w.contains("3 of 3"), "{w}");
        assert!(w.contains("app/a.py (2 of 2)"), "{w}");
        assert!(w.contains("app/b.py (1 of 1)"), "{w}");
        assert!(w.contains("EMPTY graph"), "{w}");
        assert!(w.contains("recall, not cleanliness"), "{w}");

        // Partial: one of a's edges resolved — the drop is counted, evaporation not claimed.
        let graph = vec![zzop_core::callgraph::SymbolEdge {
            from: "app/a.py#handler".to_string(),
            to: "app/a.py#ghost".to_string(),
        }];
        let w = dropped_calls_warning(&files, &graph).expect("partial drop must be disclosed");
        assert!(w.contains("2 of 3"), "{w}");
        assert!(w.contains("app/a.py (1 of 2)"), "{w}");
        assert!(!w.contains("EMPTY graph"), "{w}");

        // Zero drop: every supplied edge resolved -> silence (the same command, a zero).
        let full: Vec<zzop_core::callgraph::SymbolEdge> = [
            ("app/a.py#handler", "x"),
            ("app/a.py#handler", "y"),
            ("app/b.py#handler", "z"),
        ]
        .iter()
        .map(|(f, t)| zzop_core::callgraph::SymbolEdge {
            from: f.to_string(),
            to: t.to_string(),
        })
        .collect();
        assert!(dropped_calls_warning(&files, &full).is_none());
    }

    /// The example cap: a fourth dropping file collapses to `+1 more file(s)` — bounded output, the
    /// count still whole.
    #[test]
    fn drop_examples_cap_at_three_with_a_more_suffix() {
        let files_owned: Vec<zzop_core::FileProjection> = ["a.py", "b.py", "c.py", "d.py"]
            .iter()
            .map(|p| file_with_calls(p, &["ghost"]))
            .collect();
        let files: Vec<&zzop_core::FileProjection> = files_owned.iter().collect();
        let w = dropped_calls_warning(&files, &Vec::new()).expect("must be disclosed");
        assert!(w.contains("4 of 4"), "{w}");
        assert!(w.contains("+1 more file(s)"), "{w}");
        assert!(!w.contains("d.py"), "{w}");
    }
}
