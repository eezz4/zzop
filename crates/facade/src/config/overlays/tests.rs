//! The axis every test here holds: a bad overlay must cost a warning, never the run.

use super::*;

fn valid_overlay() -> serde_json::Value {
    serde_json::json!({
        "format": zzop_core::NORMALIZED_AST_FORMAT,
        "version": zzop_core::NORMALIZED_AST_CONTRACT_VERSION,
        "parser": "acme-adapter/1",
        "source": "svc-a",
        "files": [{
            "path": "src/routes.ts",
            "loc": 40,
            "io": {"provides": [{"kind": "http", "key": "GET /api/users", "file": "src/routes.ts", "line": 3}], "consumes": []}
        }]
    })
}

#[test]
fn a_well_formed_overlay_types_cleanly_and_warns_about_nothing() {
    let mut warnings = Vec::new();
    let out = typed_overlays(&[valid_overlay()], &mut warnings);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].parser, "acme-adapter/1");
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// The reported defect, in miniature: one required field missing from one file entry. Before this
/// module it aborted `analyze_json` with exit 1 and zero findings.
#[test]
fn a_shape_error_skips_only_that_overlay_and_names_its_parser() {
    let mut broken = valid_overlay();
    broken["files"][0]
        .as_object_mut()
        .unwrap()
        .remove("path")
        .unwrap();
    let mut warnings = Vec::new();
    let out = typed_overlays(&[broken], &mut warnings);
    assert!(out.is_empty(), "the malformed overlay must not be applied");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("acme-adapter/1"), "{}", warnings[0]);
    assert!(warnings[0].contains("SKIPPED"), "{}", warnings[0]);
    assert!(
        warnings[0].contains("adapterOverlays[0]"),
        "the caller must be told WHICH overlay: {}",
        warnings[0]
    );
}

/// The half that makes this a soft skip rather than an all-or-nothing gate: a good overlay beside a bad
/// one still applies. Without this, "skip the bad one" and "reject the batch" pass the same test.
#[test]
fn a_good_overlay_beside_a_bad_one_still_applies() {
    let mut warnings = Vec::new();
    let out = typed_overlays(
        &[
            serde_json::json!({"parser": "broken/1"}),
            valid_overlay(),
            serde_json::json!("not even an object"),
        ],
        &mut warnings,
    );
    assert_eq!(out.len(), 1, "the valid overlay survives its neighbours");
    assert_eq!(out[0].parser, "acme-adapter/1");
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings[0].contains("broken/1"), "{}", warnings[0]);
}

/// When the shape is broken past the point of carrying a `parser`, the warning says so instead of
/// printing an empty pair of backticks — and still names the index, which is always there.
#[test]
fn an_overlay_without_a_parser_id_is_identified_by_index() {
    let mut warnings = Vec::new();
    let out = typed_overlays(&[serde_json::json!({"files": []})], &mut warnings);
    assert!(out.is_empty());
    assert!(
        warnings[0].contains("adapterOverlays[0]") && warnings[0].contains("no `parser` id"),
        "{}",
        warnings[0]
    );
}

/// The message must not send the reader chasing the byte offset serde reports — it indexes a
/// re-serialization of the value, not any file the caller wrote. That coordinate is what made the
/// original failure unactionable, so saying it is meaningless is part of the fix.
#[test]
fn the_message_disowns_the_byte_offset_and_names_the_validator() {
    let mut warnings = Vec::new();
    typed_overlays(&[serde_json::json!({"parser": "acme/1"})], &mut warnings);
    assert!(warnings[0].contains("not your file"), "{}", warnings[0]);
    assert!(
        warnings[0].contains("validate-envelope"),
        "the reader gets the tool that checks the file directly: {}",
        warnings[0]
    );
}

