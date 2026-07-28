//! [`ResolvedVocabulary`] — a [`super::VocabularyConfig`] read the one way a run is allowed to read it.
//!
//! One file, one job: the "a declared, non-empty value is the whole vocabulary; anything else means the
//! judgment is not made" rule of the parent module doc is written HERE and nowhere else.
//!
//! There is no built-in arm left in this file. Until 2026-07-27 every field fell back to the constant its
//! consumer owned, which meant an undeclared key made the engine GUESS what the project calls its own
//! things; the fallbacks are gone and an undeclared key now yields `None`/`[]`, which every consumer
//! reads as "this match never succeeds". The built-in values still exist — as
//! [`super::VocabularyConfig::built_in`], which is what `zzop init` writes into the user's file, so the
//! defaults reach a run by being DECLARED rather than by being assumed.
//!
//! Borrowed (`&str`, `Vec<&str>`) rather than owned, and built ONCE per tree (`pipeline::run_file_pass`)
//! rather than per file — the per-file lanes take `&ResolvedVocabulary`.

use super::VocabularyConfig;

/// A declared scalar: `Some` only when the author wrote a non-empty string. An empty string is treated as
/// "not declared" rather than taken literally, for the safety reason in the parent module doc (an empty
/// pattern is a regex that matches every name, so "I declared nothing here" must never be spellable as
/// "treat every name as a match").
fn declared(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|s| !s.is_empty())
}

/// A declared list, borrowed as `&str`s. An empty declaration stays empty: no entry means no name can
/// match, which is this vocabulary's judgment not being made.
fn declared_list(declared: &[String]) -> Vec<&str> {
    declared.iter().map(String::as_str).collect()
}

/// [`VocabularyConfig`] with the "declared or not made" rule applied — what a rule/pass actually reads.
///
/// Every scalar is an `Option`: `None` is not "use the default", it is "do not make this judgment". Every
/// list is possibly empty, with the same meaning. Consumers must never substitute a constant for a `None`
/// — that would put a second copy of the default back in the tree, which is the defect this shape exists
/// to remove.
///
/// The parser crates' entry points take their own grouped vocabulary types (`RouterMountVocab`,
/// `WriteSiteVocab`, `PythonGuardVocab`), whose fields are borrowed SLICES held for the length of a parse.
/// This struct owns the `Vec`s those slices point into and hands the grouped view out through the
/// accessors below, so nothing is leaked and no call site re-groups the same values.
pub(crate) struct ResolvedVocabulary<'a> {
    pub(crate) auth_guard_pattern: Option<&'a str>,
    pub(crate) auth_guard_qualifier_tokens: Vec<&'a str>,
    pub(crate) auth_acquisition_standalone_pattern: Option<&'a str>,
    pub(crate) auth_acquisition_conditional_pattern: Option<&'a str>,
    pub(crate) auth_family_path_pattern: Option<&'a str>,
    pub(crate) api_segment_pattern: Option<&'a str>,
    pub(crate) java_source_root: Option<&'a str>,
    pub(crate) prisma_client_getter: Option<&'a str>,
    pub(crate) retry_wrappers: Vec<&'a str>,
    pub(crate) middleware_guard_callees: Vec<&'a str>,
    pub(crate) router_name_veto_suffixes: Vec<&'a str>,
    pub(crate) wrapper_guard_prefixes: Vec<&'a str>,
    pub(crate) env_axis_veto_substrings: Vec<&'a str>,
    pub(crate) idempotency_header_names: Vec<&'a str>,
    pub(crate) orm_receiver_pattern: Option<&'a str>,
    pub(crate) orm_write_methods: Vec<&'a str>,
    pub(crate) money_tokens: Vec<&'a str>,
    pub(crate) fetch_wrapper_export_names: Vec<&'a str>,
    pub(crate) generated_file_markers: Vec<&'a str>,
    pub(crate) python_guard_substrings: Vec<&'a str>,
    pub(crate) python_guard_anonymous_veto_substrings: Vec<&'a str>,
    pub(crate) python_guard_report_veto_prefixes: Vec<&'a str>,
    pub(crate) python_guard_report_veto_suffixes: Vec<&'a str>,
    pub(crate) rust_optional_extractor_prefixes: Vec<&'a str>,
    pub(crate) cache_lane_anchor_pattern: Option<&'a str>,
    pub(crate) file_read_callees: Vec<&'a str>,
    pub(crate) secret_param_names: Vec<&'a str>,
    pub(crate) api_version_segment_pattern: Option<&'a str>,
    pub(crate) externally_fetched_paths: Vec<&'a str>,
    pub(crate) schema_usage_skip_fields: Vec<&'a str>,
}

