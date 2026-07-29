//! Arbitrary input must not panic the TypeScript frontend. Why that matters even though
//! `crates/engine/src/pipeline/parsers.rs` catches the panic, what the generated inputs look like, and
//! what is deliberately out of scope: see `parser/tests/input_strategy.rs`'s module doc — the
//! single owner of that rationale for all eight frontends.
//!
//! Entry points hammered: every `pub` function `lib.rs` re-exports that takes source text, plus the
//! four that take a text-derived structure (`build_dep*`, `write_sites_for_symbol*`) fed from this
//! crate's own output for the same input, so the deeper functions see shapes only a real parse
//! produces. That is 40-odd swc parses per case, by far the most expensive of the eight properties.
//!
//! A second property fuzzes the DECLARED vocabulary rather than the source: `vocabulary.
//! ormReceiverPattern` reaches `Regex::new` from user config, so an arbitrary string has to be a
//! non-panicking regex input as well as a non-panicking source input.
//!
//! Case count is chosen against a measurement, not a feeling — see the comment on `CASES`.

#[path = "../../tests/input_strategy.rs"]
mod input_strategy;

use std::collections::{BTreeMap, HashMap, HashSet};

use proptest::prelude::*;
use zzop_parser_typescript as ts;

/// 1024 cases, measured at 0.57 s for all three properties in this file. swc re-parses once per entry
/// point, so a case is ~40 parses — yet still only ~0.5 ms, because swc rejects malformed input long
/// before it builds a module. How the per-crate budget is set: `input_strategy`'s "Case budget" section.
const CASES: u32 = 1024;

