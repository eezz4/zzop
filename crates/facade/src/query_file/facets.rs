//! Per-facet projections for [`super::query_file_json`] — "everything about this file", one field at a
//! time. Split out of the parent purely for the per-file line cap; each function answers exactly one
//! question about one `rel` inside one tree entry, and none of them knows what a verdict is.

use serde_json::{json, Map, Value};

pub(super) fn loc_of(tree: &Value, rel: &str) -> u64 {
    tree.pointer("/output/ir/loc")
        .and_then(Value::as_object)
        .and_then(|m| m.get(rel))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

pub(super) fn symbols_of(tree: &Value, rel: &str) -> Value {
    let all = tree
        .pointer("/output/ir/symbols")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mine: Vec<&Value> = all
        .iter()
        .filter(|s| s.get("file").and_then(Value::as_str) == Some(rel))
        .collect();
    let exported: Vec<&str> = mine
        .iter()
        .filter(|s| s.get("exported").and_then(Value::as_bool) == Some(true))
        .filter_map(|s| s.get("name").and_then(Value::as_str))
        .collect();
    json!({ "count": mine.len(), "exported": exported })
}

pub(super) fn io_of(tree: &Value, rel: &str) -> Value {
    let pick = |field: &str| -> Vec<Value> {
        tree.pointer(&format!("/output/ir/io/{field}"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter(|f| f.get("file").and_then(Value::as_str) == Some(rel))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    };
    json!({ "provides": pick("provides"), "consumes": pick("consumes") })
}

/// What `dependencies` MEANS, shipped beside it as `dependenciesMeaning` — this surface's own norm
/// (`verdictMeaning`) applied to the one other field here whose silence is ambiguous. The rule itself is
/// not restated: it is [`zzop_core::DEP_GRAPH_RESOLVED_ONLY`], the single owner, plus the consequence
/// that only a per-FILE query makes visible — an empty `imports` is a statement about resolution, not
/// about the file's source text.
///
/// Added 2026-07-31 with the `resolvedImportEdges` rename. The key NAMES stay `imports`/`importedBy`:
/// unlike the census field, they are not counts that get quoted out of context, and the caller reading
/// them is looking at one named file with the answer's meaning in the same object.
pub(super) fn dependencies_meaning() -> String {
    format!(
        "{} An EMPTY `imports` list therefore does NOT mean this file imports nothing — a file whose \
         every import is a package (`import requests`, `import React from \"react\"`) or an \
         unresolvable specifier has no edges here at all. Read it as \"no in-tree import of this file \
         was resolved\", and read `importedBy` the same way.",
        zzop_core::DEP_GRAPH_RESOLVED_ONLY
    )
}

/// The file's position in the dependency graph, both directions. `importedBy` is the half a caller
/// cannot compute from the file's own text, and is usually the one that answers "is this safe to change".
pub(super) fn deps_of(tree: &Value, rel: &str) -> Value {
    let dep = tree.pointer("/output/ir/dep").and_then(Value::as_object);
    let imports: Vec<&str> = dep
        .and_then(|d| d.get(rel))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let imported_by: Vec<&str> = dep
        .map(|d| {
            d.iter()
                .filter(|(_, v)| {
                    v.as_array()
                        .is_some_and(|a| a.iter().any(|t| t.as_str() == Some(rel)))
                })
                .map(|(k, _)| k.as_str())
                .collect()
        })
        .unwrap_or_default();
    json!({ "imports": imports, "importedBy": imported_by })
}

/// Every finding anchored in this file — the tree's own plus any cross-layer finding whose site is here.
/// Uncapped (module doc), with counts by severity and rule so a caller can triage without reading the list.
pub(super) fn findings_of(analysis: &Value, tree: &Value, rel: &str) -> Value {
    let mut list: Vec<Value> = Vec::new();
    for f in tree
        .pointer("/output/findings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        if f.get("file").and_then(Value::as_str) == Some(rel) {
            list.push(f);
        }
    }
    for f in analysis
        .get("crossLayerFindings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        if f.get("file").and_then(Value::as_str) == Some(rel) {
            list.push(f);
        }
    }
    let mut by_severity: Map<String, Value> = Map::new();
    let mut by_rule: Map<String, Value> = Map::new();
    for f in &list {
        for (field, acc) in [("severity", &mut by_severity), ("ruleId", &mut by_rule)] {
            if let Some(k) = f.get(field).and_then(Value::as_str) {
                let n = acc.get(k).and_then(Value::as_u64).unwrap_or(0) + 1;
                acc.insert(k.to_string(), json!(n));
            }
        }
    }
    json!({
        "total": list.len(),
        "bySeverity": Value::Object(by_severity),
        "byRule": Value::Object(by_rule),
        "list": list,
    })
}
