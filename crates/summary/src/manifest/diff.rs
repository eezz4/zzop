//! `zzop diff <a> <b>` — the delta between two manifests ([`super::manifest_json`]). Pure: two JSON
//! strings in, one JSON string out, no engine, no filesystem, no state.
//!
//! ## The report is ranked, not symmetric
//! A `+` is common and usually harmless (a team added a route). A BUCKET TRANSITION is rare and
//! almost always a break: `GET /api/x` moving from `edges` to `unprovidedConsumes` means the caller
//! still calls it and the route is gone. `transitions` is therefore the entry to READ FIRST, with
//! the per-relation `added`/`removed` lists as its raw evidence — those lists are not additional
//! facts, a transition's own rows also appear there; the transition entry is the reading of them.
//! (Reply key ORDER is alphabetical, not ranked: `serde_json::Map` is a `BTreeMap` here — no
//! `preserve_order` feature — which is what makes every reply in this crate byte-identical run over
//! run. Ranking is stated in the docs and in this vocabulary, never smuggled into key order.)
//!
//! ## Two honesty gates (without these, this feature manufactures silent wrongs)
//! 1. **Tool identity.** Two manifests from different zzop builds are not comparable: our own parser
//!    improvement would read as the other team breaking a contract. Default is REFUSAL; the caller
//!    can insist (`allow_tool_drift`), and then the reply carries a `toolDrift` block naming both
//!    builds — refuse or disclose, never silently compare.
//! 2. **Blindness vs deletion.** A tree that got LESS visible (more `degraded` files, or newly
//!    `joinContributionZero`, or absent from the second run entirely) explains disappearances by
//!    itself. Every removed row attributable to such a source is tagged `blindnessSuspect: true`, and
//!    `sources.coverageDropped` names the drop — so "the route vanished" is never reported when
//!    "we stopped being able to see it" is the honest reading.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::RELATIONS;

/// Diffs two manifest JSON documents. `allow_tool_drift` turns honesty gate 1 from a refusal into a
/// disclosure (see the module doc).
pub fn diff_manifests_json(
    a_json: &str,
    b_json: &str,
    allow_tool_drift: bool,
) -> Result<String, String> {
    let a = parse_manifest(a_json, "the first")?;
    let b = parse_manifest(b_json, "the second")?;

    let (tool_a, tool_b) = (tool(&a), tool(&b));
    let mut out = serde_json::Map::new();
    if tool_a != tool_b {
        if !allow_tool_drift {
            return Err(format!(
                "refusing to diff manifests produced by different zzop builds — {tool_a:?} vs \
                 {tool_b:?}. A version/parser change moves keys between buckets on its own, so the \
                 delta would read as the analyzed code breaking a contract it never broke. Re-run \
                 `zzop manifest` on BOTH trees with one binary, or pass --allow-tool-drift to diff \
                 anyway (the reply then carries a toolDrift disclosure)"
            ));
        }
        out.insert(
            "toolDrift".to_string(),
            serde_json::json!({
                "a": tool_a,
                "b": tool_b,
                "warning": "these manifests were produced by DIFFERENT zzop builds — an extraction \
                            or join change in zzop itself can move keys between buckets with no \
                            change to the analyzed code, so every delta below is unattributable",
            }),
        );
    }
    out.insert("tool".to_string(), serde_json::json!(tool_a));

    // Honesty gate 2's substrate: which sources got less visible between the two runs.
    let (sources_report, blind_sources) = diff_sources(&a, &b);
    out.insert(
        "transitions".to_string(),
        transitions(&a, &b, &blind_sources),
    );
    out.insert("sources".to_string(), sources_report);
    for relation in RELATIONS {
        let (added, removed) = diff_rows(rows(&a, relation), rows(&b, relation), &blind_sources);
        out.insert(
            relation.to_string(),
            serde_json::json!({ "added": added, "removed": removed }),
        );
    }
    serde_json::to_string_pretty(&Value::Object(out)).map_err(|e| e.to_string())
}

/// Parses one side and checks it is shaped like a manifest at all — a caller who passes an `analyze`
/// reply, or last week's unrelated JSON, gets a named error naming WHICH argument was wrong, never a
/// diff of two empty relation sets that would read as "no change".
fn parse_manifest(text: &str, which: &str) -> Result<Value, String> {
    let v: Value = serde_json::from_str(text)
        .map_err(|e| format!("{which} manifest is not valid JSON: {e}"))?;
    if !v.get("tool").map(Value::is_string).unwrap_or(false) {
        return Err(format!(
            "{which} manifest has no `tool` string — is it a `zzop manifest` output? (a manifest \
             carries tool/sources/provides/edges/buckets)"
        ));
    }
    for field in ["sources"].iter().chain(RELATIONS.iter()) {
        if !v.get(*field).map(Value::is_array).unwrap_or(false) {
            return Err(format!(
                "{which} manifest has no `{field}` array — is it a `zzop manifest` output?"
            ));
        }
    }
    Ok(v)
}

fn tool(m: &Value) -> &str {
    m["tool"].as_str().unwrap_or_default()
}

fn rows<'a>(m: &'a Value, relation: &str) -> &'a [Value] {
    m[relation].as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// `sourceId -> (joinContributionZero, degraded)`.
