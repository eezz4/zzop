//! `queryIo` — the shared endpoint-query core: a DEFINITIVE answer to "is io key X
//! provided/consumed/joined?" computed by pure post-processing over an ALREADY-PRODUCED analysis
//! output (no re-analysis, no cache interaction). Both hosts consume this one function — the JS CLI
//! (`zzop endpoint`, via the napi `queryIo` export) and the Node-free `zzop-mcp` binary
//! (`check_endpoint` tool) — so they give byte-identical answers for the same analysis.
//!
//! ## Sealed verdict vocabulary (wire contract — do not extend without a contract bump)
//! The `verdict` field is one of exactly eight tokens, derived deterministically from which
//! cross-layer join buckets contain >=1 key match:
//!
//! | bucket with a match     | verdict                 |
//! |-------------------------|-------------------------|
//! | `edges`                 | `"linked"`              |
//! | `unconsumedProvides`    | `"provided-only"`       |
//! | `unprovidedConsumes`    | `"consumed-unprovided"` |
//! | `unresolvedConsumes`    | `"unresolved-only"`     |
//! | `externalConsumes`      | `"external"`            |
//! | `ambiguousConsumes`     | `"ambiguous"`           |
//!
//! Exactly one bucket class matching yields its token; two or more yield `"mixed"` (the `counts`
//! field disambiguates); zero yield `"not-found"` (with `suggestions`).
//!
//! ## Accepted analysis shapes
//! - `analyzeTrees` output (`{trees, crossLayer, crossLayerFindings, disclosure, ...}`) — the full
//!   query runs over the join buckets.
//! - single-tree `analyze` output (`{ir, findings, ...}`) — a guided `Err`. VERIFIED: the single
//!   output DOES expose raw io facts (`ir.io.provides`/`ir.io.consumes`), but they are PRE-JOIN
//!   facts; every verdict token above is a join-bucket claim (whether a provide is consumed only
//!   the cross-layer join establishes), so mapping raw facts onto the sealed vocabulary would
//!   fabricate a join that never ran. The error still matches the pattern against `ir.io` and
//!   reports the raw counts, then tells the caller to run a trees analysis (a single tree passed
//!   through `analyzeTrees` still gets the join, intra-tree edges included).

mod scan;

use serde_json::{json, Map, Value};

use scan::{all_findings, scan_bucket, single_tree_err, suggestions};

/// Cap for each `matches.<bucket>` array — full counts always ride along uncapped in `counts`,
/// and a capped bucket discloses its remainder in `truncated`.
pub const QUERY_MATCH_LIMIT: usize = 20;
/// Cap for the `relatedFindings` array (`truncatedFindings` discloses the remainder).
pub const QUERY_FINDINGS_LIMIT: usize = 20;
/// Cap for the `suggestions` array (`not-found` verdicts only).
pub const QUERY_SUGGESTIONS_LIMIT: usize = 10;

/// Join-bucket names in engine order, paired with their verdict tokens (the sealed vocabulary
/// above). Order is the deterministic iteration order for `counts`/`matches`/`suggestions`.
const BUCKETS: [(&str, &str); 6] = [
    ("edges", "linked"),
    ("unconsumedProvides", "provided-only"),
    ("unprovidedConsumes", "consumed-unprovided"),
    ("unresolvedConsumes", "unresolved-only"),
    ("externalConsumes", "external"),
    ("ambiguousConsumes", "ambiguous"),
];

/// `{"pattern": "<non-empty string>"}` -> the pattern. Unknown keys are a caller error
/// (answered by name, never guessed around — same stance as the hosts' own argument validation).
fn parse_query(query_json: &str) -> Result<String, String> {
    let query: Value = serde_json::from_str(query_json)
        .map_err(|e| format!("zzop-facade: invalid queryIo() query JSON: {e}"))?;
    let obj = query
        .as_object()
        .ok_or_else(|| "zzop-facade: queryIo() query must be a JSON object".to_string())?;
    if let Some(unknown) = obj.keys().find(|k| k.as_str() != "pattern") {
        return Err(format!(
            "zzop-facade: unknown queryIo() query key {unknown:?} — the only supported key is \"pattern\""
        ));
    }
    let pattern = obj.get("pattern").and_then(Value::as_str).unwrap_or("");
    if pattern.is_empty() {
        return Err(
            "zzop-facade: queryIo() query requires a non-empty string \"pattern\"".to_string(),
        );
    }
    Ok(pattern.to_string())
}

