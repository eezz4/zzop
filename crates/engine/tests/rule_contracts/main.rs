//! Meta-tests — machine-enforced cross-cutting contracts every shipped rule (DSL and native) must honor.
//!
//! These contracts previously existed only as human convention (a prior audit found real drift: rule
//! messages that never told the reader how to exclude a finding,
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
//! 1. **Derived-marker uniqueness** (`derived_suppress_markers_are_globally_unique`) — markers are DERIVED
//!    `zzop-<id>-ok` (`RuleDef::suppress_marker()`), so presence and the `-ok` shape are construction guarantees;
//!    what still needs guarding is that no two rules — in any pack — derive the same marker (that would be a
//!    cross-rule co-suppression), i.e. rule ids are globally unique.
//! 2. **Message triple** (`every_dsl_rule_message_documents_how_to_exclude_it`) — every DSL rule's
//!    `message` names its own derived marker (`zzop-<id>-ok`) OR the literal `disabled_rules`/`disabledRules`
//!    string — the "how to exclude" leg of the problem+fix+exclude finding contract.
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
//!    `catalog_mentions_every_native_analysis_id`, `catalog_mentions_every_dsl_pack_id`,
//!    `catalog_sightline_rows_and_declared_rule_sightlines_are_the_same_set`) —
//!    `docs/rules/catalog.md`'s stated totals and id lists match the loaded reality, and the set of
//!    catalog rows carrying a sightline paragraph equals the set of `RuleSightline` declarations
//!    (both directions; exemptions are explicit in the test) — the reverse of
//!    `crates/engine/src/sightlines.rs`'s declared→registered pin.
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
//!    `security/sql-string-concat` (bare `UPDATE` matched inside prose), both fixed in the same commit that
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
//!     `crates/config/config-surface.json` — the single vocabulary file (originally shared with the
//!     removed JS CLI's `mapper.js` and its `KNOWN_KEYS`). See each test's own doc for exactly what its pragmatic
//!     textual-proximity extraction can and cannot prove.
//! 12. **Capability matrix** (`capability_matrix.rs`) — machine-pinned parser × channel reachability FACTS
//!     (which of `symbols`/`method_spans`/`loop_spans`/`io_provides`/`io_consumes` each of the 8 parser
//!     environments plus the lexical fallback actually projects, read off `pipeline::fresh.rs`'s own match
//!     arms and confirmed against one canary fixture per environment via the real `analyze_tree` path),
//!     cross-checked against every shipped rule's matcher so a `file_pattern` can never silently admit an
//!     environment whose required channel this engine does not project. A prior audit found this exact
//!     fact had drifted from prose ("loop spans are TS-only") while the code moved on
//!     (`parser/parser-go/src/lang/loop_spans.rs`, `go/goroutine-in-loop`). MINIMAL-EXISTENCE scope only
//!     — see that file's own module doc for the full claim boundary before reading a green run here as
//!     anything more than "the wiring exists" / "the wiring is definitely absent".
//! 13. **Kebab-case LABEL hygiene** (`dsl_pattern_labels_are_kebab_case`) — the second name layer packs
//!     declare. Contract 10 enumerates rule IDS and therefore structurally cannot see a
//!     `LabeledPattern::label`, yet `LineScan::any[].label` ships to users verbatim as
//!     `Finding.data.label` and is the only stable "which arm fired" key a multi-arm rule has. Three
//!     shipped labels had drifted into English sentences on that wire (`"ECB mode (no diffusion)"` and
//!     two siblings in `security/weak-crypto`). Same regex as contract 10, deliberately WITHOUT its
//!     uniqueness leg — label scope is rule-local and a user never types one. See that test's own doc.
//! 14. **This suite's own test wiring** (`every_rule_contracts_source_file_is_mod_registered`) — contract
//!     7 mechanizes "every shipped pack folder is actually wired to a test"; contract 14 is that same
//!     invariant turned on this file. A `.rs` file dropped into `tests/rule_contracts/` without a `mod`
//!     line below does not fail to compile and raises no warning — it is simply never compiled, so a
//!     declared defense runs never, silently, forever. Nothing else in the repo can see it either: these
//!     meta-tests run only under `cargo test --workspace`, and `scripts/check-guards-wired.sh` enumerates
//!     `scripts/check-*.sh` alone, so a missing `mod` line is invisible to every other lane.
//! 15. **Shared crates name no MCP-only vocabulary** (`host_vocabulary.rs`) — no user-facing message built
//!     in `crates/summary` or `crates/config` names a tool name or a wire argument the CLI spells
//!     differently. The same sentence reaches a `zzop` CLI user, who can call no tool and
//!     pass no JSON argument. `crates/config/src/lib_tests.rs` pinned exactly this doctrine at ONE point
//!     while five siblings drifted and shipped; this is that pin widened to the class. Pragmatic
//!     grep-proxy over prose-shaped string literals — see that file's own doc for the boundary.
//! 16. **…and no CLI-only vocabulary either** (`host_vocabulary.rs`) — the mirror of 15, in the same file
//!     because it is one doctrine: a subcommand spelling or a dash-flag in a shared message reaches an MCP
//!     client that has no argv. It had already shipped once (the `config-template` resource description
//!     told `resources/list` readers to run `zzop init`). Lanes with no MCP twin are exempt, read from
//!     `docs/contracts/surface-parity.json`'s `_cliOnlyLanes[].sources` rather than listed again here.
//! 17. **No DSL message hand-writes the engine's disable hint** (`dsl_messages.rs`'s
//!     `no_dsl_pack_message_hand_writes_the_engine_appended_disable_hint`) — the engine appends the
//!     disable sentence to EVERY DSL finding (`pipeline::findings`'s `append_disable_hints`), so an
//!     author who writes one too ships it TWICE. It shipped once (`perf/sqlalchemy-eager-relationship`),
//!     and the hand-written copy was the worse of the two — it named only the embedder field, never the
//!     config-file spelling. Contract 2 above still ACCEPTS `disabled_rules` as a "how to exclude" leg;
//!     this one removes that option for DSL specifically, so the single thing a pack author writes is
//!     their own derived `zzop-<id>-ok` marker.

