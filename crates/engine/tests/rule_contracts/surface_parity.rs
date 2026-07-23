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
//! **Known blind spot of this route** (documented here rather than hidden): `AnalyzeOutputView` has a
//! 21st possible field, `ruleOverridesApplied`, which is deliberately OMITTED from the JSON entirely (never
//! an empty `{}`) when a caller's run requested no `disabledRules`/`severityOverrides` — see
//! `zzop_engine::RuleOverridesApplied`'s own doc. The pinned-literal fixture in `analyze_tests.rs` never
//! requests an override, so its pinned key-set literal does not include `ruleOverridesApplied` either, and
//! neither does this registry/test. `ruleOverridesApplied` is consequently NOT covered by this parity
//! contract today — a real gap in the pragmatic route's completeness, not a claim that the field doesn't
//! exist or doesn't matter (it is forwarded by the MCP lane, per that source's own doc comments).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn registry_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/contracts/surface-parity.json")
}

fn facade_analyze_tests_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/facade/src/analyze_tests.rs")
}

fn mcp_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../host/src")
}

/// A parallel worktree agent is moving `crates/host`'s shaping code into a new `crates/summary` crate
/// (reply SHAPE preserved byte-for-byte, per this task's own brief) — scanned too, defensively, so TEST 3
/// survives that move with no edits here. Absent today; `concat_rs_sources` simply yields an empty string
/// for a directory that does not exist yet (`collect_rs_files` already degrades the same way — see its own
/// doc in `main.rs`).
fn summary_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/summary/src")
}

/// The 2026-07-23 product-layer split moved the MCP tool dispatch (`tools.rs`, `tools/definitions.rs`,
/// `server.rs`, `resources.rs`) out of `crates/host/src` into its own `packages/mcp/src` Cargo package
/// — scanned so TEST 3 keeps seeing every place a reply field literal could be re-emitted on the MCP
/// wire.
fn mcp_pkg_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/mcp/src")
}

/// Same split moved the CLI's own argv dispatch (`main.rs`, `cli.rs`) out of `crates/host/src` into
/// `packages/cli-bin/src` — scanned for the same reason as `mcp_pkg_src_dir`.
fn cli_bin_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/cli-bin/src")
}