/// One call per public text-taking entry point. A panic in any of them fails the property.
fn hammer(rel: &str, text: &str) {
    let files = vec![(rel.to_string(), text.to_string())];

    // Whole-file projections.
    let _ = ts::parse_ok(rel, text);
    let _ = ts::count_loc(text);
    let _ = ts::build_common_ir("src", &files);
    let symbols = ts::parse_symbols(rel, text);
    let _ = ts::parse_symbols_with_vocab(rel, text, &ts::WriteSiteVocab::built_in());
    let imports = ts::parse_imports(rel, text);
    let re_exports = ts::parse_re_exports(rel, text);
    let dynamic_imports = ts::parse_dynamic_imports(rel, text);
    let _ = ts::parse_calls(rel, text);
    let _ = ts::parse_local_identifier_refs(rel, text);
    let _ = ts::parse_asset_refs(rel, text);
    let _ = ts::parse_dead_export_facts(rel, text);
    let _ = ts::parse_exported_signature_names(rel, text);
    let _ = ts::extract_function_spans(rel, text);
    let _ = ts::extract_loop_spans(rel, text);
    let _ = ts::extract_sfc_script_imports(rel, text);

    // Adapters — the extractors that only fire once a framework marker has been recognized.
    let _ = ts::extract_class_shape_fragments(rel, text);
    let _ = ts::extract_client_base_prefix_marker(rel, text);
    let _ = ts::extract_generated_client_base_prefix_marker(rel, text);
    let _ = ts::extract_controller_provides(rel, text);
    let _ = ts::extract_controller_prefix_route_fragments(rel, text);
    let _ = ts::extract_controller_guarded_lines(rel, text);
    let _ = ts::extract_db_table_consumes(rel, text);
    let _ = ts::extract_db_table_consumes_with_vocab(rel, text, Some(ts::PRISMA_CLIENT_GETTER));
    let _ = ts::extract_query_call_sites(rel, text);
    let _ = ts::extract_query_call_sites_with_vocab(rel, text, None);
    let _ = ts::extract_entity_db_table_provides(rel, text);
    let _ = ts::extract_global_prefix_marker(rel, text);
    let _ = ts::extract_hono_client_consumes(rel, text);
    let _ = ts::extract_nest_forroutes_guarded(rel, text);
    let _ = ts::scan_pages_api_handler(rel, text);
    let _ = ts::extract_pathname_dispatch_provides(rel, text);
    let _ = ts::extract_raw_sql_db_table_consumes(rel, text);
    let _ = ts::extract_router_mount_fragments(rel, text, &["router", "app"]);
    let _ = ts::extract_router_mount_fragments_with_vocab(
        rel,
        text,
        &["router", "app"],
        &ts::RouterMountVocab::built_in(),
    );
    let _ = ts::extract_trpc_consumes(rel, text);
    let _ = ts::extract_procedure_router_fragments(rel, text);
    let _ = ts::extract_typeorm_repository_consumes(rel, text);
    let _ = ts::extract_wrapper_fragments(rel, text);
    let _ = ts::extract_http_egress(&files);
    let _ = ts::extract_http_egress_with_vocab(&files, &ts::RETRY_WRAPPERS);

    // Egress URL/const helpers: these take a fragment another extractor already pulled out, so the
    // arbitrary string is the realistic input, not a degenerate one.
    let consts = ts::const_map_fragment(rel, text);
    let _ = ts::resolve_raw_path(text, &consts);
    let _ = ts::is_external_url(text);
    let _ = ts::base_relative_path(text);

    // Resolution: fed this file's own import map, so the specifiers are whatever the input declared.
    let all_paths: HashSet<String> = files.iter().map(|(p, _)| p.clone()).collect();
    let _ = ts::resolve_file(text, rel, &all_paths);
    let _ = ts::try_ext(text, &all_paths);
    let workspace_pkgs: HashMap<String, ts::WorkspacePkg> = HashMap::new();
    let tsconfigs: BTreeMap<String, ts::TsconfigPaths> = BTreeMap::new();
    let _ = ts::resolve_file_with_workspace(text, rel, &all_paths, &workspace_pkgs, &tsconfigs);
    let dep_files = vec![(rel.to_string(), imports)];
    let dep_re_exports = vec![(rel.to_string(), re_exports)];
    let dep_dynamic = vec![(rel.to_string(), dynamic_imports)];
    let _ = ts::build_dep(&dep_files, &dep_re_exports, &dep_dynamic, &all_paths);
    let _ = ts::build_dep_with_workspace(
        &dep_files,
        &dep_re_exports,
        &dep_dynamic,
        &all_paths,
        &workspace_pkgs,
        &tsconfigs,
    );

    // Write sites are per-symbol; bounded so a symbol-dense input cannot dominate the case's cost.
    let compiled = ts::CompiledWriteSiteVocab::compile(&ts::WriteSiteVocab::built_in());
    for sym in symbols.iter().take(8) {
        let _ = ts::write_sites_for_symbol(sym, text);
        let _ = ts::write_sites_for_symbol_with_vocab(sym, text, &compiled);
    }
}

proptest! {
    #![proptest_config(input_strategy::config(CASES))]

    #[test]
    fn no_public_entry_point_panics(text in input_strategy::source_text()) {
        hammer("src/app.ts", &text);
    }

    /// `vocabulary.ormReceiverPattern` / `vocabulary.ormWriteMethods` are declared in the user's config
    /// and arrive here as raw strings — the pattern reaches `Regex::new`. An unparseable pattern must
    /// degrade, not panic.
    #[test]
    fn no_declared_write_site_vocabulary_panics(
        pattern in input_strategy::source_text(),
        method in input_strategy::source_text(),
    ) {
        let methods = [method.as_str()];
        let vocab = ts::WriteSiteVocab {
            orm_receiver_pattern: Some(pattern.as_str()),
            write_methods: &methods,
        };
        let source = "export const repo = { async save(x) { return db.user.create(x); } };";
        let symbols = ts::parse_symbols_with_vocab("src/app.ts", source, &vocab);
        let compiled = ts::CompiledWriteSiteVocab::compile(&vocab);
        for sym in symbols.iter().take(8) {
            let _ = ts::write_sites_for_symbol_with_vocab(sym, source, &compiled);
        }
    }
}

#[test]
fn no_public_entry_point_panics_on_fixed_edge_cases() {
    for text in input_strategy::FIXED_EDGE_CASES {
        hammer("src/app.ts", text);
    }
}
