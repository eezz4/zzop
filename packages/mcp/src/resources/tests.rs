//! Tests for the MCP `resources/*` handlers.
//!
//! A file of its own rather than an inline `mod tests`: the handler module reached the 400-line
//! budget `scripts/check-max-file-lines.sh` enforces, and that guard's own prescription is to split
//! into a directory module. `super::` still reaches `list`/`read`/`templates_list`, so no handler
//! changed to make this move.

/// 🔴 The THIRD surface of the starter config, which nothing pinned until 2026-10-12 (inbox N51,
/// fable round 9 axis A8-41).
///
/// Four places in this workspace state that `zzop init`, `zzop contract config-template` and
/// MCP `resources/read` cannot disagree — `packages/cli-bin/tests/cli.rs`'s
/// `init_writes_the_same_bytes_the_contract_surface_serves`, `cli-bin/src/main.rs`'s `init` arm,
/// `crates/config/src/template.rs`'s module doc, and `cli/run/init.rs`'s `run_init` doc. 📏 The test
/// named in the first of those compares exactly TWO of them: the file on disk against
/// `contract config-template`'s stdout. The MCP leg had no byte comparison anywhere
/// (`git grep -n 'config-template' -- packages/mcp crates/summary` finds mentions, no assertion).
///
/// 🔵 The drift is structurally unlikely — all three resolve through one `contracts::find` — but the
/// third leg is NOT the same expression as the other two: this handler serves
/// `served_content(doc)`, not `doc.content`. That function prepends a provenance banner to every
/// `text/markdown` document, so this surface is byte-identical to the file `init` writes only
/// because the starter config is JSONC. **That is the condition this test exists to hold**: give
/// `config-template` a markdown mime and the MCP leg silently gains a comment line that the
/// written file does not have, and a JSONC parser is the thing that would notice first.
#[test]
fn the_third_surface_serves_the_starter_config_byte_for_byte() {
    let uri = format!(
        "{}{}",
        zzop_summary::contracts::URI_PREFIX,
        zzop_summary::contracts::CONFIG_TEMPLATE_NAME
    );
    let params = serde_json::json!({ "uri": uri });
    let result = super::read(Some(&params)).expect("the starter config must be readable");
    let contents = result["contents"].as_array().expect("contents array");
    assert_eq!(contents.len(), 1, "{result}");
    let served = contents[0]["text"].as_str().expect("text");

    // Leg three against the one canon the other two legs also read.
    assert_eq!(
        served,
        zzop_config::template::CONFIG_TEMPLATE_JSONC,
        "MCP resources/read must serve the starter config unchanged -- the file `zzop init` \
         writes and `zzop contract config-template` prints are these same bytes"
    );

    // The CONDITION, stated as its own assertion so a mime change fails HERE with the reason
    // rather than somewhere downstream as a JSONC parse error.
    let doc = zzop_summary::contracts::find(zzop_summary::contracts::CONFIG_TEMPLATE_NAME)
        .expect("the starter config must be in the contract table");
    assert_ne!(
        doc.mime, "text/markdown",
        "the starter config must not be served as markdown: served_content() prepends a \
         provenance HTML comment to markdown documents, which would make this surface differ \
         from the file `zzop init` writes AND stop the result being parseable as JSONC"
    );
}

/// The rule channel is ONE template, not a tool and not one resource per id.
///
/// Both rejected shapes are measurable rather than aesthetic: a ninth tool puts its schema in
/// `tools/list`, which every session pays before asking anything (32,127 bytes measured), and
/// listing every rule id as its own resource moves that same bloat to `resources/list`. This
/// exists to shrink what an agent reads, so paying a fixed cost to do it is self-defeating.
#[test]
fn the_rule_channel_is_one_template_and_does_not_touch_the_tool_or_resource_lists() {
    let t = super::templates_list();
    let templates = t["resourceTemplates"]
        .as_array()
        .expect("resourceTemplates array");
    assert_eq!(
        templates.len(),
        1,
        "one parameterised entry, not a roster: {templates:?}"
    );
    assert_eq!(templates[0]["uriTemplate"], "zzop://rule/{id}");

    // `resources/list` stays the contract table alone -- no rule ids leaked into it.
    let listed = super::list();
    let resources = listed["resources"].as_array().expect("resources array");
    assert_eq!(
        resources.len(),
        zzop_summary::contracts::CONTRACT_DOCS.len()
    );
    assert!(
        resources.iter().all(|r| {
            r["uri"]
                .as_str()
                .is_some_and(|u| u.starts_with("zzop://contract/"))
        }),
        "the rule space must not appear in resources/list: {resources:?}"
    );
}

