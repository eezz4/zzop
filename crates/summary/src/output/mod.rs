//! Tool-output shaping: summary first, capped lists, EXPLICIT truncation disclosure. This is the
//! token-bomb guard for MCP responses, built to never lie by omission: full counts always ride along,
//! every applied cap announces `{shown, totalMatching, hint}` (a silent cap would read as "that's
//! everything") plus, on the findings lane, `severitiesNotShown` — which severity bands the cut
//! removed OUTRIGHT and the first few rows of each, because ordering by deployment role means a
//! whole `critical` band can sit past the cap while `bySeverity` still counts it, and a count alone
//! leaves a reader whose build just broke with nothing to open ([`truncation`]) — warnings are never capped (the
//! honest self-report channel outranks brevity), and
//! ordering is deterministic and lives one module over ([`ordering`], which owns the five keys and the
//! measurement behind each) — deployment role descending, then severity rank descending, then three
//! diversity keys that interleave places and rules so a window shows subjects rather than one cluster,
//! with original engine order as the final tiebreak, so the same analysis produces byte-identical tool
//! output. The key list is NOT restated here: it moved out on 2026-09-05 precisely because it had grown
//! two owners, and a second copy of an ordering is a copy that stops matching the sort.

/// Default cap for findings lists. Deliberately small: the default answer is a summary an agent can
/// reason over; the `severity`/`rule`/`limit` tool arguments are the drill-down.
const DEFAULT_FINDINGS_LIMIT: usize = 50;
/// Default cap for cross-layer edge lists (edges are small rows; agents usually want them all).
pub(crate) const DEFAULT_EDGES_LIMIT: usize = 200;
/// Default cap for `analyze_repo`'s `degraded` file-path list. `coverage.degraded` already carries the
/// full count as an uncapped scalar, so this list is supplementary detail (which files, not just how
/// many) — a large repo's full degraded-path list must never bypass the same shaping every other list
/// gets (the token-bomb guard this module exists for).
pub const DEFAULT_DEGRADED_LIMIT: usize = 50;
/// Upper bound for a caller-supplied `limit` — keeps a single tool reply bounded no matter what.
const MAX_LIMIT: usize = 1000;

mod architecture_legends;
mod bucket_keys;
mod by_directory;
mod by_rule_legend;
mod cache_signal;
mod deployment_role;
mod disclosure;
mod filters;
pub(crate) mod legends;
mod ordering;
pub(crate) mod rule_prose;
mod shown_legend;
#[cfg(test)]
mod tests;
mod timings;
mod truncation;

pub(crate) use bucket_keys::{distinct_bucket_keys, KEY_BUCKETS};
pub(crate) use cache_signal::{shape_cache_numbers, shape_cache_signal, MEANING as CACHE_MEANING};
use deployment_role::{build_paths_disclosure, is_build_surface, is_test_path};
pub(crate) use disclosure::fold as fold_disclosure;
pub use filters::severity_rank;
pub use filters::FindingFilters;
#[cfg(test)]
pub(crate) use rule_prose::{message_ref_key, template_parts_key};
pub(crate) use timings::shape_rule_timings;
pub use timings::RunKnobs;

