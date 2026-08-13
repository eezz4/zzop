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
//!    RENDERED message names its own derived marker (`zzop-<id>-ok`) OR the literal
//!    `disabled_rules`/`disabledRules` string — the "how to exclude" leg of the problem+fix+exclude finding
//!    contract. **Rendered, not authored**: the marker sentence is appended by the engine
//!    (`pipeline::findings::append_hints`), so reading the raw pack `message` would now fail for every rule
//!    that correctly stops writing its own copy. The paired guard runs the other way —
//!    `scripts/check-pack-suppress-sentence.sh` fails a pack that DOES write it — so the two together pin
//!    "exactly one copy, and the engine owns it".
//! 3. **Native message contract** (`native_rule_files_that_build_findings_mention_disabled_rules`,
//!    `disable_hint_literal_args_are_known_ids_matching_the_files_own_findings`) — a
//!    pragmatic grep-based proxy (native findings are built in code, not read from declarative data — see
//!    each test's own doc for exactly what this can and cannot prove). The first accepts either a literal
//!    `disabled_rules`/`disabledRules` mention OR a call to the shared `zzop_core::finding::disable_hint` builder every
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
//!     two siblings — then in `security/weak-crypto`, cipher arms that live in `security/weak-cipher`
//!     since the 2026-08-09 split). Same regex as contract 10, deliberately WITHOUT its
//!     uniqueness leg — label scope is rule-local and a user never types one. See that test's own doc.
//! 14. **Harness-directory test wiring** (`every_registered_flat_test_dir_source_file_is_mod_registered`)
//!     — contract 7 mechanizes "every shipped pack folder is actually wired to a test"; contract 14 is
//!     that same invariant turned on this crate's own flat `main.rs`-harness test directories
//!     (`REGISTERED_FLAT_TEST_DIRS` — this one, and each future fold target). A `.rs` file dropped into
//!     such a directory without a `mod` line in its `main.rs` does not fail to compile and raises no
//!     warning — it is simply never compiled, so a declared defense runs never, silently, forever.
//!     Nothing else in the repo can see it either: these meta-tests run only under
//!     `cargo test --workspace`, and `scripts/check-guards-wired.sh` enumerates `scripts/check-*.sh`
//!     alone, so a missing `mod` line is invisible to every other lane. Each registered directory also
//!     carries a module-count FLOOR — set-equality alone goes vacuously green on a re-rooted or
//!     mass-emptied directory (0 == 0), which is precisely the failure a wiring guard exists to catch.
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
//!     disable sentence to EVERY DSL finding (`pipeline::findings`'s `append_hints`), so an
//!     author who writes one too ships it TWICE. It shipped once (`perf/sqlalchemy-eager-relationship`),
//!     and the hand-written copy was the worse of the two — it named only the embedder field, never the
//!     config-file spelling. Contract 2 above still ACCEPTS `disabled_rules`/`disabledRules` as a "how
//!     to exclude" leg; this one removes that option for DSL specifically (both spellings), so the
//!     single thing a pack author writes is their own derived `zzop-<id>-ok` marker.
//! 18. **Recognizer CHANNEL binding** (`recognizer_channels.rs`) — the axis `recognizer_drift` names as
//!     unbound. Every `FrameworkRecognizer::emits` set must equal the channels its backing modules'
//!     own code constructs, derived from (side, `kind:`) at each io struct literal plus a type hop
//!     through the engine's `compose_*(zzop_core::…Fragment) -> Io…` signatures for the seven
//!     recognizers that return fragments instead of io. `hono` shipped declaring one side of a join it
//!     filled both sides of, and a human found it. Rows no evidence reaches are pinned by name with a
//!     reason each — see that file's own doc for what the pin does and does not mean.
//! 19. **Call-KIND binding** (`call_kind_readers.rs`) — `zzop_core::RULE_READ_CALL_KINDS` must equal the
//!     set of kinds shipped `call-scan` rules name in `CallScan::kind`. Same doctrine as
//!     `io_kind_readers.rs` (contract-less until now because the constant was honestly empty), and the
//!     same two silent drift directions: a kind read but unlisted makes an unread-kind disclosure cry
//!     wolf, a kind listed but unread makes it stay quiet about facts nothing consumes. It derives its
//!     subject set from the LOADED PACKS rather than by grepping source, because a call kind is named
//!     declaratively in JSON where an io kind is compared in Rust — a transliteration of io's grep would
//!     have found zero literals and gone vacuously green. Second leg: every kind a rule names must be a
//!     spelling some `zzop_core::CALL_KIND_*` constant fixes, which is what catches a typo the set
//!     equality alone would happily bless on both sides at once.
//! 20. **Census binaries stay standalone** (`census_binaries_stay_standalone_and_say_why`) — the
//!     deliberate exceptions to the harness fold contract 14 guards. `git_spawn_census.rs` and
//!     `analyze_parse_census.rs` each assert an equality over a PROCESS-WIDE counter, and tests within
//!     one binary share the process on parallel threads — folding either into a harness silently turns
//!     its census into a measurement of its neighbors. The pin holds three legs: each file's
//!     top-level existence, its own written reason for standing alone, and — the fold's completion
//!     guard — that the pair are the ONLY top-level `.rs` files (everything else lives in a
//!     registered harness directory; a stray top-level file is a silently re-added per-file binary).
//! 21. **Native-rule io-CHANNEL binding** (`rule_channels.rs`) — the rule-side twin of contract 18.
//!     Each rules crate's `NATIVE_ANALYSES` table states, on the row registration reads, which io
//!     channels the rule's input is drawn from; this contract holds every row to the `kind == "…"`
//!     comparisons that rule's own module makes, both directions. Contract 19's sibling
//!     (`io_kind_readers`) greps exactly those literals and deliberately discards the rule
//!     attribution, which is why "which rules go quiet when route extraction comes up empty" had no
//!     answer in production code at all. Rules the ENGINE hands a pre-filtered input name no kind
//!     themselves; each is pinned to the PRODUCING function, whose body is re-scanned by the same
//!     extractor, so the pin supplies a derived channel rather than a restated one. The two crates
//!     that declare no io are checked crate-wide instead — see that file's own doc for the boundary.
//! 22. **Java-lane evidence ladder** (`java_lane_evidence.rs`) — `docs/rules/catalog.md`'s `security`
//!     paragraph publishes HOW FAR the Java lane is validated, and that paragraph ships verbatim as the
//!     `zzop://contract/rule-catalog` MCP resource. Contract 5 already pins the count of rules admitting
//!     `.java`; this pins the two rungs above it — every such rule is anchored in the committed
//!     detection benchmark, and the per-rule Java UNIT-test counts (plus both shortfall LISTS, since a
//!     count can stay true while the names behind it rotate). The rule↔test attribution is OBSERVED,
//!     not declared: a test that writes a `.java` fixture and calls `hits(&out, "<rule id>")` is that
//!     rule's Java evidence. The claim that stood before this contract — "13 of the 18 carry a Java
//!     firing/non-firing pair" — was right for a different property and wrong by two for the one it
//!     spelled, and its own stated method (which test FILES write a `.java` path) could not produce it.

