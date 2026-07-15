//! Meta-tests — machine-enforced cross-cutting contracts every shipped rule (DSL and native) must honor.
//!
//! These contracts previously existed only as human convention (a prior audit found real drift: DSL rules
//! shipped with no `suppress_marker`, rule messages that never told the reader how to exclude a finding,
//! and `docs/rules/catalog.md` totals out of sync with the actual pack/registry data). This file loads
//! every shipped DSL pack (`rules/dsl/*.json`, via `zzop_core::load_dsl_packs`) and the native registry
//! (`zzop_engine::register_all_native`, composing `zzop_rules_graph`/`zzop_rules_http`/
//! `zzop_rules_cross_layer`/`zzop_rules_schema`/`zzop_metrics`'s own `register_native_analyses`) fresh in
//! each test, so drift in either is caught the next time
//! `cargo test --workspace` runs — no test here hand-copies rule data, everything is read from the same
//! source the engine itself loads at runtime.
//!
//! See `docs/rules/authoring-guide.md`'s "Machine-enforced contracts" section for the author-facing
//! summary of what a failing test here means.
//!
//! ## Contracts covered
//! 1. **Marker presence + convention** (`every_dsl_rule_has_a_non_empty_suppress_marker`,
//!    `suppress_markers_are_unique_within_each_pack`,
//!    `every_suppress_marker_follows_the_dash_ok_naming_convention`) — every DSL rule has a non-empty
//!    `suppress_marker`, no two rules in the same pack share one (co-suppression risk), and every marker
//!    keeps the `-ok` suffix shape users learn from the first rule they suppress.
//! 2. **Message triple** (`every_dsl_rule_message_documents_how_to_exclude_it`) — every DSL rule's
//!    `message` names its own suppress marker OR the literal `disabled_rules`/`disabledRules` string — the
//!    "how to exclude" leg of the problem+fix+exclude finding contract.
//! 3. **Native message contract** (`native_rule_files_that_build_findings_mention_disabled_rules`,
//!    `disable_hint_literal_args_are_known_ids_matching_the_files_own_findings`) — a
//!    pragmatic grep-based proxy (native findings are built in code, not read from declarative data — see
//!    each test's own doc for exactly what this can and cannot prove). The first accepts either a literal
//!    `disabled_rules` mention OR a call to the shared `zzop_core::finding::disable_hint` builder every
//!    native message's disable-hint fragment now goes through (see that test's doc for why the OR is load-
//!    bearing, not incidental); the second proves each literal `disable_hint("<id>")` argument is a real id
//!    matching what the same file actually emits (a wrong-id hint = a silent config no-op for the user).
//! 4. **Id hygiene** (`dsl_pack_ids_are_unique_across_packs`, `dsl_rule_ids_are_unique_within_each_pack`,
//!    `no_dsl_id_collides_with_a_native_analysis_id`).
//! 5. **Catalog sync** (`catalog_totals_match_loaded_rule_and_analysis_counts`,
//!    `catalog_mentions_every_native_analysis_id`, `catalog_mentions_every_dsl_pack_id`) —
//!    `docs/rules/catalog.md`'s stated totals and id lists match the loaded reality.
//! 6. **Determinism guard** (`loading_the_same_packs_dir_twice_yields_identical_pack_lists`) — loading
//!    `rules/dsl` twice yields byte-identical `RulePackDef` data in the same order (cheap regression net
//!    for map/directory-iteration-order bugs in pack parsing).
//! 7. **Pack-folder test wiring** (`every_non_stub_pack_folder_has_a_colocated_tests_rs_and_a_cargo_toml_test_entry`)
//!    — every `rules/dsl/<pack>/` folder that ships at least one rule has a co-located `<pack>.rs` AND a
//!    matching `[[test]]` entry in `rules/Cargo.toml` (see `rules/README.md`'s folder layout). Stub packs
//!    (0 rules) are exempt.
//! 8. **Kernel is rule-vocabulary-free** (`kernel_core_carries_no_native_analysis_id_string_literal`) —
//!    `crates/core` must not name any registered native
//!    analysis id as a quoted string literal (a pragmatic grep-proxy — see that test's own doc for what it
//!    can/cannot prove).
//! 9. **Bare-word anchoring** (`dangerous_bare_words_are_syntax_anchored_not_bare_prose_matches`) — no
//!    shipped DSL rule's regex matches a keyword-shaped English word (`do`/`for`/`while`/`update`/`delete`/
//!    `select`) as a bare `\bword\b` with no adjacent syntax anchor — the defect class that shipped live in
//!    `perf/api-in-loop` (bare `\bdo\b` matched inside prose like `"logged in to do this"`) and
//!    `be-security/sql-taint` (bare `UPDATE` matched inside prose), both fixed in the same commit that
//!    added this contract (a pragmatic textual-proximity proxy, not a regex semantics engine — see that
//!    test's own doc for exactly what it can/cannot prove).
//! 10. **Kebab-case id hygiene** (`rule_ids_are_kebab_case`) — every loaded DSL pack id, every loaded DSL
//!     rule's own id, and every registered native analysis id, each checked after stripping an optional
//!     leading `"cross-layer/"` prefix, matches `^[a-z0-9]+(-[a-z0-9]+)*$` (lowercase, single hyphens
//!     between groups, no camelCase/snake_case/uppercase). This is the regression guard for the cross-layer
//!     vocabulary-unification rename underway across this codebase (`unsafeReadEndpoint`/
//!     `nonIdempotentWrite`/`fe-consumes-unprovided`/`cross-layer/dead-mutation-endpoint`/
//!     `cross-layer/dangling-mutation` and others converted to this one kebab-case convention) — without a
//!     machine check, a future rule could silently reintroduce the exact camelCase-vs-kebab-case drift that
//!     effort just cleaned up.
//! 11. **Reference validation** (`every_flag_reference_in_shipped_source_names_a_real_cli_or_external_tool_flag`,
//!     `every_config_context_backtick_token_in_shipped_source_names_a_real_config_path_or_key`) — a message
//!     audit found user-facing strings recommending config keys/flags that DO NOT EXIST (`--since=all`,
//!     `--repo=`, `scanners.vocabulary.commitTypePatterns`). These two tests are the machine contract that
//!     prevents recurrence: every `--flag`-shaped token and every backtick-quoted config-key-shaped token
//!     sitting near the word "config" in a shipped Rust/JS source file must name a real knob from
//!     `packages/cli/lib/config-surface.json` — the single vocabulary file this test shares with
//!     `packages/cli/lib/mapper.js`'s `KNOWN_KEYS`. See each test's own doc for exactly what its pragmatic
//!     textual-proximity extraction can and cannot prove.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use zzop_core::{load_dsl_packs, Matcher, RuleDef, RuleMeta, RulePackDef, RuleRegistry};
use zzop_engine::register_all_native;

// ---------------------------------------------------------------------------------------------
// Shared fixtures — every test loads the SAME real data the engine loads at runtime, never a
// hand-copied inline fixture, so this file cannot drift from what actually ships.
// ---------------------------------------------------------------------------------------------

fn dsl_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl")
}

fn native_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/native")
}

fn catalog_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/rules/catalog.md")
}

/// Loads every `rules/dsl/*.json` pack, failing loudly (not silently skipping) if any file fails to
/// parse — a load error would otherwise hide real rules from every contract test below, which is worse
/// than a normal test failure.
fn load_all_packs() -> Vec<RulePackDef> {
    let result = load_dsl_packs(&dsl_dir());
    assert!(
        result.errors.is_empty(),
        "DSL pack load errors (fix the pack before rule_contracts.rs can evaluate it): {:?}",
        result.errors
    );
    result.packs.into_iter().map(|(_, pack)| pack).collect()
}

/// Every registered native analysis's metadata, owned (not borrowed from a local `RuleRegistry`) so
/// callers can use it without threading a registry lifetime through every test.
fn native_metas() -> Vec<RuleMeta> {
    let mut registry = RuleRegistry::new();
    register_all_native(&mut registry);
    registry.metas().into_iter().cloned().collect()
}

