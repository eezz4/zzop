//! Tool-output shaping: summary first, capped lists, EXPLICIT truncation disclosure. This is the
//! token-bomb guard for MCP responses, built to never lie by omission: full counts always ride along,
//! every applied cap announces `{shown, totalMatching, hint}` (a silent cap would read as "that's
//! everything") plus, on the findings lane, `severitiesNotShown` — which severity bands the cut
//! removed OUTRIGHT, because ordering by deployment role means a whole `critical` band can sit past
//! the cap while `bySeverity` still counts it ([`truncation`]) — warnings are never capped (the
//! honest self-report channel outranks brevity), and
//! ordering is deterministic — deployment role descending ([`deployment_role`]), then severity rank
//! descending, then rule round-robin (every rule's Nth finding ahead of any rule's N+1th, so a window
//! shows subjects rather than one rule's alphabetically-first cluster), with original engine order as the
//! final tiebreak — so the same analysis produces byte-identical tool output.

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

mod bucket_keys;
mod by_rule_legend;
mod cache_signal;
mod deployment_role;
mod disclosure;
mod filters;
mod rule_prose;
#[cfg(test)]
mod tests;
mod timings;
mod truncation;

pub(crate) use bucket_keys::{distinct_bucket_keys, KEY_BUCKETS};
use by_rule_legend::BY_RULE_MEANING;
pub(crate) use cache_signal::shape_cache_signal;
use deployment_role::{build_paths_disclosure, deployment_role, is_build_surface, is_test_path};
pub(crate) use disclosure::fold as fold_disclosure;
pub use filters::severity_rank;
pub use filters::FindingFilters;
#[cfg(test)]
pub(crate) use rule_prose::message_ref_key;
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
    // Round-robin counters, keyed by the BAND a finding lands in (deployment role + severity rank) plus
    // its rule id, and filled in ENGINE ORDER by the `map` below — so a finding's occurrence index is the
    // number of same-rule findings that preceded it inside its own band, and the whole key stays a pure
    // function of the pre-sort list. The band belongs in the key because the two outer keys have already
    // partitioned the list by the time this one is consulted: a rule that fires in two severity bands has
    // two independent turns, and a shared counter would let one band's traffic reorder another's.
    let mut seen_per_rule: std::collections::HashMap<(u8, u8, &str), u32> = Default::default();
    let mut matching: Vec<(usize, &serde_json::Value, u8, u8, u32)> = findings
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            let sev = f.get("severity").and_then(|v| v.as_str()).unwrap_or("");
            if severity_rank(sev) < min_rank {
                return false;
            }
            match &filters.rule {
                Some(rule) => f.get("ruleId").and_then(|v| v.as_str()) == Some(rule.as_str()),
                None => true,
            }
        })
        .map(|(i, f)| {
            let sev = f.get("severity").and_then(|v| v.as_str()).unwrap_or("");
            let rank = severity_rank(sev);
            let role = deployment_role(f, build_script_paths);
            let rule = f.get("ruleId").and_then(|v| v.as_str()).unwrap_or("");
            let counter = seen_per_rule.entry((role, rank, rule)).or_insert(0);
            let occurrence = *counter;
            *counter += 1;
            (i, f, rank, role, occurrence)
        })
        .collect();
    // DEPLOYMENT ROLE descending, THEN severity-desc within a role, original engine order as the stable
    // tiebreak — deterministic. See [`deployment_role`] for what the three roles are, why, and why role
    // is the OUTER key: a reader stops when the list stops paying, and the place they stop is the end of
    // the `critical` band, so a `critical` band made of fixtures and build scripts is a first screen that
    // found nothing. Shaping-only in both directions: nothing is dropped, counts never move, and BOTH
    // demotions announce themselves (`testPaths`/`buildPaths` below) — a demotion nobody is told about is
    // a silent filter.
    //
    // Roles are computed ONCE above rather than inside the comparator: the outer key is consulted on every
    // comparison, and it is two regex matches plus a set lookup.
    //
    // The THIRD key is RULE ROUND-ROBIN (2026-08-26): every rule's 1st finding, then every rule's 2nd, and
    // so on, with the engine index still breaking ties inside a round. Without it the tiebreak inside a
    // band was the engine index, which for a merged registry is the full path in lexicographic order — so
    // the first screen was never "the worst 40", it was "the alphabetically-first 40", and a rule whose
    // findings all sit under a late-sorting directory could not reach it at ANY count. Measured on
    // cal.com: `db/pagination-no-orderby` first appeared at rank 8 and `schema/fk-no-index` at rank 239,
    // same role, same severity, 231 places decided entirely by the letters in a path — and `apps/` (44
    // findings) filled the whole window while `packages/` (459) started at rank 48.
    //
    // Diversity, not importance: this key ranks by NOTHING about a finding except how many siblings its
    // own rule already spent, which is exactly why it can be computed here. It is deliberately CHEAP and
    // deliberately a proxy — the shipping form of "worst first" is a result-size feature, and this is the
    // step that opens the window far enough to see the material for one.
    //
    // Never emitted. The index is not a wire field, and that is the line `output-philosophy` §12 draws:
    // what is forbidden is a scalar a caller can threshold on (`confidence > 0.8`), not the act of
    // ordering — zzop already ordered by two keys before this one. Computed but not published stays out.
    matching.sort_by(|a, b| {
        b.3.cmp(&a.3)
            .then(b.2.cmp(&a.2))
            .then(a.4.cmp(&b.4))
            .then(a.0.cmp(&b.0))
    });

    let total_matching = matching.len();
    let limit = filters.limit.unwrap_or(DEFAULT_FINDINGS_LIMIT);
    let mut shown: Vec<serde_json::Value> = matching
        .iter()
        .take(limit)
        .map(|(_, f, _, _, _)| (*f).clone())
        .collect();
    // THE PROSE FOLD, applied to the WIRE and only the wire: `shown` is already cut, so a text
    // repeated only among findings the cap dropped is not repeated here and stays inline. AFTER the
    // cap deliberately — folding the full set would table prose this reply does not contain.
    // Presentation only, like the three ordering keys above: nothing dropped, no count moved, every
    // byte still reachable in this same document ([`rule_prose`] has the measurement and the pins).
    let rule_messages = rule_prose::fold(&mut shown);

    let mut out = serde_json::json!({
        "total": findings.len(),
        "bySeverity": by_severity,
        "byRule": by_rule,
        // The legend for the field one line up, UNCONDITIONAL — see [`by_rule_legend`] for why a
        // count here is not a count of places, why the repair is a sentence rather than a derived
        // site count, and why the reply where every count IS a place count still carries it.
        "byRuleMeaning": BY_RULE_MEANING,
        "shown": shown,
    });
    // Additive-only, the contract `truncated`/`testPaths`/`buildPaths` keep: present when it has
    // something to say, ABSENT (never `{}`) otherwise, so a tree whose every message is unique pays
    // nothing. The legend rides here rather than in each pointer — that is what lets a pointer stay
    // an address; a table with no legend and a legend with no table are both red in `tests`.
    if let Some(table) = rule_messages {
        out["ruleMessages"] = table;
        out["ruleMessagesMeaning"] =
            serde_json::Value::String(rule_prose::RULE_MESSAGES_MEANING.to_string());
    }
    // Additive-only, like `truncated`: present exactly when it has something to say. Counted over the
    // FULL set (the same contract as `bySeverity`/`byRule`), not the filtered one, so the number a
    // reader quotes does not shrink with their filter.
    //
    // The BUILD tier publishes itself the same way, immediately below — a demotion nobody is told about
    // is a silent filter, and the older tier's disclosure is the shape the newer one has to match. The two
    // counts are computed independently and MAY overlap (a manifest-named script under `fixtures/` is in
    // both); each states a true thing about the full set, and neither claims to be a partition. That is
    // also why this count is unchanged by the new tier: its published sentence is about test paths.
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
    if total_matching > limit {
        // The one surface where the tool arguments really do move the cap — `shape_list`'s callers
        // must NOT reuse this hint (see `shape_list`). The disclosure is built from the SORTED
        // severities of the matching set, so it can say which severity bands the cut removed
        // outright rather than only how many rows it dropped — see [`truncation`] for why that
        // sentence exists and why its population is the post-filter set.
        let ordered: Vec<&str> = matching
            .iter()
            .map(|(_, f, _, _, _)| f.get("severity").and_then(|v| v.as_str()).unwrap_or(""))
            .collect();
        out["truncated"] = truncation::findings(&ordered, limit);
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
