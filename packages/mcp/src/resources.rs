//! MCP `resources/*` handlers over the embedded authoring contracts (`crate::embedded`). This is the
//! contract-exposure half of the "author an adapter with only the binary" promise — `resources/list`
//! advertises every contract, `resources/read` returns its full text. Deterministic: same binary,
//! same list, same bytes.

const URI_PREFIX: &str = "zzop://contract/";

/// `resources/list` result — every embedded contract document, in embed order.
pub fn list() -> serde_json::Value {
    let resources: Vec<serde_json::Value> = crate::embedded::CONTRACT_DOCS
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
    let name = uri.strip_prefix(URI_PREFIX).unwrap_or("");
    match crate::embedded::CONTRACT_DOCS
        .iter()
        .find(|doc| doc.name == name)
    {
        Some(doc) => Ok(serde_json::json!({
            "contents": [{
                "uri": uri,
                "mimeType": doc.mime,
                "text": doc.content,
            }]
        })),
        None => {
            let known: Vec<String> = crate::embedded::CONTRACT_DOCS
                .iter()
                .map(|d| format!("{URI_PREFIX}{}", d.name))
                .collect();
            Err(format!(
                "unknown resource uri {uri:?} — known resources: {}",
                known.join(", ")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_contract_doc_lists_and_reads_back_its_embedded_bytes() {
        let listed = super::list();
        let resources = listed["resources"].as_array().expect("resources array");
        assert_eq!(resources.len(), crate::embedded::CONTRACT_DOCS.len());
        for doc in crate::embedded::CONTRACT_DOCS {
            let uri = format!("zzop://contract/{}", doc.name);
            let params = serde_json::json!({ "uri": uri });
            let read = super::read(Some(&params)).expect("known uri reads");
            assert_eq!(read["contents"][0]["text"].as_str().unwrap(), doc.content);
            assert_eq!(read["contents"][0]["mimeType"].as_str().unwrap(), doc.mime);
        }
    }

    #[test]
    fn unknown_uri_error_names_every_valid_resource() {
        let params = serde_json::json!({ "uri": "zzop://contract/nope" });
        let err = super::read(Some(&params)).unwrap_err();
        for doc in crate::embedded::CONTRACT_DOCS {
            assert!(err.contains(doc.name), "error should list {}", doc.name);
        }
    }

    #[test]
    fn embedded_json_contracts_parse_as_json() {
        for doc in crate::embedded::CONTRACT_DOCS {
            if doc.mime == "application/json" {
                serde_json::from_str::<serde_json::Value>(doc.content)
                    .unwrap_or_else(|e| panic!("embedded {} is not valid JSON: {e}", doc.name));
            }
        }
    }
}
