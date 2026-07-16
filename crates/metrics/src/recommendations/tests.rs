//! Covers rule selection and ordering (every applicable rule included, sorted by severity then
//! ROI desc), the per-rule filters (fat-fanout barrel/re-export/orchestrator exclusions, scope
//! excludes, permanent ignores), the glob matcher, hidden-coupling pair dedup, and cost/ROI
//! adjustments from untested paths and amplification. `deriveActionHintKey`'s branches are
//! exercised indirectly here via `action_hint_key` assertions since it has no separate call site
//! in this crate.

mod evidence;
mod pipeline;
mod rules;

use std::collections::HashSet;

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
        scope_excludes: &[],
        permanent_ignores: &[],
        untested_paths: empty_set(),
        amplification_by_path: empty_map(),
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
        data: None,
    }
}

// Static empty collections shared by tests that don't exercise untested/amplification inputs.
// std has no `const fn` HashSet::new usable in a `static`, so build lazily via `OnceLock`.
static EMPTY_SET: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();
static EMPTY_MAP: std::sync::OnceLock<HashMap<String, f64>> = std::sync::OnceLock::new();

fn empty_set() -> &'static HashSet<String> {
    EMPTY_SET.get_or_init(HashSet::new)
}
fn empty_map() -> &'static HashMap<String, f64> {
    EMPTY_MAP.get_or_init(HashMap::new)
}
