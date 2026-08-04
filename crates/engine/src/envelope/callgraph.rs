//! Mode A's call-graph pass — the envelope-mode consumer of `FileProjection::calls`, the channel that
//! lets an external producer with NO native call-graph parser turn on the call-graph-BFS rule family
//! (`mutating-route-no-auth`, `unsafe-read-endpoint`, `non-idempotent-write`) for its language.
//!
//! ## Relationship to the native pass (`analyze::native_rules::callgraph`)
//! The native pass is a second, uncached DISK re-parse (TS/Java/Python/Rust) because `FileArtifact`
//! carries no `RawCall`s. An envelope has no disk to re-parse — its `calls` channel IS the projection,
//! already validated by `zzop_core::validate_envelope` (attribution + version floor). So this pass
//! builds the same `zzop_core::callgraph::SymbolGraph` from the envelope's own facts: each file's
//! `calls` resolved against that file's own `imports` and the tree's symbol set, cross-file specifiers
//! going through [`super::resolve::resolve_envelope_specifier`] — the same exact/`./`-relative contract
//! every other envelope cross-file reference uses. An unresolvable callee's edge is dropped, never
//! guessed — identical to the native resolver's own contract (`resolve_calls_for_file`'s doc).
//!
//! ## Deviations from the native pass (documented, not bugs)
//! - **No `decorator_guarded` producers** — decorators/annotations are source-text facts an envelope
//!   does not carry. The envelope-native way to express "this route is guarded by metadata the graph
//!   can't see" is the generic attribute channel (`attributes` with key `auth-guarded`), which this
//!   pass passes through (`route_attr_store`) exactly like the native rule wiring does.
//! - **No `file_texts`** — the two scanners' `// idempotent-ok:` marker lookback reads source lines,
//!   which an envelope does not carry, so the suppression window is honestly inert here (an envelope
//!   producer has no comment to write; disclosure-only degrade, never a lost finding).
//! - **No `cache-lane-file-read`** — that rule is not a consumer of this channel: its trigger is a
//!   config-declared anchor vocabulary over native `RawCall` receiver idioms, and its findings anchor
//!   on symbols' file-read call sites the native re-parse recognizes lexically. Wiring it here without
//!   a measured envelope case would be a speculative surface (the same bar the projection contract
//!   applies to fields).
//!
//! ## Disclosure (recall-direction degrade — absence must be named)
//! An empty channel means these rules are SILENT for the envelope's language — they do not report
//! clean, they do not look. [`absence_warning`] names that state (and the channel that opens it)
//! whenever the envelope carries http routes; [`uncovered_extension_warning`] names the residual gate
//! for routes whose file extension sits outside `CALL_GRAPH_COVERED_EXTENSIONS` even when calls were
//! supplied — `mutating-route-no-auth`'s own candidate filter exempts those routes today (the covered
//! set is read from the rules crate's constant, never copied).

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use zzop_core::{is_enabled, Finding, ImportMap};

use crate::analyze::record_native_timing;
use crate::EngineConfig;

use super::resolve::resolve_envelope_specifier;

mod disclosure;

use disclosure::{absence_warning, dropped_calls_warning, uncovered_extension_warning};