use std::fs;
use std::path::{Path, PathBuf};

use zzop_core::{load_dsl_packs, RulePackDef, RuleRegistry};
use zzop_engine::register_all_native;

mod bare_words;
mod call_kind_readers;
mod capability_matrix;
mod catalog_sync;
mod channel_direction;
mod config_surface;
mod dsl_messages;
mod envelope_contract_version;
mod host_vocabulary;
mod id_hygiene;
mod io_kind_readers;
mod java_lane_evidence;
mod kernel_vocabulary;
mod literal_scan_threshold;
mod markers;
mod native_messages;
mod pack_loading;
mod path_anchor_pin;
mod policy_pins;
mod recognizer_channels;
mod recognizer_drift;
mod reference_unit_tests;
mod reference_validation;
mod rule_axis;
mod rule_channels;
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

fn exported_packs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/packs")
}

/// Every pack this BUILD ships, bundled and EXPORTED alike — `rules/dsl/**` plus `examples/packs/*.json`.
///
/// Almost every contract here wants [`load_all_packs`] instead, because its subject is *what a default
/// run loads*. Two do not, and the difference is not a preference: an exported pack is not a deleted
/// pack. It is compiled into this binary as an `example-pack-*` contract resource
/// (`zzop_config::EXAMPLE_PACK_CONTRACTS`, generated by `crates/config/build.rs` from this very
/// directory), served by `zzop contract`, explainable through `zzop explain --config`, and it RUNS the
/// moment a tree's `zzop/rules/` holds it. So a contract whose subject is *what this build's rule corpus
/// says or reads* — rather than *what fires by default* — has to read both, or it starts measuring the
/// bundle's contents instead of its own claim.
///
/// The two callers, and why each is one of them, are documented at their own assertions:
/// `rule_axis::the_severity_band_does_not_reproduce_the_axis` and
/// `call_kind_readers::rule_read_call_kinds_equals_the_kinds_shipped_call_scan_rules_name`. Both went red
/// on 2026-08-12 when the last `axis: opinion` rules were exported, and both were red for the same
/// reason: their population had narrowed under them while the property they measure had not changed.
fn load_shipped_packs() -> Vec<RulePackDef> {
    let mut packs = load_all_packs();
    let result = load_dsl_packs(&exported_packs_dir());
    assert!(
        result.errors.is_empty(),
        "exported pack load errors under {}: {:?}",
        exported_packs_dir().display(),
        result.errors
    );
    packs.extend(result.packs.into_iter().map(|(_, pack)| pack));
    assert!(
        packs.len() > load_all_packs().len(),
        "no exported pack was read from {} — this loader would then be `load_all_packs` under another \
         name, and both its callers would go quietly back to measuring the bundle alone",
        exported_packs_dir().display()
    );
    packs
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

/// Every flat `main.rs`-harness test directory in this crate, with a FLOOR on its module count.
///
/// The floor is deliberately far below today's count: its job is not to track ordinary file
/// removal (a deliberate act, visible in the diff next to its `mod` line) but to fire on
/// STRUCTURAL breakage — the walk pointed at a re-rooted/renamed directory, or a mass unhooking
/// that empties the `mod` list — where the set equality below would go vacuously green (0 == 0).
const REGISTERED_FLAT_TEST_DIRS: &[(&str, usize)] =
    &[("tests/rule_contracts", 15), ("tests/integration", 60)];

/// Contract 14 — every `.rs` file in each registered flat test directory is `mod`-registered in that
/// directory's `main.rs`, so no test can sit in such a directory silently uncompiled. Reads the
/// harness entry's own text rather than any generated list: the `mod` lines ARE the wiring, and
/// re-deriving them from anything else would just move the drift.
///
/// Both directions are asserted even though only one can actually rot: a `mod x;` with no `x.rs` is a
/// compile error (so the test binary would not build and this test could never report it), while a
/// `x.rs` with no `mod x;` compiles fine and is the real hole. The set equality is one assertion for
/// both, and its offender lists name the exact fix in each direction. A side effect worth naming:
/// the equality also makes it impossible to `mod`-declare a file that lives OUTSIDE the directory
/// (e.g. a top-level census binary) — the declared name would have no file in the walk.
///
/// Nested subdirectories are rejected outright rather than half-handled: none exist today, and a
/// `mod`-path scheme for them would be untested machinery guarding nothing.
#[test]
fn every_registered_flat_test_dir_source_file_is_mod_registered() {
    for &(dir_rel, floor) in REGISTERED_FLAT_TEST_DIRS {
        assert_flat_test_dir_fully_mod_registered(dir_rel, floor);
    }
}

fn assert_flat_test_dir_fully_mod_registered(dir_rel: &str, floor: usize) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir_rel);
    let main_rs = dir.join("main.rs");
    let text = fs::read_to_string(&main_rs)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", main_rs.display()));

    let declared: std::collections::BTreeSet<String> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("mod ")?.strip_suffix(';'))
        .map(str::to_string)
        .collect();

    let mut present = std::collections::BTreeSet::new();
    let entries =
        fs::read_dir(&dir).unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        assert!(
            !path.is_dir(),
            "{} is a subdirectory of {dir_rel} — this contract only understands a FLAT \
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
        present,
        declared,
        "{dir_rel}/ and its main.rs `mod` list disagree.\nfiles with NO `mod` line (they are \
         never compiled — no error, no warning, and every test in them runs never): {:?}\n`mod` \
         lines with no file: {:?}",
        present.difference(&declared).collect::<Vec<_>>(),
        declared.difference(&present).collect::<Vec<_>>(),
    );
    assert!(
        declared.len() >= floor,
        "{dir_rel}/ registers only {} modules but its floor is {floor} — either the directory was \
         re-rooted (this walk is no longer looking at the real tests) or its population was mass-moved \
         without updating REGISTERED_FLAT_TEST_DIRS. If the shrink is deliberate, lower the floor in \
         the same commit.",
        declared.len()
    );
}

