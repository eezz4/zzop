//! The DISCLOSURE FOLD (2026-07-29) — the run output carries the blindness registry's SHAPE (how many
//! classes, how many of them are only partially detected or not detected at all, and where the full
//! text is), never the text itself.
//!
//! # What this file has to prove, and why each part is load-bearing
//! The governing decision (coverage-disclosure 1c) says a run must report what class was checked and
//! what was not detected, because "a disclosure an agent has to notice is not a disclosure". Folding
//! stays inside that ONLY while three things hold, so all three are pinned here rather than left to
//! review:
//!   1. the reply still says gaps exist and how many (`the_analyze_reply_carries_the_fold_*`);
//!   2. the counts are DERIVED from the same registry the full text is rendered from, never a second
//!      hand-maintained list — so a class added to the engine's registry moves both, with no edit here
//!      or in the shaper (`the_fold_counts_and_the_served_document_read_the_same_registry`);
//!   3. the pointer the reply prints actually RESOLVES on the contract lane
//!      (`the_pointer_the_reply_prints_resolves_*`). A pointer to nothing is worse than no pointer.
//!
//! # Where each half of (2) is proven, and why the chain has no skipped layer
//! This crate does not reach `zzop-engine` — not in shipped code and not in tests. The layering is
//! product -> summary -> facade, and `docs/contracts/surface-parity.json`'s whole guard rests on facade
//! fields and summary shaping living in different crates, so a test-only shortcut past the facade would
//! erode the same boundary from the other side. The derivation is therefore sealed as a CHAIN:
//!   - engine (`crates/engine/src/disclosure/document.rs`): the rendered document carries every class's
//!     id, group, status and full summary, and states the tallies it was built from;
//!   - here: the REPLY's counts equal `zzop_facade::disclosure_counts()` (the registry's own tally), and
//!     the SERVED document is byte-identical to `zzop_facade::disclosure_contract_text()`, and the
//!     counts the served document states are the counts the reply prints.
//!
//! Add a class to the registry and every link moves at once; break any link and one of the two files
//! fails. Neither can be satisfied by a hand-maintained list.

use std::fs;

use zzop_summary::contracts::{self, DISCLOSURE_CONTRACT_NAME};

fn default_filters() -> zzop_summary::FindingFilters {
    zzop_summary::FindingFilters::new(None, None, None).expect("no-filter view always constructs")
}

fn tmp_tree(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("zzop-fold-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("zzop.config.jsonc"),
        zzop_config::template::CONFIG_TEMPLATE_JSONC,
    )
    .unwrap();
    fs::write(
        dir.join("api.ts"),
        "export const load = () => fetch('/api/users');\n",
    )
    .unwrap();
    dir
}

/// The live registry's own tallies, reached through the layer this crate is allowed to reach — the ONE
/// truth every folded reply must agree with.
fn live_counts() -> (usize, usize, usize, usize) {
    zzop_facade::disclosure_counts()
}

/// A paragraph that exists ONLY in the registry's prose — the kind of byte sequence that used to ride
/// every reply and must now appear only in the contract document. Taken from the rendered document
/// itself (its last `### ` section's body), so it stays real prose without this crate enumerating
/// classes: a rewrite of any one class's text cannot leave this pin asserting a string nobody writes.
fn a_class_summary() -> String {
    let text = zzop_facade::disclosure_contract_text();
    let last_section = text
        .rsplit_once("\n\n")
        .and_then(|(before, _closing_line)| before.rsplit_once("\n\n"))
        .expect("the document ends with a class body followed by its closing line")
        .1
        .to_string();
    assert!(
        last_section.len() > 100 && !last_section.starts_with('#'),
        "expected a class's prose body, got: {last_section}"
    );
    last_section
}

