//! MCP `resources/*` handlers over the embedded authoring contracts (`zzop_summary::contracts`, shared
//! with the `zzop contract [<name>]` CLI subcommand). This is the contract-exposure half of the
//! "author an adapter with only the binary" promise — `resources/list` advertises every contract,
//! `resources/read` returns its full text. Deterministic: same binary, same list, same bytes.

/// The URI space, owned by the shared contract table rather than spelled here. Three surfaces now
/// build a `zzop://contract/<name>` string — this handler pair and, since the disclosure fold, every
/// analyze-shaped reply, which prints the disclosure document's URI as a pointer. A local copy of the
/// prefix is a pointer that can drift away from the lane that has to answer it.
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
    let name = uri.strip_prefix(URI_PREFIX).unwrap_or("");
    // `embedded::find` is the shared name-lookup the `zzop contract <name>` CLI path also uses —
    // one table, one resolver, so the MCP and terminal surfaces cannot drift.
    match zzop_summary::contracts::find(name) {
        Some(doc) => Ok(serde_json::json!({
            "contents": [{
                "uri": uri,
                "mimeType": doc.mime,
                "text": doc.content,
            }]
        })),
        None => {
            let known: Vec<String> = zzop_summary::contracts::names()
                .map(|n| format!("{URI_PREFIX}{n}"))
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
        assert_eq!(
            resources.len(),
            zzop_summary::contracts::CONTRACT_DOCS.len()
        );
        for doc in zzop_summary::contracts::CONTRACT_DOCS {
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
        for doc in zzop_summary::contracts::CONTRACT_DOCS {
            assert!(err.contains(doc.name), "error should list {}", doc.name);
        }
    }

    #[test]
    fn embedded_json_contracts_parse_as_json() {
        for doc in zzop_summary::contracts::CONTRACT_DOCS {
            if doc.mime == "application/json" {
                serde_json::from_str::<serde_json::Value>(doc.content)
                    .unwrap_or_else(|e| panic!("embedded {} is not valid JSON: {e}", doc.name));
            }
        }
    }

    /// Pins the ninth resource: `rule-pack-schema` serves the exact bytes of the authored
    /// `docs/contracts/rule-pack.schema.json`, as JSON that names every matcher kind — the
    /// machine-readable twin of the `validate_rule_pack` tool. The kind list below is DERIVED from
    /// nothing, so it is the one place a new matcher must be added by hand; the compiler-backed twin
    /// (`zzop_facade`'s `every_struct_in_the_matcher_source_is_covered_by_the_parity_pin`) is what
    /// actually forces the schema definition to exist.
    #[test]
    fn rule_pack_schema_resource_is_the_dsl_pack_shape_contract() {
        let doc = zzop_summary::contracts::CONTRACT_DOCS
            .iter()
            .find(|d| d.name == "rule-pack-schema")
            .expect("rule-pack-schema resource is embedded");
        assert_eq!(doc.mime, "application/json");
        let json: serde_json::Value = serde_json::from_str(doc.content).unwrap();
        assert_eq!(json["$schema"], "http://json-schema.org/draft-07/schema#");
        for kind in [
            "lineScan",
            "methodScan",
            "symbolScan",
            "ioScan",
            "callScan",
            "literalScan",
        ] {
            assert!(
                json["definitions"][kind].is_object(),
                "missing matcher definition {kind}"
            );
        }
    }

    /// Pins the config-surface resource: it serves the same vocabulary `zzop-config` embeds,
    /// as JSON whose self-describing sections (promised by the resource description) really exist.
    #[test]
    fn config_surface_resource_is_the_self_describing_config_vocabulary() {
        // No hand-written inventory count here any more (it read `10` and went red the day an
        // eleventh document landed): a literal number has no truth source, and the list-vs-table
        // invariant it gestured at is already pinned against `CONTRACT_DOCS` itself by
        // `every_contract_doc_lists_and_reads_back_its_embedded_bytes` above.
        let doc = zzop_summary::contracts::CONTRACT_DOCS
            .iter()
            .find(|d| d.name == "config-surface")
            .expect("config-surface resource is embedded");
        assert_eq!(doc.mime, "application/json");
        assert_eq!(doc.content, zzop_config::CONFIG_SURFACE_JSON);
        let json: serde_json::Value = serde_json::from_str(doc.content).unwrap();
        for section in ["configKeys", "configPaths", "embedderFields"] {
            assert!(json.get(section).is_some(), "missing section {section}");
        }
        assert!(
            json["_docs"]["purpose"].is_string(),
            "missing _docs.purpose"
        );
    }

    /// Pins the OTHER end of the disclosure fold (2026-07-29): every analyze-shaped reply now prints
    /// `disclosure.resource` instead of the registry's ~10.6KB of prose, and this is the lane that has
    /// to answer it. A pointer this handler cannot serve would be worse than no pointer — the reader
    /// would lose the classes entirely, where before they merely cost tokens.
    ///
    /// Asserted through the WIRE, not through the table: `read` with the exact URI the reply prints,
    /// checking that the served text really is the class list (an id and a status token), and that
    /// `list` advertises it — a document that reads but is not listed is discoverable only to a reader
    /// who already knows the name.
    #[test]
    fn the_disclosure_pointer_every_reply_prints_reads_back_the_full_class_list() {
        let name = zzop_summary::contracts::DISCLOSURE_CONTRACT_NAME;
        let uri = format!("zzop://contract/{name}");
        let params = serde_json::json!({ "uri": uri });
        let read = super::read(Some(&params)).expect("the reply's own pointer must resolve");
        let text = read["contents"][0]["text"]
            .as_str()
            .expect("a served document has text");
        assert_eq!(read["contents"][0]["mimeType"], "text/markdown");
        // The wire really carries the table's bytes (this crate is a thin protocol facade — serving
        // something else would be the drift), and those bytes really are the class list: a per-class
        // heading and all three status tokens. That the list is COMPLETE against the engine's live
        // registry is sealed one crate down, in `crates/summary/tests/disclosure_fold.rs`, which is
        // where the dev-dependency for reading the registry belongs.
        let doc = zzop_summary::contracts::find(name).expect("the table serves it too");
        assert_eq!(text, doc.content);
        assert!(text.contains("### "), "no per-class heading: {text:.400}");
        for status in ["asserted", "partial", "notYetDetected"] {
            assert!(text.contains(status), "served text omits status {status}");
        }
        let listed = super::list();
        assert!(
            listed["resources"]
                .as_array()
                .expect("resources array")
                .iter()
                .any(|r| r["uri"] == serde_json::json!(uri)),
            "resources/list must advertise {uri}"
        );
    }

    /// Pins the `rule-catalog` resource: it serves the exact bytes of `docs/rules/catalog.md` — the
    /// rule-id discoverability gap a live-fire round found (`packsLoaded` gives counts only, and the
    /// dsl-reference resource points at this very file, which was NOT served over MCP before this).
    #[test]
    fn rule_catalog_resource_is_the_full_rule_id_catalog_markdown() {
        let doc = zzop_summary::contracts::CONTRACT_DOCS
            .iter()
            .find(|d| d.name == "rule-catalog")
            .expect("rule-catalog resource is embedded");
        assert_eq!(doc.mime, "text/markdown");
        assert!(doc.content.contains("# Rule catalog"));
        // Every rule id table has an `id` column header — the catalog is machine-checked totals
        // (crates/engine/tests/rule_contracts) elsewhere; this only pins that the SERVED bytes are
        // the real catalog, not an empty/truncated stand-in.
        assert!(doc.content.contains("Rule id"));
        assert!(
            doc.content.len() > 10_000,
            "catalog.md should be substantial, got {} bytes",
            doc.content.len()
        );
    }
}