impl ResolvedVocabulary<'_> {
    /// The router-mount recognizer's five lists, in the shape its entry point takes.
    pub(crate) fn router_mounts(&self) -> zzop_parser_typescript::RouterMountVocab<'_> {
        zzop_parser_typescript::RouterMountVocab {
            middleware_guard_callees: &self.middleware_guard_callees,
            router_name_veto_suffixes: &self.router_name_veto_suffixes,
            wrapper_guard_prefixes: &self.wrapper_guard_prefixes,
            env_axis_veto_substrings: &self.env_axis_veto_substrings,
            idempotency_header_names: &self.idempotency_header_names,
        }
    }

    /// The write-site recognizer's receiver pattern + write-method list.
    pub(crate) fn write_site(&self) -> zzop_parser_typescript::WriteSiteVocab<'_> {
        zzop_parser_typescript::WriteSiteVocab {
            orm_receiver_pattern: self.orm_receiver_pattern,
            write_methods: &self.orm_write_methods,
        }
    }

    /// The Python guard-name accept list plus its three veto lists.
    pub(crate) fn python_guard(&self) -> zzop_parser_python_3::PythonGuardVocab<'_> {
        zzop_parser_python_3::PythonGuardVocab {
            substrings: &self.python_guard_substrings,
            anonymous_veto_substrings: &self.python_guard_anonymous_veto_substrings,
            report_veto_prefixes: &self.python_guard_report_veto_prefixes,
            report_veto_suffixes: &self.python_guard_report_veto_suffixes,
        }
    }

    /// The Rust extractor-guard producer's one veto list.
    pub(crate) fn rust_guard(&self) -> zzop_parser_rust::RustGuardVocab<'_> {
        zzop_parser_rust::RustGuardVocab {
            optional_extractor_prefixes: &self.rust_optional_extractor_prefixes,
        }
    }
}

impl VocabularyConfig {
    /// Reads every field the one legal way, applying the parent module doc's rule in ONE place: a
    /// declared, non-empty value is the whole vocabulary; anything else means the judgment is not made.
    pub(crate) fn resolve(&self) -> ResolvedVocabulary<'_> {
        ResolvedVocabulary {
            auth_guard_pattern: declared(&self.auth_guard_pattern),
            auth_guard_qualifier_tokens: declared_list(&self.auth_guard_qualifier_tokens),
            auth_acquisition_standalone_pattern: declared(
                &self.auth_acquisition_standalone_pattern,
            ),
            auth_acquisition_conditional_pattern: declared(
                &self.auth_acquisition_conditional_pattern,
            ),
            auth_family_path_pattern: declared(&self.auth_family_path_pattern),
            api_segment_pattern: declared(&self.api_segment_pattern),
            java_source_root: declared(&self.java_source_root),
            prisma_client_getter: declared(&self.prisma_client_getter),
            retry_wrappers: declared_list(&self.retry_wrappers),
            middleware_guard_callees: declared_list(&self.middleware_guard_callees),
            router_name_veto_suffixes: declared_list(&self.router_name_veto_suffixes),
            wrapper_guard_prefixes: declared_list(&self.wrapper_guard_prefixes),
            env_axis_veto_substrings: declared_list(&self.env_axis_veto_substrings),
            idempotency_header_names: declared_list(&self.idempotency_header_names),
            orm_receiver_pattern: declared(&self.orm_receiver_pattern),
            orm_write_methods: declared_list(&self.orm_write_methods),
            money_tokens: declared_list(&self.money_tokens),
            fetch_wrapper_export_names: declared_list(&self.fetch_wrapper_export_names),
            generated_file_markers: declared_list(&self.generated_file_markers),
            python_guard_substrings: declared_list(&self.python_guard_substrings),
            python_guard_anonymous_veto_substrings: declared_list(
                &self.python_guard_anonymous_veto_substrings,
            ),
            python_guard_report_veto_prefixes: declared_list(
                &self.python_guard_report_veto_prefixes,
            ),
            python_guard_report_veto_suffixes: declared_list(
                &self.python_guard_report_veto_suffixes,
            ),
            rust_optional_extractor_prefixes: declared_list(&self.rust_optional_extractor_prefixes),
            cache_lane_anchor_pattern: declared(&self.cache_lane_anchor_pattern),
            file_read_callees: declared_list(&self.file_read_callees),
            secret_param_names: declared_list(&self.secret_param_names),
            api_version_segment_pattern: declared(&self.api_version_segment_pattern),
            externally_fetched_paths: declared_list(&self.externally_fetched_paths),
            schema_usage_skip_fields: declared_list(&self.schema_usage_skip_fields),
        }
    }
}
