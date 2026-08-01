//! WHICH git window the emitted history columns were measured over — the fact that makes
//! `changeCount`/`churn`/`authorCount`/`lastModified` readable at all.
//!
//! Those four are sums over the commits `git log` actually walked, and config `git.since` bounds that
//! walk (`zzop_git::CollectOptions::since` becomes `git log --since=<...>`). So one file scores a churn
//! of 40 over 90 days and 900 over its whole life, and both numbers are written into the SAME column
//! under the same name. The analyze reply echoes `gitWindow` for exactly this reason —
//! `zzop_git::GitWindow`'s own doc says the numbers "mean very different things over 90 days vs. full
//! history" — and this lane's rows cannot carry prose, so it echoes the window on the census instead
//! (see `cosmograph`'s stderr-disclosure doc).
//!
//! The default is full history, so the run that needs disclosing is the CONFIGURED one and the defect
//! is conditional — which is precisely why silence is the wrong answer: a reader holding one NDJSON
//! file cannot tell which of the two runs produced it.
//!
//! # `since` only, never `recentDays`
//! `gitWindow` carries `recentDays` too, and it is deliberately NOT reported here. That knob windows the
//! `recent_*` stats (`zzop_git::CollectOptions::recent_days`), and this lane emits none of them —
//! naming it would attach a caveat to columns this table does not have, which is its own kind of
//! misdirection.
//!
//! # One window per TREE
//! `analyzeTrees` takes one `EngineConfig` PER TREE (`zzop_facade`'s `AnalyzeTreesRequest`), so two
//! trees in one run can walk two different windows while `dep::collect` merges their files into one
//! table. Reporting one of them would describe some rows correctly and the rest wrongly, so
//! disagreement is reported AS disagreement rather than resolved by picking a winner.

use std::collections::BTreeSet;

use serde_json::Value;

/// The columns this note is about. Named in the sentence because a caveat whose reader cannot tell
/// which numbers it lands on is not a disclosure.
const HISTORY_COLUMNS: &str = "changeCount/churn/authorCount/lastModified";

/// Every distinct `since` bound the run's trees collected git under. `None` INSIDE the set is a tree
/// that walked full history; an EMPTY set is a run where no tree collected git at all.
///
/// The gate is `output.gitWindow` being non-null — the same "git collection ran" signal
/// [`super::node`]'s `git_axes_by_path` reads, and for the reason that doc gives: the per-file numbers
/// themselves are plain `u32`s that default to `0` when git did not run, so they cannot gate anything.
#[derive(Default, Clone)]
pub(in crate::graph) struct GitWindows(BTreeSet<Option<String>>);

impl GitWindows {
    /// Folds one tree's window in. A tree that did not collect contributes nothing — it also
    /// contributes no history columns, so there is no row of its whose window is left undescribed.
    pub(in crate::graph) fn observe(&mut self, tree: &Value) {
        let window = &tree["output"]["gitWindow"];
        if window.is_null() {
            return;
        }
        self.0.insert(window["since"].as_str().map(str::to_string));
    }

    /// The census sentence. Four outcomes, because "not collected", "full history" and "since X" are
    /// three different facts about the same column set — and a multi-tree run can be none of the three.
    pub(in crate::graph) fn note(&self) -> String {
        let mut windows = self.0.iter();
        match (windows.next(), windows.next()) {
            (None, _) => format!(
                "Git history ({HISTORY_COLUMNS}) was NOT COLLECTED on this run, so no row carries \
                 those columns."
            ),
            (Some(None), None) => {
                format!("Git history ({HISTORY_COLUMNS}) covers FULL history — no git.since bound.")
            }
            (Some(Some(since)), None) => format!(
                "Git history ({HISTORY_COLUMNS}) covers ONLY commits since {since} (config \
                 git.since) — windowed numbers, not lifetime totals."
            ),
            _ => format!(
                "Git history ({HISTORY_COLUMNS}) was collected over DIFFERENT windows per tree ({}) \
                 — those numbers are not comparable across trees.",
                self.describe()
            ),
        }
    }

    /// Every window, in the set's own order (full history first, then `since` values ascending), so a
    /// disagreement reads the same way on every run over the same input.
    fn describe(&self) -> String {
        self.0
            .iter()
            .map(|w| match w {
                None => "full history".to_string(),
                Some(since) => format!("since {since}"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}
