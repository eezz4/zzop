//! zzop-core — native engine: Common IR contracts + cross-layer linker + rule registry.
//!
//! Common IR type contracts as plain Rust structs. swc / external-parser types never leak in
//! here — parser-specific ASTs stay behind the parser crates' own boundaries. Rules and parsers
//! see only this module's Common IR.

pub mod attributes;
pub mod call_sites;
pub mod callgraph;
pub mod coupling;
pub mod dsl;
pub mod file_nodes;
pub mod finding;
pub mod fragments;
pub mod graph;
pub mod io;
pub mod ir;
pub mod node;
pub mod normalized;
pub mod pack_loader;
pub mod paths;
pub mod recognizer;
pub mod registry;
pub mod schema;
pub mod serde_util;
pub mod sightline;
pub mod string_literals;

pub use attributes::{attr_is_truthy, Attribute, AttributeStore, EntityRef};
pub use call_sites::{
    CallKind, CallSite, CALL_KIND_CONSOLE_WRITE, CALL_KIND_ENV_READ, CALL_KIND_HASH_CALL,
    CALL_KIND_PROCESS_EXEC, RULE_READ_CALL_KINDS,
};
pub use coupling::CommitFileSet;
pub use string_literals::{
    shannon_entropy_bits, value_hash_hex, BoundStringLiteral, HIGH_ENTROPY_SECRET_MIN_BITS,
};

pub use fragments::{
    ClassShapeFragment, ControllerPrefixRouteFragment, PagesApiHandlerScan, ProcedureRouterEntry,
    ProcedureRouterFragment, RouterMountEntry, RouterMountFragment, WrapperCallFragment,
    WrapperDefFragment,
};

pub use schema::{FieldAttr, SchemaEnum, SchemaField, SchemaModel, SchemaUsage};

pub use dsl::{
    apply_attr_gates, eval_pack, eval_pack_io_scan, FragmentError, IoDirection, IoScan,
    IoScanTreeContext, LabeledPattern, LineScan, Matcher, MethodScan, RuleContext, RuleDef,
    RulePackDef, SourceFile, SymbolScan, NEAR_MISS_MARKER_TOKEN_PATTERN,
};

pub use finding::{disable_hint, Finding, RuleExplain, Severity};
pub use graph::{circular_from_dep, circular_from_dep_excluding, find_cycles, ComponentEdge};
pub use io::{
    classify_consume_join, db_table_channel_casing, http_consume_interface_key, http_interface_key,
    key_carries_route_identity, link_cross_layer_io, normalize_http_path, unknown_verb_route_path,
    AmbiguousConsume, ConsumeBodyShape, ConsumeJoin, CrossLayerEdge, CrossLayerResult, IoConsume,
    IoFacts, IoKind, IoProvide, LinkOptions, ProvideBodyField, ProvideBodyShape,
    ProvideResponseShape, SourceIo, HTTP_KEY_VERBS, RULE_READ_IO_KINDS, UNKNOWN_VERB,
};
pub use ir::{
    ApiEndpoint, CommonIr, DepGraph, ImportBinding, ImportMap, MinimalIr, NonIdempotentKind,
    QueryCallSite, ReExport, SourceSymbol, SourceSymbolKind, WriteSite, DEP_GRAPH_RESOLVED_ONLY,
};
pub use node::{
    calc_risk_score, classify_lifecycle, compute_median_churn, FileNode, Lifecycle, RiskInput,
    RiskWeights, DEFAULT_RECENT_THRESHOLD_DAYS, DEFAULT_WEIGHTS,
};
pub use normalized::{
    envelope_hints, parse_contract_version, validate_envelope, validate_envelope_verdict,
    EnvelopeVerdict, FileProjection, NormalizedEnvelope, ProjectionOverrides,
    MIN_VERSION_FOR_OVERRIDES, MIN_VERSION_FOR_ROUTER_MOUNT_REF, NORMALIZED_AST_CONTRACT_VERSION,
    NORMALIZED_AST_FORMAT, SUPPORTED_NORMALIZED_AST_VERSION,
};
pub use pack_loader::{
    applies_to, check_dsl_schema_version, load_dsl_packs, pack_regex_issues,
    pack_retired_field_issues, parse_dsl_pack, LoadResult, PackLoadError,
};
pub use paths::is_test_file;
pub use recognizer::FrameworkRecognizer;
pub use registry::{
    apply_severity_override, global_exclude_matches_path, is_enabled, is_pack_enabled,
    is_suppressed, merge_findings, register_native_analysis_stub, suppression_matches_path,
    GlobalExclude, RuleConfig, RuleRegistry, Suppression, REDACTED,
};
pub use sightline::RuleSightline;

pub use file_nodes::{
    build_file_nodes, hotspot_score, DepStats, GitPathStats, GitStats, HOTSPOT_MIN_CHANGES,
};
