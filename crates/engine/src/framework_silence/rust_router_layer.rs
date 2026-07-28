//! S10: Rust router-layer auth range self-report — names, per run, the one auth idiom this engine cannot
//! see on a Rust tree, on the runs where it can actually cost the user a false positive.
//!
//! ## What changed to make this necessary
//! Until D18, `.rs` sat OUTSIDE `CALL_GRAPH_COVERED_EXTENSIONS`, so every Rust mutating route was exempt
//! from `mutating-route-no-auth` and the rule was silent by construction — nothing to disclose. Lifting
//! that exemption bought real recall (the extractor-guard edges,
//! `zzop_parser_rust::parse_extractor_guards`) and, in the same motion, created a false-positive class
//! that did not exist before: a route guarded ONLY at the router level.
//!
//! ```text
//! Router::new()
//!     .route("/admin/users", post(create_user))
//!     .route_layer(middleware::from_fn(require_auth))   // <- invisible: not an edge from create_user
//! ```
//!
//! ## Why it is disclosed rather than modeled
//! The rule's own doc already calls route-level middleware its "precision limit", and the remedy is the
//! designed one — inject `AUTH_GUARDED_ATTR` from an adapter, or declare the guard. What makes Rust
//! different is FREQUENCY, not mechanism: in Express a `.use(guard)` is one shape among several and the
//! native `router_mounts` producer prepays it; in axum/actix a tower layer is a MAINSTREAM way to apply
//! auth, so a user meeting this rule for the first time on a Rust tree could reasonably read a batch of
//! findings as the engine being wrong rather than as it being partial. Saying so costs one line.
//!
//! ## Gate: the rule's own range, not merely "Rust present"
//! Fires only when this tree has at least one MUTATING http route registered in a `.rs` file — exactly
//! the population `mutating-route-no-auth` evaluates. A Rust tree with no routes, or with read-only ones,
//! stays silent: a disclosure that fires where the rule cannot is noise, and noise is what makes real
//! disclosures ignorable.
//!
//! Pure pass over `io_provides` — no I/O, no idiom matching. It deliberately does NOT try to detect
//! whether the tree actually uses `.route_layer`: that would be a lexical guess, and the honest claim
//! here is about RANGE ("this engine cannot see that idiom"), which is true whether or not this
//! particular tree uses it.

use zzop_core::IoProvide;

/// The rule whose precision this gap costs. Named by id rather than imported for the same reason S8/S9
/// do it — `rules-http` exposes the id only as a `Finding` literal, and this module's test pins the
/// spelling.
const AFFECTED_RULE_ID: &str = "mutating-route-no-auth";

// The methods the affected rule gates on — the rule's OWN symbol, not a copy of it (T1). This was a
// local literal until 2026-07-28, justified by "rules-http does not export it" and claimed to be kept
// honest by the pin below. Both halves were wrong: the export was one keyword away (this crate already
// depends on `zzop-rules-http`), and the test below iterates whichever list this file holds, so it can
// never assert the two still agree. Measured: widening the rule's set left the workspace green.
use zzop_rules_http::WRITE_HTTP_METHODS;

/// Cap on example route files listed — the "up to 3 example paths" convention every sibling uses.
const MAX_EXAMPLES: usize = 3;

/// `Some(warning)` when this tree registers at least one MUTATING http route in a `.rs` file. `None`
/// otherwise, including for a Rust tree whose routes are all reads.
pub fn rust_router_layer_warning(io_provides: &[IoProvide]) -> Option<String> {
    let mut count = 0usize;
    let mut examples: Vec<String> = Vec::new();
    for p in io_provides
        .iter()
        .filter(|p| p.kind == "http" && p.file.ends_with(".rs"))
    {
        let Some((method, _)) = p.key.split_once(' ') else {
            continue;
        };
        if !WRITE_HTTP_METHODS.contains(&method.to_ascii_uppercase().as_str()) {
            continue;
        }
        count += 1;
        if examples.len() < MAX_EXAMPLES && !examples.contains(&p.file) {
            examples.push(p.file.clone());
        }
    }
    if count == 0 {
        return None;
    }
    Some(format!(
        "Rust auth-range gap: {count} mutating http route(s) are registered in .rs files (e.g. {}), so \
         `{AFFECTED_RULE_ID}` evaluates them. It reads auth two ways on Rust: a guard the handler CALLS, \
         and — the idiomatic one — a guard EXTRACTOR in the handler's signature (`fn create(user: \
         AuthUser, ..)`). It cannot see auth applied at the ROUTER level (`.route_layer(middleware::\
         from_fn(require_auth))`, actix's `.wrap(..)`), because a tower layer is not a call edge out of \
         the handler. A route guarded only that way WILL be reported. Three ways to close it: move the \
         guard into the handler signature, inject an `auth-guarded` attribute from an adapter overlay \
         (Mode B), or turn the rule off with `rules: {{ \"{AFFECTED_RULE_ID}\": \"off\" }}`. An optional \
         extractor never clears a route — see `vocabulary.rustOptionalExtractorPrefixes`.",
        examples.join(", "),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provide(file: &str, key: &str) -> IoProvide {
        IoProvide {
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line: 1,
            symbol: None,
            body: None,
        }
    }

    #[test]
    fn a_rust_tree_with_mutating_routes_discloses_the_layer_gap() {
        let w = rust_router_layer_warning(&[
            provide("src/http/articles.rs", "POST /api/articles"),
            provide("src/http/articles.rs", "DELETE /api/articles/{}"),
            provide("src/http/articles.rs", "GET /api/articles"),
        ])
        .expect("mutating Rust routes must disclose");
        assert!(w.contains("2 mutating http route(s)"), "{w}");
        assert!(w.contains(AFFECTED_RULE_ID), "{w}");
        assert!(w.contains("route_layer"), "{w}");
        // The remedy must be actionable without writing an adapter.
        assert!(
            w.contains("vocabulary.rustOptionalExtractorPrefixes"),
            "{w}"
        );
    }

    /// The gate is the RULE's range, not "Rust is present" — a read-only Rust tree is outside it.
    #[test]
    fn a_rust_tree_with_only_read_routes_stays_silent() {
        assert!(rust_router_layer_warning(&[
            provide("src/http/articles.rs", "GET /api/articles"),
            provide("src/http/articles.rs", "HEAD /api/articles"),
        ])
        .is_none());
    }

    #[test]
    fn a_mutating_route_in_another_language_is_not_this_disclosures_business() {
        assert!(rust_router_layer_warning(&[provide("src/api.ts", "POST /api/users")]).is_none());
    }

    /// Every method the affected rule gates on must trip this disclosure — if the two lists drift, the
    /// gap gets disclosed on some write verbs and not others, which is worse than not disclosing at all.
    #[test]
    fn every_write_method_the_rule_gates_on_trips_the_disclosure() {
        for m in WRITE_HTTP_METHODS {
            let w = rust_router_layer_warning(&[provide("a.rs", &format!("{m} /x"))]);
            assert!(w.is_some(), "{m} must disclose");
        }
    }
}