/// Every assertion that defines "folded", applied to one reply's `disclosure` block. Shared so the
/// three surfaces cannot drift into three different folds.
fn assert_folded(disclosure: &serde_json::Value, surface: &str) {
    assert!(
        disclosure.is_object(),
        "{surface}: `disclosure` must be the folded summary object, not the registry array — the full \
         text moved to the contract lane: {disclosure}"
    );
    let (classes, asserted, partial, not_yet) = live_counts();
    assert_eq!(disclosure["classes"], classes, "{surface}: class count");
    assert_eq!(
        disclosure["asserted"], asserted,
        "{surface}: asserted count"
    );
    assert_eq!(disclosure["partial"], partial, "{surface}: partial count");
    assert_eq!(
        disclosure["notYetDetected"], not_yet,
        "{surface}: notYetDetected count"
    );
    // The doctrine's floor: the run must still say, without being asked, that gaps EXIST and how big
    // they are. Counts that summed to nothing would be a fold that deleted the disclosure.
    assert!(
        partial + not_yet > 0 && classes > 0,
        "{surface}: a fold that reports no gaps is not a disclosure"
    );
    assert_eq!(
        disclosure["resource"],
        serde_json::json!(format!("zzop://contract/{DISCLOSURE_CONTRACT_NAME}")),
        "{surface}: the reply must name the resource that serves the full text"
    );
    assert_eq!(
        disclosure["command"],
        serde_json::json!(format!("zzop contract {DISCLOSURE_CONTRACT_NAME}")),
        "{surface}: and the terminal lane that serves the same bytes"
    );
    assert!(
        disclosure["note"].as_str().is_some_and(|n| !n.is_empty()),
        "{surface}: the fold must say in words what the counts are about"
    );
    let bytes = serde_json::to_string(disclosure)
        .expect("a fold is plain JSON")
        .len();
    assert!(
        bytes < 800,
        "{surface}: the fold is {bytes} bytes — it is supposed to be roughly ten lines, and the whole \
         point is that it is not the 10KB registry"
    );
}

/// The reply must not carry the registry's PROSE anywhere — the tax this fold exists to remove.
fn assert_no_class_prose(reply: &str, surface: &str) {
    assert!(
        !reply.contains(&a_class_summary()),
        "{surface}: a blindness-class summary paragraph is still riding the reply"
    );
}

/// `analyze_repo` / `zzop analyze` — the reply the fold was measured on.
#[test]
fn the_analyze_reply_carries_the_fold_not_the_registry_text() {
    let dir = tmp_tree("analyze");
    let out =
        zzop_summary::analyze_summary(Some(&dir.display().to_string()), None, &default_filters())
            .expect("analyze must succeed on a configured tree");
    let v: serde_json::Value = serde_json::from_str(&out).expect("a reply is JSON");
    assert_folded(&v["disclosure"], "analyze_repo");
    assert_no_class_prose(&out, "analyze_repo");
}

/// `cross_repo` / `zzop cross` — the join reply, whose registry block is the other half.
#[test]
fn the_cross_reply_folds_the_same_way() {
    let fe = tmp_tree("cross-fe");
    let be = tmp_tree("cross-be");
    let paths = vec![fe.display().to_string(), be.display().to_string()];
    let out = zzop_summary::cross_summary(&paths, None, &default_filters())
        .expect("cross must succeed on two configured trees");
    let v: serde_json::Value = serde_json::from_str(&out).expect("a reply is JSON");
    assert_folded(&v["disclosure"], "cross_repo");
    assert_no_class_prose(&out, "cross_repo");
}

/// `check_endpoint` / `zzop endpoint` — the third reply that forwards the registry from the analysis.
#[test]
fn the_endpoint_reply_folds_the_same_way() {
    let fe = tmp_tree("endpoint");
    let out = zzop_summary::endpoint_summary("users", Some(&fe.display().to_string()), &[], None)
        .expect("endpoint must succeed on a configured tree");
    let v: serde_json::Value = serde_json::from_str(&out).expect("a reply is JSON");
    assert_folded(&v["disclosure"], "check_endpoint");
    assert_no_class_prose(&out, "check_endpoint");
}

