//! zzop-core — native engine: Common IR contracts + cross-layer linker + rule registry.
//!
//! Common IR type contracts as plain Rust structs. swc / external-parser types never leak in
//! here — parser-specific ASTs stay behind the parser crates' own boundaries. Rules and parsers
//! see only this module's Common IR.

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
pub mod registry;
pub mod schema;
pub mod serde_util;

pub use coupling::CommitFileSet;

pub use fragments::{
    ControllerPrefixRouteFragment, RouterMountEntry, RouterMountFragment, TrpcRouterEntry,
    TrpcRouterFragment, WrapperCallFragment, WrapperDefFragment,
};

pub use schema::{FieldAttr, SchemaEnum, SchemaField, SchemaModel, SchemaUsage};

pub use dsl::{
    eval_pack, IoDirection, IoScan, LabeledPattern, LineScan, Matcher, MethodScan, RuleContext,
    RuleDef, RulePackDef, SourceFile, SymbolScan,
};

pub use finding::{disable_hint, Finding, RuleExplain, Severity};
pub use graph::{
    circular_from_dep, circular_from_dep_excluding, connected_components, find_cycles,
    ComponentEdge, ConnectedComponentsResult,
};
pub use io::{
    http_consume_interface_key, http_interface_key, link_cross_layer_io, AmbiguousConsume,
    CrossLayerEdge, CrossLayerResult, IoConsume, IoFacts, IoKind, IoProvide, LinkOptions, SourceIo,
    HTTP_KEY_VERBS,
};
pub use ir::{
    ApiEndpoint, CommonIr, DepGraph, ImportBinding, ImportMap, MinimalIr, NonIdempotentKind,
    QueryCallSite, ReExport, SourceSymbol, SourceSymbolKind, WriteSite,
};
pub use node::{
    calc_risk_score, classify_lifecycle, compute_median_churn, FileNode, Lifecycle, RiskInput,
    RiskWeights, DEFAULT_RECENT_THRESHOLD_DAYS, DEFAULT_WEIGHTS,
};
pub use normalized::{
    validate_envelope, FileProjection, NormalizedEnvelope, NORMALIZED_AST_FORMAT,
    SUPPORTED_NORMALIZED_AST_VERSION,
};
pub use pack_loader::{applies_to, load_dsl_packs, LoadResult, PackLoadError};
pub use paths::is_test_file;
pub use registry::{
    apply_severity_override, global_exclude_matches_path, is_enabled, is_suppressed,
    merge_findings, register_native_analysis_stub, suppression_matches_path, GlobalExclude,
    RuleConfig, RuleDescriptor, RuleKind, RuleMeta, RuleRegistry, Suppression,
};

pub use file_nodes::{
    build_file_nodes, hotspot_score, DepStats, GitPathStats, GitStats, HOTSPOT_MIN_CHANGES,
};