/// Contract 20 — the two census binaries stay STANDALONE top-level `tests/*.rs` files, each keeps
/// its own written reason for that, and — since the 2026-08-09 fold — they are the ONLY top-level
/// `.rs` files left. Both are process-wide-counter censuses (`zzop_git::spawn_log`,
/// `zzop_parser_typescript::parse_count`): cargo runs each `tests/*.rs` as its own process, but tests
/// WITHIN a binary share the process on parallel threads, so folding either file into a shared harness
/// makes its counter equality silently meaningless — the count would include every neighbor's spawns.
///
/// The strict top-level leg is the fold's completion guard in the OTHER direction: 73 binaries were
/// folded into `tests/integration/` for one link instead of 73, and a new top-level `.rs` quietly
/// reintroduces a per-file binary (cargo auto-discovers it — no manifest edit, no warning). A new
/// standalone binary is allowed exactly when it has a census-shaped reason to be alone; adding it to
/// the allowlist here is the act that writes that reason down.
#[test]
fn census_binaries_stay_standalone_and_say_why() {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let allowed = ["analyze_parse_census.rs", "git_spawn_census.rs"];
    for name in allowed {
        let path = tests_dir.join(name);
        let text = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{} must exist as its OWN top-level test binary — its census counter is \
                 process-wide, and a harness fold would make the counted number include every \
                 neighbor test's activity: {e}",
                path.display()
            )
        });
        assert!(
            text.contains("process-global") || text.contains("process-wide"),
            "{name} no longer says WHY it must stay a standalone binary (expected the words \
             \"process-global\" or \"process-wide\" in its doc) — restore the reason before anything \
             else; a lone file with no written reason is one refactor away from being folded."
        );
    }

    let mut top_level: Vec<String> = fs::read_dir(&tests_dir)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", tests_dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rs"))
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect();
    top_level.sort();
    assert_eq!(
        top_level, allowed,
        "top-level crates/engine/tests/*.rs must be exactly the census pair. Every other test \
         belongs in a registered harness directory (tests/integration/, tests/rule_contracts/ — one \
         link each instead of one per file). If the new file genuinely needs its own process, add it \
         to this allowlist WITH its reason written in the file, like the census pair."
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
