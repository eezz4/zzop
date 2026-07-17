//! Self-diagnostics for an analysis run — turns "ran without error but produced empty/degenerate data" (silent
//! failure) into explicit warnings. A new language, module system, repo layout, or commit convention can make zzop
//! succeed with hollow output; "no error" is not validation. The counts report success *scope* (N of M), and the
//! warnings flag degenerate signals (0 dep edges, 0 symbols, all-abstract, untagged commits) so the consumer knows
//! a coverage gap exists and where to look (config/adapter). Pure — caller computes the counts from the assembled IR.

use serde::{Deserialize, Serialize};

/// Below this file count the dep-edge sparsity heuristic is skipped (too small to judge).
const MIN_FILES_FOR_EDGE_CHECK: u32 = 10;
/// Edges-per-file ratio below which the dep graph is flagged as pathologically sparse.
const MIN_EDGES_PER_FILE: f64 = 0.05;

/// Git history counts for a run. Kept as its own type — rather than flat fields on `DiagnosticsInput` —
/// so the *presence* of git data is representable: `DiagnosticsInput::git` is `None` when git was never
/// attempted (disabled by config, or collection failed and was already reported elsewhere by the caller),
/// distinct from "git ran and found zero" (`Some` with honest zero counts). Only the `Some` case can ever
/// produce a git-window warning below — the module gates on that itself, so callers no longer need to
/// filter the resulting warning text after the fact to suppress git-window noise for a git-inactive run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiagnosticsInput {
    /// Total file-changes seen in git history.
    pub total_changes: u32,
    /// File-changes that received any commit-type tag (FIX/FEAT/...).
    pub tagged_changes: u32,
    /// File-changes tagged FIX specifically.
    pub fix_changes: u32,
    /// Number of commits in the analyzed git window.
    pub commits: u32,
    /// The config `git.since` value used (None = `since` was omitted, meaning full history). Lets the 0-commit
    /// message tell "the window is too narrow" apart from "even full history has nothing" (a submodule or
    /// brand-new files), so it doesn't pointlessly suggest widening a window that already covers everything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsInput {
    /// Source files analyzed.
    pub files: u32,
    /// Total internal dependency edges (sum over the dep graph).
    pub dep_edges: u32,
    /// Exported symbols extracted across all files.
    pub symbols: u32,
    /// Modules classified concrete (have value exports).
    pub concrete_modules: u32,
    /// Modules classified at all (concrete + abstract).
    pub total_modules: u32,
    /// Git history counts, or `None` when git was never attempted for this run. See
    /// `GitDiagnosticsInput`'s doc for why absence (not zeroed counts) is what gates the git-window
    /// warnings off.
    pub git: Option<GitDiagnosticsInput>,
    /// `RuleConfig::disabled_rules` entries that matched no known rule id at analysis time — an exact
    /// string match against nothing means the entry silently did nothing (`registry::is_enabled`'s
    /// contract: unmatched = "not disabled", not an error). This module stays vocabulary-blind (see crate
    /// doc: it has no notion of a pack/native-analysis id space of its own) — the caller diffs
    /// `disabled_rules` against the actual known-id union (native analysis ids + pack ids +
    /// `"<pack>/<rule>"` ids) and passes only the leftover, already-unknown entries here. Empty (the
    /// default) when every `disabled_rules` entry matched something, or when the caller has not wired this
    /// check.
    #[serde(default)]
    pub unknown_disabled_rule_ids: Vec<String>,
    /// `RuleConfig::severity_overrides` entries that matched no known rule id at analysis time —
    /// `registry::apply_severity_override` does an exact string match of `finding.rule_id` against this
    /// map, so an unmatched key silently remapped nothing (same "did nothing, no error" contract as
    /// `unknown_disabled_rule_ids`, over a different config knob). This module stays vocabulary-blind (see
    /// crate doc) — the caller diffs `severity_overrides` keys against the actual known-id union and passes
    /// only the leftover, already-unknown entries here. NOTE the known-id union here is narrower than
    /// `unknown_disabled_rule_ids`'s: a bare pack id is a valid `disabled_rules` entry (it drops the whole
    /// pack) but can never appear as a finding's `rule_id` (DSL findings are always `"<pack>/<rule>"`), so a
    /// bare pack id must NOT be treated as "known" here even though it is for `disabled_rules`. Empty (the
    /// default) when every `severity_overrides` entry matched something, or when the caller has not wired
    /// this check.
    #[serde(default)]
    pub unknown_severity_override_ids: Vec<String>,
    /// `RuleConfig::suppressions` entries whose `rule` matched no known rule id at analysis time —
    /// `registry::is_suppressed` does an exact string match of `entry.rule` against a finding's `rule_id`,
    /// so an unmatched `rule` silently suppressed nothing (same "did nothing, no error" contract as
    /// `unknown_disabled_rule_ids`/`unknown_severity_override_ids`, over the `suppressions` knob). This
    /// module stays vocabulary-blind (see crate doc) — the caller diffs `suppressions[].rule` against the
    /// actual known-id union and passes only the leftover, already-unknown entries here. Same narrower
    /// known-id union as `unknown_severity_override_ids` (no bare pack id — see that field's doc for why).
    /// Orthogonal to the unmatched-path/glob-filter warning (`unmatched_suppression_warnings` in
    /// `zzop-engine`): that one flags a *filter* matching no scanned file, this one flags the *rule id*
    /// itself matching nothing; a single suppression entry can trigger both independently (a typo'd rule id
    /// AND a typo'd filter are two separate mistakes on the same entry). Empty (the default) when every
    /// `suppressions` entry's `rule` matched something, or when the caller has not wired this check.
    #[serde(default)]
    pub unknown_suppression_rule_ids: Vec<String>,
}

