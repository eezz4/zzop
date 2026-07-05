//! Combines dep graph, git stats, and LOC into `FileNode`s (includes risk score, sorted descending by risk).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::node::{
    calc_risk_score, classify_lifecycle, compute_median_churn, FileNode, RiskInput, RiskWeights,
    DEFAULT_RECENT_THRESHOLD_DAYS,
};

/// A file changed fewer than this many times is not "frequently changed" -> not a hotspot.
///
/// Lives here (rather than with `zpz_metrics::roi`'s other ROI scoring, its pre-R3 home) because
/// `build_file_nodes` below — a core mechanism `zpz_engine::analyze::assemble` always calls, git-backed
/// or not — is `hotspot_score`'s only caller; core must not gain an upward dependency on `zpz_metrics` to
/// reach it — a crate-boundary decision: mechanisms a core computation depends on stay in core even when
/// related scoring logic lives in a dedicated metrics crate.
pub const HOTSPOT_MIN_CHANGES: u32 = 2;

/// Canonical hotspot = change-frequency x complexity (LOC proxy); 0 for files changed fewer than the minimum.
pub fn hotspot_score(change_count: u32, loc: u32) -> u64 {
    if change_count >= HOTSPOT_MIN_CHANGES {
        change_count as u64 * loc as u64
    } else {
        0
    }
}

// ---------------------------------------------------------------------------------------------
// DepStats / GitStats: minimal plain-data input this builder needs, declared locally here rather
// than depending on a shared parser output type, since no dep-graph/git-log parser exists in this
// workspace yet (precedent: `ActionUse` in aggregates.rs). Unify these with the real parser output
// types once dep-graph/git-log parsing lands — the field names and semantics here are the intended
// target shape.
//
// BTreeMap/BTreeSet are used here (rather than a hash-based map/set) so iteration is deterministic
// regardless of construction order.
// ---------------------------------------------------------------------------------------------

/// Fan-in/fan-out stats derived from a dep graph.
#[derive(Debug, Clone, Default)]
pub struct DepStats {
    pub fan_in: BTreeMap<String, u32>,
    pub fan_out: BTreeMap<String, u32>,
    /// All file paths that appear in the graph (as a source or as a target).
    pub all_paths: BTreeSet<String>,
}

/// Per-canonical-path accumulated git metrics.
#[derive(Debug, Clone)]
pub struct GitPathStats {
    pub change_count: u32,
    pub churn: u32,
    pub last_modified: Option<String>,
    pub author_count: u32,
    pub tag_counts: HashMap<String, u32>,
    /// Churn within the last N days (used to keep rename-only commits from counting as a recent touch).
    pub recent_churn: Option<u32>,
    pub recent_change_count: Option<u32>,
    pub author_commits: Option<HashMap<String, u32>>,
    pub recent_author_commits: Option<HashMap<String, u32>>,
}