/// Shapes a findings array into `{total, bySeverity, byRule, byRuleMeaning, shown, truncated?}`.
/// Counts are ALWAYS over the full set (the summary never shrinks with the filter); `shown` is the
/// filtered list sorted by deployment role desc, then severity-desc, capped; `truncated`
/// (`{shown, totalMatching, severitiesNotShown, hint}`) appears ONLY when `shown` is incomplete.
///
/// `build_script_paths` is the analyzed tree's OWN manifest declaration of which files are build surface
/// (`buildScriptPaths` on the facade output — see [`deployment_role`]). An empty slice is legal and means
/// exactly what it says: nothing was declared, so that tier stays empty and the ordering is the two-tier
/// one this function had before.
pub(crate) fn shape_findings(
    findings: &[serde_json::Value],
    filters: &FindingFilters,
    build_script_paths: &std::collections::HashSet<&str>,
) -> serde_json::Value {
    let mut by_severity: std::collections::BTreeMap<String, usize> = Default::default();
    let mut by_rule: std::collections::BTreeMap<String, usize> = Default::default();
    for f in findings {
        let sev = f.get("severity").and_then(|v| v.as_str()).unwrap_or("");
        let rule = f.get("ruleId").and_then(|v| v.as_str()).unwrap_or("");
        *by_severity.entry(sev.to_string()).or_default() += 1;
        *by_rule.entry(rule.to_string()).or_default() += 1;
    }

    let min_rank = filters
        .min_severity
        .as_deref()
        .map(severity_rank)
        .unwrap_or(0);
    let matching = ordering::window_order(findings, filters, min_rank, build_script_paths);

    let total_matching = matching.len();
    let limit = filters.limit.unwrap_or(DEFAULT_FINDINGS_LIMIT);
    let mut shown: Vec<serde_json::Value> =
        matching.iter().take(limit).map(|f| (*f).clone()).collect();
    // THE PROSE FOLD, applied to the WIRE and only the wire: `shown` is already cut, so a text
    // repeated only among findings the cap dropped is not repeated here and stays inline. AFTER the
    // cap deliberately — folding the full set would table prose this reply does not contain.
    // Presentation only, like the three ordering keys above: nothing dropped, no count moved, every
    // byte still reachable in this same document ([`rule_prose`] has the measurement and the pins).
    let folded = rule_prose::fold(&mut shown);

    let mut out = serde_json::json!({
        "total": findings.len(),
        "bySeverity": by_severity,
        "byRule": by_rule,
        // The legend for the field one line up, UNCONDITIONAL — see [`by_rule_legend`] for why a
        // count here is not a count of places, why the repair is a sentence rather than a derived
        // site count, and why the reply where every count IS a place count still carries it. FOLDED
        // since 2026-09-01: the caveat and the instruction it qualifies stay on the wire, in that
        // order; the vocabulary behind them is byte-identical on every run and ships once from the
        // reply-legends document instead ([`legends`]).
        "byRuleMeaning": legends::folded_string("findings.byRuleMeaning"),
        "shown": shown,
        // Unconditional, exactly like `byRuleMeaning` beside `byRule` and for the same reason: the key
        // it explains is on every reply, so a reader who could be misled is on every reply too. What it
        // stops is not a wrong sentence but an ABSENT one — `shown` is an order and reads as a ranking,
        // and nothing told anybody otherwise until 2026-09-05. See [`shown_legend`].
        "shownMeaning": legends::folded_string("findings.shownMeaning"),
    });
    // Additive-only, the contract `truncated`/`testPaths`/`buildPaths` keep: each of the fold's two
    // tables is present when it has something to say and ABSENT (never `{}`) otherwise, so a tree
    // whose every message is unique pays nothing. Each legend rides once beside its own table rather
    // than in each pointer — that is what lets a pointer stay an address; a table with no legend and
    // a legend with no table are both red in `tests`. [`rule_prose::Folded::publish`] writes both,
    // with the key names spelled there, so nothing here has to know there are two.
    folded.publish(&mut out);
    // Additive-only, like `truncated`: present exactly when it has something to say. Counted over the
    // FULL set (the same contract as `bySeverity`/`byRule`), not the filtered one, so the number a
    // reader quotes does not shrink with their filter.
    //
    // The BUILD tier publishes itself the same way, immediately below — a demotion nobody is told about
    // is a silent filter, and the older tier's disclosure is the shape the newer one has to match. The two
    // counts are computed independently and MAY overlap (a manifest-named script under `fixtures/` is in
    // both); each states a true thing about the full set, and neither claims to be a partition. That is
    // also why this count is unchanged by the new tier: its published sentence is about test paths.
    // WHERE the findings sit, as a distribution over the first path segment and nothing more. Over the
    // FULL set, like the counts above: findings concentrate hard (94.7% of fastapi's 511 under one
    // directory) and no channel of this reply said so, so a `bySeverity` that reads as a statement
    // about the project could in fact be a statement about its documentation examples.
    // [`by_directory`] carries the corpus table and the reason this channel names no directory "noise".
    //
    // ADDITIVE-ONLY since 2026-09-15 (ledger V243), which moved it onto the contract its two siblings
    // below already keep. It shipped unconditionally from birth, so one `findings` object held three
    // emission doctrines and a release would have frozen each one as it happened to be. The builder
    // owns the condition and the argument for why it is not the share threshold that module refuses.
    if let Some(block) = by_directory::by_directory(findings) {
        out["byDirectory"] = block;
    }
    let test_path_count = findings.iter().filter(|f| is_test_path(f)).count();
    if test_path_count > 0 {
        out["testPaths"] = serde_json::json!({
            "count": test_path_count,
            "meaning": "findings whose file is a test path (the DSL's shared test-paths pattern). \
                        They are still real findings — a committed credential is a leak wherever it \
                        sits, and rules that scan test paths say so in the catalog — but they sort \
                        after EVERY finding in shipped code, whatever severity either carries, so a \
                        first screen leads with production code. Nothing is dropped, no severity \
                        changes, and every count includes them.",
        });
    }
    let build_path_count = findings
        .iter()
        .filter(|f| is_build_surface(f, build_script_paths))
        .count();
    if build_path_count > 0 {
        out["buildPaths"] = build_paths_disclosure(build_path_count);
    }
    // THE FILTER'S OWN DISCLOSURE — the half `truncated` does not cover (2026-09-14, external review
    // round 22, ledger V246).
    //
    // `truncated` fires when the CAP cut the window. A `severity`/`rule` filter cuts it EARLIER, in
    // `window_order` above, and left no trace at all: measured on `corpus/frameworks/nest`,
    // `analyze --severity critical` shipped `shown: 1` beside `total: 326` with no `truncated` key and
    // no key anywhere naming a filter — a reply indistinguishable from a complete one except by
    // arithmetic the reply's own legend told the reader not to do ("nothing is dropped"). And
    // `messageByIdMeaning` asserted `truncated` was the ONLY key meaning rows were left out, which this
    // case made false.
    //
    // Additive-only, like `truncated`/`testPaths`/`buildPaths`: present exactly when a filter actually
    // removed something, so an unfiltered run pays nothing and the key's ABSENCE is the honest "your
    // view is the whole set". The counts stay over the FULL set, unchanged — what is disclosed is the
    // WINDOW, which is the thing that silently narrowed.
    let elided = findings.len().saturating_sub(total_matching);
    if elided > 0 && (filters.min_severity.is_some() || filters.rule.is_some()) {
        let mut applied = serde_json::Map::new();
        if let Some(sev) = filters.min_severity.as_deref() {
            applied.insert("severity".to_string(), serde_json::json!(sev));
        }
        if let Some(rule) = filters.rule.as_deref() {
            applied.insert("rule".to_string(), serde_json::json!(rule));
        }
        out["filtered"] = serde_json::json!({
            "applied": applied,
            "elided": elided,
            "meaning": "YOUR REQUEST narrowed this window before any cap did. `elided` is how many \
                        findings the filter above removed from `shown`; they are still counted in \
                        `total`, `bySeverity` and `byRule`, which are always over the full set. Read \
                        this key as \"the rows you are looking at are not all the rows\" — without it \
                        a filtered reply and a complete one are the same bytes, which matters most \
                        when the reply is saved, forwarded, or read in a later turn than the one that \
                        passed the filter. `truncated` beside it is a DIFFERENT cut: that one is the \
                        list cap acting on what survived this filter.",
        });
    }
    if total_matching > limit {
        // The one surface where the tool arguments really do move the cap — `shape_list`'s callers
        // must NOT reuse this hint (see `shape_list`). The disclosure is built from the SORTED
        // severities of the matching set, so it can say which severity bands the cut removed
        // outright rather than only how many rows it dropped — see [`truncation`] for why that
        // sentence exists and why its population is the post-filter set.
        //
        // The whole finding is handed over, not just its severity: the disclosure names the first
        // rows of a silenced band by `ruleId`/`file`/`line`, and the ONE list it receives is what
        // makes `shown` and the named rows provably the same partition of the same sort order.
        out["truncated"] = truncation::findings(&matching, limit);
    }
    // Zero-match rule-filter disclosure: `shown: []` from a real rule with zero findings this run is
    // indistinguishable from `shown: []` from a TYPO'd/nonexistent rule id — both look identical on the
    // wire. When a `rule` filter is present and matched nothing, cross-check it against `byRule` (built
    // from the FULL, unfiltered set above): a rule id absent from `byRule` never fired at all this run,
    // so the filter is almost certainly wrong rather than merely quiet. Deterministic and additive-only
    // (never fires when the filter matched >=1 finding, never touches `shown`/`truncated`).
    let mut notes: Vec<String> = Vec::new();
    if let Some(rule) = &filters.rule {
        if total_matching == 0 && !by_rule.contains_key(rule.as_str()) {
            notes.push(format!(
                // Names the DOCUMENT, not a route to it. This sentence used to end "read it via the
                // zzop://contract/rule-catalog resource or `zzop contract rule-catalog`" — one route
                // per host, which is better than naming only one but still worse than naming neither:
                // every host serves `rule-catalog` by that name, so the name IS the answer on both,
                // and a route list has to grow every time a surface does. Same call the starter config
                // took when the host-vocabulary contract caught two lines in it. This message was
                // invisible to that contract until 2026-07-28 — the scan truncated the file at a
                // `#[cfg(test)] mod` DECLARATION, hiding 91% of it.
                "rule filter '{rule}' matched no findings and is not among this run's fired rule ids — \
                 check the id (byRule lists what fired; the `rule-catalog` contract document lists all \
                 of them, and every zzop surface serves it under that name)"
            ));
        }
    }
    // The severity filter's twin, and the reason it exists is the asymmetry itself: `--rule <typo>`
    // explained its empty result while `--severity critical` returned `shown: []` with no `note` key at
    // all — two sibling filters answering the same "why is this empty?" question two different ways,
    // and the one that stayed silent is the one whose empty result reads most like an all-clear.
    //
    // A severity cannot be a TYPO the way a rule id can (it is a closed set, rejected before this
    // point), so this note makes the opposite move: instead of doubting the filter, it names what the
    // run DID produce. `bySeverity` is computed over the FULL set right above, so the reader learns in
    // the same breath whether "nothing critical" sits beside 40 warnings or beside nothing at all.
    if let Some(min) = &filters.min_severity {
        if total_matching == 0 {
            let census = if by_severity.is_empty() {
                "this run produced no findings at any severity".to_string()
            } else {
                format!(
                    "this run's full census is {}",
                    by_severity
                        .iter()
                        .map(|(sev, n)| format!("{sev}: {n}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            notes.push(format!(
                "severity filter '{min}' matched no findings — {census}, and `bySeverity` above counts \
                 the unfiltered set. An empty `shown` here means nothing reached that floor, NOT that \
                 nothing was found; lower the floor to see what did"
            ));
        }
    }
    // Joined rather than replaced: both filters can be present and both can come up empty, and the
    // first-wins assignment this replaced would have silently dropped whichever note lost.
    if !notes.is_empty() {
        out["note"] = serde_json::Value::String(notes.join(" "));
    }
    out
}

/// Shapes a plain list (edges, ...) into `(shown, truncated?)` with the same disclosure contract.
///
/// The caller passes the `hint`, and it must name a remedy that ACTUALLY works on this list. The
/// shared `severity`/`rule`/`limit` tool arguments reach `shape_findings` and nothing else — every
/// `shape_list` cap is a fixed constant no tool argument can move — so telling a caller here to "raise
/// limit" is advice that silently does nothing. A disclosure whose remedy is inert is worse than a
/// bare count: it reads as actionable and burns a round-trip proving otherwise (the same class as a
/// suppression marker documented at a line the scanner does not read).
pub(crate) fn shape_list(
    items: &[serde_json::Value],
    limit: usize,
    hint: &str,
) -> (Vec<serde_json::Value>, Option<serde_json::Value>) {
    let shown: Vec<serde_json::Value> = items.iter().take(limit).cloned().collect();
    let truncated = (items.len() > limit).then(|| truncation::plain(limit, items.len(), hint));
    (shown, truncated)
}
