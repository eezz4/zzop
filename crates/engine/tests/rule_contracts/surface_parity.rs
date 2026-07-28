//! Surface-parity meta-tests — see `docs/contracts/surface-parity.json`'s own `_doc` for the full
//! rationale (three historical drift incidents that motivated this registry: `configWarnings` computed by
//! the engine but never read by the JS CLI's pretty renderer; git-derived signals computed by the facade
//! but invisible in the MCP lane; run-level warnings that needed a deliberate, easy-to-forget forwarding
//! step at every delivery surface). This file makes that registry load-bearing:
//!   - TEST 1 (`registry_*_keys_match_the_facade_pinned_*_key_set`, x2) catches an unregistered field —
//!     one added or renamed on `AnalyzeOutputView`/`MultiAnalyzeOutputView` with no matching registry row.
//!   - TEST 2 (`every_omit_or_conditional_row_carries_a_non_empty_note`) catches an `omit`/
//!     `carry-conditional` row shipped with no explanation of why, or where the data IS available.
//!   - TEST 3 (`mcp_lane_forwards_exactly_the_rows_marked_carry_and_never_forwards_the_rows_marked_omit`)
//!     spot-checks the registry's `mcpAnalyzeReply` column against the real MCP source, for the strict
//!     `carry`/`omit` rows only (see that test's own doc for why `carry-conditional` rows are exempt).
//!
//! ## Route taken for TEST 1's "actual serialized key set"
//! `zzop-facade` is **not** a dev-dependency of `zzop-engine` (checked `crates/engine/Cargo.toml` — no
//! `[dev-dependencies]` section at all today); adding one is out of this task's scope, which owns only this
//! test file, its own `mod` registration, and the registry — not `Cargo.toml`. So instead of running a real
//! `analyze()`/`analyzeTrees()` through `zzop_facade` in-process the way `crates/facade/src/analyze_tests.rs`
//! itself does, this file takes the pragmatic route the task brief names explicitly: it parses the pinned
//! key-set string literals straight out of that same file's own
//! `analyze_json_top_level_key_set_is_pinned_exactly` / `analyze_trees_json_top_level_key_set_is_pinned_exactly`
//! tests — an already-pinned, already-drift-coupled truth source (any facade output field drift breaks
//! THOSE tests first, in the same crate, before it could ever reach this one silently).
//!
//! **Former blind spot of this route, CLOSED 2026-07-26.** `AnalyzeOutputView` has a 21st possible
//! field, `ruleOverridesApplied`, deliberately OMITTED from the JSON entirely (never an empty `{}`)
//! when a caller's run requested no `disabledRules`/`severityOverrides` — see
//! `zzop_engine::RuleOverridesApplied`'s own doc. The single pinned fixture this route read requested
//! no override, so its literal could never contain that key, so the registry had no row for it and the
//! field rode every surface with no parity coupling at all. This file used to disclose that and stop
//! there; a disclosure is not a guard. `analyze_tests.rs` now carries a SECOND single-tree pin —
//! `analyze_json_top_level_key_set_with_rule_overrides_is_pinned_exactly`, whose fixture DOES request
//! an override — and [`facade_pinned_key_sets`] returns the UNION of the two, so TEST 1 demands a
//! registry row for the 21st field like any other.
//!
//! The union is the correct combinator and not a shortcut: each pin is itself an EXACT `assert_eq!` on
//! its own fixture's key list, so neither can grow a field silently, and their union is exactly "every
//! key this output can produce". A future third conditional field needs its own pin added there and its
//! marker added here — the same two-step every field already costs.
//!
//! What is still NOT proven for `ruleOverridesApplied` specifically: its registry row is
//! `carry-conditional`, which TEST 3 exempts by design (see that test's own doc) — the MCP lane's
//! forwarding is a `.get()`-gated `summary.insert("ruleOverridesApplied".to_string(), ...)`, not a
//! `json!` key literal, so the literal matcher could not read it either way. That is the same standing
//! caveat every `carry-conditional` row carries, not a residue of this field's own gap.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn registry_path() -> PathBuf {
    workspace_root().join("docs/contracts/surface-parity.json")
}