/// Runs the call-graph-BFS rules over the envelope's own `calls` channel, extending `global_findings`
/// and `warnings` in place. See the module doc for the exact deviations from the native pass.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_envelope_callgraph(
    files: &[&zzop_core::FileProjection],
    all_paths: &HashSet<&str>,
    all_symbols: &[zzop_core::SourceSymbol],
    io_provides: &[zzop_core::IoProvide],
    attribute_store: &zzop_core::AttributeStore,
    config: &EngineConfig,
    profile: bool,
    rule_time: &mut HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
    warnings: &mut Vec<String>,
) {
    let run_unsafe_read = is_enabled(&config.rule_config, "unsafe-read-endpoint");
    let run_non_idempotent = is_enabled(&config.rule_config, "non-idempotent-write");
    let run_mutating_no_auth = is_enabled(&config.rule_config, "mutating-route-no-auth");
    if !run_unsafe_read && !run_non_idempotent && !run_mutating_no_auth {
        return; // every consumer disabled by config — silence is chosen, not undisclosed
    }

    let raw_calls: Vec<zzop_core::callgraph::RawCall> =
        files.iter().flat_map(|f| f.calls.iter().cloned()).collect();

    if raw_calls.is_empty() {
        if let Some(w) = absence_warning(io_provides) {
            warnings.push(w);
        }
        return;
    }
    if let Some(w) = uncovered_extension_warning(io_provides) {
        warnings.push(w);
    }

    // Same substrate the native pass hands `build_symbol_graph`, from envelope facts instead of a
    // re-parse: per-file `ImportMap`s and the declared symbol names per file.
    let imports_by_file: HashMap<String, ImportMap> = files
        .iter()
        .filter(|f| !f.imports.is_empty())
        .map(|f| (f.path.clone(), f.imports.clone()))
        .collect();
    let mut local_symbols_by_file: HashMap<String, HashSet<String>> = HashMap::new();
    for s in all_symbols {
        local_symbols_by_file
            .entry(s.file.clone())
            .or_default()
            .insert(s.name.clone());
    }
    let resolve_file_fn = |specifier: &str, from_file: &str| {
        resolve_envelope_specifier(specifier, from_file, all_paths)
    };
    let symbol_graph = zzop_core::callgraph::build_symbol_graph(
        &raw_calls,
        &imports_by_file,
        &local_symbols_by_file,
        &resolve_file_fn,
    );
    // The resolver DROPS an unresolvable edge (never guesses — its contract); this pass's added duty
    // is to say so. Without it, a channel whose every edge evaporated ran the rules over an empty
    // graph with the exact same (silent) output shape as a fully-resolved one.
    if let Some(w) = dropped_calls_warning(files, &symbol_graph) {
        warnings.push(w);
    }

    // Same reconstruction the native pass documents: endpoints from the already-collected `http`
    // provides, displayed keys therefore in normalized form.
    let api_endpoints: Vec<zzop_core::ApiEndpoint> = io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .filter_map(|p| {
            let (method, path) = p.key.split_once(' ')?;
            Some(zzop_core::ApiEndpoint {
                method: method.to_string(),
                path: path.to_string(),
                handler: p.symbol.clone().unwrap_or_default(),
            })
        })
        .collect();

    // No source text in an envelope -> empty text map; the `idempotent-ok` lookback is honestly
    // inert (module doc). Write-site evidence itself is NOT text-dependent — it rides
    // `SourceSymbol::write_sites`, which a producer may populate on the wire.
    let file_texts: HashMap<String, String> = HashMap::new();
    if run_unsafe_read {
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::scan_unsafe_read_endpoint(
            &zzop_rules_http::ScanUnsafeReadEndpointInput {
                api_endpoints: &api_endpoints,
                symbols: all_symbols,
                symbol_graph: &symbol_graph,
                files: &file_texts,
            },
        );
        record_native_timing(rule_time, t0, "unsafe-read-endpoint", found.len());
        global_findings.extend(found);
    }
    if run_non_idempotent {
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::scan_non_idempotent_write(
            &zzop_rules_http::ScanNonIdempotentWriteInput {
                api_endpoints: &api_endpoints,
                symbols: all_symbols,
                symbol_graph: &symbol_graph,
                files: &file_texts,
            },
        );
        record_native_timing(rule_time, t0, "non-idempotent-write", found.len());
        global_findings.extend(found);
    }
    if run_mutating_no_auth {
        // Same vocabulary resolution discipline as the native pass: config-declared, and an
        // undeclared key makes no judgment (the per-key built-in fallback was removed 2026-07-27).
        // What `config.vocabulary` holds here is the facade's explicit assignment — the request's
        // declared vocabulary whole, or the product default (`VocabularyConfig::built_in()`) when
        // the request declared none (`zzop_facade::analyze_envelope_json`, the envelope lane's one
        // config front-end).
        let vocab = config.vocabulary.resolve();
        // No decorator-guard producers in envelope mode (module doc) — injected `auth-guarded`
        // attributes are the envelope-native equivalent, composed via `route_attr_store`.
        let decorator_guarded: std::collections::HashSet<(String, u32)> = Default::default();
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::scan_mutating_route_no_auth(
            &zzop_rules_http::ScanMutatingRouteNoAuthInput {
                io_provides,
                symbols: all_symbols,
                symbol_graph: &symbol_graph,
                auth_guard_pattern: vocab.auth_guard_pattern,
                qualifier_guard_tokens: &vocab.auth_guard_qualifier_tokens,
                auth_acquisition_standalone_pattern: vocab.auth_acquisition_standalone_pattern,
                auth_acquisition_conditional_pattern: vocab.auth_acquisition_conditional_pattern,
                auth_family_path_pattern: vocab.auth_family_path_pattern,
                decorator_guarded: &decorator_guarded,
                route_attr_store: attribute_store,
            },
        );
        record_native_timing(rule_time, t0, "mutating-route-no-auth", found.len());
        global_findings.extend(found);
    }
}
