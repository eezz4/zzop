//! Contract 22 — the Java lane's EVIDENCE LADDER in `docs/rules/catalog.md` is derived, never asserted.
//!
//! # The defect (2026-08-13)
//!
//! The `security` pack's "Scope of validation for the Java lane" paragraph published three claims. One
//! of them — the rule count admitting `.java` — was already machine-checked by
//! `catalog_sync::every_catalog_language_admission_claim_matches_what_the_packs_matchers_admit`. The
//! other two were prose:
//!
//! * *"All 18 are exercised end-to-end by the committed detection benchmark"* — a SET claim about
//!   `cases/EXPECTED.jsonc` that nothing compared to the pack.
//! * *"13 of the 18 carry a Java firing/non-firing pair"* — whose stated measuring stick was "which
//!   `rules/dsl/security/*.rs` test FILES write a `.java` path". That method cannot produce 13: only
//!   five files write a `.java` path at all, because one file covers many rules. The number was right
//!   for a DIFFERENT property (13 rules are NAMED by some Java test) and wrong by two for the one it
//!   spelled: measured here, 11 carry both a firing and a non-firing Java case, and `hardcoded-secret`
//!   and `weak-cipher` carry firing-only Java tests.
//!
//! This paragraph is not ordinary prose. `docs/rules/catalog.md` is `include_str!`'d verbatim by
//! `crates/summary/src/contracts.rs` and served as the `zzop://contract/rule-catalog` MCP resource, so
//! an unverifiable sentence here is quoted by agents that have no checkout to check it against.
//!
//! # The attribution axis, and why it needed nothing new
//!
//! "Which rule does this test cover" is answered by OBSERVATION, not declaration: a test that writes a
//! `.java` fixture and calls `hits(&out, "<rule id>")` is that rule's Java evidence, and the assertion
//! it makes on the result says whether it is a firing or a non-firing case. That convention was already
//! universal in the pack when this contract was written — every `#[test]` in `rules/dsl/security/*.rs`
//! named at least one rule id that way, with no competing spelling — so the attribution cost one
//! extraction and zero edits to the tests. A declared mapping (an attribute, a constant, a comment
//! convention) was rejected on the doctrine this repo already applies to rule metadata: a field nothing
//! binds is hardcoding with a nicer address. Module names were rejected on measurement — 4 of the
//! pack's 23 test modules are named after a rule id, so the axis simply does not exist.
//!
//! The one way this reading could go quietly thin is a `.java` fixture test that names no rule at all;
//! that is [`every_java_fixture_test_names_the_rules_it_judges`], which is what makes the convention a
//! contract rather than a habit.
//!
//! # Scope boundary — read before citing a green run
//!
//! * Subject: the `security` pack alone, which is the pack whose catalog paragraph makes these claims.
//! * The assertion vocabulary is CLOSED and small (`.is_empty()`, `.len(), N`, and the same two through
//!   a `let` binding). An unrecognized shape is a hard failure, not a silent skip — a classifier that
//!   quietly drops what it cannot read would under-count exactly where a new test style appears.
//! * This proves a Java unit test EXISTS and which direction it asserts. It says nothing about whether
//!   the fixture is realistic; the paragraph's own closing sentences own that limit.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::load_all_packs;
use crate::path_anchor_pin::file_pattern;

/// The pack whose catalog paragraph makes the claims below.
const PACK: &str = "security";