fn facade_analyze_tests_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/facade/src/analyze_tests.rs")
}

/// `crates/summary/src` — the reply-shaping crate the since-deleted host crate's shaping code was split
/// into (reply SHAPE preserved byte-for-byte across that move). Where `analyze_summary`/`cross_summary`
/// actually build their JSON today, so it is the primary haystack for TEST 3.
fn summary_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/summary/src")
}

/// The 2026-07-23 product-layer split moved the MCP tool dispatch (`tools.rs`, `tools/definitions.rs`,
/// `server.rs`, `resources.rs`) out of the then-shared host crate into its own `packages/mcp/src` Cargo
/// package — scanned so TEST 3 keeps seeing every place a reply field literal could be re-emitted on the
/// MCP wire.
fn mcp_pkg_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/mcp/src")
}

/// Same split moved the CLI's own argv dispatch (`main.rs`, `cli/`) into `packages/cli-bin/src` —
/// scanned for the same reason as `mcp_pkg_src_dir`.
fn cli_bin_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/cli-bin/src")
}

/// The three directories TEST 3's haystack is built from, named once so the haystack builder and its
/// non-emptiness guard can never disagree about the scan set.
fn mcp_lane_dirs() -> [(&'static str, PathBuf); 3] {
    [
        ("crates/summary/src", summary_src_dir()),
        ("packages/mcp/src", mcp_pkg_src_dir()),
        ("packages/cli-bin/src", cli_bin_src_dir()),
    ]
}

pub(crate) fn load_registry() -> serde_json::Value {
    let path = registry_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()))
}

fn registry_keys(registry: &serde_json::Value, root: &str) -> BTreeSet<String> {
    registry[root]
        .as_object()
        .unwrap_or_else(|| panic!("surface-parity.json's `{root}` must be an object"))
        .keys()
        .cloned()
        .collect()
}

/// Extracts the flat string-literal array following a standalone `keys,` source line (searched from
/// `search_from` onward in `source`) — see this file's module doc for why this pragmatic text extraction
/// (rather than an in-process `zzop_facade` call) is the route TEST 1 takes.
///
/// Anchors on a *standalone* `keys,` line (`^[ \t]*keys,[ \t]*\r?\n[ \t]*\[`), not the bare substring
/// `"keys,"` — `analyze_trees_json_top_level_key_set_is_pinned_exactly` also contains an unrelated
/// `entry_keys,` assertion later in the same function, and `"entry_keys,"` ends with the substring
/// `"keys,"` too; the line-anchored regex only matches the real `keys,` line, never that one.
fn extract_pinned_keys(source: &str, search_from: usize, context: &str) -> BTreeSet<String> {
    let anchor = regex::Regex::new(r"(?m)^[ \t]*keys,[ \t]*\r?\n[ \t]*\[").expect("static regex");
    let haystack = &source[search_from..];
    let m = anchor.find(haystack).unwrap_or_else(|| {
        panic!("could not find a standalone `keys,` array literal for {context} — has the pinned test's shape changed?")
    });
    let bracket_pos = search_from + m.end() - 1; // the '[' itself: the match's last byte.
    let after_bracket = &source[bracket_pos + 1..];
    let close = after_bracket.find(']').unwrap_or_else(|| {
        panic!("no closing `]` found for the `keys,` array literal for {context}")
    });
    let array_text = &after_bracket[..close];
    let string_re = regex::Regex::new("\"([^\"]+)\"").expect("static regex");
    string_re
        .captures_iter(array_text)
        .map(|c| c[1].to_string())
        .collect()
}

