//! MCP `resources/*` handlers over the embedded authoring contracts (`zzop_summary::contracts`, shared
//! with the `zzop contract [<name>]` CLI subcommand). This is the contract-exposure half of the
//! "author an adapter with only the binary" promise — `resources/list` advertises every contract,
//! `resources/read` returns its full text. Deterministic: same binary, same list, same bytes.

/// The URI space, owned by the shared contract table rather than spelled here. Three surfaces now
/// build a `zzop://contract/<name>` string — this handler pair and, since the disclosure fold, every
/// analyze-shaped reply, which prints the disclosure document's URI as a pointer. A local copy of the
/// prefix is a pointer that can drift away from the lane that has to answer it.
mod rules;
pub use rules::templates_list;
use rules::{read_rule, RULE_URI_PREFIX};

use zzop_summary::contracts::URI_PREFIX;

/// `resources/list` result — every embedded contract document, in embed order.
pub fn list() -> serde_json::Value {
    let resources: Vec<serde_json::Value> = zzop_summary::contracts::CONTRACT_DOCS
        .iter()
        .map(|doc| {
            serde_json::json!({
                "uri": format!("{URI_PREFIX}{}", doc.name),
                "name": doc.name,
                "description": doc.description,
                "mimeType": doc.mime,
            })
        })
        .collect();
    serde_json::json!({ "resources": resources })
}

/// `resources/read`: resolves a `zzop://contract/<name>` URI to its embedded text. Unknown URIs get a
/// self-explaining error listing the valid names (an agent should never have to guess).
pub fn read(params: Option<&serde_json::Value>) -> Result<serde_json::Value, String> {
    let uri = params
        .and_then(|p| p.get("uri"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing `uri` argument".to_string())?;
    // The rule space first: it is a DIFFERENT uri prefix, so a miss here is not an error, it is
    // "not mine". Returning None lets the contract lookup below own the unknown-uri message, which
    // keeps one list of valid names rather than two that can disagree.
    if let Some(result) = read_rule(uri) {
        return result;
    }
    let name = uri.strip_prefix(URI_PREFIX).unwrap_or("");
    // `embedded::find` is the shared name-lookup the `zzop contract <name>` CLI path also uses —
    // one table, one resolver, so the MCP and terminal surfaces cannot drift.
    match zzop_summary::contracts::find(name) {
        Some(doc) => Ok(serde_json::json!({
            "contents": [{
                "uri": uri,
                "mimeType": doc.mime,
                "text": zzop_summary::contracts::served_content(doc),
            }]
        })),
        None => {
            let known: Vec<String> = zzop_summary::contracts::names()
                .map(|n| format!("{URI_PREFIX}{n}"))
                .collect();
            Err(format!(
                "unknown resource uri {uri:?} — known resources: {}. For ONE rule's full text use \
                 {RULE_URI_PREFIX}<id> with the id a finding carries in `ruleId` (see                  `resources/templates/list`).",
                known.join(", ")
            ))
        }
    }
}

#[cfg(test)]
mod tests;