/// THE DERIVATION SEAL, summary-layer half. The reply's counts, the registry's own tally and the
/// SERVED document must be three views of ONE registry: add a class and all three move, with no edit
/// anywhere else. If the fold ever becomes a hand-maintained tally, or the table starts serving a
/// committed copy of the prose instead of the live render, this fails — which is the whole reason
/// folding is allowed to stay inside decision 1c. (That the render is COMPLETE — every class's id,
/// group, status and full paragraph — is sealed in the crate that owns the registry:
/// `crates/engine/src/disclosure/document.rs`.)
#[test]
fn the_fold_counts_and_the_served_document_read_the_same_registry() {
    let doc = contracts::find(DISCLOSURE_CONTRACT_NAME)
        .expect("the disclosure contract document must be served");
    let (classes, asserted, partial, not_yet) = live_counts();

    // The table serves the LIVE render, byte for byte — not a snapshot of it taken at some past commit.
    assert_eq!(
        doc.content,
        zzop_facade::disclosure_contract_text(),
        "the contract table is serving something other than the registry's own render"
    );
    assert_eq!(doc.mime, "text/markdown");

    // The document states the same tallies the reply carries, in its own words, so a reader who
    // followed the pointer can check the numbers they were given against the text they came from.
    for stated in [
        format!("{classes} classes"),
        format!("{asserted} asserted"),
        format!("{partial} partial"),
        format!("{not_yet} notYetDetected"),
    ] {
        assert!(
            doc.content.contains(&stated),
            "the served document must state {stated:?} — the reply's counts and this text have to be \
             checkable against each other"
        );
    }

    // And the reply really is reading that same tally, end to end through the real shaper.
    let dir = tmp_tree("seal");
    let out =
        zzop_summary::analyze_summary(Some(&dir.display().to_string()), None, &default_filters())
            .expect("analyze must succeed on a configured tree");
    let v: serde_json::Value = serde_json::from_str(&out).expect("a reply is JSON");
    assert_eq!(v["disclosure"]["classes"], classes);

    // A status token the fold does not know would silently vanish from the three counts while
    // `classes` kept growing. Summing them shut is what makes "the counts move" mean "all of them".
    assert_eq!(
        asserted + partial + not_yet,
        classes,
        "a registry status outside {{asserted, partial, notYetDetected}} is uncounted by the fold — \
         add it to the fold and to this seal in the same commit"
    );
}

/// A `resource` field naming something the contract lane cannot serve is worse than no pointer at all.
/// Resolved through the SAME `contracts::find` both surfaces use (`zzop contract <name>` and MCP
/// `resources/read zzop://contract/<name>`), and asserted to be LISTED as well as readable — a document
/// that resolves but is invisible in `resources/list` / `zzop contract` is only half a pointer.
#[test]
fn the_pointer_the_reply_prints_resolves_on_the_contract_lane() {
    let dir = tmp_tree("pointer");
    let out =
        zzop_summary::analyze_summary(Some(&dir.display().to_string()), None, &default_filters())
            .expect("analyze must succeed on a configured tree");
    let v: serde_json::Value = serde_json::from_str(&out).expect("a reply is JSON");

    let uri = v["disclosure"]["resource"]
        .as_str()
        .expect("the fold prints a resource uri")
        .to_string();
    let name = uri
        .strip_prefix(contracts::URI_PREFIX)
        .unwrap_or_else(|| panic!("{uri} is not a {} uri", contracts::URI_PREFIX));
    let doc = contracts::find(name)
        .unwrap_or_else(|| panic!("the reply's own pointer {uri} does not resolve"));
    assert!(
        !doc.content.trim().is_empty(),
        "the pointer resolves to an empty document"
    );
    assert!(
        contracts::names().any(|n| n == name),
        "{name} is readable but not listed — half a pointer"
    );

    // The terminal lane's spelling is the same name, so the printed command is runnable verbatim.
    let command = v["disclosure"]["command"]
        .as_str()
        .expect("the fold prints a command");
    assert_eq!(command, format!("zzop contract {name}"));
}
