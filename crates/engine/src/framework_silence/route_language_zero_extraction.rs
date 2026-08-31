//! S17: the MIRROR of S8 — a language this tree is substantially made of, that this build declares a
//! route recognizer for, from which zero http routes were extracted, and whose call graph this build
//! cannot walk either.
//!
//! ## The gap it closes
//! S8 is documented as "the odd one out — it fires when extraction SUCCEEDED", and that is exactly its
//! blind spot. `docs/rules/catalog.md` promises, for `mutating-route-no-auth`, that *"a run that saw
//! such routes says so out loud in its own `warnings`, naming the language, the count, an example path
//! and this rule id"*. Measured 2026-08-20 on gogs `d460e50f` — roughly 300 hand-counted macaron route
//! registrations, `http` extraction 0 — the `warnings` array is byte-identical with and without that
//! promise, because there were no extracted routes to iterate. The louder the silence, the quieter the
//! disclosure. A 20-line gin control tree produces S8 normally, which is what proved the channel alive
//! and the condition wrong.
//!
//! ## Why it is narrow enough to be a warning
//! The general capability×measured cross is a FACT, not an alarm — `zzop_facade`'s
//! `query_coverage::io_channels` doc owns that judgment and it stands: as a `warnings` entry the whole
//! cross would fire on every Go CLI and every pure frontend. This tripwire takes three further
//! conditions off that cross, and each one removes a class of legitimate zero:
//!
//! 1. the PROVIDES channel only — a tree's consume side and its db side are different questions;
//! 2. the extension must be OUTSIDE
//!    [`zzop_rules_http::mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS`], so a TypeScript,
//!    Java or Python tree with no routes never reaches it — for those, the routes being absent is the
//!    whole story and no second rule goes dark behind it;
//! 3. and the disclosure it makes is therefore specific: not "you might have routes" but "if this tree
//!    serves HTTP, `mutating-route-no-auth` has nothing to judge and its silence is not a clean bill".
//!
//! What survives all three is a tree made of a language with a shipped route recognizer, whose routes
//! this build did not find, in an ecosystem whose call graph it also cannot walk. A Go CLI still
//! reaches it, and the wording carries that: it states the condition rather than asserting routes
//! exist. Over-disclosure is the safe direction here for the same reason it is in S8 — this suppresses
//! nothing and changes no verdict — and, since it is registered behind the provide-side alarm, it does
//! not reach S15 either. Two facts made that explicit rather than assumed: S15's gate is "a sibling
//! already reported an empty channel", so setting the alarm here would have added a SECOND paragraph to
//! every Go CLI; and S1/S2 name the framework when they fire, which is a better answer than naming the
//! language, so this tripwire stays quiet whenever one of them already spoke.

use std::collections::BTreeMap;

use zzop_core::recognizer::channel;

/// The rule this gap silences — READ from S8, which owns the spelling and is the module whose test
/// asserts it against the shipped rule registry.
///
/// It was its own byte-identical literal until 2026-08-31, and its doc claimed to be "pinned against
/// the shipped registry by S8's own test" while S8's test reads S8's constant and nothing else.
/// Measured: setting this copy to `mutating-route-no-auth-GHOST` left all 140 `framework_silence`
/// tests green — S8's registry pin never looks here, and this module's own test asserted
/// `w.contains("mutating-route-no-auth")`, a third hand-typed spelling that a ghost SUFFIX satisfies
/// and a rename would leave stale in lockstep with the constant it was supposed to guard. Same crate,
/// same module tree, so the repair is the symbol rather than a second pin.
use super::call_graph_language::SILENCED_RULE_ID;

