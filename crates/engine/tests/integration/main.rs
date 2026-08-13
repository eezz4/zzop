//! The engine's integration-test harness — 73 former standalone `tests/*.rs` binaries folded into
//! ONE test target (2026-08-09). What was bought: one link instead of 73 (the workspace's test-binary
//! population was the dominant disk/link cost measured 2026-08-08), at the price of three properties
//! the standalone form gave for free, each of which is handled rather than ignored:
//!
//! - **Wiring silence.** A module file with no `mod` line below compiles never, warns never, and its
//!   tests run never. Guarded by rule_contracts contract 14 (`REGISTERED_FLAT_TEST_DIRS` — this
//!   directory is registered there with a floor), not by hope.
//! - **Process isolation.** Tests here share one process on parallel threads. The two tests that
//!   assert equalities over PROCESS-WIDE counters (`git_spawn_census.rs`, `analyze_parse_census.rs`)
//!   therefore stay OUT, as top-level standalone binaries — rule_contracts contract 20 pins both the
//!   exclusion and the strict rule that they are the ONLY top-level `.rs` files left. Audited before
//!   the fold: no other file mutates env vars or the working directory.
//! - **TempDir name isolation.** Every module carries its own `TempDir` copy whose uniquifying
//!   counter is a per-module static; `std::process::id()` no longer separates two modules using the
//!   same prefix. Audited before the fold: the one cross-module prefix collision was renamed in the
//!   guard commit, and same-module repeats share a counter and are safe.
//!
//! Selection changed shape: `cargo test -p zzop-engine --test analyze_cache` is now
//! `cargo test -p zzop-engine --test integration analyze_cache::`. Note the trailing `::` — a bare
//! substring filter that matches nothing exits 0 (an empty run reads as clean), so spell filters
//! wide enough to know a typo from a pass.
mod analyze_adapter_overlay;
mod analyze_asset_ref;
mod analyze_attribute_injection;
mod analyze_axios_base_prefix_e2e;
mod analyze_be_framework_coverage_warning;
mod analyze_cache;
mod analyze_cache_lane_file_read;
mod analyze_callgraph;
mod analyze_callgraph_rust;
mod analyze_channels;
mod analyze_controller_prefix_ref;
mod analyze_coverage_census;
mod analyze_cross_layer_exclude;
mod analyze_cross_layer_findings;
mod analyze_cross_layer_retry_write;
mod analyze_cross_layer_severity_override;
mod analyze_cross_layer_test_io_filter;
mod analyze_cross_layer_vocabulary;
mod analyze_cross_layer_wildcard_route;
mod analyze_cross_tree_import_disclosure;
mod analyze_csharp_cross_layer;
mod analyze_csharp_module;
mod analyze_dead_exports;
mod analyze_diagnostics;
mod analyze_envelope;
mod analyze_exclude_score_channels;
mod analyze_git;
mod analyze_git_tree_coordinates;
mod analyze_gitignore;
mod analyze_go_cross_layer;
mod analyze_go_module;
mod analyze_http_pack_io_scan;
mod analyze_io;
mod analyze_io_java;
mod analyze_io_java_project;
mod analyze_io_natives;
mod analyze_io_scan_tree;
mod analyze_java_cross_layer;
mod analyze_java_imports;
mod analyze_java_imports_overlay;
mod analyze_loop_spans_languages;
mod analyze_minified;
mod analyze_multi_tree;
mod analyze_multi_tree_java;
mod analyze_multi_tree_nestjs;
mod analyze_native_middleware;
mod analyze_no_applicable_dsl_rules;
mod analyze_override_displacement;
mod analyze_prisma_db_table_provide;
mod analyze_profiling;
mod analyze_python_cross_layer;
mod analyze_python_package_roots;
mod analyze_response_shape;
mod analyze_routes_hono;
mod analyze_routes_pathname_dispatch;
mod analyze_rule_admission;
mod analyze_rule_config;
mod analyze_rust_cross_layer;
mod analyze_rust_self;
mod analyze_rust_test_spans;
mod analyze_schema_natives;
mod analyze_self_output_exclusion;
mod analyze_sfc_imports;
mod analyze_sql_db_table;
mod analyze_test_path_vocabulary;
mod analyze_topology_config;
mod analyze_tsconfig_paths;
mod analyze_uncompilable_rule;
mod analyze_uncovered_language;
mod analyze_unparsed_extensions;
mod analyze_vocabulary_config;
mod analyze_workspace_alias;
mod analyze_zero_scope_packs;
mod cache_vocabulary_invalidation;
mod db_table_key_cross_crate_consistency;
mod pack_prisma_schema;
mod policy_value_pins;