fn source_coverage(m: &Value) -> BTreeMap<String, (bool, i64)> {
    rows(m, "sources")
        .iter()
        .filter_map(|s| {
            let id = s["sourceId"].as_str()?.to_string();
            Some((
                id,
                (
                    s["joinContributionZero"].as_bool().unwrap_or(false),
                    s["degraded"].as_i64().unwrap_or(0),
                ),
            ))
        })
        .collect()
}

/// Honesty gate 2. Returns the `sources` report plus the set of source ids whose visibility DROPPED
/// (or which vanished) — the ids that make a disappearance a blindness suspect rather than a deletion.
fn diff_sources(a: &Value, b: &Value) -> (Value, BTreeSet<String>) {
    let (ca, cb) = (source_coverage(a), source_coverage(b));
    let mut blind: BTreeSet<String> = BTreeSet::new();
    let mut dropped: Vec<Value> = Vec::new();
    for (id, (jcz_a, deg_a)) in &ca {
        match cb.get(id) {
            // A source that is simply GONE explains every one of its rows disappearing.
            None => {
                blind.insert(id.clone());
            }
            Some((jcz_b, deg_b)) => {
                if (*jcz_b && !jcz_a) || deg_b > deg_a {
                    blind.insert(id.clone());
                    dropped.push(serde_json::json!({
                        "sourceId": id,
                        "joinContributionZero": { "a": jcz_a, "b": jcz_b },
                        "degraded": { "a": deg_a, "b": deg_b },
                    }));
                }
            }
        }
    }
    let added: Vec<&String> = cb.keys().filter(|id| !ca.contains_key(*id)).collect();
    let removed: Vec<&String> = ca.keys().filter(|id| !cb.contains_key(*id)).collect();
    (
        serde_json::json!({ "added": added, "removed": removed, "coverageDropped": dropped }),
        blind,
    )
}

/// Every source id a row names: `source` for a provide/bucket row, both ends for an edge.
fn row_sources(row: &Value) -> Vec<&str> {
    ["source", "from", "to"]
        .iter()
        .filter_map(|k| row.get(*k).and_then(Value::as_str))
        .collect()
}

fn is_blind(row: &Value, blind_sources: &BTreeSet<String>) -> bool {
    row_sources(row).iter().any(|s| blind_sources.contains(*s))
}

/// Set difference in both directions, with gate 2's tag attached to the removals (only where it
/// applies — an absent flag is never a claim that a removal is trustworthy, it is the ordinary case).
fn diff_rows(
    a: &[Value],
    b: &[Value],
    blind_sources: &BTreeSet<String>,
) -> (Vec<Value>, Vec<Value>) {
    let a_set: BTreeSet<String> = a.iter().map(Value::to_string).collect();
    let b_set: BTreeSet<String> = b.iter().map(Value::to_string).collect();
    let added: Vec<Value> = b
        .iter()
        .filter(|r| !a_set.contains(&r.to_string()))
        .cloned()
        .collect();
    let removed: Vec<Value> = a
        .iter()
        .filter(|r| !b_set.contains(&r.to_string()))
        .map(|r| tag_blind(r, blind_sources))
        .collect();
    (added, removed)
}

fn tag_blind(row: &Value, blind_sources: &BTreeSet<String>) -> Value {
    let mut row = row.clone();
    if is_blind(&row, blind_sources) {
        if let Some(obj) = row.as_object_mut() {
            obj.insert("blindnessSuspect".to_string(), Value::Bool(true));
        }
    }
    row
}

/// `(kind, key) -> {bucket names}` — `"edges"` for a linked key, plus every non-edge bucket the key
/// sits in. A key legitimately occupies more than one bucket at once (two providers, one consumed),
/// so placement is a SET and a transition is a set change, never a single-value swap.
fn placements(m: &Value) -> BTreeMap<(String, String), (BTreeSet<String>, BTreeSet<String>)> {
    let mut out: BTreeMap<(String, String), (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    for row in rows(m, "edges") {
        let entry = out.entry(identity(row)).or_default();
        entry.0.insert("edges".to_string());
        entry
            .1
            .extend(row_sources(row).iter().map(|s| s.to_string()));
    }
    for row in rows(m, "buckets") {
        let entry = out.entry(identity(row)).or_default();
        entry
            .0
            .insert(row["bucket"].as_str().unwrap_or_default().to_string());
        entry
            .1
            .extend(row_sources(row).iter().map(|s| s.to_string()));
    }
    out
}

fn identity(row: &Value) -> (String, String) {
    (
        row["kind"].as_str().unwrap_or_default().to_string(),
        row["key"].as_str().unwrap_or_default().to_string(),
    )
}

/// The ranked signal: keys present in BOTH runs whose bucket placement changed. Pure additions and
/// pure removals are deliberately NOT transitions — they ride the per-relation lists below.
fn transitions(a: &Value, b: &Value, blind_sources: &BTreeSet<String>) -> Value {
    let (pa, pb) = (placements(a), placements(b));
    let mut out: Vec<Value> = Vec::new();
    for (id, (from, sources_a)) in &pa {
        let Some((to, sources_b)) = pb.get(id) else {
            continue;
        };
        if from == to {
            continue;
        }
        let mut row = serde_json::json!({
            "kind": id.0, "key": id.1,
            "from": from.iter().collect::<Vec<_>>(),
            "to": to.iter().collect::<Vec<_>>(),
        });
        if sources_a
            .union(sources_b)
            .any(|s| blind_sources.contains(s))
        {
            row["blindnessSuspect"] = Value::Bool(true);
        }
        out.push(row);
    }
    Value::Array(out)
}
