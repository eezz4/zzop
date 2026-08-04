//! Shared fixtures for the envelope test suite. The tests themselves live in the child modules,
//! split by concern: core Mode A ingestion (`ingest`), DSL rules / config diagnostics / determinism /
//! composition / the fragment-specifier resolver (`rules_and_diagnostics`), and reserved-sentinel
//! drops + config mounts (`reserved_and_mounts`). Mode B's applied-vs-declared return contract lives in
//! (`overlay`); its end-to-end effects stay in the crate-level `tests/analyze_adapter_overlay.rs`.

use std::collections::HashMap;

use zzop_core::{FileProjection, ImportMap, IoFacts, NormalizedEnvelope, NORMALIZED_AST_FORMAT};

use crate::EngineConfig;

mod ingest;
mod overlay;
mod reserved_and_mounts;
mod rules_and_diagnostics;

fn projection(path: &str, loc: u32) -> FileProjection {
    FileProjection {
        path: path.to_string(),
        loc,
        symbols: Vec::new(),
        imports: ImportMap::new(),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: HashMap::new(),
        procedure_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        class_shape_fragments: Vec::new(),
        io: IoFacts::default(),
        degraded: false,
        is_entry: false,
        overrides: Default::default(),
        attributes: Vec::new(),
        loop_spans: Vec::new(),
        function_spans: Vec::new(),
        test_spans: Vec::new(),
        calls: Vec::new(),
    }
}

fn envelope(files: Vec<FileProjection>) -> NormalizedEnvelope {
    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "test-parser/1".to_string(),
        source: "test".to_string(),
        files,
    }
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "test".to_string(),
        ..EngineConfig::default()
    }
}
