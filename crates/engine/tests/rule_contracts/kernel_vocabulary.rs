//! Contract 8: kernel is rule-vocabulary-free — crates/core must not name a native analysis id.

use std::fs;
use std::path::{Path, PathBuf};

use zzop_core::RuleRegistry;
use zzop_engine::register_all_native;

use crate::collect_rs_files;

/// `crates/core/src`, resolved relative to this crate's own manifest dir (same "sibling package"
/// pattern as `native_dir`/`dsl_dir`/`catalog_path` above).
fn core_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/src")
}

/// Every `crates/core/src/**/*.rs` file, recursively, EXCEPT the mechanism ROOTS
/// (`registry.rs`/`dsl.rs`) and the TEST files inside their module directories (`registry/`/
/// `dsl/` — both were split into directory modules under the 300-line ratchet, moving the
/// example-id-using unit tests into sibling test files the old filename match missed).
/// Deliberately narrow: the LOGIC submodules of those trees (`registry/config.rs`,
/// `dsl/eval.rs`, ...) stay checked — only illustrative test data is exempt, exactly the
/// rationale `kernel_core_carries_no_native_analysis_id_string_literal`'s doc gives.
fn core_rs_files_excluding_mechanism_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(&core_src_dir(), &mut out);
    out.retain(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if matches!(name, "registry.rs" | "dsl.rs") {
            return false;
        }
        let in_mechanism_dir = p
            .components()
            .any(|c| matches!(c.as_os_str().to_str(), Some("registry") | Some("dsl")));
        let is_test_file = name.starts_with("test") || name.ends_with("_tests.rs");
        !(in_mechanism_dir && is_test_file)
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
