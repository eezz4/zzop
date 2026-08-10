//! Cross-tree package-import disclosure — the honest self-report for what splitting a workspace into
//! per-package trees silently costs.
//!
//! Measured on a 22-package pnpm monorepo (2026-08-06): `{"trees": "auto"}` produced 5,262 dep-graph
//! links of which ZERO crossed a package boundary; the identical tree analyzed as ONE tree produced
//! 6,651 links of which 1,364 crossed one, across 23 distinct package pairs. Nothing was broken — the
//! TypeScript resolver already maps `@base/utils-fe` through `package.json#name`/`exports`/`main` and
//! through tsconfig `paths`. A dep graph is simply built PER TREE, so an import leaving its tree is
//! censused as an external package (`AnalyzeOutput::package_imports`) and can never become an edge.
//! The reader saw "22 independent islands" and nothing in the output contradicted that reading.
//!
//! What is stated here is an OBSERVATION, never a guess (the repo's never-guess doctrine): the reported
//! specifiers are exactly this tree's `package_imports` entries that MATCH another tree's `source_id`
//! in this same run — a set intersection over data the run already holds, so it cannot be wrong about
//! what it names. Matching reuses `zzop_parser_typescript::match_workspace_pkg`, the resolver's OWN
//! workspace-package matcher (exact, scoped sub-path `@base/utils-fe/auth/hash`, unscoped sub-path
//! `lodash/fp`, and a refusal for the `@/` path alias), so this disclosure and the resolver can never
//! disagree about what "names that package" means.
//!
//! Pushed onto the IMPORTING tree's own `AnalyzeOutput::warnings` — the same per-tree engine
//! self-report channel `filter_join_io`'s join-input drop and the topology-host zero-effect tripwire
//! already use.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use zzop_parser_typescript::match_workspace_pkg;

use super::PackageImportSummary;
use crate::AnalyzeOutput;

/// How many matched specifiers the message names inline — the same "up to 3 examples" convention
/// `unparsed_extension_warning` and the join-input drop sample already use. Going over it is stated in
/// the message (`showing the first N of M`), never silently swallowed.
const MAX_EXAMPLES: usize = 3;

/// Appends the disclosure to every tree whose package-import census names another tree of this run.
/// Silent (nothing appended) for a tree with no such specifier, and for a whole run that resolved fewer
/// than 2 distinct source ids — with no other tree to cross into, an unresolved package import is an
/// ordinary external dependency and there is nothing observed to report.
pub(super) fn disclose(outputs: &mut [(PathBuf, String, AnalyzeOutput)]) {
    let all_ids: BTreeSet<String> = outputs
        .iter()
        .map(|(_, source, _)| source.clone())
        .collect();
    if all_ids.len() < 2 {
        return;
    }
    for (_, source, output) in outputs.iter_mut() {
        // Lookup-only map: `match_workspace_pkg` is generic over the value type, so `()` is the whole
        // payload. Built from the sorted `BTreeSet` and never ITERATED, so no HashMap ordering can
        // reach the message text; the message's own order comes from `package_imports`, which arrives
        // sorted out of `PackageImportSummary::census`' `BTreeMap` fold.
        let others: HashMap<String, ()> = all_ids
            .iter()
            .filter(|id| *id != source)
            .map(|id| (id.clone(), ()))
            .collect();
        let warning = {
            let matched: Vec<&PackageImportSummary> = output
                .package_imports
                .iter()
                .filter(|p| match_workspace_pkg(&p.specifier, &others).is_some())
                .collect();
            (!matched.is_empty()).then(|| compose(&matched))
        };
        if let Some(warning) = warning {
            output.warnings.push(warning);
        }
    }
}

/// The message: what was observed, the mechanism that produced it, how far the effect reaches, and the
/// remedy TOGETHER WITH its cost. `matched` is non-empty and in sorted-specifier order.
fn compose(matched: &[&PackageImportSummary]) -> String {
    let shown = matched.len().min(MAX_EXAMPLES);
    let examples = matched[..shown]
        .iter()
        .map(|p| {
            format!(
                "{} ({} file(s), e.g. {})",
                p.specifier, p.file_count, p.example_file
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let truncated = if matched.len() > shown {
        format!(
            " (showing the first {shown} of {}, sorted by specifier)",
            matched.len()
        )
    } else {
        String::new()
    };
    format!(
        "cross-tree package imports: {} import specifier(s) in this tree name ANOTHER tree analyzed in \
         this same run, so each was censused as an EXTERNAL package instead of becoming a dependency \
         edge: {examples}{truncated}. Each tree's dependency graph holds in-tree edges only, so an \
         import that crosses a tree boundary can never be a dep edge — it lands in this tree's \
         package-import census instead. Everything derived from the dep graph therefore stops at the \
         tree boundary: import cycles, fan-in/fan-out, dead/unimported exports and the `dep` graph \
         domain all read this tree as if those imports were third-party. Analyzing the whole workspace \
         as ONE tree (\"roots\": [\".\"], no \"trees\") turns those package-to-package imports into real \
         dep edges — at the cost of the cross-layer join, which needs >= 2 trees with distinct \
         sourceIds to fire and therefore does not run in a one-tree analysis.",
        matched.len()
    )
}
