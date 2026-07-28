//! The SECURITY-POSTURE domain of `zzop graph` — the mutating attack surface and which of it is guarded.
//!
//! # Why this is not the join map again
//! This domain and `--domain join` draw overlapping NODES (io keys), which is exactly the trap that got
//! "io/endpoint verdict" removed from the domain list — it turned out to be the join map under another
//! name. So the axis is stated before anything else:
//!
//! - **`--domain join` colours a key by its JOIN bucket** — is this route linked, unconsumed, consumed
//!   but unprovided. A question about wiring between trees.
//! - **This domain colours a route by its GUARD status** — is this write-shaped surface protected. A
//!   question about exposure inside one tree.
//!
//! Those are orthogonal: a perfectly `linked` route can be wide open, and an `unconsumedProvides` route
//! can be fully guarded. Neither picture answers the other's question, which is what makes this a domain
//! rather than a recolouring.
//!
//! # Only MUTATING routes, because only they have an answer here
//! A GET is not "unguarded", it is a read; drawing it beside an open `DELETE` would put two different
//! meanings under one colour. The write-method set is the same one the rule gates on, and it is spelled
//! here rather than imported because this crate's layering forbids a SHIPPED dependency below
//! `zzop-facade`/`zzop-config` — so the relation is sealed the way §6's T2 tier prescribes, by a test
//! that reads BOTH lists (`the_drawn_write_verbs_equal_the_rules_own_vocabulary`, using
//! `zzop-rules-http` as a dev-dependency).
//!
//! Until 2026-07-28 this paragraph claimed the set was "pinned the same way" as
//! `framework_silence::rust_router_layer`'s copy, by "a test that checks the disclosure fires on
//! exactly the shapes the rule evaluates". That pin did not exist in either place: both tests iterate
//! their own local list, which cannot detect divergence from the rule. The release audit proved it by
//! widening the rule's vocabulary and watching the workspace stay green. `rust_router_layer` now
//! imports the symbol outright (it may depend on the rule crate); this module cannot, so it pins.
//!
//! # Guard status comes from the rule's verdict, never re-derived
//! A route is drawn UNGUARDED iff this run reported `mutating-route-no-auth` at its own file+line.
//! Re-deriving it here would be a second implementation of a security judgment, and the two would
//! eventually disagree — the same reason the dep domain reads cycle membership off `circular` instead of
//! re-running Tarjan.
//!
//! **The consequence has to be said out loud, and the document says it**: absence of a finding is not
//! proof of a guard. The rule exempts routes it cannot judge (a language outside the call-graph covered
//! set, an unresolved handler, a test file, the auth-acquisition surface), and every one of those lands
//! in the same "no finding" state as a genuinely guarded route. So the third state is real and is drawn:
//! `guarded-or-exempt`, never a bare `guarded`.

use std::collections::BTreeMap;

use serde_json::Value;

/// Default per-tree route cap — a posture picture is for triage, and a hundred routes is a wall.
pub const DEFAULT_POSTURE_TOP: usize = 20;

/// The methods `mutating-route-no-auth` gates on. Duplicated deliberately — see the module doc.
const WRITE_METHODS: &[&str] = &["POST", "PUT", "PATCH", "DELETE"];

struct Route {
    source: String,
    key: String,
    file: String,
    unguarded: bool,
}

