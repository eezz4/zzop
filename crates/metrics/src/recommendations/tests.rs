//! Covers rule selection and ordering (every applicable rule included, sorted by severity then
//! ROI desc), the per-rule filters (fat-fanout barrel/re-export/orchestrator exclusions), the
//! config-exclude post-filter, and hidden-coupling pair dedup. `deriveActionHintKey`'s branches are
//! exercised indirectly here via `action_hint_key` assertions since it has no separate call site
//! in this crate.

mod evidence;
mod pipeline;
mod rules;

use zzop_core::{DepGraph, Finding, Lifecycle};

use crate::coupling::CouplingMap;

use super::*;

fn node(path: &str) -> FileNode {
    FileNode {
        id: path.to_string(),
        path: path.to_string(),
        change_count: 0,
        churn: 0,
        last_modified: None,
        author_count: 1,
        loc: 50,
        tag_counts: HashMap::new(),
        fan_in: 0,
        fan_out: 0,
        total_connections: 0,
        risk_score: 50.0,
        ..Default::default()
    }
}

fn tags(fix: u32) -> HashMap<String, u32> {
    let mut m = HashMap::new();
    m.insert("FIX".to_string(), fix);
    m
}

fn empty_input<'a>(
    nodes: &'a [FileNode],
    dep: &'a DepGraph,
    coupling: &'a CouplingMap,
) -> BuildRecInput<'a> {
    BuildRecInput {
        nodes,
        dep,
        coupling,
        circular: &[],
        excludes: &[],
        findings: &[],
    }
}

fn critical_finding(path: &str, rule_id: &str) -> Finding {
    Finding {
        rule_id: rule_id.to_string(),
        severity: Severity::Critical,
        file: path.to_string(),
        line: 1,
        message: "test fixture critical finding".to_string(),
        evidence_paths: Vec::new(),
        data: None,
    }
}