use std::fs;
use std::path::{Path, PathBuf};

use zzop_core::{load_dsl_packs, RulePackDef, RuleRegistry};
use zzop_engine::register_all_native;

mod bare_words;
mod capability_matrix;
mod catalog_sync;
mod config_surface;
mod dsl_messages;
mod envelope_contract_version;
mod host_vocabulary;
mod id_hygiene;
mod io_kind_readers;
mod kernel_vocabulary;
mod markers;
mod native_messages;
mod pack_loading;
mod policy_pins;
mod recognizer_drift;
mod reference_unit_tests;
mod reference_validation;
mod surface_parity;

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

/// Loads every `rules/dsl/*.json` pack, failing loudly (not silently skipping) if any file fails to
/// parse — a load error would otherwise hide real rules from every contract test below, which is worse
/// than a normal test failure.
fn load_all_packs() -> Vec<RulePackDef> {
    let result = load_dsl_packs(&dsl_dir());
    assert!(
        result.errors.is_empty(),
        "DSL pack load errors (fix the pack before rule_contracts can evaluate it): {:?}",
        result.errors
    );
    result.packs.into_iter().map(|(_, pack)| pack).collect()
}

/// Every registered native analysis id, owned (not borrowed from a local `RuleRegistry`) so callers can
/// use it without threading a registry lifetime through every test.
fn native_ids() -> Vec<String> {
    let mut registry = RuleRegistry::new();
    register_all_native(&mut registry);
    registry.ids().to_vec()
}

/// Contract 14 — every `.rs` file in `crates/engine/tests/rule_contracts/` is `mod`-registered in this
/// file, so no meta-test can sit in this directory silently uncompiled. Reads this file's own text rather
/// than any generated list: the `mod` lines above ARE the wiring, and re-deriving them from anything else
/// would just move the drift.
///
/// Both directions are asserted even though only one can actually rot: a `mod x;` with no `x.rs` is a
/// compile error (so the test binary would not build and this test could never report it), while a
/// `x.rs` with no `mod x;` compiles fine and is the real hole. The set equality is one assertion for
/// both, and its offender lists name the exact fix in each direction.
///
/// Nested subdirectories are rejected outright rather than half-handled: none exist today, and a
/// `mod`-path scheme for them would be untested machinery guarding nothing.
#[test]
fn every_rule_contracts_source_file_is_mod_registered() {
    let this_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rule_contracts");
    let main_rs = this_dir.join("main.rs");
    let text = fs::read_to_string(&main_rs)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", main_rs.display()));

    let declared: std::collections::BTreeSet<String> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("mod ")?.strip_suffix(';'))
        .map(str::to_string)
        .collect();

    let mut present = std::collections::BTreeSet::new();
    let entries = fs::read_dir(&this_dir)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", this_dir.display()));
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        assert!(
            !path.is_dir(),
            "{} is a subdirectory of tests/rule_contracts — this contract only understands a FLAT \
             directory of `mod <file>;` siblings. Flatten it, or extend this test to walk nested module \
             paths before adding one.",
            path.display()
        );
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().and_then(|e| e.to_str()) == Some("rs") && stem != "main" {
            present.insert(stem.to_string());
        }
    }

    assert_eq!(
        present, declared,
        "tests/rule_contracts/ and this file's `mod` list disagree.\nfiles with NO `mod` line (they are \
         never compiled — no error, no warning, and every contract in them runs never): {:?}\n`mod` \
         lines with no file: {:?}",
        present.difference(&declared).collect::<Vec<_>>(),
        declared.difference(&present).collect::<Vec<_>>(),
    );
}

/// Whether a path is TEST source rather than shipped source: any `tests` directory component, or a
/// file named `tests.rs` / `*_test.rs` / `*_tests.rs`. A fixture that mimics a shipped emission spells
/// the shipped literals on purpose; counting those as emissions made a guard dictate what a test may
/// name its own variables, which is not a contract.
///
/// Shared (rather than re-spelled per contract file) because two contracts now subtract the same set
/// from a [`collect_rs_files`] walk — `surface_parity`'s MCP-lane haystack and `markers`' native-rule
/// scan — and two copies of "what counts as a test file" is how one of them silently starts scanning a
/// fixture the other ignores.
fn is_test_source(path: &Path) -> bool {
    if path.components().any(|c| c.as_os_str() == "tests") {
        return true;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    name == "tests.rs" || name.ends_with("_test.rs") || name.ends_with("_tests.rs")
}

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
