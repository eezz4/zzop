//! Convention vocabulary — the names a PROJECT picks, declared instead of guessed.
//!
//! A name vocabulary splits three ways (the policy census's own axis taxonomy). A **fact** is fixed by a
//! framework or a language (`@GetMapping`, `router.post`): nobody can rename it, so it stays built in. A
//! **cap/threshold** is a number, not a name, and is out of scope here. A **convention** is a name the
//! project itself chooses — what it calls its auth guards, which directories hold build output, where its
//! Java sources live. Holding a convention as a built-in literal means the engine GUESSES what a project
//! calls its own things and silently misclassifies every project that names them differently, which is the
//! failure mode `GitOptions::commit_subject_patterns` already refuses on the commit-subject axis.
//!
//! So each field here is one convention vocabulary, declarable from a `zzop.config.jsonc` under
//! `vocabulary.*` and carried into the engine on the request. Two contracts govern them:
//!
//! 1. **Per key, whole replacement — never an element-wise merge.** A declared key is the whole
//!    list/pattern (the same "one origin, no merge" rule `packs.extraDirs` and `git.commitTypePatterns`
//!    already state). There is no way to append one token to a list, deliberately: a merged vocabulary
//!    has two authors and neither can say what the effective set is.
//! 2. **Absent or empty means the judgment is NOT MADE** (2026-07-27; before that date it fell back to a
//!    built-in). An undeclared vocabulary matches nothing: no name is a guard, no banner marks a file
//!    generated, no path is externally fetched. It is emphatically not read literally — an empty guard
//!    pattern would be a regex matching EVERY name, so "declare nothing" is normalized to "match
//!    nothing" rather than taken at face value. Turning a whole rule off is still `rules: { "<id>": "off" }`.
//!
//! **[`VocabularyConfig::extra_test_path_patterns`] is the one documented exception to BOTH** (2026-08-10),
//! and the exception is principled rather than grandfathered — read its field doc before adding a second
//! one. Both contracts above are calibrated for a vocabulary whose absence costs an UNDER-report: no name
//! is a guard, so more routes report; no banner marks a file generated, so more exports report. Saying
//! less than we could is a cost the caller chose. Test paths run the other way. With no default, code the
//! language itself compiles only under `go test` is judged as shipping code, and every finding on it is a
//! WRONG CLAIM — not silence. A different failure direction earns a different policy, so that key carries
//! a built-in default nobody has to declare and a declaration can only ADD to it. The measurement that
//! forced this: a tree of nothing but `handler_test.go` / `test_login.py` / `UserTests.cs` produced 14
//! findings and 1 for the identical bytes under `tests/`.
//!
//! [`VocabularyConfig::built_in`] is where zzop's own suggested values live, and every entry in it names
//! the constant the consuming rule/pass already owned — no second copy. Those values reach a run by being
//! WRITTEN INTO THE USER'S FILE by `zzop init`, never by being assumed here: that is the whole difference
//! this module exists to make, and `zzop-config` no longer injects them into a request that did not ask.
//! A config is mandatory for every analysis lane precisely so this cannot leave a user silently blind.
//!
//! ## This struct is a cache key ingredient
//!
//! Several of these vocabularies reach the CACHED per-file lane: `ormReceiverPattern`/`ormWriteMethods`
//! decide `SourceSymbol::write_sites`, `prismaClientGetter` decides which `db-table` consumes exist, the
//! router-mount guard words decide a fragment's `auth-guarded` attribute, and `moneyTokens` decides a
//! cached Prisma finding. So `cache::vocabulary_fingerprint` hashes this WHOLE struct into both halves of
//! every `CacheKey` — see that function for why the whole struct rather than the per-lane subset. Two
//! consequences for anyone adding a field here: it is in the key automatically (the fingerprint is over
//! the serialization, not over a hand-listed tuple), and every field must serialize unconditionally —
//! a `#[serde(skip_serializing_if)]` would make two different vocabularies hash the same.

mod normalizers;
mod resolved;
mod test_paths;

use serde::{Deserialize, Serialize};

