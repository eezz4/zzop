//! Contract — every `Finding.data` key a SHIPPED consumer reads is a key some rule actually emits.
//!
//! # The defect this exists to notice
//! `crates/summary/src/graph/dep.rs` read `data["members"]` for cycle membership. No rule in this repo
//! has ever emitted `members`; `rules-graph`'s `circular_findings` emits `cycle`. The read therefore
//! always found nothing and cycle membership silently collapsed to the finding's anchor — on a real tree
//! a 510-file cycle drew as one file, on a lane whose own legend promises a hexagon per member.
//!
//! It survived for months because the two test files covering that lane HAND-WROTE fixtures spelling the
//! same invented key. A fixture is authored by the same person as the code it tests, so it inherits the
//! same wrong assumption, and no assertion over it can ever see the producer and the consumer disagree.
//! Both fixtures are now one, built by the rule itself (`crates/summary/src/graph/dep/tests.rs`'s
//! `circular_finding`), which closes that one lane. This contract closes the CLASS: `data` is a free-form `serde_json::Value` at the
//! engine boundary, so nothing in the type system relates the key a rule writes to the key a consumer
//! reads, and every future consumer is one typo away from the same silent half-answer.
//!
//! # What this checks, and therefore what it cannot see
//! A guard's subject definition IS its blind-spot definition, so this section is the second half of the
//! contract rather than a caveat: a green here vouches for exactly the property below and nothing wider.
//! It is a SPELLING check over source text, in one direction:
//!
//!   *every top-level `data` key read by shipped code under [`consumer_roots`] is spelled as a JSON
//!   object key somewhere under [`producer_roots`].*
//!
//! Three consequences, stated rather than discovered later:
//! - It cannot tell a key emitted by rule A from the same key read on a finding of rule B. Relating
//!   key to RULE would need a per-rule table of `data` shapes, which is the thing `Finding::data`'s own
//!   doc refuses (a thirteenth rule leaves such a table silently short). A cross-rule mixup is a
//!   different, rarer defect than "this key exists nowhere".
//! - It cannot see a key assembled at runtime on either side (`format!`, a `&str` variable). Both
//!   directions of that are invisible: a producer that computes its key contributes nothing to the
//!   producer set, and a consumer that computes one is never checked. Today neither side does it —
//!   [`producer_keys`]/[`consumer_census`] are the census, and any future runtime-assembled key is a
//!   hole this contract announces by not covering rather than by failing.
//! - It deliberately accepts a key spelled in a NESTED position of some producer's payload. Tightening
//!   that would mean parsing `json!` bodies structurally for a gain of nothing measured: the key that
//!   caused this contract was spelled in no position at all.
//!
//! # The direction that is NOT asserted, and why
//! The mirror census — keys a rule PRODUCES that no crate reads — is deliberately not a failure here.
//! `data` is a PUBLIC payload: its audience is the analyze reply's reader (a person, an MCP client, a
//! script), not this workspace. "No internal consumer" is the normal, intended state for most of these
//! keys, so asserting it would fire on 115 of the 118 (2026-08-30 census). The count is worth knowing and
//! worth revisiting when a payload budget is in question; it is not worth a red build.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::{collect_rs_files, is_test_source};

/// Where a `Finding.data` payload is BUILT. Both roots verified 2026-08-30 by grepping the workspace for
/// `data: Some(` / `data = Some(` outside tests: every shipped construction site is in one of them —
/// `crates/core/src/dsl/*_scan.rs` (which build the payload for all DSL rules, so a DSL pack has no
/// `data` shape of its own) and the four native rule crates.
fn producer_roots() -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![
        manifest.join("../core/src/dsl"),
        manifest.join("../../rules/native"),
    ]
}

/// Where a `Finding.data` payload is READ. Every crate's `src/`, derived by `read_dir` rather than
/// listed, for the reason `host_vocabulary::shared_src_dirs` states: a hand-listed haystack can only
/// ever cover the crates someone thought to name, and the crate that grows the next bad read is exactly
/// the one nobody thought to name.
fn consumer_roots() -> Vec<PathBuf> {
    let crates_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&crates_root)
        .unwrap_or_else(|e| panic!("{} must be readable ({e})", crates_root.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.is_dir())
        .map(|p| p.join("src"))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Shipped `.rs` under `roots`, tests subtracted. A test fixture spells a payload's keys on purpose, and
/// sometimes spells one it is asserting the ABSENCE of — `crates/core/src/dsl/tests_literal_scan.rs`
/// reads `valueHash` to prove that key is NOT emitted. Counting that as a consumer would make this
/// contract fail on a test whose whole point is the same contract, so the subtraction has to catch it.
///
/// [`is_test_source`] alone does not: this workspace also uses a `tests_`/`test_` FILENAME PREFIX for
/// in-`src` test modules (`tests_literal_scan.rs`, `test_support.rs`), which that shared predicate — a
/// suffix-and-directory rule — was never written to see. The prefix arm is added here rather than in the
/// shared predicate because widening a helper five other contracts depend on would change their subject
/// sets too, silently, in the same commit.
fn shipped_sources(roots: &[PathBuf]) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for root in roots {
        collect_rs_files(root, &mut files);
    }
    files.retain(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        !is_test_source(p) && !name.starts_with("test_") && !name.starts_with("tests_")
    });
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let text = std::fs::read_to_string(&p)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", p.display()));
            (display_path(&p), text)
        })
        .collect()
}

