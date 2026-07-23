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
//!     (`parser/parser-go/src/lang/loop_spans.rs`, `go/go-goroutine-in-loop`). MINIMAL-EXISTENCE scope only
//!     — see that file's own module doc for the full claim boundary before reading a green run here as
//!     anything more than "the wiring exists" / "the wiring is definitely absent".

use std::fs;
use std::path::{Path, PathBuf};

use zzop_core::{load_dsl_packs, RuleMeta, RulePackDef, RuleRegistry};
use zzop_engine::register_all_native;

mod bare_words;
mod capability_matrix;
mod catalog_sync;
mod config_surface;
mod id_hygiene;
mod kernel_vocabulary;
mod markers;
mod native_messages;
mod pack_loading;
mod policy_pins;
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

/// Every registered native analysis's metadata, owned (not borrowed from a local `RuleRegistry`) so
/// callers can use it without threading a registry lifetime through every test.
fn native_metas() -> Vec<RuleMeta> {
    let mut registry = RuleRegistry::new();
    register_all_native(&mut registry);
    registry.metas().into_iter().cloned().collect()
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