/// Normalized git-log stats + rename-chain map.
#[derive(Debug, Clone, Default)]
pub struct GitStats {
    /// canonical (current) path -> accumulated metrics.
    pub by_path: BTreeMap<String, GitPathStats>,
    /// old path -> canonical path (used when joining with the dep graph).
    pub alias_to_canonical: BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------------------------
// buildFileNodes
// ---------------------------------------------------------------------------------------------

/// `is_source(id)` — true when `id` is a language this engine has a parser frontend for (the same
/// classification `zpz_engine::dispatch::dispatch` makes; threaded in as a closure, not a hardcoded
/// extension list here, since `core` must not gain an upward dependency on `zpz_engine` — same crate-
/// boundary rule as the `zpz_metrics` note above). Files it says "no" to (data/config/assets — json,
/// md, lockfiles, images, ... — see `build_one`'s doc) still get real `churn`/`loc`/`change_count`;
/// only `risk_score`/`hotspot_score` are zeroed for them, so they never misleadingly dominate a
/// risk-sorted list or a folder's `totalRisk` rollup (`zpz_metrics::aggregate_by_folder`) — a large
/// generated JSON dump is honest churn/size data but is not "risk" (misleading diagnostics are treated
/// as product defects; see docs/modules/napi.md's `nodes` field note).
pub fn build_file_nodes<F>(
    dep: &DepStats,
    git: &GitStats,
    loc_by_path: &HashMap<String, u32>,
    weights: &RiskWeights,
    is_source: F,
) -> Vec<FileNode>
where
    F: Fn(&str) -> bool,
{
    let ids = collect_canonical_ids(dep, git);

    let mut rename_by: BTreeMap<String, u32> = BTreeMap::new();
    for canonical in git.alias_to_canonical.values() {
        *rename_by.entry(canonical.clone()).or_insert(0) += 1;
    }

    let mut nodes: Vec<FileNode> = ids
        .iter()
        .map(|id| {
            let rename_count = rename_by.get(id).copied().unwrap_or(0);
            build_one(
                id,
                dep,
                git,
                loc_by_path,
                weights,
                rename_count,
                is_source(id),
            )
        })
        // Drop files absent from both the dep graph and disk (deleted git-history remnants).
        .filter(|n| !(n.loc == 0 && n.fan_in == 0 && n.fan_out == 0))
        .collect();

    let churns: Vec<u32> = nodes.iter().map(|n| n.churn).collect();
    let median_churn = compute_median_churn(&churns);
    let now = now_ms();
    for n in &mut nodes {
        n.lifecycle = Some(classify_lifecycle(
            n.churn,
            n.last_modified.as_deref().and_then(parse_iso_to_ms),
            median_churn,
            now,
            DEFAULT_RECENT_THRESHOLD_DAYS,
            n.recent_churn,
        ));
    }

    nodes.sort_by(|a, b| {
        b.risk_score
            .partial_cmp(&a.risk_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    nodes
}

fn collect_canonical_ids(dep: &DepStats, git: &GitStats) -> BTreeSet<String> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for p in &dep.all_paths {
        let canonical = git
            .alias_to_canonical
            .get(p)
            .cloned()
            .unwrap_or_else(|| p.clone());
        ids.insert(canonical);
    }
    for p in git.by_path.keys() {
        ids.insert(p.clone());
    }
    ids
}

/// `is_source` = this id's own classification from `build_file_nodes`'s `is_source` closure — see that
/// function's doc for why `risk_score`/`hotspot_score` are the only two fields gated on it (`churn`/
/// `loc`/`change_count` etc. stay real either way; a non-source file's size/churn is honest data, just
/// not "risk").
#[allow(clippy::too_many_arguments)]
fn build_one(
    id: &str,
    dep: &DepStats,
    git: &GitStats,
    loc_by_path: &HashMap<String, u32>,
    weights: &RiskWeights,
    rename_count: u32,
    is_source: bool,
) -> FileNode {
    let dep_path = find_dep_path(id, dep, git);
    let git_entry = git.by_path.get(id);
    let fan_in = dep_path
        .as_deref()
        .and_then(|p| dep.fan_in.get(p))
        .copied()
        .unwrap_or(0);
    let fan_out = dep_path
        .as_deref()
        .and_then(|p| dep.fan_out.get(p))
        .copied()
        .unwrap_or(0);
    let total_connections = fan_in + fan_out;
    let change_count = git_entry.map(|g| g.change_count).unwrap_or(0);
    let churn = git_entry.map(|g| g.churn).unwrap_or(0);
    let author_count = git_entry.map(|g| g.author_count).unwrap_or(0);
    let tag_counts = git_entry.map(|g| g.tag_counts.clone()).unwrap_or_default();
    let loc = loc_by_path.get(id).copied().unwrap_or(0);
    let risk_score = if is_source {
        calc_risk_score(
            &RiskInput {
                change_count,
                churn,
                total_connections,
            },
            weights,
        )
    } else {
        0.0
    };
    // Canonical hotspot = change-FREQUENCY x complexity (LOC proxy); `hotspot_score` above already applies the
    // "changed at least HOTSPOT_MIN_CHANGES times" gate. Zeroed for non-source files for the same reason as
    // `risk_score` just above: a data file's `loc` is a lexical line count, not a complexity signal, so
    // change_count * loc for e.g. a huge JSON dump is not a meaningful "hotspot".
    let hotspot = if is_source {
        hotspot_score(change_count, loc) as f64
    } else {
        0.0
    };
    FileNode {
        id: id.to_string(),
        path: id.to_string(),
        change_count,
        churn,
        last_modified: git_entry.and_then(|g| g.last_modified.clone()),
        author_count,
        loc,
        tag_counts,
        fan_in,
        fan_out,
        total_connections,
        risk_score,
        hotspot_score: Some(hotspot),
        rename_count: Some(rename_count),
        lifecycle: None,
        recent_churn: git_entry.and_then(|g| g.recent_churn),
        recent_change_count: git_entry.and_then(|g| g.recent_change_count),
        author_commits: git_entry.and_then(|g| g.author_commits.clone()),
        recent_author_commits: git_entry.and_then(|g| g.recent_author_commits.clone()),
    }
}

/// Resolves the dep-graph key for a canonical id: either the id itself, or (if the id was renamed) a past alias
/// that still appears in the dep graph — this is how a renamed file's OLD fan-in/fan-out metrics are merged onto
/// its new canonical id.
fn find_dep_path(canonical_id: &str, dep: &DepStats, git: &GitStats) -> Option<String> {
    if dep.all_paths.contains(canonical_id) {
        return Some(canonical_id.to_string());
    }
    for (alias, canonical) in &git.alias_to_canonical {
        if canonical == canonical_id && dep.all_paths.contains(alias) {
            return Some(alias.clone());
        }
    }
    None
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Minimal ISO-8601 date/date-time string -> epoch milliseconds (UTC). Handles the shapes emitted by git-log
/// timestamps: date-only "YYYY-MM-DD" (interpreted as UTC midnight, matching JS `Date.parse`) and full
/// "YYYY-MM-DDTHH:MM:SS(.sss)?Z". No date/time crate is available in this workspace (see Cargo.toml); returns None
/// for anything else, which `classify_lifecycle` treats the same as a null lastModified (infinitely old).
fn parse_iso_to_ms(s: &str) -> Option<i64> {
    if s.len() < 10 {
        return None;
    }
    let bytes = s.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;

    let mut hour: i64 = 0;
    let mut minute: i64 = 0;
    let mut second: i64 = 0;
    let mut millis: i64 = 0;
    if bytes.len() >= 19 && bytes[10] == b'T' {
        hour = s.get(11..13)?.parse().ok()?;
        minute = s.get(14..16)?.parse().ok()?;
        second = s.get(17..19)?.parse().ok()?;
        if bytes.len() > 19 && bytes[19] == b'.' {
            let frac: String = s[20..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if !frac.is_empty() {
                let mut padded = frac;
                padded.truncate(3);
                while padded.len() < 3 {
                    padded.push('0');
                }
                millis = padded.parse().ok()?;
            }
        }
    }

    let days = days_from_civil(year, month, day);
    Some(days * 86_400_000 + hour * 3_600_000 + minute * 60_000 + second * 1000 + millis)
}

/// Days since 1970-01-01 (UTC) for a proleptic-Gregorian civil date. Howard Hinnant's `days_from_civil` algorithm.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    //! Exercises `build_file_nodes`: dep/churn/risk combine correctly and sort descending by risk, rename
    //! chains merge a past path's dep metrics into the canonical id, files with dep edges but no git
    //! history are still included as nodes, and recent/author-commit fields thread through from git
    //! stats. Since no dep-graph/git-log parser exists in this workspace yet, the two small helpers below
    //! build `DepStats`/`GitStats` directly from plain test fixtures.
    use super::*;
    use crate::node::DEFAULT_WEIGHTS;

    fn dep_stats(edges: &[(&str, &[&str])]) -> DepStats {
        let mut fan_in = BTreeMap::new();
        let mut fan_out = BTreeMap::new();
        let mut all_paths = BTreeSet::new();
        for (src, deps) in edges {
            all_paths.insert((*src).to_string());
            fan_out.insert((*src).to_string(), deps.len() as u32);
            for d in *deps {
                all_paths.insert((*d).to_string());
                *fan_in.entry((*d).to_string()).or_insert(0) += 1;
            }
        }
        DepStats {
            fan_in,
            fan_out,
            all_paths,
        }
    }

    struct GitEntryInput {
        path: &'static str,
        change_count: u32,
        churn: u32,
        last_modified: &'static str,
        author_count: u32,
        renamed_from: &'static [&'static str],
    }

    fn git_stats(entries: &[GitEntryInput]) -> GitStats {
        let mut by_path = BTreeMap::new();
        let mut alias_to_canonical = BTreeMap::new();
        for e in entries {
            by_path.insert(
                e.path.to_string(),
                GitPathStats {
                    change_count: e.change_count,
                    churn: e.churn,
                    last_modified: Some(e.last_modified.to_string()),
                    author_count: e.author_count,
                    tag_counts: HashMap::new(),
                    recent_churn: None,
                    recent_change_count: None,
                    author_commits: None,
                    recent_author_commits: None,
                },
            );
            for alias in e.renamed_from {
                alias_to_canonical.insert((*alias).to_string(), e.path.to_string());
            }
        }
        GitStats {
            by_path,
            alias_to_canonical,
        }
    }

    #[test]
    fn combines_fan_change_churn_and_sorts_descending_by_risk() {
        let dep = dep_stats(&[("a.js", &["b.js", "c.js"]), ("b.js", &["c.js"])]);
        let git = git_stats(&[
            GitEntryInput {
                path: "a.js",
                change_count: 10,
                churn: 200,
                last_modified: "2026-01-01",
                author_count: 3,
                renamed_from: &[],
            },
            GitEntryInput {
                path: "b.js",
                change_count: 2,
                churn: 30,
                last_modified: "2026-01-02",
                author_count: 1,
                renamed_from: &[],
            },
        ]);
        let nodes = build_file_nodes(&dep, &git, &HashMap::new(), &DEFAULT_WEIGHTS, |_| true);
        assert_eq!(nodes[0].id, "a.js");
        assert_eq!(nodes[0].fan_out, 2);
        assert!(nodes[0].risk_score > nodes[1].risk_score);
    }

    #[test]
    fn rename_tracking_merges_past_path_dep_metrics_into_canonical_id() {
        let dep = dep_stats(&[("old.js", &["x.js"]), ("y.js", &["old.js"])]);
        let git = git_stats(&[GitEntryInput {
            path: "new.js",
            change_count: 5,
            churn: 50,
            last_modified: "2026-01-03",
            author_count: 2,
            renamed_from: &["old.js"],
        }]);
        let nodes = build_file_nodes(&dep, &git, &HashMap::new(), &DEFAULT_WEIGHTS, |_| true);
        let node = nodes.iter().find(|n| n.id == "new.js").unwrap();
        assert_eq!(node.fan_in, 1);
        assert_eq!(node.fan_out, 1);
        assert_eq!(node.change_count, 5);
    }

    #[test]
    fn files_with_dep_edges_but_no_git_history_are_still_included_as_nodes() {
        let dep = dep_stats(&[("lonely.js", &["other.js"])]);
        let git = git_stats(&[]);
        let nodes = build_file_nodes(&dep, &git, &HashMap::new(), &DEFAULT_WEIGHTS, |_| true);
        assert!(nodes.iter().any(|n| n.id == "lonely.js"));
    }

    #[test]
    fn recent_and_author_commit_fields_are_threaded_through_from_git_stats() {
        let dep = dep_stats(&[("a.js", &["b.js"])]);
        let mut by_path = BTreeMap::new();
        by_path.insert(
            "a.js".to_string(),
            GitPathStats {
                change_count: 10,
                churn: 200,
                last_modified: Some("2026-01-01".to_string()),
                author_count: 2,
                tag_counts: HashMap::new(),
                recent_churn: Some(5),
                recent_change_count: Some(3),
                author_commits: Some(HashMap::from([("a@example.com".to_string(), 7)])),
                recent_author_commits: Some(HashMap::from([("a@example.com".to_string(), 2)])),
            },
        );
        let git = GitStats {
            by_path,
            alias_to_canonical: BTreeMap::new(),
        };
        let nodes = build_file_nodes(&dep, &git, &HashMap::new(), &DEFAULT_WEIGHTS, |_| true);
        let node = nodes.iter().find(|n| n.id == "a.js").unwrap();
        assert_eq!(node.recent_churn, Some(5));
        assert_eq!(node.recent_change_count, Some(3));
        assert_eq!(
            node.author_commits,
            Some(HashMap::from([("a@example.com".to_string(), 7)]))
        );
        assert_eq!(
            node.recent_author_commits,
            Some(HashMap::from([("a@example.com".to_string(), 2)]))
        );
    }

    #[test]
    fn hotspot_requires_min_changes() {
        assert_eq!(hotspot_score(5, 100), 500);
        assert_eq!(hotspot_score(1, 100), 0); // changed once -> not a hotspot
    }

    /// Non-source files (per `is_source`) get `risk_score` and `hotspot_score` zeroed even under heavy
    /// churn/loc — the misleading-diagnostics fix: a huge data file (e.g. a degraded JSON dump) must not
    /// dominate a risk-sorted list or a folder's `totalRisk` rollup. `churn`/`loc`/`change_count` stay
    /// real (only risk/hotspot are gated). A same-shaped source file is unaffected.
    #[test]
    fn non_source_files_get_zero_risk_and_hotspot_but_keep_real_churn_and_loc() {
        let dep = dep_stats(&[]);
        let mut loc_by_path = HashMap::new();
        loc_by_path.insert("data/recipes_de-DE.json".to_string(), 50_000);
        loc_by_path.insert("src/app.ts".to_string(), 50_000);
        let git = git_stats(&[
            GitEntryInput {
                path: "data/recipes_de-DE.json",
                change_count: 20,
                churn: 500,
                last_modified: "2026-01-01",
                author_count: 3,
                renamed_from: &[],
            },
            GitEntryInput {
                path: "src/app.ts",
                change_count: 20,
                churn: 500,
                last_modified: "2026-01-01",
                author_count: 3,
                renamed_from: &[],
            },
        ]);
        let nodes = build_file_nodes(&dep, &git, &loc_by_path, &DEFAULT_WEIGHTS, |id| {
            id == "src/app.ts"
        });

        let data_node = nodes
            .iter()
            .find(|n| n.id == "data/recipes_de-DE.json")
            .unwrap();
        assert_eq!(data_node.risk_score, 0.0);
        assert_eq!(data_node.hotspot_score, Some(0.0));
        // churn/loc/change_count untouched — only risk/hotspot are zeroed.
        assert_eq!(data_node.churn, 500);
        assert_eq!(data_node.loc, 50_000);
        assert_eq!(data_node.change_count, 20);

        let source_node = nodes.iter().find(|n| n.id == "src/app.ts").unwrap();
        assert!(source_node.risk_score > 0.0);
        assert_eq!(source_node.hotspot_score, Some(20.0 * 50_000.0));

        // With risk zeroed, the data file no longer outranks the source file in the risk-sorted output.
        assert_eq!(nodes[0].id, "src/app.ts");
    }
}