/// Probe paths a `.java` file can plausibly sit at. A `file_pattern` is EVALUATED against these rather
/// than text-matched, because `(?i)\.java$` and an anchored `(^|/)services/.+\.java$` are the same
/// question only to a regex engine. Several layouts, because a thin probe silently drops any rule whose
/// pattern anchors on a directory segment — the failure mode that reported the Python widening one rule
/// short on 2026-08-12.
const JAVA_PROBES: &[&str] = &[
    "A.java",
    "src/main/java/com/example/svc/A.java",
    "services/A.java",
    "src/main/java/com/example/controllers/A.java",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn catalog_text() -> String {
    fs::read_to_string(repo_root().join("docs/rules/catalog.md")).expect("catalog.md is readable")
}

/// The catalog's blockquote continuations (`\n> `) folded away and whitespace collapsed, so a claim
/// sentence can be matched without encoding today's line wrapping into the regex — a reflow would
/// otherwise silently turn every check below into a no-op.
fn flattened_catalog() -> String {
    let text = catalog_text().replace("\r\n", "\n").replace("\n>", " ");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every `security` rule whose compiled `file_pattern` admits at least one [`JAVA_PROBES`] path.
fn java_admitting_rules() -> BTreeSet<String> {
    let packs = load_all_packs();
    let pack = packs
        .iter()
        .find(|p| p.id == PACK)
        .unwrap_or_else(|| panic!("the `{PACK}` pack no longer loads from rules/dsl"));
    let admitting: BTreeSet<String> = pack
        .rules
        .iter()
        .filter(|r| {
            regex::Regex::new(file_pattern(&r.matcher))
                .is_ok_and(|re| JAVA_PROBES.iter().any(|p| re.is_match(p)))
        })
        .map(|r| r.id.clone())
        .collect();
    assert!(
        admitting.len() >= 10,
        "only {} `{PACK}` rule(s) admit a Java probe path — either the pack stopped loading or the \
         probe set went thin, and every claim in this file would then be judged against a population \
         that is not the Java lane",
        admitting.len()
    );
    admitting
}

// ---------------------------------------------------------------------------------------------
// Rung 1 — the detection benchmark
// ---------------------------------------------------------------------------------------------

/// The `security` rule ids anchored on a line of the `java/` tree in `cases/EXPECTED.jsonc`.
///
/// Parsed line-wise rather than as JSON: the file is JSONC with heavy commentary, and the only shape
/// needed is `"java/<path>:<line>": [ids]`. Reading stops at the first `"<section>": [` / `{` key
/// (`benign`, `gap`, `untracked`), so a GAP entry — which means "this does NOT fire today" — can never
/// be counted as evidence that it does.
fn benchmark_anchored_security_rules() -> BTreeSet<String> {
    let text = fs::read_to_string(repo_root().join("cases/EXPECTED.jsonc"))
        .expect("cases/EXPECTED.jsonc is readable");
    let section = regex::Regex::new(r#"^\s*"[a-z]+"\s*:\s*[\[{]"#).expect("static regex");
    let key = regex::Regex::new(r#"^\s*"java/[^"]+"\s*:\s*\[(?<ids>[^\]]*)\]"#).expect("static");
    let id = regex::Regex::new(r#""security/(?<id>[a-z0-9-]+)""#).expect("static regex");

    let mut out = BTreeSet::new();
    for line in text.lines() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        if section.is_match(line) {
            break;
        }
        if let Some(caps) = key.captures(line) {
            for m in id.captures_iter(&caps["ids"]) {
                out.insert(m["id"].to_string());
            }
        }
    }
    out
}

/// The paragraph's first rung: every rule that admits `.java` is scored by the committed benchmark.
///
/// Both directions matter and neither subsumes the other. A rule admitting `.java` with no anchored
/// line has NO end-to-end Java evidence while the paragraph says it does — that is the over-claim. An
/// anchored line naming a rule that no longer admits `.java` is a permanent false negative in the
/// score, which the benchmark itself would report but nobody would connect to this sentence.
#[test]
fn every_java_admitting_rule_is_anchored_in_the_detection_benchmark() {
    let admitting = java_admitting_rules();
    let anchored = benchmark_anchored_security_rules();

    let stated = one_number(
        r"All (?<n>\d+) are exercised end-to-end by the committed detection benchmark",
        "All N are exercised end-to-end by the committed detection benchmark",
    );
    assert_eq!(
        stated,
        admitting.len(),
        "catalog.md's Java-lane paragraph says all {stated} are exercised end-to-end, but {} \
         `{PACK}` rules admit `.java`",
        admitting.len()
    );

    let unscored: Vec<&String> = admitting.difference(&anchored).collect();
    assert!(
        unscored.is_empty(),
        "{unscored:?} admit `.java` but anchor no line of the `java/` tree in cases/EXPECTED.jsonc — \
         the catalog says all {stated} are exercised end-to-end by the benchmark, which for these is \
         false. Add the expectation, or narrow the sentence in the same commit."
    );
    let stale: Vec<&String> = anchored.difference(&admitting).collect();
    assert!(
        stale.is_empty(),
        "cases/EXPECTED.jsonc anchors {stale:?} on a `java/` line, but their file_pattern admits no \
         Java probe path — a scored expectation that can never be met"
    );
}

// ---------------------------------------------------------------------------------------------
// Rung 2 — per-rule Java UNIT tests, attributed by observation
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct Evidence {
    firing: BTreeSet<String>,
    non_firing: BTreeSet<String>,
}

/// `(file name, source)` for every co-located test module of the `security` pack. `security.rs` is the
/// harness (it defines `hits`/`scan`/`TempDir` and declares the modules), not a test module.
fn pack_test_sources() -> Vec<(String, String)> {
    let dir = repo_root().join("rules/dsl").join(PACK);
    let harness = format!("{PACK}.rs");
    let mut out: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .filter(|n| n.ends_with(".rs") && *n != harness)
        .map(|n| {
            let text = fs::read_to_string(dir.join(&n)).expect("test module is readable");
            (n, text)
        })
        .collect();
    out.sort();
    assert!(
        out.len() >= 10,
        "only {} test module(s) found under rules/dsl/{PACK} — the extraction lost its directory and \
         every count below would be measured against almost nothing",
        out.len()
    );
    out
}

/// One `#[test]` body per element, paired with its function name.
fn test_bodies(source: &str) -> Vec<(String, &str)> {
    let name = regex::Regex::new(r"fn\s+(?<name>\w+)").expect("static regex");
    source
        .split("#[test]")
        .skip(1)
        .map(|body| {
            let n = name
                .captures(body)
                .map(|c| c["name"].to_string())
                .unwrap_or_else(|| "<unnamed>".to_string());
            (n, body)
        })
        .collect()
}

/// Whether this test body writes a `.java` fixture to disk — the marker that makes its assertions Java
/// evidence rather than evidence on some other language.
fn writes_a_java_fixture(body: &str) -> bool {
    let re = regex::Regex::new(r#"dir\.write\(\s*"[^"\n]+\.java""#).expect("static regex");
    re.is_match(body)
}

/// Every `hits(&out, "<id>")` call in `body`, as `(rule id, fires)`. The recognized assertion shapes
/// are the only ones the pack uses; anything else returns `Err` with the offending fragment, because a
/// classifier that silently skipped what it could not read would under-count exactly where a new test
/// style appears.
fn classify(body: &str) -> Result<Vec<(String, bool)>, String> {
    let call = regex::Regex::new(
        r#"(?:let\s+(?<bind>\w+)\s*=\s*)?hits\(&out,\s*"(?<id>[a-z0-9-]+)"\)\s*(?:(?<empty>\.is_empty\(\))|\.len\(\)\s*,\s*(?<n>\d+))?"#,
    )
    .expect("static regex");

    let mut out = Vec::new();
    for caps in call.captures_iter(body) {
        let id = caps["id"].to_string();
        let fires = if caps.name("empty").is_some() {
            false
        } else if let Some(n) = caps.name("n") {
            n.as_str() != "0"
        } else if let Some(bind) = caps.name("bind") {
            // `let h = hits(&out, "id");` — the verdict is the first assertion made about `h` after it.
            let rest = &body[caps.get(0).expect("group 0").end()..];
            let asserted = regex::Regex::new(&format!(
                r"\b{}\s*(?:(?<empty>\.is_empty\(\))|\.len\(\)\s*,\s*(?<n>\d+))",
                regex::escape(bind.as_str())
            ))
            .expect("escaped binding");
            match asserted.captures(rest) {
                Some(c) if c.name("empty").is_some() => false,
                Some(c) => &c["n"] != "0",
                None => {
                    return Err(format!(
                        "`{}` binds `hits(&out, \"{id}\")` and is never asserted on",
                        bind.as_str()
                    ))
                }
            }
        } else {
            return Err(format!(
                "bare `hits(&out, \"{id}\")` with no recognized assertion"
            ));
        };
        out.push((id, fires));
    }
    Ok(out)
}

/// Rule id -> the Java tests that assert it fires / does not fire, each named `file::fn` so a rule
/// asserted twice inside one test is still one piece of evidence.
fn java_unit_evidence() -> BTreeMap<String, Evidence> {
    let mut out: BTreeMap<String, Evidence> = BTreeMap::new();
    for (file, source) in pack_test_sources() {
        for (name, body) in test_bodies(&source) {
            if !writes_a_java_fixture(body) {
                continue;
            }
            let classified = classify(body).unwrap_or_else(|e| {
                panic!(
                    "rules/dsl/{PACK}/{file}::{name}: {e} — extend `classify`'s assertion vocabulary \
                     in the same commit as the new test shape, or the catalog's Java ladder silently \
                     drops this test"
                )
            });
            for (id, fires) in classified {
                let slot = out.entry(id).or_default();
                let at = format!("{file}::{name}");
                if fires {
                    slot.firing.insert(at);
                } else {
                    slot.non_firing.insert(at);
                }
            }
        }
    }
    out
}

/// Parses a single `\d+` out of the flattened catalog with `pattern`, failing loudly (never silently
/// skipping) when the sentence was reworded out from under it.
fn one_number(pattern: &str, shape: &str) -> usize {
    let text = flattened_catalog();
    let re = regex::Regex::new(pattern).expect("static regex");
    let caps = re.captures(&text).unwrap_or_else(|| {
        panic!(
            "docs/rules/catalog.md carries no sentence of the shape \"{shape}\" — it was reworded, and \
             this contract would then pass vacuously. Restore the shape or re-point the regex in the \
             same commit."
        )
    });
    caps["n"].parse().expect("digits")
}

/// The backtick-quoted rule ids listed after `label` in the Java-lane paragraph.
fn listed_after(label: &str) -> BTreeSet<String> {
    let text = flattened_catalog();
    let re = regex::Regex::new(&format!(r"{}(?<list>[^.]*)\.", regex::escape(label)))
        .expect("escaped label");
    let caps = re
        .captures(&text)
        .unwrap_or_else(|| panic!("docs/rules/catalog.md carries no \"{label}\" list — restore it or re-point this contract in the same commit"));
    regex::Regex::new(r"`(?<id>[a-z0-9-]+)`")
        .expect("static regex")
        .captures_iter(&caps["list"])
        .map(|c| c["id"].to_string())
        .collect()
}

/// The paragraph's second rung: how many of the Java-admitting rules carry a per-rule Java UNIT test,
/// and how many of those carry both directions. Both counts AND both shortfall lists are asserted —
/// a count alone can stay true while the names behind it rotate.
#[test]
fn the_catalogs_java_unit_test_ladder_matches_what_the_pack_tests_carry() {
    let admitting = java_admitting_rules();
    let evidence = java_unit_evidence();

    let orphans: Vec<&String> = evidence
        .keys()
        .filter(|id| !admitting.contains(*id))
        .collect();
    assert!(
        orphans.is_empty(),
        "{orphans:?} are asserted on a `.java` fixture but their file_pattern admits no Java probe \
         path — the test cannot be testing what it looks like it tests"
    );

    let named: BTreeSet<&String> = evidence.keys().collect();
    let paired: BTreeSet<&String> = evidence
        .iter()
        .filter(|(_, e)| !e.firing.is_empty() && !e.non_firing.is_empty())
        .map(|(id, _)| id)
        .collect();

    let re = r"(?<n>\d+) of the (?<total>\d+) are named by at least one Java unit test, and (?<paired>\d+) of those carry both a firing and a non-firing Java case";
    let text = flattened_catalog();
    let caps = regex::Regex::new(re).expect("static regex").captures(&text).unwrap_or_else(|| {
        panic!(
            "docs/rules/catalog.md carries no \"N of the M are named by at least one Java unit test, \
             and P of those carry both a firing and a non-firing Java case\" sentence — restore the \
             shape or re-point this contract in the same commit"
        )
    });
    let (stated_named, stated_total, stated_paired) = (
        caps["n"].parse::<usize>().expect("digits"),
        caps["total"].parse::<usize>().expect("digits"),
        caps["paired"].parse::<usize>().expect("digits"),
    );

    assert_eq!(
        stated_total,
        admitting.len(),
        "the ladder's denominator is not the Java-admitting rule count"
    );
    assert_eq!(
        stated_named,
        named.len(),
        "catalog.md says {stated_named} of the Java-admitting rules are named by a Java unit test; \
         measured {}: {named:?}",
        named.len()
    );
    assert_eq!(
        stated_paired,
        paired.len(),
        "catalog.md says {stated_paired} carry both a firing and a non-firing Java case; measured {}: \
         {paired:?}",
        paired.len()
    );

    let firing_only: BTreeSet<String> = named
        .iter()
        .filter(|id| !paired.contains(**id))
        .map(|id| (*id).clone())
        .collect();
    assert_eq!(
        listed_after("Firing-only, no Java near-miss:"),
        firing_only,
        "catalog.md's firing-only list is not the set measured out of the pack's Java tests"
    );

    let untested: BTreeSet<String> = admitting
        .difference(&evidence.keys().cloned().collect())
        .cloned()
        .collect();
    assert_eq!(
        listed_after("the whole of their Java evidence:"),
        untested,
        "catalog.md's no-Java-unit-test list is not the set measured out of the pack's Java tests"
    );
}

/// What makes the attribution above a CONTRACT rather than a habit: a `.java` fixture test that names
/// no rule id is invisible to every count in this file, and nothing else in the repo would notice.
/// Rejecting that shape is cheaper than any declared mapping and cannot itself go stale.
#[test]
fn every_java_fixture_test_names_the_rules_it_judges() {
    let call = regex::Regex::new(r#"hits\(&out,\s*"[a-z0-9-]+"\)"#).expect("static regex");
    let mut java_tests = 0usize;
    let mut silent: Vec<String> = Vec::new();
    for (file, source) in pack_test_sources() {
        for (name, body) in test_bodies(&source) {
            if !writes_a_java_fixture(body) {
                continue;
            }
            java_tests += 1;
            if !call.is_match(body) {
                silent.push(format!("{file}::{name}"));
            }
        }
    }
    assert!(
        java_tests >= 20,
        "only {java_tests} `.java` fixture test(s) found in rules/dsl/{PACK} — the extraction needle \
         (`dir.write(\"….java\"`) stopped matching how these tests are written, and the catalog's Java \
         ladder is being derived from almost nothing"
    );
    assert!(
        silent.is_empty(),
        "{silent:?} write a `.java` fixture but call `hits(&out, \"<rule id>\")` for no rule — the \
         rule<->test attribution behind docs/rules/catalog.md's Java ladder is read off exactly that \
         call, so such a test is evidence for nothing. Name the rule it judges."
    );
}