/// THE TEMPLATE'S DESCRIPTION IS A CLAIM ABOUT ITS OWN INPUT DOMAIN, and it was false
/// (2026-09-14, external review round 22, ledger V245).
///
/// It read: "`id` is the id a finding already carries in `ruleId` (e.g. `security/hardcoded-secret`
/// for a DSL rule, **or a bare id for a native analysis**)". All 60 native analysis ids are refused
/// here — the lookup reads the compiled-in DSL pack data and nothing else. Round 20 (`37f02d04`)
/// had already fixed the REFUSAL TEXT for this case; the advertisement was not touched, and a
/// client reads the advertisement BEFORE it tries, so every native finding cost a wasted round trip.
///
/// This asserts the description against the resolver in both directions, from one probe each. A
/// description is the only part of a resource a client can act on without calling it, so it is the
/// part that has to be checked against what calling it does.
#[test]
fn the_template_description_matches_what_the_resolver_actually_accepts() {
    let t = super::templates_list();
    let description = t["resourceTemplates"][0]["description"]
        .as_str()
        .expect("the template carries a description");

    // A NATIVE analysis id. `circular` is registered (it appears in the rule catalog) and is not a
    // DSL rule, which is exactly the pair of properties this case needs.
    let uri = |id: &str| serde_json::json!({"uri": format!("{}{id}", super::RULE_URI_PREFIX)});
    let native = super::read(Some(&uri("circular")));
    assert!(
        native.is_err(),
        "a native analysis id must be refused here, or the description below is the false one"
    );
    assert!(
        description.contains("REFUSED"),
        "the resolver refuses native ids and the description must say so before a client spends a \
         round trip finding out: {description}"
    );
    assert!(
        description.contains("rule-catalog"),
        "saying `refused` without saying where the answer IS leaves the client exactly as stuck: \
         {description}"
    );

    // A DSL rule id, to prove the refusal above is about the id KIND and not about this resource
    // being broken — without this half, deleting the resolver would pass every assertion above.
    let dsl = super::read(Some(&uri("security/hardcoded-secret")));
    assert!(
        dsl.is_ok(),
        "a `<pack>/<rule>` id must still resolve: {dsl:?}"
    );
    assert!(
        !description.contains("bare id for a native analysis"),
        "the retired claim is back on the wire: {description}"
    );
}

/// The wire bytes ARE `zzop explain`'s bytes, because both call one function.
///
/// This is the claim the whole channel rests on: the product decision moved the fixed explanation
/// to one owner, and two owners that merely agree today is what this repo keeps finding. Pinned
/// against a real shipped rule rather than a fixture, so a change to the rendering is caught here
/// and not only in the CLI's own tests.
#[test]
fn reading_a_rule_returns_exactly_what_explain_renders() {
    let id = "security/hardcoded-secret";
    let expected = zzop_summary::explain(id).expect("a shipped rule id must resolve");

    let params = serde_json::json!({ "uri": format!("zzop://rule/{id}") });
    let got = super::read(Some(&params)).expect("a shipped rule id must read");
    let content = &got["contents"][0];

    assert_eq!(content["text"].as_str().expect("text"), expected);
    assert_eq!(content["mimeType"], "text/plain");
    assert_eq!(content["uri"], format!("zzop://rule/{id}"));
}