/// Loads `crates/facade/src/analyze_tests.rs` and returns `(single_tree_keys, multi_tree_keys)`.
///
/// `single_tree_keys` is the UNION of the two single-tree pins — the base one
/// (`analyze_json_top_level_key_set_is_pinned_exactly`, whose fixture requests no override) and the
/// override one (`analyze_json_top_level_key_set_with_rule_overrides_is_pinned_exactly`) — because the
/// base fixture structurally cannot see `ruleOverridesApplied`; see this file's module doc.
/// `multi_tree_keys` comes from `analyze_trees_json_top_level_key_set_is_pinned_exactly`, which has no
/// conditional field and so needs no second pin.
fn facade_pinned_key_sets() -> (BTreeSet<String>, BTreeSet<String>) {
    let path = facade_analyze_tests_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    // `find` returns the FIRST occurrence, and `..._with_rule_overrides_...` does not have
    // `..._is_pinned_exactly` as a prefix of the base name, so the two markers cannot alias.
    let single_markers = [
        "fn analyze_json_top_level_key_set_is_pinned_exactly",
        "fn analyze_json_top_level_key_set_with_rule_overrides_is_pinned_exactly",
    ];
    let multi_marker = "fn analyze_trees_json_top_level_key_set_is_pinned_exactly";

    let mut single_keys = BTreeSet::new();
    for marker in single_markers {
        let start = text.find(marker).unwrap_or_else(|| {
            panic!(
                "{marker} not found in {} — has it been renamed? Every pinned single-tree key-set \
                 literal must stay findable here; dropping one silently shrinks what this contract \
                 demands a registry row for",
                path.display()
            )
        });
        single_keys.extend(extract_pinned_keys(&text, start, marker));
    }
    let multi_start = text.find(multi_marker).unwrap_or_else(|| {
        panic!(
            "{multi_marker} not found in {} — has it been renamed?",
            path.display()
        )
    });

    let multi_keys = extract_pinned_keys(&text, multi_start, multi_marker);
    (single_keys, multi_keys)
}

/// Canonicalized `.rs` paths the registry declares as the implementation of a CLI-only lane
/// (`_cliOnlyLanes[<lane>].sources`) — the files TEST 3 subtracts from the MCP lane.
///
/// This is what turns `_cliOnlyLanes` from pure documentation into the scope boundary of the guard: a
/// lane that ships without declaring its sources leaves them inside the MCP lane, so a CLI-only
/// emission of an `omit` field fails the build until someone deliberately records the lane. Both
/// directions of TEST 3 subtract the same set, and the CARRY direction is the reason it must be both:
/// a CLI-only lane emits `coverage`/`warnings`/`disclosure` too, so leaving those files in the haystack
/// would let a CLI-only emission keep a `carry` row green after the MCP reply silently stopped
/// forwarding it — the registry's founding bug class, reintroduced through the back door.
///
/// A declared path that does not resolve is a hard failure, not a silent skip: a moved lane source must
/// update this registry in the same commit, or the exclusion would quietly widen back.
pub(crate) fn cli_only_lane_sources(registry: &serde_json::Value) -> BTreeSet<PathBuf> {
    let lanes = registry["_cliOnlyLanes"]
        .as_object()
        .expect("surface-parity.json's `_cliOnlyLanes` must be an object");
    let mut out = BTreeSet::new();
    for (lane, entry) in lanes {
        if lane.starts_with('_') {
            continue; // `_doc`, the block's own prose — not a lane.
        }
        let sources = entry.get("sources").and_then(|v| v.as_array()).unwrap_or_else(|| {
            panic!(
                "_cliOnlyLanes[{lane:?}] must declare a `sources` array naming the .rs files that \
                 implement it — crates/engine/tests/rule_contracts/surface_parity.rs subtracts exactly \
                 those from the MCP lane it scans"
            )
        });
        for source in sources {
            let rel = source.as_str().unwrap_or_else(|| {
                panic!("_cliOnlyLanes[{lane:?}].sources entries must be workspace-relative path strings")
            });
            let path = workspace_root().join(rel);
            let canonical = std::fs::canonicalize(&path).unwrap_or_else(|e| {
                panic!(
                    "_cliOnlyLanes[{lane:?}].sources names {rel}, which does not resolve ({e}) — a \
                     moved or renamed lane source must update this registry in the SAME commit"
                )
            });
            out.insert(canonical);
        }
    }
    out
}