/// Extends the counts of `DiagnosticsInput` with warnings. Rust has no struct inheritance, so the input is
/// embedded and flattened for JSON, keeping the counts and warnings as top-level fields on one object;
/// `Deref` gives ergonomic field access (`d.files`) without duplicating every field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisDiagnostics {
    #[serde(flatten)]
    pub input: DiagnosticsInput,
    /// Human-readable warnings for degenerate signals — empty when the run looks healthy.
    pub warnings: Vec<String>,
    /// The `unknown_disabled_rule_ids`/`unknown_severity_override_ids` self-reports — split OUT of
    /// `warnings` (2026-07-17) because these are config problems (a typo'd `disabled_rules`/
    /// `severity_overrides` entry did nothing), not degenerate-output signals, and belong on the
    /// consumer's config-warnings channel even though this module (not the config front-end) is what
    /// computes them — only analysis time has the known-rule-id set a config parser never sees. The
    /// caller (`zzop-engine`'s `analyze::assemble`/`envelope::analyze_envelope`) routes this into
    /// `AnalyzeOutput::config_warnings` instead of `AnalyzeOutput::warnings`. Deliberately does NOT
    /// include `unknown_suppression_rule_ids`'s self-report — that one stays in `warnings` (unchanged,
    /// out of this split's scope). Empty when every entry matched a known id, or none were given.
    /// `#[serde(default)]` so a pre-split serialized form (which has no such field) still deserializes.
    #[serde(default)]
    pub config_warnings: Vec<String>,
}

impl std::ops::Deref for AnalysisDiagnostics {
    type Target = DiagnosticsInput;
    fn deref(&self) -> &DiagnosticsInput {
        &self.input
    }
}

/// Renders a count with the correctly-pluralized noun ("1 entry" / "2 entries") — replaces the old
/// "N entry/entries" double-word form, which rendered literally (unresolved) in user-facing warning text.
fn entry_count(n: usize) -> String {
    if n == 1 {
        "1 entry".to_string()
    } else {
        format!("{n} entries")
    }
}