/// An id the binary does not carry fails with `explain`'s own sentence, not a second wording.
///
/// FLOOR in the other direction too: the shipped id above must NOT take this path, or this test
/// would pass over a channel that rejects everything.
#[test]
fn an_unknown_rule_id_fails_with_the_shared_lookup_message() {
    let params = serde_json::json!({ "uri": "zzop://rule/not-a-real-rule" });
    let err = super::read(Some(&params)).expect_err("an unknown id must not resolve");
    let from_explain = zzop_summary::explain("not-a-real-rule").expect_err("same lane");
    assert_eq!(err, from_explain, "one owner for the failure sentence too");

    let ok = serde_json::json!({ "uri": "zzop://rule/security/hardcoded-secret" });
    assert!(
        super::read(Some(&ok)).is_ok(),
        "FLOOR: a shipped id must resolve, or the assertion above is about a dead channel"
    );
}

/// A uri in neither space still gets the CONTRACT lookup's error, and that error now names the
/// rule space -- an agent holding a `ruleId` must not read a list that omits the thing it wants.
#[test]
fn an_unknown_uri_names_both_spaces() {
    let params = serde_json::json!({ "uri": "zzop://nonsense/x" });
    let err = super::read(Some(&params)).expect_err("unknown space must not resolve");
    assert!(err.contains("known resources:"), "{err}");
    assert!(
        err.contains("zzop://rule/"),
        "the rule space must be named for a reader holding a ruleId: {err}"
    );
}

#[test]
/// The read-back pins `served_content`, not `content`, and the difference is the point (review
/// ledger V74): a markdown contract is served with a one-line provenance banner naming the build
/// that baked it, because these documents are compiled in and a reader otherwise cannot tell a
/// 68-commit-stale copy from a current one. Pinning `content` here would pass while the surface an
/// agent actually receives went unwatched — which is what this test did before.
fn every_contract_doc_lists_and_reads_back_exactly_what_the_binary_serves() {
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
        assert_eq!(
            read["contents"][0]["text"].as_str().unwrap(),
            zzop_summary::contracts::served_content(doc)
        );
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

/// Pins the `rule-pack-schema` resource — named, not numbered. This test resolves the row by NAME
/// (`find(|d| d.name == "rule-pack-schema")`) and asserts nothing whatsoever about where it sits in
/// `CONTRACT_DOCS`, so an ordinal in this sentence is unfalsifiable by construction: no assertion
/// below it can go red when the table is reordered or appended to.
///
/// It had already gone stale. This line used to open "Pins the ninth resource", true when written
/// and false by 2026-08-09 — `CONTRACT_DOCS` is appended to freely (it grew again while this very
/// comment was being fixed), and nothing anywhere pins the row's index. Deleted rather than
/// recounted on purpose: a corrected number would be the same ghost with a later expiry date, and
/// no reader of THIS test needs the position to understand what it checks. If a position ever does
/// become load-bearing, it needs an assertion, not a comment.
///
/// `rule-pack-schema` serves the exact bytes of the authored
/// `docs/contracts/rule-pack.schema.json`, as JSON that names every matcher kind — the
/// machine-readable twin of the `validate_rule_pack` tool. The kind list below is DERIVED from
/// nothing, so it is the one place a new matcher must be added by hand; the compiler-backed twin
/// (`zzop_facade`'s `every_struct_in_the_pack_definition_source_has_a_schema_mirror`) is what
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
    // `served_content`, not `content`: a markdown contract carries a provenance banner naming the
    // build that baked it (review ledger V74). The banner PREPENDS, so the table's bytes are all
    // still here -- the assertions below read them through it.
    assert_eq!(text, zzop_summary::contracts::served_content(doc));
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
/// rule-id discoverability gap a live-fire round found (`packsLoaded` gave counts only then; it
/// carries `ruleIds` since 2026-08-20, but only for the packs one RUN loaded, while this resource
/// answers with no run at all — and the dsl-reference resource points at this very file, which was
/// NOT served over MCP before this).
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
