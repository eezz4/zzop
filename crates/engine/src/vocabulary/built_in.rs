//! [`VocabularyConfig::built_in`] — zzop's own suggested value for every convention vocabulary,
//! split from `vocabulary.rs` purely for the repo per-file line cap (the parent module doc owns the
//! contract prose; every entry here names the constant its consumer already owns — no second copy).

use super::{FeatureSlicedDesignVocab, VocabularyConfig, DEFAULT_JAVA_SOURCE_ROOT};
impl VocabularyConfig {
    /// zzop's own suggested value for every convention vocabulary, each one naming the constant its
    /// consumer already owns — no second copy anywhere in the workspace.
    ///
    /// This is NOT what an undeclared run uses. Since 2026-07-27 an undeclared vocabulary makes no
    /// judgment at all ([`resolved`]), and these values reach a run only by being written into the user's
    /// own `zzop.config.jsonc` — `crates/config`'s starter template is generated from exactly this
    /// struct, and `template_tests` pins the two together. So the suggestions are a document the user
    /// owns and edits, never an assumption the engine makes behind them.
    pub fn built_in() -> Self {
        fn owned(v: &[&str]) -> Vec<String> {
            v.iter().map(|s| (*s).to_string()).collect()
        }
        let router = zzop_parser_typescript::RouterMountVocab::built_in();
        let write = zzop_parser_typescript::WriteSiteVocab::built_in();
        let python = zzop_parser_python_3::PythonGuardVocab::built_in();
        VocabularyConfig {
            auth_guard_pattern: Some(zzop_rules_http::DEFAULT_AUTH_GUARD_PATTERN.to_string()),
            auth_guard_qualifier_tokens: owned(zzop_rules_http::QUALIFIER_GUARD_TOKENS),
            auth_acquisition_standalone_pattern: Some(
                zzop_rules_http::AUTH_ACQUISITION_STANDALONE_PATTERN.to_string(),
            ),
            auth_acquisition_conditional_pattern: Some(
                zzop_rules_http::AUTH_ACQUISITION_CONDITIONAL_PATTERN.to_string(),
            ),
            auth_family_path_pattern: Some(zzop_rules_http::AUTH_FAMILY_PATH_PATTERN.to_string()),
            api_segment_pattern: Some(zzop_rules_http::API_SEGMENT_PATTERN.to_string()),
            java_source_root: Some(DEFAULT_JAVA_SOURCE_ROOT.to_string()),
            // Empty on purpose — see the field's own doc: there is no honest default for where a
            // project's Python packages live beyond the two roots the resolver already tries as facts.
            python_package_roots: Vec::new(),
            skip_dirs: owned(crate::dispatch::DEFAULT_SKIP_DIRS),
            // Empty on purpose, and NOT for the reason `python_package_roots` above is empty. That one
            // has no honest default; this one's defaults are real, and they live in the shared
            // `${test-paths}` fragment (`zzop_core::dsl::test_path_re`), which applies whether or not
            // the author writes anything. An entry here is a project's own EXTRA spelling — see the
            // field doc for why this key is the struct's one additive member.
            extra_test_path_patterns: Vec::new(),
            prisma_client_getter: Some(zzop_parser_typescript::PRISMA_CLIENT_GETTER.to_string()),
            retry_wrappers: owned(&zzop_parser_typescript::RETRY_WRAPPERS),
            middleware_guard_callees: owned(router.middleware_guard_callees),
            router_name_veto_suffixes: owned(router.router_name_veto_suffixes),
            wrapper_guard_prefixes: owned(router.wrapper_guard_prefixes),
            env_axis_veto_substrings: owned(router.env_axis_veto_substrings),
            idempotency_header_names: owned(router.idempotency_header_names),
            orm_receiver_pattern: write.orm_receiver_pattern.map(str::to_string),
            orm_write_methods: owned(write.write_methods),
            money_tokens: owned(zzop_rules_schema::MONEY_TOKENS),
            fetch_wrapper_export_names: owned(crate::framework_silence::WRAPPER_EXPORT_NAMES),
            generated_file_markers: owned(crate::generated_banner::MARKERS),
            python_guard_substrings: owned(python.substrings),
            python_guard_anonymous_veto_substrings: owned(python.anonymous_veto_substrings),
            python_guard_report_veto_prefixes: owned(python.report_veto_prefixes),
            python_guard_report_veto_suffixes: owned(python.report_veto_suffixes),
            rust_optional_extractor_prefixes: owned(
                zzop_parser_rust::RUST_OPTIONAL_EXTRACTOR_PREFIXES,
            ),
            // No built-in anchor on purpose — see the field's own doc. `zzop init` writes the key with a
            // comment telling the author what to put there, which is the honest form of "we cannot know
            // this": the key is present and visible, and it judges nothing until they answer.
            cache_lane_anchor_pattern: None,
            file_read_callees: owned(zzop_rules_graph::DEFAULT_FILE_READ_CALLEES),
            secret_param_names: owned(zzop_rules_cross_layer::SECRET_PARAM_NAMES),
            sensitive_response_field_substrings: owned(
                zzop_rules_cross_layer::SENSITIVE_RESPONSE_FIELD_SUBSTRINGS,
            ),
            sensitive_response_field_exact_names: owned(
                zzop_rules_cross_layer::SENSITIVE_RESPONSE_FIELD_EXACT,
            ),
            sensitive_response_field_suffixes: owned(
                zzop_rules_cross_layer::SENSITIVE_RESPONSE_FIELD_SUFFIXES,
            ),
            api_version_segment_pattern: Some(
                zzop_rules_cross_layer::VERSION_SEGMENT_PATTERN.to_string(),
            ),
            externally_fetched_paths: owned(zzop_rules_cross_layer::EXTERNALLY_FETCHED_PATHS),
            schema_usage_skip_fields: owned(zzop_rules_schema::SKIP_FIELD_NAMES),
            router_names: owned(crate::io::DEFAULT_ROUTER_NAMES),
            hierarchy_shared_dirs: owned(zzop_metrics::DEFAULT_HIERARCHY_SHARED_DIRS),
            feature_sliced_design: FeatureSlicedDesignVocab {
                slice_containers: owned(zzop_metrics::DEFAULT_FSD_SLICE_CONTAINERS),
                entry: owned(zzop_metrics::DEFAULT_FSD_ENTRY),
                shared: owned(zzop_metrics::DEFAULT_FSD_SHARED),
                base_dirs: owned(zzop_metrics::DEFAULT_FSD_BASE_DIRS),
            },
        }
    }
}