/// Whether a path is TEST source rather than emission source: any `tests` directory component, or a
/// file named `tests.rs` / `*_test.rs` / `*_tests.rs`. A fixture that mimics an engine output spells the
/// engine's own key literals on purpose; counting those as emissions made the guard dictate what a test
/// may name its own variables, which is not a wire contract.
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

/// A source file reduced to its (line-)comment-free text. Rust line comments — `//`, `///` and `//!` —
/// are dropped whole, because prose that MENTIONS a field name is not a place that emits it: a doc
/// comment reading ``the `"ir":` block`` is documentation, and treating it as a leak is what made the
/// guard's verdict depend on how carefully a module doc was worded.
///
/// Line-granularity, deliberately: stripping a TRAILING `//` comment off a code line would have to
/// decide whether the `//` is inside a string literal, and a lexer is far more machinery than a
/// textual-proximity proxy earns. The residual blind spot is a `/* ... */` block comment (this
/// workspace writes none) and a real emission sharing a line with a trailing comment (this workspace's
/// `rustfmt` style puts neither on one line).
fn emission_text(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// TEST 3's haystack: the emission text of every `.rs` file under `crates/summary/src` (the shaping
/// logic, and since the 2026-07-26 teardown also the embedded contract table), `packages/mcp/src` (the
/// MCP tool schemas/dispatch) and `packages/cli-bin/src` (the CLI's own argv dispatch) — MINUS test
/// sources ([`is_test_source`]) and MINUS the registry-declared CLI-only lane implementations
/// ([`cli_only_lane_sources`]). What remains is what an `analyze_repo`/`cross_repo` reply can actually
/// be built from.
///
/// A fourth directory, `crates/host/src`, was DROPPED here in that teardown rather than re-pointed. Its
/// only contribution to this haystack was the shared pass-through dispatch (deleted outright — both
/// products call `zzop-summary` directly now) and the embedded contract table, which moved into
/// `crates/summary/src` and is therefore still walked.
///
/// `crates/facade/src` is deliberately NOT added in its place — but not for the reason this doc used to
/// give. It claimed the facade "spells every one of these fields, `omit` rows included", so adding it
/// would invert the contract into "the producer must never name its own fields". MEASURED 2026-07-26,
/// and false: the facade serializes through `#[serde(rename_all = "camelCase")]` derives on
/// `crates/facade/src/output.rs`'s output views, so it writes no wire-key literals at all — all 24
/// registry field names score ZERO `"<field>":` hits across the 17 non-test `.rs` files under
/// `crates/facade/src`. Including it would have been INERT, not inverting. It stays out because it would
/// assert nothing, which is the weaker and true reason.
///
/// The property that measurement actually exposes belongs to the MATCHER, not to this directory list:
/// **it is structurally blind to serde-derived emission**, anywhere — the facade is merely where that
/// was measured. So the `omit` direction only ever catches a `json!`-style RE-EMISSION under the field's
/// own name; a handler that forwards a whole struct, or a whole `serde_json::Value`, puts every `omit`
/// field on the wire at zero literal hits. What that leaves open is narrower than it first sounds,
/// because the two axes have different owners: field EXISTENCE (inventory) is covered by TEST 1, which
/// reconciles the registry against the facade's own pinned key sets and therefore cannot miss a field
/// however it is serialized. The residual gap is DELIVERY SHAPE — whether a field that exists reaches
/// the MCP wire — and only for the bulk-forward spelling. This haystack is the delivery lane only, and
/// the check over it is a literal-re-emission proxy within that lane.
///
/// Files are joined with a form-feed (an unambiguous non-code byte that can never straddle a real
/// match) — the checks below never need to attribute a match back to a specific file. A missing
/// directory yields nothing rather than failing (`crate::collect_rs_files` degrades that way by design),
/// which is exactly why [`every_haystack_dir_actually_contributes_emission_text`] pins each one as
/// non-empty: an emptied scan root would otherwise leave every `omit` row trivially green.
fn mcp_lane_sources() -> String {
    mcp_lane_files()
        .iter()
        .map(|path| emission_text(&std::fs::read_to_string(path).unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\x0c")
}

/// The FILES TEST 3's haystack is built from — every `.rs` file under [`mcp_lane_dirs`], MINUS test
/// sources ([`is_test_source`]) and MINUS the registry-declared CLI-only lane implementations
/// ([`cli_only_lane_sources`]).
///
/// Split out from [`mcp_lane_sources`] (which is just this set's emission text) so the non-emptiness
/// guard can assert membership against the SAME post-subtraction set the test consumes, instead of
/// re-deriving a raw listing of its own. That is not a stylistic preference: the `contracts.rs` pin
/// below used to read a raw `collect_rs_files` listing, which meant adding that path to any
/// `_cliOnlyLanes[*].sources` array subtracted it from the real haystack while the pin stayed green.
fn mcp_lane_files() -> Vec<PathBuf> {
    let excluded = cli_only_lane_sources(&load_registry());
    let mut files = Vec::new();
    for (_name, dir) in mcp_lane_dirs() {
        crate::collect_rs_files(&dir, &mut files);
    }
    files.retain(|path| !is_test_source(path));
    files.retain(|path| {
        !std::fs::canonicalize(path).is_ok_and(|canonical| excluded.contains(&canonical))
    });
    files
}

/// NON-EMPTINESS GUARD for TEST 3's haystack — what makes its `omit` direction mean anything.
///
/// TEST 3 already asserts the whole haystack is non-empty, but that is a weak bar: three of the four
/// former scan roots could empty out and the fourth would still satisfy it, while every `omit` row went
/// trivially green (nothing to find) and every `carry` row leaned on whichever directory was left. That
/// is not hypothetical either — the 2026-07-26 `crates/host` teardown left `mcp_src_dir()` pointing at a
/// deleted directory and the suite stayed green with the root silently contributing nothing.
///
/// So each directory is pinned to contribute emission text on its own, and the file the teardown
/// RELOCATED into this haystack (`contracts.rs`, host -> summary) is pinned by name.
/// The two failure modes are reported separately (the directory is GONE vs. the directory is there and
/// matched nothing), because they need different fixes and the verdict cannot tell them apart from an
/// empty `Vec` alone — a single message asserting "renamed, moved or deleted" was diagnosing a cause it
/// had not measured.
#[test]
fn every_haystack_dir_actually_contributes_emission_text() {
    for (name, dir) in mcp_lane_dirs() {
        assert!(
            dir.is_dir(),
            "TEST 3 haystack directory {name} DOES NOT EXIST ({}) — it was renamed, moved or deleted, \
             and `collect_rs_files` degraded silently to zero files. Every `omit` row would go trivially \
             green over the narrowed haystack. Re-point it in `mcp_lane_dirs`, or drop it deliberately \
             and say so in `mcp_lane_sources`' doc.",
            dir.display()
        );
        let mut files = Vec::new();
        crate::collect_rs_files(&dir, &mut files);
        let non_test: Vec<_> = files.iter().filter(|p| !is_test_source(p)).collect();
        assert!(
            !non_test.is_empty(),
            "TEST 3 haystack directory {name} EXISTS but holds no non-test .rs file ({} .rs files \
             found in total) — its emission source moved elsewhere, or the whole directory became test- \
             only. Every `omit` row would go trivially green over the narrowed haystack. Re-point it in \
             `mcp_lane_dirs`, or drop it deliberately and say so in `mcp_lane_sources`' doc.",
            files.len()
        );
    }

    // The relocated file must be inside the haystack TEST 3 ACTUALLY CONSUMES — asserted against
    // `mcp_lane_files()` (post-subtraction) and by canonicalized full path. Both halves are corrections:
    // this pin's first version read a raw `collect_rs_files(&summary_src_dir(), ..)` listing and compared
    // BASENAMES, so (a) any `contracts.rs` anywhere under that tree satisfied it, and (b) adding this
    // exact path to any `_cliOnlyLanes[*].sources` array in `docs/contracts/surface-parity.json` would
    // subtract it from the real haystack while this guard stayed green — a pin that cannot see the
    // subtraction it exists to survive. Canonicalized rather than string-compared for the reason the
    // sibling `reference_validation.rs` census pin records: every path here is built by joining `..`
    // onto `CARGO_MANIFEST_DIR`, so a listing entry literally reads
    // `crates/engine/../../crates/summary/src/contracts.rs` and a suffix test fails on a present file
    // (that sibling's first version was measured doing exactly that).
    let haystack: BTreeSet<PathBuf> = mcp_lane_files()
        .iter()
        .filter_map(|p| std::fs::canonicalize(p).ok())
        .collect();
    let relocated = summary_src_dir().join("contracts.rs");
    let expected = std::fs::canonicalize(&relocated).unwrap_or_else(|e| {
        panic!(
            "crates/summary/src/contracts.rs does not exist ({e}) — the embedded contract table moved \
             here in the 2026-07-26 `crates/host` teardown; if it moved again, re-point this pin"
        )
    });
    assert!(
        haystack.contains(&expected),
        "crates/summary/src/contracts.rs is not in the haystack TEST 3 consumes — it is the one part of \
         the deleted host crate's emission text this lane still owns. Either it moved again (re-point \
         the scan), or it is being SUBTRACTED: check that no `_cliOnlyLanes[*].sources` entry in \
         docs/contracts/surface-parity.json names it."
    );
}

#[test]
fn registry_analyze_output_view_keys_match_the_facade_pinned_single_tree_key_set() {
    let registry = load_registry();
    let registry_keys = registry_keys(&registry, "analyzeOutputView");
    let (facade_keys, _multi) = facade_pinned_key_sets();
    assert_eq!(
        registry_keys, facade_keys,
        "docs/contracts/surface-parity.json's `analyzeOutputView` key set must equal the facade's pinned \
         single-tree top-level key set (crates/facade/src/analyze_tests.rs's \
         analyze_json_top_level_key_set_is_pinned_exactly) — add/remove the registry row in the SAME \
         commit as any facade output field change.\nregistry-only keys: {:?}\nfacade-only keys: {:?}",
        registry_keys.difference(&facade_keys).collect::<Vec<_>>(),
        facade_keys.difference(&registry_keys).collect::<Vec<_>>(),
    );
}

#[test]
fn registry_multi_analyze_output_view_keys_match_the_facade_pinned_multi_tree_key_set() {
    let registry = load_registry();
    let registry_keys = registry_keys(&registry, "multiAnalyzeOutputView");
    let (_single, facade_keys) = facade_pinned_key_sets();
    assert_eq!(
        registry_keys, facade_keys,
        "docs/contracts/surface-parity.json's `multiAnalyzeOutputView` key set must equal the facade's \
         pinned multi-tree top-level key set (crates/facade/src/analyze_tests.rs's \
         analyze_trees_json_top_level_key_set_is_pinned_exactly) — add/remove the registry row in the SAME \
         commit as any facade output field change.\nregistry-only keys: {:?}\nfacade-only keys: {:?}",
        registry_keys.difference(&facade_keys).collect::<Vec<_>>(),
        facade_keys.difference(&registry_keys).collect::<Vec<_>>(),
    );
}

/// A row's status string for one of the three surface keys — `None` when the row lacks that key entirely
/// (a registry authoring bug, not a legitimate state; callers panic on `None` rather than silently skip).
fn row_status<'a>(row: &'a serde_json::Value, surface: &str) -> Option<&'a str> {
    row.get(surface).and_then(|v| v.as_str())
}

// Only one delivery surface remains: the MCP `analyze_repo`/`cross_repo` reply, which is the same
// shaped summary the `zzop analyze`/`zzop cross` CLI subcommands print — `zzop`, the `packages/cli-bin`
// binary. NOT `zzop-mcp`, as this comment said until 2026-07-27: that binary takes no analysis
// subcommand at all (bare invocation and `mcp` both start the stdio server; `version` and `help` are
// the only others), so the sentence attributed real subcommands to a binary that rejects them. The same
// false sentence was duplicated into `docs/contracts/surface-parity.json`'s `_doc` — one claim, two
// copies, both wrong, which is the standing argument for citing the registry rather than restating it.
// @zzop/cli is a zero-logic shim that spawns the native `zzop` binary — it has no render surface of its
// own (no `jsCliRender`/`mdReport`, which briefly existed here across the npm distribution's
// removal-then-restoration; see the registry's own `_doc` historical note).
const SURFACES: [&str; 1] = ["mcpAnalyzeReply"];

#[test]
fn every_omit_or_conditional_row_carries_a_non_empty_note() {
    let registry = load_registry();
    for root in ["analyzeOutputView", "multiAnalyzeOutputView"] {
        let fields = registry[root]
            .as_object()
            .unwrap_or_else(|| panic!("surface-parity.json's `{root}` must be an object"));
        for (field, row) in fields {
            let needs_note = SURFACES.iter().any(|surface| {
                let status = row_status(row, surface).unwrap_or_else(|| {
                    panic!("{root}.{field} is missing the required string field `{surface}`")
                });
                status == "omit" || status == "carry-conditional"
            });
            if !needs_note {
                continue;
            }
            let note = row.get("note").and_then(|v| v.as_str()).unwrap_or("");
            assert!(
                !note.trim().is_empty(),
                "{root}.{field} has an omit/carry-conditional status on at least one surface but an empty \
                 `note` — every omit/conditional row must explain why, and where the data IS available"
            );
        }
    }
}

/// TEST 3 — mcp truthfulness. Scoped to `analyzeOutputView` (the single-tree shape `analyze_repo` forwards)
/// per this task's design. Only STRICT `carry`/`omit` rows are checked: a `carry-conditional` row
/// (shaped/capped/gated forwarding — e.g. `findings` via `output::shape_findings`, or `gitWindow` via a
/// `.get()`-gated forward) legitimately may or may not spell the field name as a bare `"field":` json! key
/// literal, so asserting either direction on it would be noise, not signal (see that status's own rows'
/// notes in the registry for what actually happens).
///
/// **Matcher**: the literal substring `"<field>":` — quote, field name, quote, colon, with no whitespace
/// between the field name and either quote — searched in [`mcp_lane_sources`], i.e. the scanned emission
/// text with test sources and registry-declared CLI-only lane implementations subtracted. This matches
/// this codebase's own `serde_json::json!({ "key": value })` emission style throughout the scanned delivery lane
/// (confirmed by inspection: every forwarded key in `analyze.rs`/`tools.rs` is written exactly this way,
/// e.g. `"fileCount": output_view["fileCount"]`). It is deliberately narrower than a bare substring
/// search: a short field name like `ir` would otherwise false-positive inside unrelated identifiers that
/// merely CONTAIN "ir" (`circular`, `directory`, ...) — anchoring on the exact `"ir":` shape avoids that
/// class entirely, the same "textual-proximity proxy with a precise shape gate" spirit as this crate's
/// own `config_surface.rs` checks.
///
/// **Scope, corrected 2026-07-26.** The haystack used to be every `.rs` byte under the four directories,
/// which made this test assert something wider than the contract it names: it could not tell a CLI-only
/// lane's deliberate emission from an MCP leak, it counted doc comments and test fixtures as emissions,
/// and — the symptom that exposed it — it therefore dictated the public key name of a new CLI-only
/// surface, which had to spell its per-tree IR `commonIr` and route its fixture's placeholder through a
/// `const` to stay green. A guard whose scope forces a product naming decision has the wrong scope. The
/// three subtractions ([`emission_text`], [`is_test_source`], [`cli_only_lane_sources`]) narrow the
/// haystack to what an MCP reply can actually be built from, and each has its own doc for what it gives
/// up. Note the CLI-only subtraction applies to BOTH directions, and is if anything STRICTER for `carry`
/// — see [`cli_only_lane_sources`].
///
/// **What this proves**: a `carry` row's field name is emitted as a JSON key literal somewhere the MCP
/// reply is built, and an `omit` row's field name is not. **What this cannot prove**: a key built
/// dynamically (`format!("{field}")` as a key, or a `.get(field)` lookup with no matching literal
/// re-emission under the same name) is invisible to this scan either way — same "pragmatic proxy, not a
/// semantics engine" caveat every grep-based contract in this file carries.
#[test]
fn mcp_lane_forwards_exactly_the_rows_marked_carry_and_never_forwards_the_rows_marked_omit() {
    let registry = load_registry();
    let sources = mcp_lane_sources();
    assert!(
        !sources.is_empty(),
        "found no MCP-lane .rs emission text under crates/summary/src, \
         packages/mcp/src, or packages/cli-bin/src — path resolution or the CLI-only-lane subtraction \
         likely broke (per-directory coverage is pinned by \
         every_haystack_dir_actually_contributes_emission_text, which fails first and more precisely)"
    );
    let fields = registry["analyzeOutputView"]
        .as_object()
        .expect("analyzeOutputView must be an object");
    for (field, row) in fields {
        let status = row_status(row, "mcpAnalyzeReply")
            .unwrap_or_else(|| panic!("analyzeOutputView.{field} is missing `mcpAnalyzeReply`"));
        let key_literal = format!("\"{field}\":");
        let present = sources.contains(&key_literal);
        match status {
            "carry" => assert!(
                present,
                "analyzeOutputView.{field} is marked `carry` for mcpAnalyzeReply, but {key_literal:?} does \
                 not appear as a forwarded JSON key literal anywhere in the MCP lane \
                 (crates/summary/src, packages/mcp/src, packages/cli-bin/src, minus tests and minus the \
                 `_cliOnlyLanes` implementations) — either the registry is stale (fix the row) or the MCP \
                 lane silently stopped forwarding this field (fix the code). A CLI-only lane emitting it \
                 does NOT count: that is precisely the drift this row guards against"
            ),
            "omit" => assert!(
                !present,
                "analyzeOutputView.{field} is marked `omit` for mcpAnalyzeReply, but {key_literal:?} DOES \
                 appear as a forwarded JSON key literal in the MCP lane \
                 (crates/summary/src, packages/mcp/src, packages/cli-bin/src, minus tests and minus the \
                 `_cliOnlyLanes` implementations) — either the registry is stale (the MCP lane now \
                 forwards this field: update the row and its note) or this is an unintended new leak. If \
                 the emission belongs to a CLI-only lane, declare that lane and its `sources` in the \
                 registry's `_cliOnlyLanes` instead of renaming the key around this guard"
            ),
            "carry-conditional" => { /* exempt from this strict check — see this test's own doc */ }
            other => panic!("analyzeOutputView.{field}.mcpAnalyzeReply has an unknown status {other:?}"),
        }
    }
}