/// THE HOLE THIS CLOSES (2026-08-31, release audit). The test above pins `validate-envelope` — a CLI
/// SUBCOMMAND, one `packages/cli-bin/src/main.rs` really dispatches — and pinned nothing at all about
/// the contract DOCUMENT the same sentence sends the reader to. So the warning shipped
/// `zzop://contract/normalized-ast` / `zzop contract normalized-ast`, a name that resolves to NOTHING:
/// `zzop_summary::contracts::find` matches `doc.name == name` exactly, and the file that name was
/// guessed from (`docs/NORMALIZED_AST.md`) is served as `envelope-guide`. MEASURED, not assumed:
/// `zzop contract normalized-ast` exits 1 with `unknown contract "normalized-ast" — known contracts:
/// ...`, and the MCP twin does the SAME — `packages/mcp/src/resources.rs`'s `read` answers an unknown
/// URI with every valid one, off the same `contracts::names()`. So neither audience is stranded; what
/// the minted name costs is a wasted round trip and a reader who now distrusts the message that sent
/// them. That is the honest size of it, and it is why this pin is worth having anyway: the message is
/// the only thing here that CAN be wrong, because both error paths derive their lists.
///
/// The subject set is DERIVED, not listed. A hand-written "valid names" array here would be the THIRD
/// copy of that vocabulary (after the table itself and the message), and the third copy is the one that
/// goes stale — which is this defect exactly, one level out. So the names are read out of the table's
/// own source. As TEXT rather than through `zzop_summary`, because this crate sits BELOW that one
/// (`zzop-summary` depends on `zzop-facade`, so there is no import to make); reading a sibling crate's
/// source for a derived subject set is the shape
/// `crates/engine/tests/rule_contracts/host_vocabulary.rs` already uses for the MCP tool list and the
/// CLI subcommand list.
///
/// KNOWN NARROWING, written down rather than left to be discovered: the DERIVED `example-pack-<stem>`
/// rows are invisible to this parse — `crates/config/build.rs` generates them and they have no `name:`
/// row in the file. No message in this module names one today; if one ever does, this test goes RED on
/// a name that is genuinely served, and the fix is to teach the parse that family, not to delete the pin.
#[test]
fn every_contract_document_the_warning_names_is_one_the_binary_actually_serves() {
    let names = served_contract_names();
    // Non-vacuity. A parse that silently returned nothing would vouch for every name ever written.
    assert!(
        names.len() >= 8,
        "parsed only {} contract name(s) out of the table source — the extraction broke, and an empty \
         subject set makes this pin vacuously green: {names:?}",
        names.len()
    );
    assert!(
        names.iter().any(|n| n == "envelope-guide"),
        "the parse found names but not `envelope-guide`, which the table demonstrably serves: {names:?}"
    );
    // The invalidation probe, kept as an assertion so it cannot decay into a comment: the exact
    // spelling this pin was written for must NOT be in the set, or its red case is gone.
    assert!(
        !names.iter().any(|n| n == "normalized-ast"),
        "`normalized-ast` is now a served contract, so this pin can no longer go red — re-point it"
    );

    let mut warnings = Vec::new();
    typed_overlays(&[serde_json::json!({"parser": "acme/1"})], &mut warnings);
    let warning = &warnings[0];

    // BOTH dialects, each judged on its own. A literal-level "mentions some real name somewhere" test
    // would pass a message that spelled one dialect right and minted the other.
    let resources = contract_names_after(warning, "zzop://contract/");
    let commands = contract_names_after(warning, "`zzop contract ");
    assert!(
        !resources.is_empty() && !commands.is_empty(),
        "the warning must name the contract on BOTH surfaces — an MCP host cannot run a CLI \
         subcommand, so a CLI-only spelling hands half this message's audience nothing to type: \
         {warning}"
    );
    assert_eq!(
        resources, commands,
        "the two dialects name different documents, so one audience is being sent somewhere the other \
         is not: {warning}"
    );
    for doc in &resources {
        assert!(
            names.contains(doc),
            "the warning sends the reader to `zzop://contract/{doc}` (`zzop contract {doc}`), which \
             this binary does not serve. Served: {names:?}\nWarning: {warning}"
        );
    }
}

/// Every `<name>` the embedded-contract table serves, read out of that table's own source.
///
/// Two row shapes, both RESOLVED rather than assumed: `name: "envelope-guide"` (a literal) and
/// `name: DISCLOSURE_CONTRACT_NAME` (an identifier), where the identifier is looked up as a
/// `pub const <IDENT>: &str = "<value>";` in the same file. An identifier that cannot be resolved
/// PANICS instead of contributing nothing — a row this parse cannot follow is a served document the
/// assertion above would otherwise silently stop covering.
fn served_contract_names() -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../summary/src/contracts.rs");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read the contract table at {}: {e}", path.display()));
    let mut out = Vec::new();
    for raw in src.lines() {
        let Some(rest) = raw.trim().strip_prefix("name: ") else {
            continue;
        };
        let rest = rest.trim_end_matches(',');
        if let Some(lit) = rest.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
            out.push(lit.to_string());
            continue;
        }
        let needle = format!("pub const {rest}: &str = \"");
        let value = src
            .split_once(&needle)
            .and_then(|(_, after)| after.split_once('"'))
            .map(|(v, _)| v.to_string())
            .unwrap_or_else(|| {
                panic!(
                    "the contract table serves a row named by `{rest}`, and no matching \
                     `pub const ... : &str = \"...\";` was found beside it. A row this parse cannot \
                     follow is a document nothing here checks — re-point it in the commit that moved it."
                )
            });
        out.push(value);
    }
    out
}

/// The `<name>` following each occurrence of `open`, taken while the characters can still belong to a
/// contract name. Deduplicated and sorted, so the two dialects compare as SETS.
fn contract_names_after(text: &str, open: &str) -> Vec<String> {
    // The warning is one `\`-continued Rust literal, so a name can arrive split across a line break
    // plus indentation. Collapse whitespace first — the same normalization `host_vocabulary.rs` does,
    // and for the same measured reason: without it a wrapped name reads as a truncated one and this
    // pin becomes a false RED.
    let norm = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = norm[from..].find(open) {
        let at = from + rel + open.len();
        let name: String = norm[at..]
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
            .collect();
        if !name.is_empty() {
            out.push(name);
        }
        from = at;
    }
    out.sort();
    out.dedup();
    out
}
