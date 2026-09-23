//! C. Input / config — the run differed from what the user thought they asked for.
//!
//! One group of the blindness registry, split from the parent module on the file-size cap along the
//! seam the array already had. The parent concatenates the four in declared order.

use super::super::types::{BlindnessClass, DisclosureStatus, INPUT_CONFIG};

pub(super) const ROWS: &[BlindnessClass] = &[
    BlindnessClass {
        id: "input-scope-error",
        group: INPUT_CONFIG,
        summary: "A root that does not exist / is not a directory, or that yields zero files, \
                  self-reports as a leading warning. So does the one scope filter that used to be \
                  silent: `vocabulary.skipDirs` prunes whole directories before any file under them is \
                  read, which is not a smaller answer about the tree but a complete answer about a \
                  different one, and a run that pruned anything now names the directories and the key \
                  alongside the other scope warnings. A too-narrow root that still matches SOME files \
                  (partial scope) is still not detected.",
        status: DisclosureStatus::Partial,
    },
    BlindnessClass {
        id: "config-error",
        group: INPUT_CONFIG,
        summary: "A setting that named something this build does not have is reported as a diagnostic, so \
                  a run that quietly differed from the one the author asked for does not read as \"that \
                  problem is absent\". This row named ONE of those checks until 2026-08-29 and there are \
                  seven families of them, all self-reporting: (1) an id matching no known rule in \
                  `disabledRules` (which `packs.disabled` and a per-rule \"off\" both fold into), in \
                  `severityOverrides`, or in `suppressions` — each judged against the known-id union that \
                  the mechanism it feeds actually matches on, so a bare pack id counts as known for the \
                  first and not for the other two; (2) an entry in `packs.only` / `packsOnly` naming no \
                  loaded pack, plus the total-typo case, which is called out on its own because its \
                  failure mode is the inverse — it gates EVERY DSL rule off rather than none; (3) an \
                  unrecognized key at every declared scope, top level through `trees` entries and their \
                  nested objects, each reported with the accepted key list for that scope, and three \
                  retired keys answered with why they no longer do anything rather than with the generic \
                  typo guess; (4) a `vocabulary` pattern that does not compile, which says it was IGNORED \
                  so the run made no judgment that depends on it — the expensive direction, since an \
                  unjudged axis is silent rather than wrong; (5) an unparseable entry in \
                  `git.commitTypePatterns` or `git.commitSubjectPatterns`, skipped and counted; (6) a \
                  `parsers.globOverrides[].language` this build does not have, skipped rather than \
                  failing the run; (7) a path or glob filter that matched nothing — a dead `exclude` \
                  entry, or a dead per-rule filter on a suppression whose id was itself valid, which is \
                  an orthogonal diagnostic to (1) and can fire alongside it on the same entry. Each lands \
                  in `configWarnings` or in `warnings` by channel, never in both.",
        // `asserted`, and that is a claim about MECHANISM rather than about the sentence above: each
        // family is computed from the request itself on every run, so none of them can be silently
        // absent. The sentence was nonetheless wrong in the way a hand-written enumeration goes wrong —
        // it named one seventh of what ships. That direction matters: it UNDERSTATES the disclosure
        // rather than overstating it, which is why the status survived an audit the prose did not.
        //
        // FIRING PIN: crates/engine/tests/integration/analyze_rule_config.rs::unknown_severity_override_id_surfaces_a_self_report_warning
        // FIRING PIN: crates/engine/tests/integration/analyze_rule_config.rs::a_real_severity_override_id_does_not_trigger_the_unknown_id_warning
        status: DisclosureStatus::Asserted,
    },
];