fn catalog_text() -> String {
    let path = catalog_path();
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------------------------
// 1. Marker presence
// ---------------------------------------------------------------------------------------------

/// Every DSL rule ships a non-empty `suppress_marker`. A rule with no marker (or an empty-string one)
/// cannot be suppressed inline (see `RuleDef::suppress_marker`'s doc in `crates/core/src/dsl.rs`) — the
/// only way to quiet a single false positive is `disabled_rules`, which throws away every future true
/// positive from that rule too. A prior audit found DSL rules shipped with no marker at all; this test
/// makes that class of drift a hard failure instead of a convention someone has to remember.
#[test]
fn every_dsl_rule_has_a_non_empty_suppress_marker() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        for rule in &pack.rules {
            let ok = rule
                .suppress_marker
                .as_deref()
                .is_some_and(|m| !m.trim().is_empty());
            if !ok {
                offenders.push(format!("{}/{}", pack.id, rule.id));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "DSL rules with no non-empty suppress_marker: {offenders:#?}"
    );
}

/// Within one pack, no two rules may share a `suppress_marker`. Two rules sharing a marker co-suppress: a
/// `// marker-ok` comment a reader placed to vet ONE rule's finding silently also suppresses the OTHER
/// rule's finding wherever its own line/lookback window overlaps — the reader never opted into that.
#[test]
fn suppress_markers_are_unique_within_each_pack() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        let mut by_marker: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for rule in &pack.rules {
            if let Some(marker) = rule.suppress_marker.as_deref() {
                if !marker.trim().is_empty() {
                    by_marker.entry(marker).or_default().push(rule.id.as_str());
                }
            }
        }
        for (marker, rules) in by_marker {
            if rules.len() > 1 {
                offenders.push(format!(
                    "pack `{}`: marker `{marker}` shared by rules {rules:?} (co-suppression risk)",
                    pack.id
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "duplicate suppress_marker within a pack: {offenders:#?}"
    );
}

/// Every `suppress_marker` ends in `-ok` — the naming convention every one of the shipped markers follows
/// (a 2026-07-13 uniformity sweep measured 112/112) and the shape the authoring guide's example teaches
/// (`debug-token-ok`). The convention is load-bearing for users, not cosmetic: someone who has learned
/// `// <marker>-ok` from one rule will type that shape for the next rule from memory, and a rule whose
/// marker deviates (`nplus1_allow`, `skip-x`) silently fails to suppress for them. Deviating on purpose is
/// a policy change: adjust this test in the same commit and say why.
#[test]
fn every_suppress_marker_follows_the_dash_ok_naming_convention() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        for rule in &pack.rules {
            if let Some(marker) = rule.suppress_marker.as_deref() {
                if !marker.trim().is_empty() && !marker.ends_with("-ok") {
                    offenders.push(format!("{}/{}: `{marker}`", pack.id, rule.id));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "suppress_marker values deviating from the `-ok` suffix convention every other marker follows: \
         {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Message triple — problem (the rest of `message`) + fix (the rest of `message`) + exclude (this leg)
// ---------------------------------------------------------------------------------------------

/// Every DSL rule's `message` names its own suppress marker OR the literal `disabled_rules`/`disabledRules`
/// string somewhere in the text — the "how to exclude" leg of zzop's finding contract (every finding must
/// tell the reader the problem, the fix, AND how to turn it off — zzop's finding-output design
/// principle; see docs/rules/authoring-guide.md's quality bar). A rule that legitimately has no
/// per-finding marker (native-analysis-style disable-only rules ported into the DSL, if any ever are) still
/// passes via the `disabled_rules` leg — this test accepts EITHER, not just the marker.
#[test]
fn every_dsl_rule_message_documents_how_to_exclude_it() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        for rule in &pack.rules {
            let marker_leg = rule
                .suppress_marker
                .as_deref()
                .is_some_and(|m| !m.trim().is_empty() && rule.message.contains(m));
            let disabled_leg =
                rule.message.contains("disabled_rules") || rule.message.contains("disabledRules");
            if !(marker_leg || disabled_leg) {
                offenders.push(format!(
                    "{}/{} (suppress_marker={:?}) — message mentions neither its own marker nor \
                     disabled_rules/disabledRules",
                    pack.id, rule.id, rule.suppress_marker
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "rule messages missing the \"how to exclude\" leg: {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. Native message contract — pragmatic grep proxy, not a semantic proof (see doc below)
// ---------------------------------------------------------------------------------------------

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Every `rules/native/*/src/**/*.rs` file, recursively (native rule crates nest modules, e.g.
/// `rules-graph/src/cross_layer/*.rs` — a non-recursive `*/src/*.rs` glob would miss those).
fn native_rs_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(&native_dir(), &mut out);
    out.sort();
    out
}

/// Native findings are built in code (`Finding { rule_id: "...", message: format!(...), .. }`), not read
/// from declarative pack data the way contract 2 above inspects DSL `message` fields directly — there is no
/// single field this test can statically evaluate the way it can `RuleDef::message`. Instead this is a
/// pragmatic grep: read every native rule source file, and if the file contains a literal `rule_id: "`
/// token (i.e. it constructs at least one `Finding` for a hardcoded rule id), assert the SAME file also
/// EITHER contains the literal substring `disabled_rules` somewhere, OR calls the shared
/// `zzop_core::finding::disable_hint(` builder.
///
/// The two-leg OR is not incidental: a 2026-07-10 audit found the disable-hint fragment
/// (`` `rules: { "<id>": "off" }` (embedders: `disabled_rules`) ``) hand-written at ~34 native message call
/// sites, drifted at 31 of them, plus one plain-string (non-`format!`) site that shipped a literal `{{`
/// because a mechanical format!-escaping sweep assumed every site was inside a `format!` call. Every site
/// was converted to call `disable_hint`, the single builder that fragment now has (see its own doc comment
/// and unit tests in `crates/core/src/finding.rs`) — so the literal `disabled_rules` string itself moved
/// OUT of most native rule files and into that one function's `format!` body. A file whose message text is
/// spliced around `disable_hint`'s output (e.g. `rules/native/rules-http/src/route_shadowing.rs`, whose
/// sentence reads "...disable {tail}" rather than "...Disable via config...") may carry neither `rule_id: "`
/// co-located `disabled_rules` text NOR the literal substring at all anymore — only the `disable_hint(` call
/// site — so a check requiring the literal string alone would now false-positive on every converted file.
/// The `disabled_rules` leg stays (not just `disable_hint(`) because a file's own `#[cfg(test)]` module
/// legitimately still asserts `message.contains("disabled_rules")` as a regression pin, and a hand-authored
/// file that never adopts the helper but still spells out the convention correctly should not be forced to.
///
/// **What this proves**: a file that builds at least one `Finding` via a literal `rule_id: "..."`
/// assignment also EITHER names `disabled_rules` OR calls `disable_hint(` somewhere in its own source — in
/// every rule module audited while writing this test, that satisfies the "how to exclude" leg of the
/// finding's message, either directly (pre-sweep sites) or indirectly through the shared builder
/// (post-sweep sites, whose own `disable_hint` unit tests in `crates/core/src/finding.rs` pin the
/// `disabled_rules` mention at the one source of truth).
///
/// **What this CANNOT prove** (documented per the task's "keep it pragmatic" instruction, not silently
/// assumed):
/// - That the `disabled_rules` mention (or `disable_hint(` call) is actually inside the live
///   `Finding::message` value reaching the user, as opposed to a doc comment describing the convention or a
///   `#[cfg(test)]`-only assertion/import. This is a file-level co-occurrence check, not an AST-level check
///   tying either substring to a specific `Finding` construction site.
/// - That a rule id built dynamically (a variable, a format! expansion, a shared constructor in a
///   different file) is caught at all — only the literal token `rule_id: "` is detected, so a native rule
///   authored in an unusual shape can slip past this test silently.
/// - Anything about DSL packs or JS quick-rules — out of scope here; contract 2 above covers DSL directly,
///   since DSL `message` IS declarative data this crate can inspect precisely.
///
/// A failure here is a strong, actionable signal (the flagged file almost certainly ships a finding with no
/// exclude-hint), but is not a certainty — read the flagged file before assuming the fix is "add one
/// sentence to a format! string" or "call disable_hint."
#[test]
fn native_rule_files_that_build_findings_mention_disabled_rules() {
    let mut offenders = Vec::new();
    for path in native_rs_files() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if text.contains("rule_id: \"")
            && !text.contains("disabled_rules")
            && !text.contains("disable_hint(")
        {
            offenders.push(path.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "native rule source files construct a Finding (literal `rule_id: \"...\"`) but never mention \
         `disabled_rules` and never call `disable_hint(` anywhere in the same file — the finding's message \
         likely omits the \"how to exclude\" hint every other native rule includes (see this test's own doc \
         comment for exactly what this check can/cannot prove): {offenders:#?}"
    );
}

/// Every literal `disable_hint("<id>")` argument in shipped native/engine source (a) names a KNOWN id
/// (native analysis id or `"<pack>/<rule>"` DSL id) and (b), when the same file also constructs findings
/// via literal `rule_id: "..."`, matches one of THOSE ids. The test above proves a hint exists; this one
/// proves the hint is not a lie — a hint naming a stale or copy-pasted-from-another-rule id sends the user
/// to disable the wrong thing, and the config entry they add becomes a silent no-op (the exact class the
/// unknown-disabled/override/suppression warnings were built to catch — this seals it at the SOURCE).
/// Same pragmatic-grep caveats as above: only literal `disable_hint("...")` and `rule_id: "..."` tokens
/// are seen; a dynamically built hint or id is invisible here. A 2026-07-13 sweep measured 0 violations
/// across 33 files / 35 literal call shapes; this pins that state.
#[test]
fn disable_hint_literal_args_are_known_ids_matching_the_files_own_findings() {
    fn quoted_after(text: &str, needle: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = text;
        while let Some(pos) = rest.find(needle) {
            let after = &rest[pos + needle.len()..];
            match after.find('"') {
                Some(end) => {
                    out.insert(after[..end].to_string());
                    rest = &after[end..];
                }
                None => break,
            }
        }
        out
    }

    let mut known: BTreeSet<String> = native_metas().iter().map(|m| m.id.clone()).collect();
    for pack in load_all_packs() {
        for rule in &pack.rules {
            known.insert(format!("{}/{}", pack.id, rule.id));
        }
    }

    let mut files = native_rs_files();
    collect_rs_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );

    let mut offenders = Vec::new();
    for path in files {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let hints = quoted_after(&text, "disable_hint(\"");
        if hints.is_empty() {
            continue;
        }
        let emitted = quoted_after(&text, "rule_id: \"");
        for hint in hints {
            if !known.contains(&hint) {
                offenders.push(format!(
                    "{}: disable_hint(\"{hint}\") names no known rule/analysis id",
                    path.display()
                ));
            } else if !emitted.is_empty() && !emitted.contains(&hint) {
                offenders.push(format!(
                    "{}: disable_hint(\"{hint}\") but this file's findings carry rule_id {:?}",
                    path.display(),
                    emitted.iter().collect::<Vec<_>>()
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "disable_hint(...) call sites whose literal id argument is stale, mistyped, or belongs to a \
         different rule than the file emits — the hint would send users to disable the wrong id (a silent \
         config no-op): {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Id hygiene
// ---------------------------------------------------------------------------------------------

#[test]
fn dsl_pack_ids_are_unique_across_packs() {
    let packs = load_all_packs();
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for pack in &packs {
        *counts.entry(pack.id.as_str()).or_insert(0) += 1;
    }
    let dups: Vec<&str> = counts
        .into_iter()
        .filter(|&(_, c)| c > 1)
        .map(|(id, _)| id)
        .collect();
    assert!(
        dups.is_empty(),
        "duplicate DSL pack ids across rules/dsl/*.json: {dups:?}"
    );
}

#[test]
fn dsl_rule_ids_are_unique_within_each_pack() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
        for rule in &pack.rules {
            *counts.entry(rule.id.as_str()).or_insert(0) += 1;
        }
        for (id, c) in counts {
            if c > 1 {
                offenders.push(format!("{}/{id} (x{c})", pack.id));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "duplicate rule ids within a pack: {offenders:#?}"
    );
}

/// No DSL `"pack"` id and no DSL `"pack/rule"` id may collide with a native analysis id — `is_enabled`
/// (`crates/core/src/registry.rs`) gates every layer through ONE shared exact-string-match id space, so a
/// collision would mean disabling one layer's rule via `disabled_rules` silently also disables an unrelated
/// layer's rule (or a pack id colliding with a bare native id, same hazard).
#[test]
fn no_dsl_id_collides_with_a_native_analysis_id() {
    let packs = load_all_packs();
    let native_ids: BTreeSet<String> = native_metas().into_iter().map(|m| m.id).collect();
    let mut offenders = Vec::new();
    for pack in &packs {
        if native_ids.contains(&pack.id) {
            offenders.push(format!(
                "DSL pack id `{}` collides with a native analysis id",
                pack.id
            ));
        }
        for rule in &pack.rules {
            let full = format!("{}/{}", pack.id, rule.id);
            if native_ids.contains(&full) {
                offenders.push(format!(
                    "DSL rule id `{full}` collides with a native analysis id"
                ));
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
}

// ---------------------------------------------------------------------------------------------
// 5. Catalog sync — docs/rules/catalog.md must match the loaded reality, not a hand-updated snapshot
// ---------------------------------------------------------------------------------------------

/// Parses `docs/rules/catalog.md`'s totals sentence (the `**Totals** (...): N DSL packs, N DSL rules, N
/// native analysis ids.` line near the top of the file) and asserts the three numbers match what
/// `load_dsl_packs`/`register_native_analyses` actually produce. The sentence is intentionally phrased in
/// one fixed, easily-`regex`-parsed shape (restructured for this test — see the doc's own totals line) so a
/// human editing prose around it doesn't accidentally break this test's ability to find the numbers; if you
/// legitimately need to reword that sentence, keep the `N DSL packs, N DSL rules, N native analysis ids`
/// clause's shape intact (or update this regex to match, deliberately, in the same commit).
#[test]
fn catalog_totals_match_loaded_rule_and_analysis_counts() {
    let text = catalog_text();
    let re =
        regex::Regex::new(r"(\d+)\s+DSL packs,\s*(\d+)\s+DSL rules,\s*(\d+)\s+native analysis ids")
            .expect("static regex");
    let caps = re.captures(&text).unwrap_or_else(|| {
        panic!(
            "docs/rules/catalog.md's totals sentence not found in the expected \"N DSL packs, N DSL \
             rules, N native analysis ids\" shape — either the doc's totals line was reworded (update it \
             back to that shape, or update this test's regex in the same commit) or the file moved"
        )
    });
    let stated_packs: usize = caps[1].parse().expect("digits");
    let stated_rules: usize = caps[2].parse().expect("digits");
    let stated_natives: usize = caps[3].parse().expect("digits");

    let packs = load_all_packs();
    let actual_rules: usize = packs.iter().map(|p| p.rules.len()).sum();
    let actual_natives = native_metas().len();

    assert_eq!(
        stated_packs,
        packs.len(),
        "catalog.md states {stated_packs} DSL packs, but rules/dsl/*.json loads {}",
        packs.len()
    );
    assert_eq!(
        stated_rules, actual_rules,
        "catalog.md states {stated_rules} DSL rules, but the loaded packs total {actual_rules}"
    );
    assert_eq!(
        stated_natives, actual_natives,
        "catalog.md states {stated_natives} native analysis ids, but register_native_analyses registers \
         {actual_natives}"
    );
}

#[test]
fn catalog_mentions_every_native_analysis_id() {
    let text = catalog_text();
    let missing: Vec<String> = native_metas()
        .into_iter()
        .map(|m| m.id)
        .filter(|id| !text.contains(id.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "native analysis ids registered but absent from docs/rules/catalog.md's text: {missing:?}"
    );
}

#[test]
fn catalog_mentions_every_dsl_pack_id() {
    let text = catalog_text();
    let packs = load_all_packs();
    let missing: Vec<&str> = packs
        .iter()
        .map(|p| p.id.as_str())
        .filter(|id| !text.contains(id))
        .collect();
    assert!(
        missing.is_empty(),
        "DSL pack ids loaded but absent from docs/rules/catalog.md's text: {missing:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 6. Determinism guard — pack-load order/content must not depend on OS directory-iteration order
// ---------------------------------------------------------------------------------------------

/// Loading `rules/dsl` twice must yield the same packs, in the same order, with the same content.
/// `RulePackDef` (and everything it nests: `RuleDef`, `Matcher`, ...) derives `Debug` but not
/// `Serialize`/`PartialEq`, so this test uses `{:?}` (Debug) formatting as a pragmatic serialization
/// stand-in for the equality check — good enough to catch the class of bug this guards against (a
/// nondeterministic map/directory-listing iteration order leaking into parsed field/rule order), which is
/// exactly what `pack_loader::load_dsl_packs`'s own "sorted by file name" doc comment promises never
/// happens.
#[test]
fn loading_the_same_packs_dir_twice_yields_identical_pack_lists() {
    let dir = dsl_dir();
    let first = load_dsl_packs(&dir);
    let second = load_dsl_packs(&dir);

    assert_eq!(
        first.errors.len(),
        second.errors.len(),
        "load-error count differs between two loads of the same directory"
    );
    assert_eq!(
        first.packs.len(),
        second.packs.len(),
        "pack count differs between two loads of the same directory"
    );

    let first_ids: Vec<&str> = first.packs.iter().map(|(_, p)| p.id.as_str()).collect();
    let second_ids: Vec<&str> = second.packs.iter().map(|(_, p)| p.id.as_str()).collect();
    assert_eq!(
        first_ids, second_ids,
        "pack load ORDER differs between two loads of the same directory"
    );

    for ((path_a, pack_a), (path_b, pack_b)) in first.packs.iter().zip(second.packs.iter()) {
        assert_eq!(
            path_a, path_b,
            "pack path differs at the same index between two loads"
        );
        assert_eq!(
            format!("{pack_a:?}"),
            format!("{pack_b:?}"),
            "pack `{}` deserialized differently across two loads of the same file",
            pack_a.id
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 7. Pack-folder test wiring — every non-stub pack folder has a co-located <pack>.rs AND a
//    matching [[test]] entry in rules/Cargo.toml
// ---------------------------------------------------------------------------------------------

/// Reads `rules/Cargo.toml`'s raw text so the pack-folder-wiring test below can pragmatically check for a
/// `path = "dsl/<pack>/<pack>.rs"` substring, without pulling in a TOML parser dependency this workspace
/// otherwise has no use for (same "keep it pragmatic" approach as contract 3's grep-based check).
fn rule_packs_cargo_toml_text() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/Cargo.toml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Every `rules/dsl/<pack>/` folder that ships at least one rule must have a co-located `<pack>.rs`
/// (`rules/dsl/<pack>/<pack>.rs`) AND a matching `[[test]]` entry in `rules/Cargo.toml` whose `path` points
/// at it — otherwise the pack's end-to-end coverage would silently never run under `cargo test
/// --workspace`. Stub packs (0 rules — see `rules/README.md`'s "Stub packs") are exempt: there is nothing
/// to exercise yet. Pragmatic textual check (no TOML parser dependency, no AST comparison): looks for the
/// literal `dsl/<pack>/<pack>.rs` substring (forward slashes, as Cargo requires even on Windows) anywhere
/// in `rules/Cargo.toml`'s text.
#[test]
fn every_non_stub_pack_folder_has_a_colocated_tests_rs_and_a_cargo_toml_test_entry() {
    let dsl_root = dsl_dir();
    let entries = fs::read_dir(&dsl_root)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", dsl_root.display()));
    let cargo_toml_text = rule_packs_cargo_toml_text();

    let mut pack_dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    pack_dirs.sort();

    let mut offenders = Vec::new();
    for pack_dir in pack_dirs {
        let pack_name = pack_dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("pack dir has a UTF-8 name")
            .to_string();

        // This pack's own JSON file(s) directly under the folder (mirrors load_dsl_packs's depth-1
        // subdirectory scan) — sum rule counts across them (normally exactly one file per folder).
        let json_files: Vec<PathBuf> = fs::read_dir(&pack_dir)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", pack_dir.display()))
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();

        let rule_count: usize = json_files
            .iter()
            .map(|p| {
                let text = fs::read_to_string(p)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", p.display()));
                let pack: RulePackDef = serde_json::from_str(&text)
                    .unwrap_or_else(|e| panic!("failed to parse {}: {e}", p.display()));
                pack.rules.len()
            })
            .sum();

        if rule_count == 0 {
            continue; // stub pack — exempt
        }

        let tests_rs = pack_dir.join(format!("{pack_name}.rs"));
        if !tests_rs.is_file() {
            offenders.push(format!(
                "rules/dsl/{pack_name}/ ships {rule_count} rule(s) but has no co-located {pack_name}.rs"
            ));
            continue;
        }

        let expected_path_fragment = format!("dsl/{pack_name}/{pack_name}.rs");
        if !cargo_toml_text.contains(&expected_path_fragment) {
            offenders.push(format!(
                "rules/dsl/{pack_name}/{pack_name}.rs exists but rules/Cargo.toml has no [[test]] entry \
                 with path = \"{expected_path_fragment}\""
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "pack-folder test wiring drift: {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 8. Kernel is rule-vocabulary-free — crates/core must not name a native analysis id
// ---------------------------------------------------------------------------------------------

/// `crates/core/src`, resolved relative to this crate's own manifest dir (same "sibling package"
/// pattern as `native_dir`/`dsl_dir`/`catalog_path` above).
fn core_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/src")
}

/// Every `crates/core/src/**/*.rs` file, recursively, EXCEPT `registry.rs` and `dsl.rs` — see
/// `kernel_core_carries_no_native_analysis_id_string_literal`'s doc for why those two are exempt.
fn core_rs_files_excluding_mechanism_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(&core_src_dir(), &mut out);
    out.retain(|p| {
        !matches!(
            p.file_name().and_then(|n| n.to_str()),
            Some("registry.rs") | Some("dsl.rs")
        )
    });
    out.sort();
    out
}

/// The kernel-is-rule-vocabulary-free central contract: `zzop_core::register_native_analysis_stub`
/// (`crates/core/src/registry.rs`) is a generic, id-agnostic MECHANISM — the kernel itself must never
/// name a specific native analysis id. Every id lives in its owning rules crate's own
/// `register_native_analyses` (`zzop_rules_graph`, `zzop_rules_http`, `zzop_rules_cross_layer`,
/// `zzop_rules_schema`, `zzop_metrics`), composed by
/// `zzop_engine::register_all_native` — never hand-copied here, so this test cannot drift from the real id
/// list the same way contract 5's catalog-sync tests can't.
///
/// Pragmatic grep-proxy, same spirit as contract 3: for every registered id, checks whether the exact
/// double-quoted literal `"<id>"` appears anywhere in a `crates/core/src` file. Quoted (not a bare
/// substring scan) deliberately — core legitimately contains unrelated identifiers/prose that happen to
/// contain an id as a substring without naming it as rule vocabulary (e.g. the function name
/// `circular_from_dep`, the `#[allow(unreachable_patterns)]` attribute, a `"GET /health"` test fixture, a
/// doc example `"graph/circular"`) — none of those are the quoted 1:1 id literal `"circular"`/
/// `"unreachable"`/`"health"` a `Finding::rule_id` or `RuleMeta::id` assignment would actually use.
///
/// `registry.rs` and `dsl.rs` are exempt: `registry.rs` hosts `register_native_analysis_stub` itself (whose
/// own doc/tests legitimately use synthetic example ids, and whose PRE-EXISTING `is_enabled`/
/// `disabled_rules` unit tests happen to reuse a couple of real ids — `"circular"`/`"unreachable"` — purely
/// as convenient, arbitrary example strings for a generic string-matching contract that would pass
/// identically with any other id; those tests assert nothing about what `"circular"` specifically means).
/// `dsl.rs` is exempt for the same "generic mechanism, illustrative example data" reason, in case a future
/// DSL fixture there happens to reuse a real id string. No other `core/src` file has a legitimate reason to
/// quote a native analysis id — a hit anywhere else means real rule vocabulary crept back into the kernel.
///
/// **What this proves**: no registered id appears as an exact quoted string literal in any non-exempt
/// `crates/core/src` file.
/// **What this CANNOT prove**: that an id is referenced indirectly (built via `format!`/`concat!`, or
/// spelled with different quoting/escaping); that the two exempt files stay free of an ACTUAL new
/// dependency on rule semantics beyond their known illustrative uses (a human reviewing a diff to those two
/// files is still the real backstop, same caveat contract 3's own doc makes about its grep-proxy).
#[test]
fn kernel_core_carries_no_native_analysis_id_string_literal() {
    let mut registry = RuleRegistry::new();
    register_all_native(&mut registry);
    let ids: Vec<String> = registry.metas().into_iter().map(|m| m.id.clone()).collect();
    assert!(
        !ids.is_empty(),
        "sanity: register_all_native registered no ids at all"
    );

    let mut offenders = Vec::new();
    for path in core_rs_files_excluding_mechanism_files() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for id in &ids {
            let quoted = format!("\"{id}\"");
            if text.contains(&quoted) {
                offenders.push(format!(
                    "{}: quotes native analysis id literal {quoted}",
                    path.display()
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "crates/core source files name a native analysis id as a quoted string literal — the kernel \
         must carry zero rule vocabulary (an id belongs in its owning rules crate's own \
         register_native_analyses, registered into core's registry only via the generic \
         register_native_analysis_stub mechanism): {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 9. Bare-word anchoring — a keyword-shaped regex must not match prose/string-literal text
// ---------------------------------------------------------------------------------------------

/// English words that read as ordinary prose (a JSDoc example, a log message, a string literal like
/// `"logged in to do this"` or `"waiting for ${x}"`) but are also meaningful loop/SQL keywords when they
/// appear as real syntax. A DSL pattern that matches one of these as a bare `\bword\b` with no adjacent
/// syntax anchor fires on prose too — exactly the defect class two shipped rules had (`perf/api-in-loop`
/// matched bare `\bdo\b`; `be-security/sql-taint` matched bare `UPDATE`), both fixed in the same commit
/// that added this contract. Deliberately a small, curated list (not "every English word that's also a
/// keyword") — these are the words shipped rules have actually tripped over in practice; extend this list
/// only once real usage finds a new one, the same "fix the whole class, not the one sampled rule"
/// discipline `docs/rules/authoring-guide.md`'s checklist item 4 documents.
const DANGEROUS_BARE_WORDS: &[&str] = &["do", "for", "while", "update", "delete", "select"];

/// A character that, sitting close to a dangerous word in the pattern's own SOURCE TEXT, is good textual
/// evidence the word is anchored to real syntax rather than free-standing prose: an open paren/brace (a
/// call or block following the word), a quote (a string-literal boundary wrapping it), or a `+`/`.`/`[`
/// (a concatenation/member-access/character-class token adjacent to it). Deliberately NOT `)`/`}`/`|` —
/// alternation pipes and closing delimiters sit next to EVERY word in a bare `\bfor\b|\bwhile\b|\bdo\b`
/// alternation (the exact shipped bug this contract targets), so treating them as anchors would make the
/// heuristic accept the very defect it exists to catch.
const ANCHOR_CHARS: &[char] = &['(', '{', '"', '\'', '+', '.', '['];

/// How far (in bytes) from a dangerous word's own start/end — or from its innermost enclosing regex
/// group's open/close paren, see `enclosing_group` — this contract looks for an `ANCHOR_CHARS` hit.
/// Deliberately small relative to a whole pattern (patterns here run 80-250+ characters): a real anchor in
/// every shipped rule sits within a handful of bytes of the word or its group boundary (`\bdo\s*\{`'s `{`
/// is 4 bytes after `do`; `(?:get|post|...|delete)\s*\(`'s `(` is ~4 bytes after the enclosing group's own
/// `)`) — a window this size cannot mistake "some quote/paren exists somewhere in this 200-byte regex" for
/// "this specific word is anchored".
const ANCHOR_WINDOW: usize = 12;

/// Every regex-bearing field capable of hiding a bare dangerous word against real scanned source text —
/// deliberately NOT `file_pattern`/`require_file`/`require_file_all`/`require_file_absent`/`exclude_pattern`/
/// `file_exclude_pattern`: those gate which FILES get scanned or veto an otherwise-matched line, they never
/// themselves shape a finding's matched text the way `line_pattern`/`any`/`patterns`/`absent` do (a bare
/// `\b(?:SELECT|INSERT|UPDATE|DELETE|MERGE)\b` in `be-security/sql-taint`'s own `require_file` only widens
/// which files reach the real `line_pattern` check below it — intentionally bare, not the latent bug its
/// `line_pattern` was).
fn regex_bearing_texts(rule: &RuleDef) -> Vec<(&'static str, &str)> {
    match &rule.matcher {
        Matcher::LineScan(m) => {
            let mut out = Vec::new();
            if let Some(p) = &m.line_pattern {
                out.push(("line_pattern", p.as_str()));
            }
            if let Some(alts) = &m.any {
                for lp in alts {
                    out.push(("any[].pattern", lp.pattern.as_str()));
                }
            }
            out
        }
        Matcher::MethodScan(m) => {
            let mut out = Vec::new();
            for lp in &m.patterns {
                out.push(("patterns[].pattern", lp.pattern.as_str()));
            }
            for lp in &m.absent {
                out.push(("absent[].pattern", lp.pattern.as_str()));
            }
            out
        }
        Matcher::SymbolScan(m) => m
            .name_pattern
            .as_deref()
            .map(|p| vec![("name_pattern", p)])
            .unwrap_or_default(),
        Matcher::IoScan(m) => m
            .key_pattern
            .as_deref()
            .map(|p| vec![("key_pattern", p)])
            .unwrap_or_default(),
    }
}

/// Finds the byte offsets of every `\b<word>\b` (case-insensitive) occurrence of any `DANGEROUS_BARE_WORDS`
/// entry inside `pattern`'s own SOURCE TEXT — i.e. this scans the regex STRING itself as plain text, not
/// scanned source code. Reuses the same `\b` word-boundary semantics the shipped rules themselves rely on,
/// so "does word X appear as a standalone word in this regex" is answered the same way "does word X appear
/// as a standalone word in a source file" would be (e.g. `update` inside `updateMany` never matches, since
/// there is no word boundary between `e` and `M`).
fn dangerous_word_occurrences(pattern: &str) -> Vec<(usize, usize, &'static str)> {
    let mut out = Vec::new();
    for &word in DANGEROUS_BARE_WORDS {
        let re = regex::Regex::new(&format!(r"(?i)\b{word}\b")).expect("static word regex");
        for m in re.find_iter(pattern) {
            out.push((m.start(), m.end(), word));
        }
    }
    out
}

/// Whether an unescaped paren (`(` or `)` not immediately preceded by a single `\`) sits at byte offset `i`
/// in `bytes` — an escaped `\(`/`\)` is a LITERAL character the pattern matches in scanned text (e.g.
/// `\bfor\s*(?:\(|await\b)`'s `\(` matches a real `(` in source code), not a grouping metacharacter, so the
/// enclosing-group scan below must not count it as one. Pragmatic single-backslash lookback (does not
/// handle a doubled `\\(` escaped-backslash-then-paren edge case) — consistent with every other heuristic in
/// this file being a textual proxy, not a real regex parser.
fn is_unescaped_paren(bytes: &[u8], i: usize) -> bool {
    matches!(bytes[i], b'(' | b')') && (i == 0 || bytes[i - 1] != b'\\')
}

/// The innermost enclosing `(...)`/`(?:...)` group's open- and close-paren byte offsets for the span
/// `[start, end)`, found by a plain paren-depth scan outward from the span in both directions (ignoring
/// escaped parens, see `is_unescaped_paren`) — NOT a real regex parser (no awareness of character classes,
/// where an unescaped `(` inside `[...]` is a literal character, not a group; no pattern this contract
/// currently scans puts a paren inside a character class, so this gap has never mattered in practice, but
/// it is a real gap in what this function can prove). Returns `None` when the span is not inside any group
/// at all (e.g. a bare `\bfor\b` sitting directly in a top-level alternation with no wrapping `(...)`).
fn enclosing_group(pattern: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let bytes = pattern.as_bytes();

    let mut depth = 0i32;
    let mut open = None;
    let mut i = start;
    while i > 0 {
        i -= 1;
        if !is_unescaped_paren(bytes, i) {
            continue;
        }
        if bytes[i] == b')' {
            depth += 1;
        } else if depth == 0 {
            open = Some(i);
            break;
        } else {
            depth -= 1;
        }
    }
    let open = open?;

    let mut depth = 0i32;
    let mut close = None;
    let mut j = end;
    while j < bytes.len() {
        if is_unescaped_paren(bytes, j) {
            if bytes[j] == b'(' {
                depth += 1;
            } else if depth == 0 {
                close = Some(j);
                break;
            } else {
                depth -= 1;
            }
        }
        j += 1;
    }
    let close = close?;
    Some((open, close))
}

/// Whether any `ANCHOR_CHARS` byte appears in `bytes[lo..hi]` (`hi` clamped to `bytes.len()`, `lo >= hi`
/// meaning an empty/invalid window yields no anchor). Operates on raw bytes (not a `str` slice) so it can
/// never panic on a non-UTF-8-boundary offset — every `ANCHOR_CHARS` entry is single-byte ASCII, so a raw
/// byte comparison is exact regardless of where `lo`/`hi` land relative to any multi-byte character
/// elsewhere in the pattern.
fn window_has_anchor(bytes: &[u8], lo: usize, hi: usize) -> bool {
    let hi = hi.min(bytes.len());
    if lo >= hi {
        return false;
    }
    bytes[lo..hi]
        .iter()
        .any(|&b| ANCHOR_CHARS.contains(&char::from(b)))
}

/// Whether the dangerous-word occurrence at `[start, end)` in `pattern` is anchored to real syntax. Anchored
/// when EITHER: (a) an `ANCHOR_CHARS` byte sits within `ANCHOR_WINDOW` bytes immediately before `start` or
/// immediately after `end` in the pattern's own text, or (b) the word sits inside a `(...)`/`(?:...)` group
/// (`enclosing_group`) and an `ANCHOR_CHARS` byte sits within `ANCHOR_WINDOW` bytes immediately before that
/// group's own open paren or immediately after its close paren — the shape every real
/// alternation-of-keywords rule in the shipped packs uses (the anchor lives just outside the group wrapping
/// the whole alternative list, not next to any one word inside it — e.g.
/// `(?:get|post|put|patch|delete|head|options|request|fetch|send|query)\s*\(`'s `(` sits right after the
/// group's own `)`, dozens of bytes from `delete` itself, so only the group-boundary check (b), not the
/// immediate-proximity check (a), anchors that occurrence).
fn is_anchored(pattern: &str, start: usize, end: usize) -> bool {
    let bytes = pattern.as_bytes();
    let immediate = window_has_anchor(bytes, start.saturating_sub(ANCHOR_WINDOW), start)
        || window_has_anchor(bytes, end, end + ANCHOR_WINDOW);
    if immediate {
        return true;
    }
    if let Some((open, close)) = enclosing_group(pattern, start, end) {
        return window_has_anchor(bytes, open.saturating_sub(ANCHOR_WINDOW), open)
            || window_has_anchor(bytes, close + 1, close + 1 + ANCHOR_WINDOW);
    }
    false
}

/// Contract #9 — no shipped DSL rule matches a `DANGEROUS_BARE_WORDS` entry as free-standing prose. See the
/// module doc's contract #9 entry and `ANCHOR_CHARS`/`ANCHOR_WINDOW`/`is_anchored`'s own docs for exactly
/// what "anchored" means and what this heuristic can and cannot prove: it is a textual-proximity check on
/// the regex pattern's own SOURCE STRING, not a regex semantics engine — it cannot understand alternation
/// grouping beyond simple paren-depth counting, so a sufficiently contrived pattern (e.g. a real anchor
/// sitting outside even the word's own enclosing group, further out than this contract's innermost-group
/// check reaches) could still evade it. It exists to catch the concrete, real defect class two shipped
/// rules had (`perf/api-in-loop` matched bare `\bdo\b` inside prose string literals like `"logged in to do
/// this"`; `be-security/sql-taint` matched bare `UPDATE` inside prose), not to be a sound regex analyzer —
/// a human reviewing a new rule's pattern by eye remains the real backstop for a pattern this heuristic
/// doesn't flag.
#[test]
fn dangerous_bare_words_are_syntax_anchored_not_bare_prose_matches() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        for rule in &pack.rules {
            for (field, text) in regex_bearing_texts(rule) {
                for (start, end, word) in dangerous_word_occurrences(text) {
                    if !is_anchored(text, start, end) {
                        offenders.push(format!(
                            "{}/{} ({field}): bare `{word}` at byte {start}..{end} in {text:?} has no \
                             adjacent syntax anchor ({ANCHOR_CHARS:?} within {ANCHOR_WINDOW} bytes, or \
                             just outside its enclosing regex group) — it will match \"{word}\" inside \
                             ordinary prose/string-literal text, not just real syntax",
                            pack.id, rule.id
                        ));
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "DSL rule patterns match a dangerous bare word with no syntax anchor — see this test's own doc \
         comment for what \"anchored\" means: {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 10. Kebab-case id hygiene — every rule id follows one casing convention
// ---------------------------------------------------------------------------------------------

/// Strips an optional leading `"cross-layer/"` namespace prefix — that prefix marks a cross-layer JOIN
/// finding's pack namespace, not part of the bare id itself, so the kebab-case check below applies to the
/// id with it removed.
fn strip_cross_layer_prefix(id: &str) -> &str {
    id.strip_prefix("cross-layer/").unwrap_or(id)
}

/// Contract #10 — every DSL pack id, every DSL rule id, and every registered native analysis id (after
/// `strip_cross_layer_prefix`) matches `^[a-z0-9]+(-[a-z0-9]+)*$`: lowercase letters/digits, single hyphens
/// between groups, no leading/trailing/double hyphens, no uppercase, no underscore, no camelCase. This is
/// the machine-enforced regression guard for the cross-layer vocabulary-unification rename underway across
/// this codebase — rule ids like `unsafeReadEndpoint`/`nonIdempotentWrite`/`fe-consumes-unprovided`/
/// `cross-layer/dead-mutation-endpoint`/`cross-layer/dangling-mutation` were converted to kebab-case as
/// part of that effort; without this test, a future rule could silently reintroduce the same
/// camelCase-vs-kebab-case drift.
#[test]
fn rule_ids_are_kebab_case() {
    let kebab = regex::Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").expect("static regex");
    let mut offenders = Vec::new();

    let packs = load_all_packs();
    for pack in &packs {
        let bare = strip_cross_layer_prefix(&pack.id);
        if !kebab.is_match(bare) {
            offenders.push(format!(
                "DSL pack id `{}` (checked as `{bare}`) is not kebab-case",
                pack.id
            ));
        }
        for rule in &pack.rules {
            let bare = strip_cross_layer_prefix(&rule.id);
            if !kebab.is_match(bare) {
                offenders.push(format!(
                    "DSL rule id `{}/{}` (checked as `{bare}`) is not kebab-case",
                    pack.id, rule.id
                ));
            }
        }
    }

    for meta in native_metas() {
        let bare = strip_cross_layer_prefix(&meta.id);
        if !kebab.is_match(bare) {
            offenders.push(format!(
                "native analysis id `{}` (checked as `{bare}`) is not kebab-case",
                meta.id
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "rule ids must match ^[a-z0-9]+(-[a-z0-9]+)*$ after stripping an optional leading `cross-layer/` \
         prefix (lowercase, single hyphens between groups, no camelCase/snake_case/uppercase) — a hit here \
         means the cross-layer vocabulary-unification rename's kebab-case convention broke again: \
         {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 11. Reference validation — a shipped message must never recommend a config key/flag that does not
//     exist. This is the machine contract for the defect class a message audit found live: `--since=all`,
//     `--repo=`, and `scanners.vocabulary.commitTypePatterns` were all recommended by real messages despite
//     none of them being real knobs. Both checks below load
//     `packages/cli/lib/config-surface.json` — the single vocabulary file also consumed by
//     `packages/cli/lib/mapper.js`'s `KNOWN_KEYS` (see that file), so the CLI's own runtime and this test
//     can never disagree about what a valid flag/config key is.
// ---------------------------------------------------------------------------------------------

/// `packages/cli/lib/config-surface.json`'s path, resolved relative to this crate's own manifest dir (same
/// "sibling package, read across the tree, never hand-copied" pattern as `catalog_path`/`dsl_dir`/
/// `native_dir` above).
fn config_surface_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/cli/lib/config-surface.json")
}

/// Mirrors `config-surface.json`'s `configKeys` object.
#[derive(serde::Deserialize)]
struct ConfigKeysSurface {
    top: Vec<String>,
    packs: Vec<String>,
    git: Vec<String>,
    report: Vec<String>,
    tree: Vec<String>,
    #[serde(rename = "ruleObject")]
    rule_object: Vec<String>,
}

/// Mirrors `config-surface.json`'s top-level shape. `#[serde(rename_all = "camelCase")]` maps this
/// struct's snake_case field names to the file's camelCase keys; the file's own `_docs` field is simply
/// ignored (serde drops unrecognized fields by default — no `deny_unknown_fields` here on purpose, the
/// same "an older/newer consumer degrades to ignored" contract `crates/facade/src/lib.rs`'s own request
/// types document).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigSurface {
    config_keys: ConfigKeysSurface,
    config_paths: Vec<String>,
    cli_flags: Vec<String>,
    embedder_fields: Vec<String>,
    external_tool_flags: Vec<String>,
    allowlisted_tokens: Vec<String>,
}

/// Loads and parses `config-surface.json`, failing loudly (not silently skipping) on a missing/malformed
/// file — same "a load error would otherwise hide real data from every test below" reasoning as
/// `load_all_packs`'s doc above.
fn load_config_surface() -> ConfigSurface {
    let path = config_surface_path();
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()))
}

/// Whether `line` is a comment line — a Rust `//`-prefixed line, or a JS `//`/`/*`/`*`-prefixed line
/// (covers a JS line comment, a block-comment opener, and every continuation line of a `/* ... */` or
/// JSDoc `/** ... */` block in this codebase's own style, which always starts a continuation line with a
/// leading `*`). One check serves both scanned languages: a Rust doc comment (`///`/`//!`) already starts
/// with `//`, so the same predicate correctly treats it as a comment too. Pragmatic line-level check (same
/// "keep it pragmatic" spirit as this file's other proxies, e.g. `native_rule_files_that_build_findings_...`
/// above): it does not track true multi-line block-comment START/end state — a `/* ... */` block whose
/// continuation lines do NOT start with `*` would not be fully skipped — but every block comment in this
/// codebase's actual style does, so the gap has never mattered in practice. Applied identically for both
/// contract-11 checks below: a message reaching a REAL reader is a string literal sitting on a CODE line,
/// never inside a doc comment describing the convention.
fn is_comment_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*')
}

/// CHECK A's own regex: a `--`-prefixed, all-lowercase, hyphen/digit-friendly flag token — deliberately
/// matches the exact shape a CLI/tool flag is spelled in prose (`--since`, `--depth`, `--unshallow`), not a
/// general "starts with two dashes" scan (an em dash-adjacent `--` in prose, or a `--` inside a code
/// comment, is excluded by the comment-line skip above, not by this regex).
fn flag_reference_regex() -> regex::Regex {
    regex::Regex::new(r"--[a-z][a-z0-9-]{1,}").expect("static regex")
}

/// CHECK A extraction: every `--flag`-shaped token appearing on a non-comment line of `text`, in order.
/// Pure — no vocabulary lookup here, see `unknown_flag_references` for the validation step. Deliberately
/// line-level (not a single whole-file regex pass): comment-skipping is naturally a per-line decision (see
/// `is_comment_line`'s doc), and every flag reference this contract cares about is short enough to never
/// span a line break in practice.
fn extract_flag_references(text: &str) -> Vec<String> {
    let re = flag_reference_regex();
    let mut out = Vec::new();
    for line in text.lines() {
        if is_comment_line(line) {
            continue;
        }
        out.extend(re.find_iter(line).map(|m| m.as_str().to_string()));
    }
    out
}

/// CHECK A validation: which of `flags` names neither a real CLI flag nor a real external-tool flag —
/// i.e. which ones `config-surface.json` does not vouch for. Returns them in the order found (may contain
/// duplicates — the real-tree test below reports every occurrence, not just distinct offenders, so a
/// reader can see how many places need fixing).
fn unknown_flag_references(flags: &[String], vocab: &ConfigSurface) -> Vec<String> {
    let allowed: BTreeSet<&str> = vocab
        .cli_flags
        .iter()
        .map(String::as_str)
        .chain(vocab.external_tool_flags.iter().map(String::as_str))
        .collect();
    flags
        .iter()
        .filter(|f| !allowed.contains(f.as_str()))
        .cloned()
        .collect()
}

/// CHECK B's shape gate: a backtick-quoted token counts as "config-key-shaped" only when it looks like a
/// bare identifier or a dotted/bracketed path (`git.since`, `trees[].root`, `disabled_rules`) — NOT a JSON
/// snippet like `` rules: { "circular": "off" } `` (spaces/colons/braces/quotes all fail this shape), which
/// every native rule's own disable-hint legitimately embeds right next to the word "config". A token this
/// gate rejects is simply not checked at all (neither accepted nor an offender) — see this contract's
/// module-doc entry for why that is the intentionally narrow scope, not a gap.
fn config_key_shape_regex() -> regex::Regex {
    regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*([.\[\]]+[A-Za-z0-9_\[\]]*)*$")
        .expect("static regex")
}

/// CHECK B extraction: every backtick-quoted token sitting on a non-comment line of `text` within 120
/// bytes of a whole-word (case-insensitive) "config" occurrence — also required to itself be on a
/// non-comment line, so a rustdoc comment that happens to backtick-reference an ordinary Rust identifier
/// near its own prose use of "config" (extremely common in this codebase's `///`/`//!` docs — e.g.
/// `` `EngineConfig` ``, `` `ScoresConfig` ``) is never even considered. Both the "config" occurrences and
/// the backtick tokens are located in a COMMENT-BLANKED copy of `text` (`is_comment_line`-flagged lines
/// replaced with spaces of the same length) rather than filtering post-hoc, so byte offsets — and therefore
/// the ±120 distance itself — stay computed on a single consistent coordinate space that never lets a
/// "config" mention on one line anchor a match to a backtick token on an unrelated comment line one line
/// below/above it.
///
/// Word-boundary "config" matching (`\bconfig\b`, not a bare substring scan) is deliberate: this codebase's
/// source is full of identifiers that merely CONTAIN "config" (`EngineConfig`, `ScoresConfig`,
/// `RuleConfig`, `config.rs` filenames) without naming the word "config" on its own — a substring scan
/// production-tested against the real tree here turned up 170+ incidental hits from exactly that class
/// before switching to `\bconfig\b` cut it to the single-digit count this contract's allowlist actually
/// documents.
fn extract_config_context_tokens(text: &str) -> Vec<String> {
    let masked: String = text
        .lines()
        .map(|line| {
            if is_comment_line(line) {
                " ".repeat(line.len())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let config_re = regex::Regex::new(r"(?i)\bconfig\b").expect("static regex");
    let backtick_re = regex::Regex::new(r"`([^`]*)`").expect("static regex");

    let config_positions: Vec<usize> = config_re.find_iter(&masked).map(|m| m.start()).collect();
    if config_positions.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for caps in backtick_re.captures_iter(&masked) {
        let whole = caps.get(0).expect("group 0 always matches");
        let near = config_positions
            .iter()
            .any(|&cpos| cpos.abs_diff(whole.start()) <= 120 || cpos.abs_diff(whole.end()) <= 120);
        if near {
            out.push(caps[1].to_string());
        }
    }
    out
}

/// CHECK B validation: which of `tokens` (as extracted by `extract_config_context_tokens`) names neither a
/// real config path/key nor an allowlisted/embedder token. A token that is not config-key-shaped at all
/// (`config_key_shape_regex` rejects it — e.g. a JSON snippet) is silently skipped, not reported: this
/// contract only judges tokens shaped like a knob a reader could actually go try.
///
/// Per-shape rule (mirrors the task's own two-branch contract):
/// - **Dotted/bracketed** (contains `.` or `[`): valid when the WHOLE token is in `configPaths` or
///   `allowlistedTokens`, OR its first segment (split on `.`/`[`) is in `embedderFields` — an embedder field
///   can legitimately be written with its own dotted continuation in a message (none do today, but the
///   shape is allowed the same way a bare embedder field name is).
/// - **Single word**: valid when it is a top-level config key, a nested key name from ANY of
///   `packs`/`git`/`report`/`tree`/`ruleObject` (the nested scopes are flattened into one name set here —
///   a message says `` `dir` `` or `` `since` `` without also repeating its parent, so there is no unambiguous
///   way to check a nested key against only its OWN parent's scope from the token text alone), an embedder
///   field, or an allowlisted token.
fn unknown_config_context_tokens(tokens: &[String], vocab: &ConfigSurface) -> Vec<String> {
    let shape = config_key_shape_regex();

    let top: BTreeSet<&str> = vocab.config_keys.top.iter().map(String::as_str).collect();
    let mut nested: BTreeSet<&str> = BTreeSet::new();
    for scope in [
        &vocab.config_keys.packs,
        &vocab.config_keys.git,
        &vocab.config_keys.report,
        &vocab.config_keys.tree,
        &vocab.config_keys.rule_object,
    ] {
        nested.extend(scope.iter().map(String::as_str));
    }
    let paths: BTreeSet<&str> = vocab.config_paths.iter().map(String::as_str).collect();
    let embedder: BTreeSet<&str> = vocab.embedder_fields.iter().map(String::as_str).collect();
    let allow: BTreeSet<&str> = vocab
        .allowlisted_tokens
        .iter()
        .map(String::as_str)
        .collect();

    tokens
        .iter()
        .filter(|t| {
            if !shape.is_match(t) {
                return false; // not config-key-shaped at all — out of scope, not an offense.
            }
            let is_dotted = t.contains('.') || t.contains('[');
            if is_dotted {
                if paths.contains(t.as_str()) || allow.contains(t.as_str()) {
                    return false;
                }
                let first_seg = t.split(['.', '[']).next().unwrap_or(t.as_str());
                !embedder.contains(first_seg)
            } else {
                !(top.contains(t.as_str())
                    || nested.contains(t.as_str())
                    || embedder.contains(t.as_str())
                    || allow.contains(t.as_str()))
            }
        })
        .cloned()
        .collect()
}

/// Every `rules/native/<crate>/src/**/*.rs` file (recursively under each crate's OWN `src/` dir — narrower
/// than contract 3's `native_rs_files`, which walks all of `rules/native` regardless of subdirectory; today
/// every `.rs` file under `rules/native` happens to live under a `src/`, so the two agree in practice, but
/// this contract's scanned-file set is specified as `rules/native/**/src/**/*.rs` and this function honors
/// that literally so a future non-`src` `.rs` file added elsewhere under a crate — a `build.rs`, a
/// crate-root `tests/` dir — is not silently swept in).
fn native_rule_src_rs_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(native_dir()) else {
        return out;
    };
    for entry in entries.filter_map(Result::ok) {
        let crate_dir = entry.path();
        if crate_dir.is_dir() {
            collect_rs_files(&crate_dir.join("src"), &mut out);
        }
    }
    out
}

/// `crates/engine/src/**/*.rs`, recursively — this crate's own `src/` dir (`CARGO_MANIFEST_DIR` itself,
/// since `rule_contracts.rs` lives in `crates/engine/tests/`).
fn engine_src_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out
}

/// `crates/metrics/src/**/*.rs`, recursively (sibling package, same pattern as `core_src_dir`).
fn metrics_src_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../metrics/src"),
        &mut out,
    );
    out
}

/// `packages/cli/lib/*.js` — direct children ONLY (not recursive: `packages/cli/lib` has no subdirectories
/// today, and the task's own scanned-file set names this one non-recursively, unlike every `.rs` glob
/// above).
fn cli_lib_js_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/cli/lib");
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("js") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// The full scanned-file set for contract 11's two real-tree checks: `rules/native/**/src/**/*.rs` +
/// `crates/engine/src/**/*.rs` + `crates/metrics/src/**/*.rs` + `packages/cli/lib/*.js`, sorted so a
/// failing assertion's offender list has a stable, diffable order across runs.
fn reference_validation_scanned_files() -> Vec<PathBuf> {
    let mut out = native_rule_src_rs_files();
    out.extend(engine_src_files());
    out.extend(metrics_src_files());
    out.extend(cli_lib_js_files());
    out.sort();
    out
}

/// Contract #11, CHECK A — every `--flag`-shaped token on a non-comment line of every scanned file must
/// name a real CLI flag or a real external tool's flag (`config-surface.json`'s `cliFlags` ∪
/// `externalToolFlags`). This is the exact machine check that would have caught the shipped `--since=all`/
/// `--repo=` defects (see `flag_reference_unit_tests` below for those pinned as unit tests).
///
/// **What this proves**: every `--flag`-shaped token reachable on a code line of a scanned source file
/// names a flag `config-surface.json` vouches for.
/// **What this CANNOT prove** (same "pragmatic proxy, not a semantic engine" caveat as this file's other
/// grep-based contracts): a flag built dynamically (`format!("--{name}")`) is invisible to this text scan;
/// a flag inside a STRING that is itself embedded in a doc comment example (as opposed to a real `//`/`/*`
/// prose line) is not distinguished from a real message — this is a textual proxy over source text, not an
/// AST-aware "is this reachable from a `Finding::message`" check.
#[test]
fn every_flag_reference_in_shipped_source_names_a_real_cli_or_external_tool_flag() {
    let vocab = load_config_surface();
    let mut offenders = Vec::new();
    for path in reference_validation_scanned_files() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for flag in unknown_flag_references(&extract_flag_references(&text), &vocab) {
            offenders.push(format!("{}: `{flag}`", path.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "shipped source names a --flag that is not a real CLI flag or a real external tool flag (not in \
         config-surface.json's cliFlags/externalToolFlags — the exact defect class `--since=all`/`--repo=` \
         shipped as): {offenders:#?}"
    );
}

/// Contract #11, CHECK B — every backtick-quoted, config-key-shaped token sitting within 120 bytes of the
/// word "config" on a non-comment line of every scanned file must name a real config path/key
/// (`config-surface.json`'s `configPaths` ∪ `configKeys` ∪ `embedderFields` ∪ `allowlistedTokens`). This is
/// the exact machine check that would have caught the shipped `scanners.vocabulary.commitTypePatterns`
/// defect (see `config_context_unit_tests` below for that pinned as a unit test).
///
/// **Allowlist entries** (each earned, not padding — see `config-surface.json`'s own `_docs.allowlistedTokens`
/// for the summary, and this list for the exact source line each was found at):
/// - `zzop.config.jsonc` — the CLI's own config filename; not currently backtick-quoted anywhere in the
///   scanned tree (it appears as plain prose, e.g. `crates/metrics/src/diagnostics.rs`), allowlisted
///   preemptively so a future backtick-quoted mention does not spuriously fail.
/// - `Authorization` — `rules/native/rules-cross-layer/src/cross_layer/external_secret_in_url.rs`'s
///   `external-secret-in-url` message recommends moving a secret to an `` `Authorization` `` HTTP header;
///   that backtick sits ~50 bytes before the SAME message's own "Disable via config `rules: {...}`" clause,
///   putting it inside the 120-byte window purely by co-location, not because it names a config knob.
/// - `IoConsume` — `rules/native/rules-cross-layer/src/cross_layer/sdk_import_no_visible_consume.rs`'s
///   message names the `` `IoConsume` `` Rust fact type a Mode B adapter would project calls into; same
///   "shares a sentence with the disable hint" co-location, not a config reference.
/// - `crossLayer.unresolvedConsumes` — `rules/native/rules-cross-layer/src/cross_layer/unconsumed_endpoint.rs`'s
///   message points a reader at the `` `crossLayer.unresolvedConsumes` `` OUTPUT field (part of the JSON
///   `analyzeTrees()` returns, not an input config path) for corroborating evidence; same co-location
///   pattern.
///
/// **What this proves**: every backtick-quoted, identifier/dotted-path-shaped token within 120 bytes of
/// "config" on a code line of a scanned source file names a real config path/key, embedder field, or
/// allowlisted non-config token.
/// **What this CANNOT prove**: a config-key reference with no backticks and no adjacent "config" text is
/// invisible to this scan (prose references are explicitly out of scope — see the module doc); a
/// dynamically-built message (`format!("`{key}`")`) is invisible the same way CHECK A's dynamic-flag gap
/// is.
#[test]
fn every_config_context_backtick_token_in_shipped_source_names_a_real_config_path_or_key() {
    let vocab = load_config_surface();
    let mut offenders = Vec::new();
    for path in reference_validation_scanned_files() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let tokens = extract_config_context_tokens(&text);
        for tok in unknown_config_context_tokens(&tokens, &vocab) {
            offenders.push(format!("{}: `{tok}`", path.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "shipped source has a backtick-quoted, config-key-shaped token near the word \"config\" that names \
         no real config path/key (not in config-surface.json's configPaths/configKeys/embedderFields/ \
         allowlistedTokens — the exact defect class `scanners.vocabulary.commitTypePatterns` shipped as): \
         {offenders:#?}"
    );
}

/// Synthetic, deliberately tiny vocabulary for the CHECK A/B unit tests below — independent of the real
/// `config-surface.json` (which can grow over time) so these tests stay a fixed, minimal pin of exactly the
/// three historical defects, never accidentally passing because the real file happens to allowlist
/// something today.
fn tiny_synthetic_vocab() -> ConfigSurface {
    ConfigSurface {
        config_keys: ConfigKeysSurface {
            top: vec!["rules".to_string(), "git".to_string()],
            packs: vec![],
            git: vec!["since".to_string()],
            report: vec![],
            tree: vec![],
            rule_object: vec![],
        },
        config_paths: vec!["git.since".to_string()],
        cli_flags: vec!["--config".to_string()],
        embedder_fields: vec!["disabled_rules".to_string()],
        external_tool_flags: vec!["--unshallow".to_string(), "--depth".to_string()],
        allowlisted_tokens: vec![],
    }
}

#[cfg(test)]
mod flag_reference_unit_tests {
    use super::*;

    /// Pins the shipped `--since=all` defect: `--since` is not a real CLI flag (the real flag is
    /// `--severity`) and not a real external tool flag either — CHECK A must reject it.
    #[test]
    fn rejects_the_shipped_since_all_defect() {
        let vocab = tiny_synthetic_vocab();
        let flags = extract_flag_references("re-run with `--since=all`");
        assert_eq!(
            unknown_flag_references(&flags, &vocab),
            vec!["--since".to_string()]
        );
    }

    /// A real external tool flag (git's own `--unshallow`, the fix for a shallow clone) must be accepted —
    /// proves CHECK A does not reject every unfamiliar-looking flag, only ones absent from the vocabulary.
    #[test]
    fn accepts_a_real_external_git_flag() {
        let vocab = tiny_synthetic_vocab();
        let flags = extract_flag_references("git fetch --unshallow");
        assert!(unknown_flag_references(&flags, &vocab).is_empty());
    }

    /// Pins the shipped `--repo=<path>` defect: `zzop` has no `--repo` flag (roots/trees are config-only,
    /// see `packages/cli/bin/zzop.js`'s real flag set) — CHECK A must reject it.
    #[test]
    fn rejects_the_shipped_repo_path_defect() {
        let vocab = tiny_synthetic_vocab();
        let flags = extract_flag_references("--repo=<path>");
        assert_eq!(
            unknown_flag_references(&flags, &vocab),
            vec!["--repo".to_string()]
        );
    }

    /// A flag reference inside a comment line must be invisible to extraction entirely — comments are not
    /// messages a reader of `zzop`'s OUTPUT ever sees.
    #[test]
    fn ignores_a_flag_reference_inside_a_comment_line() {
        assert!(extract_flag_references("// re-run with --since=all").is_empty());
        assert!(extract_flag_references("* re-run with --since=all").is_empty());
    }
}

#[cfg(test)]
mod config_context_unit_tests {
    use super::*;

    /// Pins the shipped `scanners.vocabulary.commitTypePatterns` defect: `scanners` is not a real top-level
    /// config key (the real top-level keys are `roots`/`trees`/`packs`/`rules`/`exclude`/`git`/`cacheDir`/
    /// `sizeCap`/`format`/`failOn`/`report`) — CHECK B must reject the whole dotted token on its first
    /// segment.
    #[test]
    fn rejects_the_shipped_commit_type_patterns_defect() {
        let vocab = tiny_synthetic_vocab();
        let tokens = extract_config_context_tokens(
            "add patterns in config under `scanners.vocabulary.commitTypePatterns`",
        );
        assert_eq!(
            unknown_config_context_tokens(&tokens, &vocab),
            vec!["scanners.vocabulary.commitTypePatterns".to_string()]
        );
    }

    /// A JSON-snippet-shaped backtick token (spaces/colons/braces/quotes) is not config-key-shaped at all —
    /// `rules` itself IS a real top key, but the snippet as a whole must not even reach the shape gate, let
    /// alone be reported as an offender.
    #[test]
    fn a_json_snippet_shaped_token_is_out_of_scope_not_an_offense() {
        let vocab = tiny_synthetic_vocab();
        let tokens = extract_config_context_tokens(
            "in zzop.config.jsonc via `rules: { \"circular\": \"off\" }`",
        );
        assert!(unknown_config_context_tokens(&tokens, &vocab).is_empty());
    }

    /// An embedder-field reference (the "embedders: `disabled_rules`" leg every native rule's disable-hint
    /// carries) must be accepted.
    #[test]
    fn accepts_an_embedder_field_reference() {
        let vocab = tiny_synthetic_vocab();
        let tokens =
            extract_config_context_tokens("disable via config (embedders: `disabled_rules`)");
        assert!(unknown_config_context_tokens(&tokens, &vocab).is_empty());
    }

    /// A real dotted config path (`git.since`) must be accepted.
    #[test]
    fn accepts_a_real_dotted_config_path() {
        let vocab = tiny_synthetic_vocab();
        let tokens =
            extract_config_context_tokens("configured via `git.since` in your config file");
        assert!(unknown_config_context_tokens(&tokens, &vocab).is_empty());
    }

    /// A config-key-shaped backtick token farther than 120 bytes from any "config" occurrence must not even
    /// be extracted — proves the distance window is enforced, not just the vocabulary lookup.
    #[test]
    fn ignores_a_token_outside_the_120_byte_window() {
        let filler = "x".repeat(200);
        let text = format!("config {filler} `scanners.vocabulary.commitTypePatterns`");
        assert!(extract_config_context_tokens(&text).is_empty());
    }

    /// A config-key-shaped backtick token inside a comment line must be invisible to extraction entirely,
    /// even when a "config" occurrence sits right next to it on the same comment line — a doc comment is
    /// not a message a reader of `zzop`'s OUTPUT ever sees.
    #[test]
    fn ignores_a_token_inside_a_comment_line() {
        assert!(extract_config_context_tokens(
            "// add patterns in config under `scanners.vocabulary.commitTypePatterns`"
        )
        .is_empty());
    }
}

// ---------------------------------------------------------------------------------------------
// 12. Cross-pack policy-vocabulary pin — be-reliability/sync-fs-in-handler and be-db/client-per-request
//     must share one "handler-context" evidence definition
// ---------------------------------------------------------------------------------------------

/// Finds a loaded pack by id, panicking with a clear message if it's missing — same "fail loudly" spirit as
/// `load_all_packs`.
fn find_pack<'a>(packs: &'a [RulePackDef], id: &str) -> &'a RulePackDef {
    packs
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("pack `{id}` not loaded"))
}

/// Finds a rule by id within a pack, panicking with a clear message if it's missing.
fn find_rule<'a>(pack: &'a RulePackDef, id: &str) -> &'a RuleDef {
    pack.rules
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("rule `{}/{id}` not loaded", pack.id))
}

/// Extracts a `Matcher::MethodScan` rule's `patterns[]` entry with the given `label`, panicking if the rule
/// isn't a method-scan rule or has no pattern with that label — both are authoring errors this pin exists to
/// catch, not conditions worth silently tolerating.
fn method_scan_pattern_by_label<'a>(rule: &'a RuleDef, label: &str) -> &'a str {
    match &rule.matcher {
        Matcher::MethodScan(m) => m
            .patterns
            .iter()
            .find(|lp| lp.label == label)
            .unwrap_or_else(|| panic!("{}: no patterns[] entry labeled `{label}`", rule.id))
            .pattern
            .as_str(),
        other => panic!("{}: expected a MethodScan matcher, got {other:?}", rule.id),
    }
}

/// Policy pin: `be-reliability/sync-fs-in-handler` and `be-db/client-per-request` both approximate "this
/// function looks like a request handler" with a `patterns[]` entry labeled `handler-context` — the SAME
/// evidence definition, deliberately duplicated across the two packs (a DSL rule can't reference another
/// pack's pattern). Nothing else stops one pack's copy drifting from the other's during an unrelated edit —
/// each pack's own fixtures only exercise its own copy, so a silent fork of what counts as "handler context"
/// (e.g. one pack keeping the naive `res` bare-word evidence a mono-hub 0.10.0 field review found false-
/// positives on, while the other adopts the tightened one) would ship unnoticed. This test loads both
/// shipped DSL packs fresh (via `load_dsl_packs`, same helper every other contract here uses — never a hand-
/// copied inline fixture), extracts each rule's own `handler-context` pattern string, and asserts they are
/// byte-identical, so a future edit to one without the other fails loudly here instead of drifting unnoticed.
#[test]
fn handler_context_pattern_is_identical_across_be_reliability_and_be_db() {
    let packs = load_all_packs();
    let be_reliability = find_pack(&packs, "be-reliability");
    let be_db = find_pack(&packs, "be-db");

    let sync_fs_rule = find_rule(be_reliability, "sync-fs-in-handler");
    let client_per_request_rule = find_rule(be_db, "client-per-request");

    let sync_fs_pattern = method_scan_pattern_by_label(sync_fs_rule, "handler-context");
    let client_per_request_pattern =
        method_scan_pattern_by_label(client_per_request_rule, "handler-context");

    assert_eq!(
        sync_fs_pattern, client_per_request_pattern,
        "be-reliability/sync-fs-in-handler and be-db/client-per-request's `handler-context` patterns have \
         drifted — they encode the same handler-evidence policy and must stay byte-identical (see this \
         test's own doc comment)"
    );
}