pub use normalizers::{normalizer_for, NormalizedKey, NORMALIZED_VOCABULARY_KEYS};
pub(crate) use resolved::ResolvedVocabulary;
pub(crate) use test_paths::extra_test_path_tail;

/// One run's declared convention vocabulary — the wire shape of the config file's `vocabulary` object
/// (camelCase) and the engine-side field on [`crate::EngineConfig`]. Every field is optional; see the
/// module doc for the replacement/fallback contract. Resolve it through [`VocabularyConfig::resolve`]
/// rather than reading fields directly, so the fallback rule lives in exactly one place.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VocabularyConfig {
    /// How this project spells its own auth-guard functions — matched against every symbol name the
    /// `mutating-route-no-auth` call-graph BFS visits (`zzop_rules_http::DEFAULT_AUTH_GUARD_PATTERN`).
    pub auth_guard_pattern: Option<String>,
    /// Camel tokens of this project's guard CLASS names — the qualifier half of the same rule's
    /// two-segment match (`zzop_rules_http::QUALIFIER_GUARD_TOKENS`).
    pub auth_guard_qualifier_tokens: Vec<String>,
    /// URL path segments that ARE the auth-acquisition surface on their own, so a mutating route there
    /// cannot require pre-existing auth (`zzop_rules_http::AUTH_ACQUISITION_STANDALONE_PATTERN`).
    pub auth_acquisition_standalone_pattern: Option<String>,
    /// URL path segments that mark auth acquisition only alongside an auth-family segment — `/auth/register`
    /// is exempt, `/devices/register` is not (`zzop_rules_http::AUTH_ACQUISITION_CONDITIONAL_PATTERN`).
    pub auth_acquisition_conditional_pattern: Option<String>,
    /// The auth-family segments the conditional tier above is judged against
    /// (`zzop_rules_http::AUTH_FAMILY_PATH_PATTERN`).
    pub auth_family_path_pattern: Option<String>,
    /// The URL segments this project uses to mark an API surface — `unprovided-consume`'s gate against
    /// flagging ordinary asset fetches (`zzop_rules_http::API_SEGMENT_PATTERN`).
    pub api_segment_pattern: Option<String>,
    /// The source-root segment a Spring security config's scope is cut at, so one module's posture never
    /// exempts a sibling module's routes. Defaults to the Maven/Gradle layout, which a project can relocate.
    pub java_source_root: Option<String>,
    /// Extra places this project's Python ABSOLUTE imports resolve from, on top of the two built-in
    /// ecosystem-fact roots (tree root and `src/`, which always apply). Each entry is either an extra
    /// root directory (`"backend"` — `app.api.main` also tries `backend/app/api/main.py`) or a
    /// package-name mapping (`"tml="` / `"tml=lib"` — the editable-install idiom where the import name
    /// points at a tree directory that does not carry it). Candidates only: the engine filters every
    /// one against the paths that actually exist, so a wrong declaration adds a failed lookup and can
    /// never invent an edge (`zzop_parser_python_3::python_import_candidates`). No built-in — which
    /// directories carry a project's packages is knowable only to that project, so `built_in()` ships
    /// it empty and an empty declaration is byte-for-byte the undeclared behavior.
    pub python_package_roots: Vec<String>,
    /// Directory names never walked while analyzing a tree — build output and tool state, both of which a
    /// project names itself. Carried into `DispatchConfig::skip_dirs` by the facade.
    pub skip_dirs: Vec<String>,
    /// EXTRA path regexes this project's test code lives at, ADDED to the language conventions zzop
    /// already knows (`zzop_core::dsl::test_path_re` — `_test.go`, `test_*.py`, `*Tests.cs`,
    /// `FooTest.java`, `tests/`, `*.spec.ts`, …). The ONE key in this struct that is additive rather
    /// than replacing, and the one whose built-in default applies even when the author declares nothing.
    /// Both exceptions are argued at
    /// [`RulePackDef::extend_test_path_exclusions`](zzop_core::RulePackDef::extend_test_path_exclusions)
    /// — the short form is that a test convention is fixed by the LANGUAGE rather than chosen by the
    /// project, so a missing default makes zzop judge test code as production, which is a wrong claim
    /// and not the polite under-report every other key here degrades to.
    ///
    /// Declare a project's own spellings: `["(^|/)it/", "(^|/)acceptance/"]`. Each entry is a regex over
    /// the tree-relative, forward-slash path; one that does not compile is dropped with a `warnings`
    /// entry naming it, never a panic and never a silently inert exclusion (the same contract
    /// `GitOptions::commit_type_patterns` states). `built_in()` ships it EMPTY, which is byte-for-byte
    /// the undeclared behavior: the built-ins live in the shared fragment, not here, so an empty list
    /// is not "no test paths" — it is "nothing beyond the ones every Go/Python/C#/Java/TS project
    /// already has".
    ///
    /// Reaches the rules through `pipeline::gate_pack_rules`, which rewrites the
    /// `file_exclude_pattern` of every rule that declined `${test-paths…}`. Per TREE by construction:
    /// each tree carries its own `EngineConfig` and its own packs, so one root's declaration can never
    /// reach another's.
    ///
    /// ONE bundled rule is out of its reach — `reliability/sync-fs-in-handler`, which excludes through a
    /// pack-LOCAL extension of the shared vocabulary rather than the shared vocabulary itself. It still
    /// carries every built-in language convention; it just will not learn a project's extra arm. The
    /// mechanism and why it cannot be closed cheaply are at
    /// `zzop_core::dsl::fragments::is_shared_test_path_vocabulary`.
    pub extra_test_path_patterns: Vec<String>,
    /// The zero-argument accessor this project calls to reach its ORM client — the anchor of the
    /// `getPrisma().<model>.<method>()` shape the `db-table` consume recognizer keys off
    /// (`zzop_parser_typescript::PRISMA_CLIENT_GETTER`). CACHED IR lane.
    pub prisma_client_getter: Option<String>,
    /// The identifiers this project's retry helpers are called, whose subtree's write egress is tagged
    /// `retry_configured` (`zzop_parser_typescript::adapters::egress::retry::RETRY_WRAPPERS`). CACHED IR lane.
    pub retry_wrappers: Vec<String>,
    /// Dotted callee names that are guard-certain in a route's middleware position, e.g.
    /// `passport.authenticate` (`RouterMountVocab::middleware_guard_callees`). CACHED IR lane.
    pub middleware_guard_callees: Vec<String>,
    /// Identifier tails that mean "sub-router / DI object", vetoing the guard judgment before it runs —
    /// `authController` is not a gate (`RouterMountVocab::router_name_veto_suffixes`). CACHED IR lane.
    pub router_name_veto_suffixes: Vec<String>,
    /// Rejection-verb prefixes accepted in the handler-WRAPPER position only (`requireAdmin(handler)`)
    /// (`RouterMountVocab::wrapper_guard_prefixes`). CACHED IR lane.
    pub wrapper_guard_prefixes: Vec<String>,
    /// Words that mark a check as being about the ENVIRONMENT rather than the caller, vetoing the wrapper
    /// judgment (`RouterMountVocab::env_axis_veto_substrings`). CACHED IR lane.
    pub env_axis_veto_substrings: Vec<String>,
    /// The header names this project accepts as idempotency-key evidence
    /// (`RouterMountVocab::idempotency_header_names`). CACHED IR lane.
    pub idempotency_header_names: Vec<String>,
    /// How this project names its data-access receivers — the pattern that decides whether
    /// `x.update(...)` is a write site (`zzop_parser_typescript::DEFAULT_ORM_RECEIVER_PATTERN`).
    /// CACHED IR lane.
    pub orm_receiver_pattern: Option<String>,
    /// The method names on such a receiver that count as a write
    /// (`zzop_parser_typescript::DEFAULT_WRITE_METHODS`). CACHED IR lane.
    pub orm_write_methods: Vec<String>,
    /// The field-name tokens that make a schema column "money", which `schema/float-money` judges
    /// against (`zzop_rules_schema::MONEY_TOKENS`). CACHED FINDINGS lane.
    pub money_tokens: Vec<String>,
    /// Exported names that make a module this project's own fetch WRAPPER, for the extraction-blindness
    /// census (`crate::framework_silence::WRAPPER_EXPORT_NAMES`).
    pub fetch_wrapper_export_names: Vec<String>,
    /// The banner markers this project's code generators write into their output, which exempt a file
    /// from dead-export reporting (`crate::generated_banner::MARKERS`).
    pub generated_file_markers: Vec<String>,
    /// Substrings that make a Python dependency callable an auth guard
    /// (`zzop_parser_python_3::PYTHON_GUARD_SUBSTRINGS`).
    pub python_guard_substrings: Vec<String>,
    /// Substrings marking a Python callable that RETURNS `None` for an anonymous caller instead of
    /// rejecting it, so it is not a gate (`zzop_parser_python_3::PYTHON_GUARD_ANONYMOUS_VETO_SUBSTRINGS`).
    pub python_guard_anonymous_veto_substrings: Vec<String>,
    /// Name prefixes marking a Python noun-form producer rather than a check
    /// (`zzop_parser_python_3::PYTHON_GUARD_REPORT_VETO_PREFIXES`).
    pub python_guard_report_veto_prefixes: Vec<String>,
    /// Name suffixes marking a Python noun-form producer rather than a check
    /// (`zzop_parser_python_3::PYTHON_GUARD_REPORT_VETO_SUFFIXES`).
    pub python_guard_report_veto_suffixes: Vec<String>,
    /// Name prefixes marking a Rust request extractor that ADMITS an anonymous caller (`MaybeAuthUser`),
    /// so its presence in a handler signature is not a gate
    /// (`zzop_parser_rust::RUST_OPTIONAL_EXTRACTOR_PREFIXES`).
    ///
    /// Undeclared means the veto is not applied — the same "declared or not judged" rule every key here
    /// follows. The consequence is worth stating because it runs the OTHER way from the usual one: for a
    /// veto, "not judged" is the permissive direction, so a project that declares `authGuardPattern` and
    /// leaves this empty lets an optional extractor clear a mutating route. `zzop init` writes both.
    pub rust_optional_extractor_prefixes: Vec<String>,
    /// How this project spells the entry point of a CACHED per-file lane, matched against a symbol's
    /// name — the anchor half of `cache-lane-file-read`. No built-in: which of a project's functions
    /// carry a closure promise is knowable only to that project, so there is nothing honest to default
    /// to and `built_in()` leaves it `None`. Undeclared means the rule makes no judgment.
    pub cache_lane_anchor_pattern: Option<String>,
    /// Callee names that count as reading the filesystem — the sink half of `cache-lane-file-read`.
    /// Ships a built-in (`zzop_rules_graph::DEFAULT_FILE_READ_CALLEES`) because these are Rust/Node
    /// standard-library spellings rather than names a project picks, unlike the anchor above.
    pub file_read_callees: Vec<String>,
    /// Query/path parameter names this project uses for secrets, which must never appear in a URL
    /// (`zzop_rules_cross_layer::SECRET_PARAM_NAMES`).
    pub secret_param_names: Vec<String>,
    /// Response DTO field-name SUBSTRINGS this project treats as sensitive — matched against
    /// lowercased, `_`/`-`-stripped declared response field names by
    /// `cross-layer/sensitive-response-field`
    /// (`zzop_rules_cross_layer::SENSITIVE_RESPONSE_FIELD_SUBSTRINGS`). Three sibling keys rather
    /// than one because the three matching axes answer different false-positive classes — see the
    /// consts' own docs; each axis follows the declared-or-not-judged rule independently.
    pub sensitive_response_field_substrings: Vec<String>,
    /// Response DTO field names that are sensitive only as the WHOLE normalized name (`token`, not
    /// `tokenCount`) (`zzop_rules_cross_layer::SENSITIVE_RESPONSE_FIELD_EXACT`).
    pub sensitive_response_field_exact_names: Vec<String>,
    /// Response DTO field-name SUFFIXES that are sensitive (`accessToken` ends in `token`)
    /// (`zzop_rules_cross_layer::SENSITIVE_RESPONSE_FIELD_SUFFIXES`).
    pub sensitive_response_field_suffixes: Vec<String>,
    /// How this project spells an API VERSION segment, stripped before cross-tree route keys are compared
    /// (`zzop_rules_cross_layer::VERSION_SEGMENT_PATTERN`).
    pub api_version_segment_pattern: Option<String>,
    /// Route path fragments this project serves to callers OUTSIDE the analyzed trees, so an unconsumed
    /// endpoint there is not dead (`zzop_rules_cross_layer::EXTERNALLY_FETCHED_PATHS`).
    pub externally_fetched_paths: Vec<String>,
    /// Schema field names exempt from the unreferenced-field usage check because this project treats them
    /// as boilerplate (`zzop_rules_schema::SKIP_FIELD_NAMES`).
    pub schema_usage_skip_fields: Vec<String>,
    /// The identifiers this project gives its own router values, which the router-mount recognizer treats
    /// as routers (`crate::io::DEFAULT_ROUTER_NAMES`). CACHED IR lane — it decides which call sites become
    /// route provides, so it rides the parser fingerprint.
    pub router_names: Vec<String>,
    /// Directory names this project treats as shared/cross-cutting infrastructure rather than a layer, so
    /// they are exempt from upward-import and sibling-cross violations
    /// (`zzop_metrics::DEFAULT_HIERARCHY_SHARED_DIRS`). A DIFFERENT axis from [`FeatureSlicedDesignVocab::shared`] despite
    /// the overlapping words — see that field.
    pub hierarchy_shared_dirs: Vec<String>,
    /// This project's Feature-Sliced Design layout. Nested rather than flattened into four sibling keys
    /// because the four answer ONE question ("what is your FSD layout?") and are meaningless apart: an
    /// `entry` list only means something relative to `slice_containers` and `shared`. The `vocabulary`
    /// roof exists to group by the user's question (see the module doc), and this is where that grouping
    /// says "one question" rather than "four".
    ///
    /// Nesting changes no contract: replacement granularity is the LEAF, exactly as `packs.disabled` does
    /// not clear `packs.extraDirs` and `git.since` does not clear `git.recentDays`.
    pub feature_sliced_design: FeatureSlicedDesignVocab,
}