fn load_registry() -> serde_json::Value {
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

/// Loads `crates/facade/src/analyze_tests.rs` and returns `(single_tree_keys, multi_tree_keys)` — the two
/// pinned top-level key-set literals (`analyze_json_top_level_key_set_is_pinned_exactly` /
/// `analyze_trees_json_top_level_key_set_is_pinned_exactly`).
fn facade_pinned_key_sets() -> (BTreeSet<String>, BTreeSet<String>) {
    let path = facade_analyze_tests_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    let single_marker = "fn analyze_json_top_level_key_set_is_pinned_exactly";
    let multi_marker = "fn analyze_trees_json_top_level_key_set_is_pinned_exactly";

    let single_start = text.find(single_marker).unwrap_or_else(|| {
        panic!(
            "{single_marker} not found in {} — has it been renamed?",
            path.display()
        )
    });
    let multi_start = text.find(multi_marker).unwrap_or_else(|| {
        panic!(
            "{multi_marker} not found in {} — has it been renamed?",
            path.display()
        )
    });

    let single_keys = extract_pinned_keys(&text, single_start, single_marker);
    let multi_keys = extract_pinned_keys(&text, multi_start, multi_marker);
    (single_keys, multi_keys)
}

/// Every `.rs` file's content under `dir`, concatenated with a form-feed separator (an unambiguous
/// non-code byte that can never straddle a real match) — good enough for the pragmatic substring checks
/// below, which never need to attribute a match back to a specific file. Yields an empty string when `dir`
/// does not exist (e.g. `crates/summary/src` before the parallel refactor lands) — `crate::collect_rs_files`
/// already degrades a missing directory to "nothing collected" (see its own doc in `main.rs`).
fn concat_rs_sources(dir: &Path) -> String {
    let mut files = Vec::new();
    crate::collect_rs_files(dir, &mut files);
    files
        .iter()
        .map(|f| std::fs::read_to_string(f).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\x0c")
}

/// The MCP lane's scanned sources for TEST 3: `crates/host/src` (the shared dispatch + embedded
/// contracts), `crates/summary/src` (the shaping logic — see `summary_src_dir`'s doc), and, since the
/// 2026-07-23 product-layer split, `packages/mcp/src` (the MCP tool schemas/dispatch) and
/// `packages/cli-bin/src` (the CLI's own argv dispatch) — every source a reply field literal could be
/// re-emitted from, across both host products.
fn mcp_lane_sources() -> String {
    let mut combined = concat_rs_sources(&mcp_src_dir());
    combined.push('\x0c');
    combined.push_str(&concat_rs_sources(&summary_src_dir()));
    combined.push('\x0c');
    combined.push_str(&concat_rs_sources(&mcp_pkg_src_dir()));
    combined.push('\x0c');
    combined.push_str(&concat_rs_sources(&cli_bin_src_dir()));
    combined
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
// shaped summary the `zzop-mcp analyze`/`cross` CLI subcommands print. @zzop/cli is a zero-logic shim
// that spawns that native binary — it has no render surface of its own (no `jsCliRender`/`mdReport`,
// which briefly existed here across the npm distribution's removal-then-restoration; see the
// registry's own `_doc` historical note).
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
/// between the field name and either quote. This matches this codebase's own `serde_json::json!({ "key":
/// value })` emission style throughout `crates/host/src` (confirmed by inspection: every forwarded key in
/// `analyze.rs`/`tools.rs` is written exactly this way, e.g. `"fileCount": output_view["fileCount"]`).
/// This is deliberately narrower than a bare substring search: a short field name like `ir` would otherwise
/// false-positive inside unrelated identifiers/prose that merely CONTAIN "ir" as a substring (`circular`,
/// `directory`, doc-comment mentions of `CommonIr`, ...) — anchoring on the exact `"ir":` shape (a literal
/// JSON key EMISSION, not just the two letters appearing anywhere) avoids that class of false positive
/// entirely, the same "textual-proximity proxy with a precise shape gate" spirit as this crate's own
/// `config_surface.rs` checks.
///
/// **What this proves**: a `carry` row's field name is emitted as a JSON key literal somewhere in the MCP
/// lane's sources, and an `omit` row's field name is not. **What this cannot prove**: a key built
/// dynamically (`format!("{field}")` as a key, or a `.get(field)` lookup with no matching literal
/// re-emission under the same name) is invisible to this scan either way — same "pragmatic proxy, not a
/// semantics engine" caveat every grep-based contract in this file carries.
#[test]
fn mcp_lane_forwards_exactly_the_rows_marked_carry_and_never_forwards_the_rows_marked_omit() {
    let registry = load_registry();
    let sources = mcp_lane_sources();
    assert!(
        !sources.is_empty(),
        "found no .rs sources under crates/host/src, crates/summary/src, packages/mcp/src, or \
         packages/cli-bin/src — path resolution likely broke"
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
                 not appear as a forwarded JSON key literal anywhere under crates/host/src, \
                 crates/summary/src, packages/mcp/src, or packages/cli-bin/src — either the registry is \
                 stale (fix the row) or the MCP lane silently stopped forwarding this field (fix the code)"
            ),
            "omit" => assert!(
                !present,
                "analyzeOutputView.{field} is marked `omit` for mcpAnalyzeReply, but {key_literal:?} DOES \
                 appear as a forwarded JSON key literal under crates/host/src, crates/summary/src, \
                 packages/mcp/src, or packages/cli-bin/src — either the registry is stale (the MCP lane \
                 now forwards this field — update the row and its note) or this is an unintended new leak"
            ),
            "carry-conditional" => { /* exempt from this strict check — see this test's own doc */ }
            other => panic!("analyzeOutputView.{field}.mcpAnalyzeReply has an unknown status {other:?}"),
        }
    }
}