/// A repo-rooted-ish path for offender lists — enough to open the file, without the machine's own
/// directory layout leaking into an assertion message.
fn display_path(path: &Path) -> String {
    let full = path.to_string_lossy().replace('\\', "/");
    match full.rfind("/crates/").or_else(|| full.rfind("/rules/")) {
        Some(i) => full[i + 1..].to_string(),
        None => full,
    }
}

/// Every string literal used as a JSON object key inside a `json!` body, plus every `.insert("k", …)`
/// first argument — the two forms a `data` payload is built with in this workspace.
///
/// Scans the WHOLE producer file rather than only the text at `data: Some(`, and that is the point:
/// `line_scan.rs` binds its payload to a local first (`let data = json!({…}); … data: Some(data)`), so an
/// extraction anchored on the assignment sees a bare identifier and reports zero keys — the exact
/// vacuous-green this file is written to avoid.
fn producer_keys() -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (path, text) in shipped_sources(&producer_roots()) {
        for key in json_object_keys(&text)
            .into_iter()
            .chain(insert_keys(&text))
        {
            out.entry(key).or_default().insert(path.clone());
        }
    }
    out
}

/// Every top-level `data` key read by shipped code (`key -> {file:line}`), plus how many reads each
/// ACCESS FORM accounted for.
///
/// Two forms, because a consumer sees a finding either as raw JSON or as a typed `Finding`:
/// `["data"]["k"]` / `["data"].get("k")` (the graph and summary lanes, which read an analyze reply) and
/// `.data … ["k"]` / `.data … .get("k")` (the engine, which holds `Finding` values). Both are matched by
/// finding the `data` anchor and taking the FIRST string-literal key that follows it within a short
/// window — the window is what keeps `.data` from adopting an unrelated `.get("…")` further down the
/// function.
///
/// The per-form count is the floor this contract needs and a total could not give: the shipped consumer
/// population is three sites, small enough that a plain count floor is either brittle (equal to today's
/// number) or vacuous (`> 0`, satisfied by whichever form still happens to match). One form silently
/// ceasing to match is the exact way this contract would rot into a green that vouches for nothing.
fn consumer_census() -> (
    BTreeMap<String, BTreeSet<String>>,
    BTreeMap<&'static str, usize>,
) {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut per_form: BTreeMap<&'static str, usize> =
        DATA_ANCHORS.iter().map(|a| (*a, 0usize)).collect();
    for (path, text) in shipped_sources(&consumer_roots()) {
        for (key, offset, anchor) in data_key_reads(&text) {
            let line = text[..offset].lines().count();
            out.entry(key).or_default().insert(format!("{path}:{line}"));
            *per_form.entry(anchor).or_default() += 1;
        }
    }
    (out, per_form)
}

/// THE CONTRACT. A consumer key with no producer anywhere is a read that can only ever return nothing —
/// and, as `dep.rs` showed, a lane that then reports a confident half-answer rather than failing.
#[test]
fn every_data_key_a_shipped_consumer_reads_is_a_key_some_rule_emits() {
    let producers = producer_keys();
    let (consumers, per_form) = consumer_census();

    // Floors on BOTH derived subject sets, because either one going empty passes this contract without
    // reading a byte — and the two failure modes read very differently, so they say so separately. The
    // producer number is a floor far under today's census (118 keys), there to fire on a broken walk
    // rather than on ordinary churn.
    assert!(
        producers.len() >= 40,
        "derived only {} producer data key(s) from {:?} — a rule corpus this repo has never had, so \
         this is a broken scan and every consumer below would be vouched for by an empty whitelist",
        producers.len(),
        producer_roots()
    );
    for (form, count) in &per_form {
        assert!(
            *count > 0,
            "the `{form}` access form matched NOTHING under {:?}. Both forms are live in this \
             workspace (`[\"data\"][\"…\"]` in crates/summary's graph lanes, `.data … .get(\"…\")` in \
             crates/engine), so a zero here means the matcher stopped recognizing this workspace's \
             code — not that nobody reads a payload that way any more. Every read of that shape is \
             unchecked until it is fixed. Full census: {consumers:#?}",
            consumer_roots()
        );
    }

    let orphans: Vec<String> = consumers
        .iter()
        .filter(|(key, _)| !producers.contains_key(*key))
        .map(|(key, sites)| format!("{key:?} read at {sites:?}"))
        .collect();
    assert!(
        orphans.is_empty(),
        "these `Finding.data` keys are READ by shipped code and EMITTED by no rule, so the read \
         returns nothing on every run — and a consumer that treats \"absent\" as \"empty\" reports a \
         confident wrong answer rather than failing:\n  {}\n\nFix the CONSUMER to spell the key its \
         producer actually emits. Adding the key to the producer as an alias is the wrong repair: one \
         fact carried by two transports means every later consumer has to know which producers spell \
         which, and the payload grows a duplicate of something it already carries.",
        orphans.join("\n  ")
    );
}