/// The Feature-Sliced Design layer names a project picks — the nested half of
/// [`VocabularyConfig::feature_sliced_design`]. Every field follows the same declared-or-not-judged rule as its parent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FeatureSlicedDesignVocab {
    /// Directories whose children are slices (`zzop_metrics::DEFAULT_FSD_SLICE_CONTAINERS`). A project
    /// spelling this `modules` scores every cross-module import as cross-slice until it says so.
    pub slice_containers: Vec<String>,
    /// The entry layer — the outermost ring that may import anything
    /// (`zzop_metrics::DEFAULT_FSD_ENTRY`).
    pub entry: Vec<String>,
    /// The shared layer — the innermost ring that may be imported by anything
    /// (`zzop_metrics::DEFAULT_FSD_SHARED`). Distinct from
    /// [`VocabularyConfig::hierarchy_shared_dirs`]: this one names an FSD LAYER, that one exempts a
    /// directory from layering checks entirely. The default lists overlap in wording, which is exactly
    /// why they are two keys rather than one — two questions must not share one answer.
    pub shared: Vec<String>,
    /// The base layer beneath `shared` (`zzop_metrics::DEFAULT_FSD_BASE_DIRS`).
    pub base_dirs: Vec<String>,
}

/// Default for [`VocabularyConfig::java_source_root`]: the Maven/Gradle standard layout. A Spring security
/// config's exemption scope is cut at the first occurrence of this segment, so `service-a`'s config yields
/// `service-a/src/main/java/` and can never reach `service-b/src/main/java/...`. It lives HERE rather than
/// beside its one reader (`analyze::native_rules::callgraph::decorator_gate::spring_app_root`, which now
/// takes the resolved value as an argument) because a declarable vocabulary has exactly one default and
/// this module is where every one of them is assembled.
pub(crate) const DEFAULT_JAVA_SOURCE_ROOT: &str = "src/main/java/";

mod built_in;
