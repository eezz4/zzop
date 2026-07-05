//! Meta-tests — machine-enforced cross-cutting contracts every shipped rule (DSL and native) must honor.
//!
//! These contracts previously existed only as human convention (a prior audit found real drift: DSL rules
//! shipped with no `suppress_marker`, rule messages that never told the reader how to exclude a finding,
//! and `docs/rules/catalog.md` totals out of sync with the actual pack/registry data). This file loads
//! every shipped DSL pack (`rules/dsl/*.json`, via `zzop_core::load_dsl_packs`) and the native registry
//! (`zzop_engine::register_all_native`, composing `zzop_rules_graph`/`zzop_rules_schema`/`zzop_metrics`'s own
//! `register_native_analyses`) fresh in each test, so drift in either is caught the next time
//! `cargo test --workspace` runs — no test here hand-copies rule data, everything is read from the same
//! source the engine itself loads at runtime.
//!
//! See `docs/rules/authoring-guide.md`'s "Machine-enforced contracts" section for the author-facing
//! summary of what a failing test here means.
//!
//! ## Contracts covered
//! 1. **Marker presence** (`every_dsl_rule_has_a_non_empty_suppress_marker`,
//!    `suppress_markers_are_unique_within_each_pack`) — every DSL rule has a non-empty `suppress_marker`,
//!    and no two rules in the same pack share one (co-suppression risk).
//! 2. **Message triple** (`every_dsl_rule_message_documents_how_to_exclude_it`) — every DSL rule's
//!    `message` names its own suppress marker OR the literal `disabled_rules`/`disabledRules` string — the
//!    "how to exclude" leg of the problem+fix+exclude finding contract.
//! 3. **Native message contract** (`native_rule_files_that_build_findings_mention_disabled_rules`) — a
//!    pragmatic grep-based proxy (native findings are built in code, not read from declarative data — see
//!    that test's own doc for exactly what this can and cannot prove).
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
//!    `packages/core` must not name any registered native
//!    analysis id as a quoted string literal (a pragmatic grep-proxy — see that test's own doc for what it
//!    can/cannot prove).
//! 9. **Bare-word anchoring** (`dangerous_bare_words_are_syntax_anchored_not_bare_prose_matches`) — no
//!    shipped DSL rule's regex matches a keyword-shaped English word (`do`/`for`/`while`/`update`/`delete`/
//!    `select`) as a bare `\bword\b` with no adjacent syntax anchor — the defect class that shipped live in
//!    `perf/api-in-loop` (bare `\bdo\b` matched inside prose like `"logged in to do this"`) and
//!    `java-security/sql-taint` (bare `UPDATE` matched inside prose), both fixed in the same commit that
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
/// cannot be suppressed inline (see `RuleDef::suppress_marker`'s doc in `packages/core/src/dsl.rs`) — the
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
/// contains the literal substring `disabled_rules` somewhere — the convention every native rule module
/// already follows (see e.g. `rules/native/rules-graph/src/cross_layer/duplicate_route.rs`, which both
/// puts `disabled_rules: ["cross-layer/duplicate-route"]` in its finding's message text AND asserts
/// `message.contains("disabled_rules")` in its own `#[cfg(test)]` module).
///
/// **What this proves**: a file that builds at least one `Finding` via a literal `rule_id: "..."`
/// assignment also names `disabled_rules` somewhere in its own source — in every rule module audited while
/// writing this test, that mention lives inside the exact `message`/`hint` string the finding exposes to a
/// reader, since there is essentially no other reason a rule-authoring file would contain that literal
/// string.
///
/// **What this CANNOT prove** (documented per the task's "keep it pragmatic" instruction, not silently
/// assumed):
/// - That the `disabled_rules` mention is actually inside the live `Finding::message` value reaching the
///   user, as opposed to a doc comment describing the convention or a `#[cfg(test)]`-only assertion. This
///   is a file-level co-occurrence check, not an AST-level check tying the substring to a specific
///   `Finding` construction site.
/// - That a rule id built dynamically (a variable, a format! expansion, a shared constructor in a
///   different file) is caught at all — only the literal token `rule_id: "` is detected, so a native rule
///   authored in an unusual shape can slip past this test silently.
/// - Anything about DSL packs or JS quick-rules — out of scope here; contract 2 above covers DSL directly,
///   since DSL `message` IS declarative data this crate can inspect precisely.
///
/// A failure here is a strong, actionable signal (the flagged file almost certainly ships a finding with no
/// exclude-hint), but is not a certainty — read the flagged file before assuming the fix is "add one
/// sentence to a format! string."
#[test]
fn native_rule_files_that_build_findings_mention_disabled_rules() {
    let mut offenders = Vec::new();
    for path in native_rs_files() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if text.contains("rule_id: \"") && !text.contains("disabled_rules") {
            offenders.push(path.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "native rule source files construct a Finding (literal `rule_id: \"...\"`) but never mention \
         `disabled_rules` anywhere in the same file — the finding's message likely omits the \"how to \
         exclude\" hint every other native rule includes (see this test's own doc comment for exactly what \
         this check can/cannot prove): {offenders:#?}"
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
/// (`packages/core/src/registry.rs`) gates every layer through ONE shared exact-string-match id space, so a
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
// 8. Kernel is rule-vocabulary-free — packages/core must not name a native analysis id
// ---------------------------------------------------------------------------------------------

/// `packages/core/src`, resolved relative to this crate's own manifest dir (same "sibling package"
/// pattern as `native_dir`/`dsl_dir`/`catalog_path` above).
fn core_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/src")
}

/// Every `packages/core/src/**/*.rs` file, recursively, EXCEPT `registry.rs` and `dsl.rs` — see
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
/// (`packages/core/src/registry.rs`) is a generic, id-agnostic MECHANISM — the kernel itself must never
/// name a specific native analysis id. Every id lives in its owning rules crate's own
/// `register_native_analyses` (`zzop_rules_graph`, `zzop_rules_schema`, `zzop_metrics`), composed by
/// `zzop_engine::register_all_native` — never hand-copied here, so this test cannot drift from the real id
/// list the same way contract 5's catalog-sync tests can't.
///
/// Pragmatic grep-proxy, same spirit as contract 3: for every registered id, checks whether the exact
/// double-quoted literal `"<id>"` appears anywhere in a `packages/core/src` file. Quoted (not a bare
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
/// `packages/core/src` file.
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
        "packages/core source files name a native analysis id as a quoted string literal — the kernel \
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
/// matched bare `\bdo\b`; `java-security/sql-taint` matched bare `UPDATE`), both fixed in the same commit
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
/// `\b(?:SELECT|INSERT|UPDATE|DELETE|MERGE)\b` in `java-security/sql-taint`'s own `require_file` only widens
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
/// this"`; `java-security/sql-taint` matched bare `UPDATE` inside prose), not to be a sound regex analyzer —
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