// ---------------------------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------------------------

/// Keys inside every `json!( … )` body in `text`. A key is a string literal followed (past whitespace)
/// by a single `:` — `::` is a path separator, never a key.
fn json_object_keys(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let needle: Vec<char> = "json!".chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(start) = find_chars(&chars, &needle, i) {
        let Some(open) = (start..chars.len()).find(|&j| chars[j] == '(') else {
            break;
        };
        let end = balanced_end(&chars, open, '(', ')');
        out.extend(string_keys(&chars[open..end]));
        i = end.max(start + 1);
    }
    out
}

/// `.insert("key", …)` first arguments — the map-building form of a payload.
fn insert_keys(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let needle: Vec<char> = ".insert(".chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(start) = find_chars(&chars, &needle, i) {
        let after = start + needle.len();
        if let Some(key) = leading_string_literal(&chars[after.min(chars.len())..]) {
            out.push(key);
        }
        i = after;
    }
    out
}

fn find_chars(haystack: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (from..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

/// The two spellings that introduce a `data` payload read. `["data"]` is the raw-JSON form (a lane
/// reading an analyze reply); `.data` is the typed form (code holding a `Finding`).
const DATA_ANCHORS: [&str; 2] = ["[\"data\"]", ".data"];

/// Every `data` key read, as `(key, byte_offset_of_the_anchor, anchor)`.
fn data_key_reads(text: &str) -> Vec<(String, usize, &'static str)> {
    const WINDOW: usize = 160;
    let mut out = Vec::new();
    for anchor in DATA_ANCHORS {
        let mut from = 0usize;
        while let Some(rel) = text[from..].find(anchor) {
            let at = from + rel;
            let after = at + anchor.len();
            // `.data` must be a field access, not the prefix of a longer identifier (`.database`).
            let next_is_ident = text[after..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
            if !next_is_ident {
                let end = (after + WINDOW).min(text.len());
                if let Some(key) = first_key_in(&text[after..floor_char_boundary(text, end)]) {
                    out.push((key, at, anchor));
                }
            }
            from = after;
        }
    }
    out
}

/// The first `["k"]` or `.get("k")` in `window`, whichever comes first — `None` if the window holds
/// neither (a `.data` that is passed along rather than indexed, e.g. `redact.rs`'s `data.as_mut()`).
fn first_key_in(window: &str) -> Option<String> {
    let bracket = window.find('[').and_then(|i| {
        let chars: Vec<char> = window[i + 1..].chars().collect();
        leading_string_literal(&chars).map(|k| (i, k))
    });
    let getter = window.find(".get(").and_then(|i| {
        let chars: Vec<char> = window[i + ".get(".len()..].chars().collect();
        leading_string_literal(&chars).map(|k| (i, k))
    });
    match (bracket, getter) {
        (Some((bi, bk)), Some((gi, gk))) => Some(if bi <= gi { bk } else { gk }),
        (Some((_, k)), None) | (None, Some((_, k))) => Some(k),
        (None, None) => None,
    }
}

/// A string literal at the very start of `chars` (whitespace allowed before it), as its content.
fn leading_string_literal(chars: &[char]) -> Option<String> {
    let mut i = 0usize;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if chars.get(i) != Some(&'"') {
        return None;
    }
    i += 1;
    let mut out = String::new();
    while i < chars.len() && chars[i] != '"' {
        if chars[i] == '\\' {
            i += 1;
        }
        out.push(*chars.get(i)?);
        i += 1;
    }
    is_key_like(&out).then_some(out)
}

/// String literals in `chars` that are followed by exactly one `:` — i.e. used as object keys.
fn string_keys(chars: &[char]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != '"' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut lit = String::new();
        while j < chars.len() && chars[j] != '"' {
            if chars[j] == '\\' {
                j += 1;
            }
            if let Some(c) = chars.get(j) {
                lit.push(*c);
            }
            j += 1;
        }
        let mut k = j + 1;
        while k < chars.len() && chars[k].is_whitespace() {
            k += 1;
        }
        if chars.get(k) == Some(&':') && chars.get(k + 1) != Some(&':') && is_key_like(&lit) {
            out.push(lit);
        }
        i = j + 1;
    }
    out
}

/// A JSON object key this workspace would actually write: an identifier-shaped name. Excludes the
/// message text, path fragments and format strings that also sit in these files.
fn is_key_like(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn balanced_end(chars: &[char], open: usize, opener: char, closer: char) -> usize {
    let mut depth = 0usize;
    let mut i = open;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
        } else if c == opener {
            depth += 1;
        } else if c == closer {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
        i += 1;
    }
    chars.len()
}

fn floor_char_boundary(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}
