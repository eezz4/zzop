//! Contract 11: reference validation — a shipped message must never recommend a config key/flag that does
//! not exist. This is the machine contract for the defect class a message audit found live: `--since=all`,
//! `--repo=`, and `scanners.vocabulary.commitTypePatterns` were all recommended by real messages despite
//! none of them being real knobs. Both checks below load
//! `packages/cli/lib/config-surface.json` — the single vocabulary file also consumed by
//! `packages/cli/lib/mapper.js`'s `KNOWN_KEYS` (see that file), so the CLI's own runtime and this test
//! can never disagree about what a valid flag/config key is.
//!
//! The vocabulary structs and the extraction/validation helpers live in `config_surface.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config_surface::{
    extract_config_context_tokens, extract_flag_references, load_config_surface,
    unknown_config_context_tokens, unknown_flag_references,
};
use crate::{collect_rs_files, native_dir};

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
/// since `rule_contracts` lives in `crates/engine/tests/`).
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

/// `crates/config/src/**/*.rs`, recursively — the Rust config front-end's warnings/errors are
/// user-facing (mapper shape errors, mount validation, workspaces quirks) and name config keys and
/// flags exactly like engine messages do. Added 2026-07-17: the scan-set expansion trigger
/// ("before the next message-adding batch") had been bypassed twice — user-facing messages kept
/// landing here unscanned.
fn config_src_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../config/src"),
        &mut out,
    );
    out
}

/// `crates/facade/src/**/*.rs`, recursively — facade errors (queryIo argument errors, request
/// validation) are host-forwarded verbatim and name request fields/config vocabulary.
fn facade_src_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../facade/src"),
        &mut out,
    );
    out
}

/// `packages/mcp/src/**/*.rs`, recursively — the MCP host's tool descriptions, guided errors, usage
/// text, and embedded-resource glue are the FIRST surface a binary-only user reads; they name config
/// keys (`configPath`, `packsDir`, ...) and must stay inside the vouched vocabulary.
fn mcp_src_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/mcp/src"),
        &mut out,
    );
    out
}

/// `crates/summary/src/**/*.rs`, recursively — the shared summary/shaping crate behind every host
/// (`zzop-mcp` today): its guided errors, tool-argument validation messages, and disclosure/warning
/// text carry exactly the same config-key/flag references `packages/mcp/src` used to own alone before
/// this logic moved out into its own crate (2026-07-17 zzop-summary extraction).
fn summary_src_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../summary/src"),
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
/// `crates/engine/src/**/*.rs` + `crates/metrics/src/**/*.rs` + `crates/config/src/**/*.rs` +
/// `crates/facade/src/**/*.rs` + `crates/summary/src/**/*.rs` + `packages/mcp/src/**/*.rs` +
/// `packages/cli/lib/*.js`, sorted so a failing assertion's offender list has a stable, diffable order
/// across runs.
fn reference_validation_scanned_files() -> Vec<PathBuf> {
    let mut out = native_rule_src_rs_files();
    out.extend(engine_src_files());
    out.extend(metrics_src_files());
    out.extend(config_src_files());
    out.extend(facade_src_files());
    out.extend(summary_src_files());
    out.extend(mcp_src_files());
    out.extend(cli_lib_js_files());
    out.sort();
    out
}

/// Contract #11, CHECK A — every `--flag`-shaped token on a non-comment line of every scanned file must
/// name a real CLI flag or a real external tool's flag (`config-surface.json`'s `cliFlags` ∪
/// `externalToolFlags`). This is the exact machine check that would have caught the shipped `--since=all`/
/// `--repo=` defects (see `reference_unit_tests.rs`'s `flag_reference_unit_tests` for those pinned as
/// unit tests).
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
/// defect (see `reference_unit_tests.rs`'s `config_context_unit_tests` for that pinned as a unit test).
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
/// - `require` — `rules/native/rules-graph/src/unreachable.rs`'s `unreachable` message's "Disable via
///   config `rules: {...}` ... if this island is reached by a mechanism this graph doesn't see (e.g.
///   dynamic `` `require` ``, a plugin loader)" aside names Node's `require()` as an example of an
///   invisible-to-the-graph reachability mechanism, not a config knob; same co-location pattern.
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