/// `analyzeTrees` output -> mermaid text for the posture domain. Pure, like its siblings.
pub(super) fn project(v: &Value, scope: Option<&str>, top: usize) -> String {
    let empty = Vec::new();
    let trees = v["trees"].as_array().unwrap_or(&empty);
    let mut routes: Vec<Route> = Vec::new();
    let mut total = 0usize;

    for t in trees {
        let source = t["sourceId"].as_str().unwrap_or("").to_string();
        // (file, line) pairs the rule actually reported — the verdict, not a re-derivation.
        let flagged: Vec<(String, u64)> = t["output"]["findings"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .filter(|f| f["ruleId"].as_str() == Some("mutating-route-no-auth"))
            .filter_map(|f| {
                Some((
                    f["file"].as_str()?.to_string(),
                    f["line"].as_u64().unwrap_or(0),
                ))
            })
            .collect();

        let mut per_tree = 0usize;
        for p in t["output"]["ir"]["io"]["provides"]
            .as_array()
            .unwrap_or(&empty)
        {
            if p["kind"].as_str() != Some("http") {
                continue;
            }
            let (Some(key), Some(file)) = (p["key"].as_str(), p["file"].as_str()) else {
                continue;
            };
            let Some((method, _)) = key.split_once(' ') else {
                continue;
            };
            if !WRITE_METHODS.contains(&method.to_ascii_uppercase().as_str()) {
                continue;
            }
            total += 1;
            if let Some(prefix) = scope {
                if !source.starts_with(prefix) && !file.starts_with(prefix) {
                    continue;
                }
            }
            if per_tree >= top {
                continue;
            }
            per_tree += 1;
            let line = p["line"].as_u64().unwrap_or(0);
            routes.push(Route {
                source: source.clone(),
                key: key.to_string(),
                file: file.to_string(),
                unguarded: flagged.iter().any(|(f, l)| f == file && *l == line),
            });
        }
    }
    render(&routes, total, scope, top)
}

fn render(routes: &[Route], total: usize, scope: Option<&str>, top: usize) -> String {
    let unguarded = routes.iter().filter(|r| r.unguarded).count();
    let mut out = String::new();
    out.push_str("%% zzop graph --domain posture — mutating attack surface and its guard status\n");
    out.push_str(&format!(
        "%% mutating routes: drawn {} / total {} | reported unguarded: {} | per-tree cap --top {top}{}\n",
        routes.len(),
        total,
        unguarded,
        scope.map(|s| format!(" | --scope {s}")).unwrap_or_default()
    ));
    out.push_str(
        "%% NOT drawn: read routes (a GET is not unguarded, it is a read) and non-http io. Guard status \
         is this run's `mutating-route-no-auth` verdict, never re-derived here.\n",
    );
    out.push_str("flowchart LR\n");

    let mut by_source: BTreeMap<&str, Vec<(usize, &Route)>> = BTreeMap::new();
    for (i, r) in routes.iter().enumerate() {
        by_source.entry(r.source.as_str()).or_default().push((i, r));
    }
    for (source, items) in &by_source {
        out.push_str(&format!("  subgraph {}\n", sanitize(source)));
        for (i, r) in items {
            let label = format!("{}<br/>{}", r.key, r.file).replace('"', "'");
            // Shape carries the verdict, not only a class: a renderer that drops styling still shows it.
            if r.unguarded {
                out.push_str(&format!("    r{i}>\"{label}\"]\n"));
            } else {
                out.push_str(&format!("    r{i}[\"{label}\"]\n"));
            }
        }
        out.push_str("  end\n");
    }

    // The third state is the whole honesty of this picture — see the module doc.
    out.push_str(&format!(
        "  zzopLegend[\"flag shape = reported unguarded ({unguarded}). box = GUARDED-OR-EXEMPT, not \
         proven guarded: the rule also stays silent on routes it cannot judge (uncovered language, \
         unresolved handler, test file, auth-acquisition path). Absence of a finding is not proof of a \
         guard.\"]\n"
    ));
    if routes.len() < total {
        out.push_str(&format!(
            "  zzopNote[\"PARTIAL VIEW: {} of {} mutating routes drawn.\"]\n",
            routes.len(),
            total
        ));
    }
    if total == 0 {
        out.push_str(
            "  zzopEmpty[\"No mutating http routes extracted. That is NOT the same as a repo with no \
             write surface — check the run's own warnings for an extraction gap first.\"]\n",
        );
    }
    out
}

/// Mermaid subgraph ids cannot carry punctuation; a source id can.
fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "tree".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests;