/// `queryIo(analysisJson, queryJson)`: the definitive endpoint query. See the module doc for the
/// sealed verdict vocabulary, the accepted analysis shapes, and the single-tree guided error.
/// Every failure mode returns `Err(message)` — never a panic.
pub fn query_io_json(analysis_json: &str, query_json: &str) -> Result<String, String> {
    let pattern = parse_query(query_json)?;
    let analysis: Value = serde_json::from_str(analysis_json)
        .map_err(|e| format!("zzop-facade: invalid queryIo() analysis JSON: {e}"))?;

    let cross_layer =
        match analysis.get("crossLayer") {
            Some(cl) if cl.is_object() => cl,
            _ if analysis.get("ir").is_some() => return Err(single_tree_err(&analysis, &pattern)),
            _ => return Err(
                "zzop-facade: queryIo() analysis JSON is not a zzop analysis output (expected an \
                 analyzeTrees() output with `crossLayer`, or a single-tree analyze() output)"
                    .to_string(),
            ),
        };

    let needle = pattern.to_lowercase();
    let mut counts = Map::new();
    let mut matches = Map::new();
    let mut truncated = Map::new();
    let mut matched_classes: Vec<&str> = Vec::new();
    let mut matched_keys_lower: Vec<String> = Vec::new();
    for (bucket, verdict_token) in BUCKETS {
        let scanned = scan_bucket(cross_layer[bucket].as_array(), &needle);
        if scanned.count > 0 && !matched_classes.contains(&verdict_token) {
            matched_classes.push(verdict_token);
        }
        if scanned.count > scanned.shown.len() {
            truncated.insert(
                bucket.to_string(),
                json!(scanned.count - scanned.shown.len()),
            );
        }
        counts.insert(bucket.to_string(), json!(scanned.count));
        matches.insert(bucket.to_string(), Value::Array(scanned.shown));
        for key in scanned.matched_keys_lower {
            if !matched_keys_lower.contains(&key) {
                matched_keys_lower.push(key);
            }
        }
    }

    let verdict = match matched_classes.as_slice() {
        [] => "not-found",
        [single] => single,
        _ => "mixed",
    };

    let mut related: Vec<Value> = Vec::new();
    let mut related_total = 0usize;
    for finding in all_findings(&analysis) {
        let message = finding["message"].as_str().unwrap_or("").to_lowercase();
        if message.contains(&needle) || matched_keys_lower.iter().any(|k| message.contains(k)) {
            related_total += 1;
            if related.len() < QUERY_FINDINGS_LIMIT {
                related.push(finding.clone());
            }
        }
    }

    let mut out = Map::new();
    out.insert("pattern".to_string(), json!(pattern));
    out.insert("verdict".to_string(), json!(verdict));
    out.insert("counts".to_string(), Value::Object(counts));
    out.insert("matches".to_string(), Value::Object(matches));
    if !truncated.is_empty() {
        out.insert("truncated".to_string(), Value::Object(truncated));
    }
    out.insert("relatedFindings".to_string(), Value::Array(related));
    if related_total > QUERY_FINDINGS_LIMIT {
        out.insert(
            "truncatedFindings".to_string(),
            json!(related_total - QUERY_FINDINGS_LIMIT),
        );
    }
    if verdict == "not-found" {
        out.insert(
            "suggestions".to_string(),
            json!(suggestions(cross_layer, &pattern)),
        );
    }
    // Forwarded verbatim — the run-global blindness-class registry rides on every facade output.
    out.insert(
        "disclosure".to_string(),
        analysis.get("disclosure").cloned().unwrap_or(Value::Null),
    );

    serde_json::to_string(&Value::Object(out))
        .map_err(|e| format!("zzop-facade: failed to serialize queryIo() output: {e}"))
}
