//! Per-artifact classification helpers `collect()`'s main accumulation loop calls once per artifact/
//! binding — split out purely to keep `collect()` under the line-count ratchet, no new decision logic
//! beyond what `collect()` itself used to run inline (see each function's own doc for the history this
//! carries forward).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::super::helpers::{
    is_csharp_source_ext, is_csharp_std_import, is_go_source_ext, is_go_std_import,
    is_java_source_ext, is_java_std_import, is_python_source_ext, is_rust_source_ext, rust_head,
    RUST_STD_CRATE_FAMILY,
};

/// "Bring an adapter" per-extension disclosure (`unparsed_extension_warning`)'s collection step: records
/// `rel`'s extension when `dispatch_lang` is `None` (no native parser frontend claims it) AND `rel` is
/// not already covered by a fact-carrying adapter overlay (`overlay_covered_paths`) AND the extension
/// isn't one of the deliberately-silent `NON_SOURCE_EXTENSIONS`.
///
/// Deliberately NOT gated on `!artifact.degraded`: a normal-sized dispatch-`None` file has `degraded:
/// false` (no adapter to run, but nothing failed either); an OVERSIZED file of the same unparsed
/// extension short-circuits to `degraded: true` before dispatch is even consulted for language selection
/// (`pipeline::compute_fresh_artifact`'s oversized branch), yet `dispatch_lang` is still `None` for it —
/// same "no native parser exists for this extension" fact, so it belongs in this count too (that file's
/// size is a SEPARATE, already-disclosed fact via `degraded`/`silent-truncation`, not a reason to hide
/// the extension gap). Extensionless files (README, Dockerfile) are deliberately excluded from v1:
/// ambiguous by construction — often config/docs, no reliable language signal to name. The per-extension
/// entry caps its sample `rel`s at 3 (during collection, not at emission, so a huge tree never holds more
/// than 3 rels per extension).
pub(super) fn record_unparsed_extension(
    rel: &str,
    dispatch_lang: Option<crate::dispatch::Language>,
    overlay_covered_paths: &HashSet<&str>,
    unparsed_extensions: &mut BTreeMap<String, (usize, Vec<String>)>,
) {
    if dispatch_lang.is_some() || overlay_covered_paths.contains(rel) {
        return;
    }
    let Some(ext) = std::path::Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
    else {
        return;
    };
    if crate::dispatch::is_non_source_extension(&ext) {
        return;
    }
    let entry = unparsed_extensions
        .entry(ext)
        .or_insert((0usize, Vec::new()));
    entry.0 += 1;
    if entry.1.len() < 3 {
        entry.1.push(rel.to_string());
    }
}

/// Package-import census staging for one non-relative import binding — the per-binding decision
/// `collect()`'s main loop used to make inline. A Python/Rust/Go binding is DEFERRED (pushed onto its own
/// language's F5 candidate list) rather than censused immediately, since telling an in-tree specifier
/// apart from a genuinely external package needs state (`ts_paths`/`rust_workspace`/`go_modules`) not
/// final until the whole accumulation loop finishes — see `super::census`'s own doc for the drain step
/// this staging feeds. Every other language's package import is unaffected: censused immediately, same
/// as before this staging existed at all.
///
/// - Python: every non-relative specifier is staged, `original` translated to `None` for a plain
///   `import a.b.c` / star import (`"*"` never names a real submodule).
/// - Rust: `crate::`/`super::`/`self::` heads are never staged (module-path resolution happens
///   unconditionally in `super::dep_graph::merge_rust_dep_edges`, independent of this census), and
///   neither is a `RUST_STD_CRATE_FAMILY` head (never a real external dependency).
/// - Go: a standard-library import (`is_go_std_import` — Go's own dot-in-first-segment rule) is never
///   staged, same treatment as Rust's std family.
/// - Java: a JDK-namespace import (`is_java_std_import` — `java`/`javax` head, `helpers`'s own doc) is
///   never staged, same treatment as Rust's/Go's std families; `jakarta.*` IS staged (external framework
///   family, not a JDK namespace).
/// - C#: a BCL/framework `using` (`is_csharp_std_import` — `System`/`Microsoft` head, `helpers`'s own doc)
///   is never staged, same treatment as Java's `java`/`javax` family.
#[allow(clippy::too_many_arguments)]
pub(super) fn stage_package_import_candidate(
    specifier: &str,
    original: &str,
    rel: &str,
    is_python: bool,
    is_rust: bool,
    is_go: bool,
    is_java: bool,
    is_csharp: bool,
    python_candidates: &mut Vec<(String, Option<String>, String)>,
    rust_candidates: &mut Vec<(String, String)>,
    go_candidates: &mut Vec<(String, String)>,
    java_candidates: &mut Vec<(String, String)>,
    csharp_candidates: &mut Vec<(String, String)>,
    package_import_files: &mut BTreeMap<String, BTreeSet<String>>,
) {
    if specifier.starts_with('.') || specifier.starts_with('/') {
        return;
    }
    // Test-surface imports (`e2e/*.spec.ts`, `_test.go`, `Test*.java`, ...) are not deployed egress /
    // provide surface, so they must not feed the framework-silence census: a tree whose ONLY http-client
    // (S4), server-framework (S2), or ORM (S6) import lives in test code is not a dark app — the tripwire
    // would false-fire. Same "not deployed, so not real surface" reasoning the cross-layer join already
    // applies to test-classified io facts (`filter_join_io`, D11).
    if zzop_core::is_test_file(rel) {
        return;
    }
    if is_python {
        let orig = (original != "*").then(|| original.to_string());
        python_candidates.push((specifier.to_string(), orig, rel.to_string()));
    } else if is_rust {
        let head = rust_head(specifier);
        if !matches!(head, "crate" | "super" | "self") && !RUST_STD_CRATE_FAMILY.contains(&head) {
            rust_candidates.push((head.to_string(), rel.to_string()));
        }
    } else if is_go {
        if !is_go_std_import(specifier) {
            go_candidates.push((specifier.to_string(), rel.to_string()));
        }
    } else if is_java {
        if !is_java_std_import(specifier) {
            java_candidates.push((specifier.to_string(), rel.to_string()));
        }
    } else if is_csharp {
        if !is_csharp_std_import(specifier) {
            csharp_candidates.push((specifier.to_string(), rel.to_string()));
        }
    } else {
        package_import_files
            .entry(specifier.to_string())
            .or_default()
            .insert(rel.to_string());
    }
}

/// `rel`'s dispatched-language classification, precomputed once per artifact for
/// `stage_package_import_candidate`'s three language-gate booleans — avoids re-deriving the same
/// extension check per import binding (an artifact can carry many imports).
pub(super) struct LangGates {
    pub(super) is_python: bool,
    pub(super) is_rust: bool,
    pub(super) is_go: bool,
    pub(super) is_java: bool,
    pub(super) is_csharp: bool,
}

impl LangGates {
    pub(super) fn for_rel(rel: &str) -> Self {
        Self {
            is_python: is_python_source_ext(rel),
            is_rust: is_rust_source_ext(rel),
            is_go: is_go_source_ext(rel),
            is_java: is_java_source_ext(rel),
            is_csharp: is_csharp_source_ext(rel),
        }
    }
}
