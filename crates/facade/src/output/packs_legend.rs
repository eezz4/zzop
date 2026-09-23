//! `packsLoadedMeaning` — the legend for `packsLoaded`, and the one channel in this reply that was
//! shipping numbers with no statement of what they measure.
//!
//! # Why a SIBLING key and not a `meaning` inside the object
//! The repo's default is to put a disclosure INSIDE the thing it describes (`cache`, `ruleTimings`,
//! `findings.truncated.severitiesNotShown`), precisely so a consumer that read the numbers cannot have
//! failed to read what they omit. `packsLoaded` is an ARRAY, and the only "inside" it has is a row —
//! which would repeat the whole legend once per loaded pack (the bundled packs, on a default run). So it
//! takes the other established shape, the one every array/scalar channel here already uses:
//! `painMeaning`, `criticalTopMeaning`, `dispatchMeaning`, `blindSpotMeaning`, `scoreMeanings`. The
//! name is the one an outside auditor reached for and did not find.
//!
//! # Two ambiguities, measured, both closed here
//! * `filesInScope` was published identically whether or not the pack was evaluated — a positive
//!   scanned-file count on a pack that read nothing (`zzop_engine::PackLoaded`'s own doc carries the
//!   repro). The row now splits the key; this legend is where the split is stated.
//! * `zeroAdmissionRules` shipped with no `meaning` and no `--help` mention, and "admission 0" has two
//!   readings whose implications are OPPOSITE — "no file ever reached this rule" (harmless: its zero
//!   is scope) versus "files reached it and nothing fired" (a clean bill). It has always meant the
//!   first. Saying so is the whole fix; the field's own computation is untouched.
//!
//! # Silence when there is nothing to say
//! The `didNotRun` entry appears only when at least one pack really did not run, and the whole legend
//! is absent when no pack loaded at all. A disclosure that fires with nothing to disclose spends the
//! reader's trust on noise, which is what makes them skip the one that matters.

use std::collections::BTreeMap;

use zzop_engine::PackLoaded;

const ROW: &str =
    "one entry per DSL rule pack this run LOADED, sorted by id. `rules` and `ruleIds` \
describe the pack as loaded, before any per-rule gating. Loading is not running: a pack that was \
switched off still loaded, and says so in its own row.";

const FILES_IN_SCOPE: &str = "analyzed files matching at least one of this pack's rules' \
`file_pattern` — path candidacy, checked before any file content is read, and never a \"matched\" or \
\"found N usages\" count. Its PRESENCE means this pack ran: on a pack that did not run the key is \
absent and `filesInScopeIfEnabled` carries the same census as a counterfactual.";

const ZERO_ADMISSION_RULES: &str = "rules of this pack whose own path gates (`file_pattern` minus \
that rule's `file_exclude_pattern`) admitted no analyzed file: they read not one byte of this tree, \
so their zero findings are scope, never \"checked and clean\". A rule NOT listed did admit files, and \
this field says nothing about whether it then fired. Absent when every rule admitted a file, when the \
pack's `filesInScope` is 0 (already \"all of them\"), and on a pack that did not run.";

const DID_NOT_RUN: &str = "this pack loaded but was never evaluated, so every other number in its row \
is about the pack and this tree's paths, not about a scan: `disabled` = its id is in the run's \
disabled list (config `packs.disabled`, or a `rules` entry set to off; embedders: `disabledRules`), \
`notAllowlisted` = a pack allowlist is in force (config `packs.only`, embedders: `packsOnly`) and \
does not name it. Zero findings from such a pack mean NOT ANALYZED — never analyzed-and-clean.";

/// EVERY sentence this legend can carry, unconditionally — the RUN-FREE view of the same constants
/// [`packs_loaded_meaning`] serves conditionally.
///
/// It exists because `zzop_summary` folds this legend out of the shaped reply and serves the full text
/// from a contract document instead. That document has no run: rendering it from one would make its
/// contents depend on which packs a particular analysis happened to gate off, and the reader following
/// the pointer is by construction someone who no longer has that reply in front of them. Reading the
/// same constants is what keeps the folded pointer and the document it points at from ever disagreeing
/// — the discipline `disclosure_contract_text` already follows for the blindness registry.
///
/// `didNotRun` is present here and conditional there, and that asymmetry is the point: the conditional
/// form is a disclosure about THIS run, the unconditional one is the vocabulary a reader looks up.
/// The `source` token vocabulary, as one sentence per token — see `PackSource::wire_meanings`, which
/// owns both the spellings and the sentences (review ledger V214: a value explains itself so no reader
/// has to memorize a list this project is free to extend).
fn source_tokens() -> &'static str {
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        let tokens = zzop_engine::PackSource::wire_meanings()
            .iter()
            .map(|(token, meaning)| format!("`{token}`: {meaning}"))
            .collect::<Vec<_>>()
            .join(" ");
        format!("which of this row's bytes came from where. {tokens}")
    })
}

pub fn packs_loaded_legend() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("row", ROW),
        ("filesInScope", FILES_IN_SCOPE),
        ("zeroAdmissionRules", ZERO_ADMISSION_RULES),
        ("source", source_tokens()),
        ("didNotRun", DID_NOT_RUN),
    ])
}

/// The legend for this run's `packsLoaded`, or `None` when no pack loaded (an empty roster has no
/// entries to explain, and the run already carries a `warnings` line saying zero packs loaded).
///
/// `BTreeMap`, like `scoreMeanings`: key order is a pure function of which entries are present, never
/// of iteration luck (§6 output determinism).
pub(super) fn packs_loaded_meaning(
    packs: &[PackLoaded],
) -> Option<BTreeMap<&'static str, &'static str>> {
    if packs.is_empty() {
        return None;
    }
    let mut legend = BTreeMap::from([
        ("row", ROW),
        ("filesInScope", FILES_IN_SCOPE),
        ("zeroAdmissionRules", ZERO_ADMISSION_RULES),
        ("source", source_tokens()),
    ]);
    // Computed from the rows themselves, never from "was a knob set": a `packs.only` naming every
    // loaded pack, or a `disabled` entry that is a typo, gates nothing and must produce no sentence.
    if packs.iter().any(|p| p.did_not_run.is_some()) {
        legend.insert("didNotRun", DID_NOT_RUN);
    }
    Some(legend)
}