pub fn build_diagnostics(i: DiagnosticsInput) -> AnalysisDiagnostics {
    let mut warnings: Vec<String> = Vec::new();
    let mut config_warnings: Vec<String> = Vec::new();

    // Degenerate dep graph = 0 edges OR pathologically few relative to file count (< 1 edge per 20 files in a
    // non-tiny repo). A healthy graph has ~1+ edges/file; near-zero means imports went unresolved (custom module
    // system / URL import scheme like Deno's `ext:` / `npm:`). Catching the near-zero case too closes a
    // silent-failure gap where a single stray edge suppressed the warning while every dep-based signal (circular,
    // coupling, criticality, seams) was still hollow.
    // A single-file package trivially has no INTERNAL edges (nothing to import from) -- that is expected, not a
    // parser failure, so only flag 0 edges when there are >=2 files that could have linked. A real single-file
    // parse failure still surfaces via the 0-symbols warning below.
    if i.files > 1 && i.dep_edges == 0 {
        warnings.push(format!(
            "0 internal dependency edges across {} files — EITHER the code genuinely has few internal imports (a tiny/entry package, or framework wiring that isn't an ESM import edge — e.g. Angular `@NgModule`/decorator DI), OR the parser could not resolve this module system. Dep graph, circular, fan-in/out, coupling are empty either way; check whether these files really import each other before assuming a parser gap.",
            i.files
        ));
    } else if i.files >= MIN_FILES_FOR_EDGE_CHECK
        && (i.dep_edges as f64) < (i.files as f64) * MIN_EDGES_PER_FILE
    {
        warnings.push(format!(
            "only {} dependency edge(s) across {} files — the parser likely could not resolve this module system (custom/URL import scheme?), so imports went unresolved (dep graph, circular, fan-in/out, coupling, criticality, seams are empty or meaningless).",
            i.dep_edges, i.files
        ));
    }

    if i.files > 0 && i.symbols == 0 {
        warnings.push(format!(
            "0 exported symbols across {} files — EITHER the tree genuinely exports nothing (entry-point/binary-style code with no public API — e.g. a CLI `main()` or script that only calls its own internal functions) OR export/symbol detection failed for this module system. Symbol risk, symbol cycles, dead-export are empty either way; check whether these files really have no exports before assuming a parser gap.",
            i.files
        ));
    }

    if i.total_modules > 1 && i.concrete_modules == 0 {
        warnings.push(format!(
            "all {} modules classified abstract — export detection likely failed (main-sequence / SDP scores are meaningless).",
            i.total_modules
        ));
    }

    // Git-window warnings only make sense once git was actually attempted for this run (see
    // `GitDiagnosticsInput`'s doc) — `i.git == None` means the module simply has nothing to say about
    // commits/changes, so none of the checks below run at all.
    if let Some(git) = &i.git {
        if i.files > 0 && git.total_changes == 0 {
            // Distinguish the two very different causes of "no churn", because conflating them sends the reader
            // hunting for a tooling bug when the real cause is usually just an empty time window. If the window
            // holds 0 commits, the history is (almost always) entirely older than `--since` -- name that first,
            // with the fix. Only when commits DID land in the window but touched none of these files is a
            // pathspec/submodule mismatch the likely culprit.
            if git.commits == 0 {
                let msg = match &git.since {
                    Some(since) => format!(
                        "0 commits in the analyzed window — the history is likely entirely older than the configured `git.since: \"{}\"` window. Widen or remove `since` in zzop.config.jsonc (omitting `since` entirely analyzes full history). Churn, FIX, lifecycle, coupling, bug-prone are all empty until the window includes real history.",
                        since
                    ),
                    None => "0 commits touch these files even over the full history (`git.since` was already omitted) — widening the window further will NOT help. Most likely a git submodule (the parent repo records only the submodule's SHA, not its file history — point a `roots` entry / `trees[].root` at the submodule checkout so it is analyzed as its own tree), or brand-new/untracked files. Churn, FIX, lifecycle, coupling, bug-prone stay empty.".to_string(),
                };
                warnings.push(msg);
            } else {
                warnings.push(format!(
                    "0 git changes across {} files despite {} commit(s) in the window — the git pathspec / source-extension filter likely does not match this repo's layout/language, OR the code lives in a git submodule (submodule commits are not in the parent repo's log — point a `roots` entry / `trees[].root` at the submodule checkout so it is analyzed as its own tree). Churn, FIX, lifecycle, coupling, bug-prone are all empty.",
                    i.files, git.commits
                ));
            }
        } else if git.commits <= 1 && i.files > 1 {
            // History present but degenerate: a single commit (typically a shallow/`--depth=1` clone, or a freshly
            // squashed repo) gives every file the same changeCount, so churn carries no information. Risk/lifecycle
            // rank by size alone and the hotspot signal collapses to pure LOC -- warn so the consumer fetches full
            // history before trusting them.
            warnings.push(format!(
                "only {} commit(s) in the analyzed window across {} files — this is a genuinely thin history, not necessarily a tool problem. Causes: a shallow clone (`git clone --depth=1` → fix with `git fetch --unshallow`), a freshly squashed or young repo (nothing to fix — the repo simply has few commits), or a `git.since` window (in zzop.config.jsonc) narrower than the history — widen it, or omit `since` entirely for full history. With so little history, churn-based signals (hotspot, lifecycle, bug-prone, silent-criticality) rank by size alone.",
                git.commits, i.files
            ));
        }

        if git.total_changes > 0 && git.tagged_changes == 0 {
            warnings.push(format!(
                "0 of {} file-changes classified by commit type — the commit-message convention was not recognized (recognized forms: a `[FIX]`-style bracket prefix, or a `fix:`/`fixed:`/`bugfix:`/`hotfix:` keyword prefix at the subject start). If this repo uses a different convention, teach it via `git.commitTypePatterns` in zzop.config.jsonc (a `[{{ pattern, tag }}, ...]` table that replaces the default vocabulary). Until commit subjects match one of the recognized forms or a custom table, bug-prone, FIX hotspots, versioning-candidate, fixRatio stay disabled.",
                git.total_changes
            ));
        }
    }

    // Unlike every other check above, this one is not "empty/degenerate output" — it flags a config entry
    // that had NO effect at all (a typo'd/stale `disabled_rules` id), which is otherwise indistinguishable
    // from a working exclusion (see `unknown_disabled_rule_ids`'s doc). This is a CONFIG problem, not a
    // degenerate-output signal, so it rides `config_warnings` (not `warnings`) — see `AnalysisDiagnostics::
    // config_warnings`'s doc for why this module still computes it despite the split.
    if !i.unknown_disabled_rule_ids.is_empty() {
        let mut ids = i.unknown_disabled_rule_ids.clone();
        ids.sort();
        ids.dedup();
        config_warnings.push(format!(
            "disabled rules have {} matching no known rule id: {} — these did NOT disable anything (check for a typo; a valid id is a bare pack id, a native analysis id, or a \"<pack>/<rule>\" id; config dialect `rules: {{ \"<id>\": \"off\" }}` for a rule id, or `packs.disabled` for a bare pack id; embedders: `disabledRules`).",
            entry_count(ids.len()),
            ids.join(", ")
        ));
    }

    // Same "config entry had NO effect at all" class as `unknown_disabled_rule_ids` above, over
    // `severity_overrides` instead — see that field's doc and `unknown_severity_override_ids`'s doc for why
    // the valid-id enumeration named here (no bare pack id) differs from the disabled-rules one. Also
    // `config_warnings`, same reasoning as `unknown_disabled_rule_ids` just above.
    if !i.unknown_severity_override_ids.is_empty() {
        let mut ids = i.unknown_severity_override_ids.clone();
        ids.sort();
        ids.dedup();
        config_warnings.push(format!(
            "severity overrides have {} matching no known rule id: {} — these did NOT remap any finding's severity (check for a typo; a valid id is a native analysis id or a \"<pack>/<rule>\" id; config dialect `rules: {{ \"<id>\": \"<severity>\" }}`, embedders: `severityOverrides`).",
            entry_count(ids.len()),
            ids.join(", ")
        ));
    }

    // Same "config entry had NO effect at all" class as the two checks above, over `suppressions` instead —
    // see that field's doc for why this is a distinct failure from `unmatched_suppression_warnings`'s dead
    // path/glob filter (bad rule id vs. dead file filter; both can fire for the same entry, and that is
    // correct — they are orthogonal diagnostics over the same config entry).
    if !i.unknown_suppression_rule_ids.is_empty() {
        let mut ids = i.unknown_suppression_rule_ids.clone();
        ids.sort();
        ids.dedup();
        warnings.push(format!(
            "suppressions have {} whose rule matches no known rule id: {} — these did NOT suppress anything (check for a typo; a valid id is a native analysis id or a \"<pack>/<rule>\" id; config dialect `rules: {{ \"<id>\": {{ \"exclude\": [...] }} }}`, embedders: `suppressions`).",
            entry_count(ids.len()),
            ids.join(", ")
        ));
    }

    AnalysisDiagnostics {
        input: i,
        warnings,
        config_warnings,
    }
}

#[cfg(test)]
mod tests;