/// `Some(warning)` when at least one `(io.provides, ext)` row of the shared cross names an extension
/// outside the call-graph-covered set. `None` otherwise — including the ordinary case where every
/// route-bearing language of the tree DID extract.
pub fn route_language_zero_extraction_warning(
    structural_files: &BTreeMap<String, usize>,
    io_provides: &[zzop_core::IoProvide],
    io_consumes: &[zzop_core::IoConsume],
) -> Option<String> {
    // MEASURE here rather than at the call site: the cross's two halves belong together, and a caller
    // that assembled the counts itself would be a second owner of the db-kind partition.
    let extracted = crate::zero_extraction::extracted_by_channel(io_provides, io_consumes);
    let covered = zzop_rules_http::mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS;
    let rows: Vec<String> =
        crate::zero_extraction::zero_extraction_rows(structural_files, &extracted)
            .into_iter()
            .filter(|r| r.channel == channel::PROVIDES && !covered.contains(&r.ext.as_str()))
            .map(|r| {
                format!(
                    ".{} ({} structural file(s), recognizer(s) in this build: {})",
                    r.ext,
                    r.structural_files,
                    r.recognizers.join(", ")
                )
            })
            .collect();
    if rows.is_empty() {
        return None;
    }
    Some(format!(
        "Route-extraction coverage gap: this tree is substantially made of {} — this build ships a \
         route recognizer for that language and extracted ZERO http routes from it. If this tree \
         serves HTTP, every route-shaped rule saw nothing to judge, and `{SILENCED_RULE_ID}` is \
         doubly silent there, since that language also has no call-site extractor in this build \
         (languages this build DOES walk: {}). If it genuinely serves no HTTP — a CLI, a library, a \
         worker — this line is simply the honest provide-side reading of it and needs no action. Two \
         ways to close it if it is a gap: supply the routes through an adapter overlay (Mode B), or \
         teach the parser for that language the framework's registration shape. This is a \
         self-report, not a finding: nothing is suppressed and no verdict changed.",
        rows.join("; "),
        covered.join("/"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::{IoConsume, IoProvide};

    fn structural(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
        pairs.iter().map(|(e, n)| (e.to_string(), *n)).collect()
    }

    fn provide(file: &str, kind: &str) -> IoProvide {
        IoProvide {
            response: None,
            kind: kind.to_string(),
            key: "k".to_string(),
            file: file.to_string(),
            line: 1,
            symbol: None,
            body: None,
            ..Default::default()
        }
    }

    /// The gogs shape, which is the whole reason this tripwire exists: a Go tree with hundreds of route
    /// registrations, zero extracted, and an `analyze` reply that said nothing about it.
    #[test]
    fn a_go_tree_with_no_extracted_routes_is_disclosed_by_language() {
        // The db-side fill is the control that matters here: 12 GORM `db-table` provides are exactly
        // what made gogs read as "contributed joinable io" while ~300 routes were invisible, so a
        // filled db channel must not vouch for the empty route channel.
        let provides = vec![provide("internal/db/user.go", "db-table"); 12];
        let w = route_language_zero_extraction_warning(
            &structural(&[("go", 301), ("md", 20)]),
            &provides,
            &[],
        )
        .expect("a route-recognized language at 0 must be disclosed");
        assert!(w.contains(".go (301 structural file(s)"), "{w}");
        // The CONSTANT, not a fourth hand-typed spelling: a literal here is satisfied by any id that
        // merely starts with it (a `-GHOST` suffix passed), and it would go stale in lockstep with
        // the copy it was supposed to guard. The registry assertion lives on the constant, in S8.
        assert!(w.contains(SILENCED_RULE_ID), "{w}");
        assert!(w.contains("ZERO http routes"), "{w}");
    }

    /// Seals the silence side twice over: a covered language never reaches this warning (S8 and the
    /// route rules own that tree), and a language that DID extract is not a gap at all.
    #[test]
    fn a_covered_language_and_a_filled_channel_both_stay_silent() {
        assert!(
            route_language_zero_extraction_warning(&structural(&[("ts", 200)]), &[], &[]).is_none(),
            "a call-graph-covered language is not this tripwire's subject"
        );
        let routes = vec![provide("internal/api/h.go", "http"); 7];
        assert!(
            route_language_zero_extraction_warning(&structural(&[("go", 301)]), &routes, &[])
                .is_none(),
            "a language that extracted routes is not a gap"
        );
    }

    /// The share floor is the shared cross's, not a second one: a handful of `.go` files in a
    /// TypeScript monorepo is rounding error, and a fact table dominated by those buries the row that
    /// carries the finding.
    #[test]
    fn a_rounding_error_minority_language_is_below_the_floor() {
        let routes = vec![provide("src/api.ts", "http"); 40];
        let consumes: Vec<IoConsume> = Vec::new();
        assert!(
            route_language_zero_extraction_warning(
                &structural(&[("ts", 500), ("go", 2)]),
                &routes,
                &consumes,
            )
            .is_none(),
            "2 of 502 files is below MIN_UNCOVERED_EXTENSION_SHARE_PCT"
        );
    }
}
