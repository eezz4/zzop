//! THE LEGEND FOLD (2026-09-01) — the RUN-INVARIANT legends of an analyze-shaped reply ship their
//! full text ONCE, from the `reply-legends` contract document, instead of on every call. How many
//! there are is [`FOLDED_KEYS`]'s to say, never this sentence's: it read "the four" until 2026-09-14,
//! by which time the answer was eight.
//!
//! # What this file has to prove, and why it is two directions rather than one
//! `product/oss-reach.md` states the pair this fold is judged on: *"a reply must not carry the same
//! prose twice"* AND *"everything must still be reachable"*. Either alone is satisfiable by cheating —
//! the first by deleting the sentences, the second by shipping three copies — so both are pinned here,
//! over the SAME derived list:
//!   1. every section of the served document is ABSENT from a real shaped reply
//!      (`the_analyze_reply_carries_no_legend_prose`);
//!   2. every folded key is still THERE, non-empty, and carries a pointer that RESOLVES
//!      (`every_folded_key_still_ships_with_a_resolvable_pointer`);
//!   3. the readings a reader must not get wrong survived the shortening
//!      (`the_short_notes_keep_the_sentences_that_change_what_a_zero_means`).
//!
//! # Why the section bodies are READ OUT of the document rather than listed here
//! A hand-listed set of needles is a list that rots: the day a fifth legend is folded, a test naming
//! four would pass while the fifth's prose rode every reply. Parsing the document the reply POINTS AT
//! makes the population of this test exactly the population of the fold — the same device
//! `disclosure_fold.rs` uses when it takes its needle out of the rendered document instead of naming a
//! class.

use std::fs;

use zzop_summary::contracts::{self, REPLY_LEGENDS_CONTRACT_NAME};

mod reply_needle;

use reply_needle::as_json_body;

/// Which SHAPED REPLY a folded key rides on. One list, two replies since 2026-09-15.
///
/// 🔴 This enum is the fix for a blind spot, not a convenience. Every test in this file read the
/// ANALYZE reply and nothing else, and the fold list is shared across products — so the day
/// `module_map.meaning` joined it, "the key is still on the wire" and "no prose rides the reply" would
/// both have been asserted against a document that never carries that key. Both would have passed.
/// That is the same population failure this repo has now measured ten times: a guard whose subjects
/// come from one list cannot see what the list does not name.
#[derive(Clone, Copy, PartialEq)]
enum Reply {
    Analyze,
    ModuleMap,
}

/// The reply paths the fold covers, spelled as a reader spells them, each beside the reply that
/// carries it. Used ONLY to check that each key is still on the wire — never as the source of the
/// prose needles, which are derived.
const FOLDED_KEYS: &[(Reply, &str)] = &[
    (Reply::Analyze, "coverageGaps.meaning"),
    (Reply::Analyze, "findings.byRuleMeaning"),
    (Reply::Analyze, "findings.shownMeaning"),
    (Reply::Analyze, "findings.byDirectory.meaning"),
    (Reply::Analyze, "nativeAnalysesMeaning"),
    (Reply::Analyze, "packsLoadedMeaning"),
    (Reply::Analyze, "architecture.topRecommendationMeaning"),
    (Reply::Analyze, "architecture.criticalTopMeaning"),
    // The map reply's whole legend is one key, and its dotted name is `module_map.meaning` in the
    // document while the wire path is the bare `meaning` — see `wire_path`.
    (Reply::ModuleMap, "module_map.meaning"),
];

/// A folded key's DOCUMENT name is its reply path prefixed by the reply it belongs to, once more than
/// one reply is in the list. Only the map's key needs the prefix stripped; the analyze keys are spelled
/// the same in both places because that reply was the whole population when the fold was built.
fn wire_path(key: &str) -> &str {
    key.strip_prefix("module_map.").unwrap_or(key)
}

/// 🔴 The two `architecture.*` keys above ride an object that is GIT-GATED — `architecture_summary`
/// returns `None` when a run collected no history, so on the plain [`tmp_tree`] this file used before
/// 2026-09-14 those keys are absent and every assertion about them passes by not running.
///
/// That is not hypothetical: it is how the pair got folded with no wire test at all for a few minutes,
/// and it is the same shape as the vacuous-population failures this repo has now measured six times.
/// [`git_tree`] exists so the needles have something to find, and
/// [`the_architecture_legends_are_actually_on_the_wire_here`] proves the fixture did its job before
/// anything else leans on it.
const GIT_GATED_KEYS: &[&str] = &[
    "architecture.topRecommendationMeaning",
    "architecture.criticalTopMeaning",
];

fn default_filters() -> zzop_summary::FindingFilters {
    zzop_summary::FindingFilters::new(None, None, None).expect("no-filter view always constructs")
}

fn tmp_tree(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("zzop-legend-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("zzop.config.jsonc"),
        zzop_config::template::CONFIG_TEMPLATE_JSONC,
    )
    .unwrap();
    // The `fetch` line is what the io/cross-layer pins want; the credential is what makes this file
    // PRODUCE a finding, which the distribution block below needs (a directory with no finding is not
    // a directory as far as that fold is concerned).
    // 🔴 The credential literals here and below are SPLIT with `concat!`, the convention
    // `rules/dsl/security/vendor_token_committed.rs` established and `check-*` enforces: a contiguous
    // prefix+body in a committed file blocks the next release's branch AND tag push, whether or not
    // the value is synthetic. The bytes the fixture writes are unchanged.
    fs::write(
        dir.join("api.ts"),
        concat!(
            "export const load = () => fetch('/api/users');\n",
            "export const T = \"sk_li",
            "ve_4tYw8nQ1pR6vX3mL0zJdKeRfTgYhUjIkOlPqAsDf\";\n"
        ),
    )
    .unwrap();
    // A SECOND directory, so `findings.byDirectory` has a distribution to report. That block became
    // additive-only on 2026-09-15 (ledger V243) — absent when fewer than two directories hold
    // findings — and it is in the fold list, so a one-directory fixture makes every assertion about
    // its folded note fail. Same shape as `git_tree` two functions down: a key that is conditional on
    // the TREE needs a fixture that produces the condition, or the pins are about nothing.
    fs::create_dir_all(dir.join("lib")).unwrap();
    fs::write(
        dir.join("lib/util.ts"),
        concat!(
            "export const KEY = \"sk_li",
            "ve_9f3Kq2mZx7Lp0WvB4tRnQwErTyUiOpAsDfGhJkLz\";\n"
        ),
    )
    .unwrap();
    dir
}

/// A tree with git HISTORY, which is what makes the reply carry an `architecture` object at all. Same
/// shape `score_population_disclosure.rs` uses, and for the same reason stated there.
fn git_tree(name: &str) -> std::path::PathBuf {
    let dir = tmp_tree(name);
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("git must be runnable for this test")
    };
    git(&["init", "-q", "."]);
    git(&["add", "-A"]);
    git(&[
        "-c",
        "user.email=t@example.com",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "seed",
    ]);
    dir
}

fn reply_of(dir: &std::path::Path) -> (String, serde_json::Value) {
    let out =
        zzop_summary::analyze_summary(Some(&dir.display().to_string()), None, &default_filters())
            .expect("analyze must succeed on a configured tree");
    let v = serde_json::from_str(&out).expect("a reply is JSON");
    (out, v)
}

fn analyze_reply(name: &str) -> (String, serde_json::Value) {
    reply_of(&git_tree(name))
}

/// The MODULE MAP reply over the same fixture, at the grain a caller reaches for first.
///
/// The map needs no git history — it is a fold of the import graph — but it is built on `git_tree`
/// anyway so that one fixture answers for both replies and neither can drift onto a tree the other
/// never saw.
fn map_reply(dir: &std::path::Path) -> (String, serde_json::Value) {
    let out = zzop_summary::module_map(&[dir.display().to_string()], None, 1)
        .expect("the module map must succeed on a configured tree");
    let v = serde_json::from_str(&out).expect("a reply is JSON");
    (out, v)
}

/// Both shaped replies over ONE fixture tree, in the order [`Reply`] declares them.
fn both_replies(name: &str) -> Vec<(Reply, String, serde_json::Value)> {
    let dir = git_tree(name);
    let (a_text, a) = reply_of(&dir);
    let (m_text, m) = map_reply(&dir);
    vec![(Reply::Analyze, a_text, a), (Reply::ModuleMap, m_text, m)]
}

fn document() -> &'static str {
    contracts::find(REPLY_LEGENDS_CONTRACT_NAME)
        .expect("the reply-legends contract document must be served")
        .content
}

/// Every PROSE body in the served document, one entry per paragraph that is not a heading. This is the
/// population the fold removed from the wire, derived from the document rather than listed.
fn served_bodies() -> Vec<String> {
    document()
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty() && !p.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// FLOOR: the needles have to be real, long prose, or "absent from the reply" is a statement about
/// nothing. Checked before every use, in both directions — a document that rendered to headings alone
/// would otherwise make this whole file vacuously green.
fn assert_needles_are_real(bodies: &[String]) {
    assert!(
        bodies.len() >= FOLDED_KEYS.len(),
        "the served document has {} prose bodies for {} folded legends — it is not rendering the text \
         it is supposed to be the home of",
        bodies.len(),
        FOLDED_KEYS.len()
    );
    let total: usize = bodies.iter().map(String::len).sum();
    assert!(
        total > 6_000,
        "the served document holds only {total} bytes of prose; the fold removed roughly 8,500 from \
         every reply, so this is not where it went"
    );
    for body in bodies {
        assert!(
            body.len() > 100,
            "a served body is {} bytes — too short to be one of the legends this fold moved: {body}",
            body.len()
        );
    }
}

/// DIRECTION 1 — no legend prose rides EITHER shaped reply. This is the byte cost the fold exists to
/// remove, and it is measured against the exact text the reply's own pointer serves.
///
/// FLOOR, both directions: [`assert_needles_are_real`] proves the needles are real prose, and the
/// canary below proves the COMPARISON can fail — each reply is searched for a string it provably
/// carries before it is searched for the bodies that must not be there. An "absent from the reply"
/// test whose `contains` can never match is the same green as a working fold.
#[test]
fn the_shaped_replies_carry_no_legend_prose() {
    let bodies = served_bodies();
    assert_needles_are_real(&bodies);

    for (which, reply, v) in both_replies("absent") {
        // Each reply's own canary, read off the reply rather than named here: the folded NOTE that
        // key really ships. A shared canary would only prove the encoding against one of them.
        let present = match which {
            Reply::Analyze => v["findings"]["byRuleMeaning"].as_str(),
            Reply::ModuleMap => v["meaning"].as_str(),
        }
        .expect("every shaped reply carries at least one folded legend as a string");
        assert!(
            reply.contains(&as_json_body(present)),
            "CANARY: this test's own needle encoding cannot find a string this reply provably \
             carries, so its every 'absent' below would be a statement about the encoding rather \
             than about the fold"
        );

        for body in &bodies {
            assert!(
                !reply.contains(&as_json_body(body)),
                "a legend body served by the `{REPLY_LEGENDS_CONTRACT_NAME}` document is STILL \
                 riding every reply — that is the duplication the fold removes: {body}"
            );
        }
    }
}

/// FLOOR for the two git-gated keys, asserted BEFORE anything else reads them. Without history the
/// `architecture` object is absent and every needle aimed into it passes by not running — so this test
/// exists to make that failure loud instead of green, and it is the reason [`git_tree`] replaced the
/// plain temp tree on 2026-09-14.
///
/// It also checks the NEGATIVE half, which is what makes it a statement about the gate rather than
/// about this fixture: the same tree WITHOUT `git init` must carry no `architecture` at all. If that
/// ever stops being true, the fixture is doing nothing and this file should say so.
#[test]
fn the_architecture_legends_are_actually_on_the_wire_here() {
    let (_, gated) = reply_of(&tmp_tree("no-history"));
    assert!(
        gated.get("architecture").is_none(),
        "the `architecture` object is supposed to be git-gated; if it now ships without history,          `git_tree` is no longer buying this file anything: {gated}"
    );

    let (_, v) = analyze_reply("gated");
    let architecture = v.get("architecture").unwrap_or_else(|| {
        panic!("the git-seeded fixture must produce an `architecture` object: {v}")
    });
    for key in GIT_GATED_KEYS {
        let leaf = key.split('.').next_back().expect("a dotted key has a leaf");
        assert!(
            architecture[leaf].as_str().is_some_and(|t| t.len() > 200),
            "`{key}` is not a non-empty string on this reply, so every assertion this file makes              about it is vacuous: {architecture}"
        );
    }
}

/// DIRECTION 2 — nothing became unreachable. Every folded key is still present and non-empty, and the
/// pointer it prints resolves through the same `contracts::find` both hosts use, and is LISTED (a
/// document that resolves but is invisible in `resources/list` / `zzop contract` is half a pointer).
#[test]
fn every_folded_key_still_ships_with_a_resolvable_pointer() {
    let replies = both_replies("pointer");
    let uri = format!("{}{REPLY_LEGENDS_CONTRACT_NAME}", contracts::URI_PREFIX);
    let command = format!("zzop contract {REPLY_LEGENDS_CONTRACT_NAME}");

    for (which, key) in FOLDED_KEYS {
        let v = &replies
            .iter()
            .find(|(w, _, _)| w == which)
            .expect("every declared reply is built above")
            .2;
        let value = wire_path(key)
            .split('.')
            .fold(v, |node, segment| &node[segment])
            .clone();
        assert!(
            !value.is_null(),
            "`{key}` left the reply — the fold shortens a legend, it does not delete the key"
        );
        let rendered = serde_json::to_string(&value).expect("a folded legend is plain JSON");
        assert!(
            rendered.contains(&uri) && rendered.contains(&command),
            "`{key}` must name BOTH host dialects of its pointer — this text reaches a CLI reader with \
             argv and an MCP client with none: {rendered}"
        );
        // A ceiling rather than a target, and deliberately generous: the fold's job is to stop shipping
        // a VOCABULARY on every call, not to hit a byte number, and one of these keeps a token
        // dictionary inline on purpose (`coverageGaps.meaning` defines the two `kind` values its own
        // rows carry). What this catches is re-inflation — a legend creeping back toward the four-digit
        // sizes measured before the fold (4,416 / 1,962 / 1,074 / 1,022).
        assert!(
            rendered.len() < 2_000,
            "`{key}` is {} bytes after folding; it is supposed to be a note plus a pointer, and the \
             vocabulary behind it is one lookup away",
            rendered.len()
        );
    }

    // The pointer really resolves, and is really listed.
    let name = uri
        .strip_prefix(contracts::URI_PREFIX)
        .expect("the uri is built from the prefix constant");
    let doc = contracts::find(name).expect("the reply's own pointer must resolve");
    assert!(!doc.content.trim().is_empty(), "the pointer serves nothing");
    assert!(
        contracts::names().any(|n| n == name),
        "{name} is readable but not listed — half a pointer"
    );
    assert_eq!(doc.mime, "text/markdown");
}

/// DIRECTION 3 — the shortening did not drop the sentences that change what a ZERO MEANS. These are the
/// clauses each legend was ADDED for; a fold that kept the bytes and lost these would pass both
/// directions above and still be the failure the whole disclosure doctrine is about.
#[test]
fn the_short_notes_keep_the_sentences_that_change_what_a_zero_means() {
    let replies = both_replies("notes");
    let v = &replies[0].2;
    let must_say: &[(&str, &[&str])] = &[
        // A row is a place to look, and its `kind` is what says whether a zero cost anything.
        (
            "coverageGaps.meaning",
            &["NEVER as a verdict", "`kind`", "two causes"],
        ),
        // The count is of findings, and the caveat has to reach the reader before the instruction.
        (
            "findings.byRuleMeaning",
            &["Counts FINDINGS, not places", "NOT across rules"],
        ),
        // The distribution is not a verdict, the evidence that makes that an argument rather than a
        // disclaimer survives, and the remedy names BOTH config keys — a remedy naming one would be
        // advice to do the wrong one half the time, and it is the clause a reader acts on fastest.
        (
            "findings.byDirectory.meaning",
            &[
                "never a verdict",
                "NOT evidence that a directory is noise",
                "`exclude`",
                "`vocabulary.skipDirs`",
                "FULL finding set",
            ],
        ),
        // The three ways an absence is not a measured zero, and the ACTION for the one a per-tree
        // reply cannot answer on its own.
        (
            "nativeAnalysesMeaning",
            &[
                "NOT evaluated",
                "never analyzed-and-clean",
                "run the cross-layer join",
                "measured zero",
            ],
        ),
        // The one that inverts a clean bill of health.
        (
            "packsLoadedMeaning",
            &["NOT ANALYZED", "never analyzed-and-clean"],
        ),
        // Four auditors misread this object, each on a different word, and each clause here answers
        // one of them: `severity` is not a finding severity (three auditors, 2026-08-20), a rule's
        // disable does not reach this lane (a fourth, 2026-09-13), and `idMeaning` rides beside `id`
        // so the kind vocabulary needs no lookup — the clause that REPLACES a lookup rather than
        // deferring one, which is the clause a pointer can least afford to swallow.
        (
            "architecture.topRecommendationMeaning",
            &[
                "not a finding severity",
                "findings.bySeverity",
                "disable `recommendations`",
                "`idMeaning`",
            ],
        ),
        // The population, and the sentence that keeps a reader who narrowed `pain` from concluding
        // this list narrowed too. `analyze::architecture`'s own unit test pins the second phrase on
        // the value this reply actually carries, so dropping it here reds in two places.
        (
            "architecture.criticalTopMeaning",
            &[
                "test files are NOT excluded",
                "This holds in EVERY run",
                "blast_radius",
            ],
        ),
    ];
    // The map's note answers four misreadings of a MAP, and they are not the analyze reply's four:
    // that the two edge numbers count one population, that a `lines` sum covers a whole module, that
    // a row could have been capped away, and that a big box is a judgement about a big box.
    let map_must_say: &[&str] = &[
        "never a verdict",
        "READ THE TWO EDGE NUMBERS AS A PAIR",
        "not a subset failure",
        "`linesMeasuredOver`",
        "absent rather than `0`",
        "NOTHING IS CAPPED",
    ];
    let map_note = replies[1].2["meaning"]
        .as_str()
        .expect("the map reply's legend is a string");
    for phrase in map_must_say {
        assert!(
            map_note.contains(phrase),
            "`module_map.meaning`'s short note dropped {phrase:?} — that clause is the reading this \
             legend exists to prevent a reader from getting wrong, and it does not survive a \
             pointer: {map_note}"
        );
    }

    for (key, phrases) in must_say {
        let text = serde_json::to_string(&key.split('.').fold(v, |node, s| &node[s]))
            .expect("a folded legend is plain JSON");
        for phrase in *phrases {
            assert!(
                text.contains(phrase),
                "`{key}`'s short note dropped {phrase:?} — that clause is the reading this legend \
                 exists to prevent a reader from getting wrong, and it does not survive a pointer: \
                 {text}"
            );
        }
    }
    // The caveat still precedes the instruction, the property `by_rule_legend`'s own test pins on the
    // full text. Shortening is exactly the edit that reorders a sentence by accident.
    let by_rule = v["findings"]["byRuleMeaning"]
        .as_str()
        .expect("byRuleMeaning is a string");
    let caveat = by_rule.find("Counts FINDINGS, not places").expect("caveat");
    let instruction = by_rule.find("read an entry as").expect("instruction");
    assert!(
        caveat < instruction,
        "the caveat must reach the reader before the instruction it qualifies: {by_rule}"
    );
}

/// The document is rendered from the SAME sources the reply's notes are built from, so a legend cannot
/// be edited on one side only. Sealed the way `disclosure_fold.rs` seals its own chain: the served
/// bytes are the live render, never a snapshot, and the full sentences the run-free facade accessors
/// return are all in it.
#[test]
fn the_served_document_is_the_live_render_of_the_legends_own_owners() {
    let doc = document();
    for (key, sentence) in zzop_facade::native_analyses_legend()
        .into_iter()
        .chain(zzop_facade::packs_loaded_legend())
    {
        assert!(
            doc.contains(sentence),
            "the served document does not carry `{key}`'s own sentence verbatim — it is a copy, not a \
             render, and the two will disagree"
        );
    }
    // `didNotRun` is CONDITIONAL on the wire and unconditional in the document. That asymmetry is the
    // reason the facade has a run-free accessor at all: a reader following the pointer no longer has
    // the run that would have decided whether to print it.
    assert!(
        doc.contains("NOT ANALYZED"),
        "the document must carry the `didNotRun` sentence even though a reply may not"
    );
}

// --- THE POPULATION GUARD (2026-09-14, ledger V240) ------------------------------------------------
//
// Everything above takes its subjects from the FOLD LIST. That is the right population for "did the
// fold work", and the wrong one for "is anything missing from the fold" — a legend that qualifies and
// was never added is invisible to every test in this file. It happened: `architecture`'s two legends
// met the criterion from the day the fold landed and sat inline for thirteen days, in neither the fold
// list nor the module doc's list of deliberate exclusions.
//
// So this half derives its population from the REPLY.

/// Legends that are run-invariant, big enough to be worth folding, and deliberately NOT folded — each
/// with the reason, so the prose carve-out list in `output::legends`' module doc is machine-checked
/// instead of merely written.
///
/// An entry here is a claim that has to hold on its own terms; it is not a waiver. Adding one is the
/// triage moment, exactly as `tests_fragments::name_census` is for fragment names.
const NOT_FOLDED: &[(&str, &str)] = &[(
    "cache.meaning",
    "ships INSIDE the object it describes, which is the whole point of it: `cache_signal`'s module doc \
     argues that a consumer who reads the hit/miss counts must not be able to miss what they do and do \
     not prove, and a pointer is exactly the thing a reader can decline to follow",
)];

/// Every legend-shaped string in a reply, as `dotted.path -> (text, already folded)`. Keys are matched
/// by SHAPE (`*Meaning`, `meaning`, `note`) rather than by a list, so a legend added tomorrow is in the
/// population tomorrow.
///
/// 🔴 "Already folded" is read off the SIBLING `resource` key, not off the pointer's wording. This
/// repo has two fold mechanisms with two different sentences — `legends`' string form ends with
/// `INLINE_POINTER`, while `disclosure`'s object form says "Identical every run, so the full text
/// ships once, not per call". A detector keyed on one wording reports the other as unfolded, which is
/// how the first draft of this test demanded that `disclosure.note` be folded a second time. What both
/// forms genuinely share is the machine pointer beside the prose.
fn legend_strings(v: &serde_json::Value) -> std::collections::BTreeMap<String, (String, bool)> {
    fn walk(
        node: &serde_json::Value,
        trail: &str,
        out: &mut std::collections::BTreeMap<String, (String, bool)>,
    ) {
        let Some(map) = node.as_object() else { return };
        let pointed_at = map
            .get("resource")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|r| r.contains("contract/"));
        for (k, val) in map {
            let here = if trail.is_empty() {
                k.clone()
            } else {
                format!("{trail}.{k}")
            };
            match val.as_str() {
                Some(t) if k.ends_with("Meaning") || k == "meaning" || k == "note" => {
                    let folded = pointed_at || t.contains("contract/reply-legends");
                    out.insert(here, (t.to_string(), folded));
                }
                _ => walk(val, &here, out),
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(v, "", &mut out);
    out
}

/// EVERY legend-shaped string one tree produces, across every shaped reply, keyed so a map key and an
/// analyze key cannot collide.
///
/// 🔴 This is the population half of the 2026-09-15 widening, and it is the half that matters most.
/// [`FOLDED_KEYS`] gaining a `Reply` column makes the "did the fold work" tests see two replies; this
/// makes the "is anything MISSING from the fold" test see two. Without it, a new run-invariant legend
/// born on the map reply would be invisible here — which is exactly how `architecture`'s two legends
/// sat unfolded for thirteen days, one reply earlier.
fn every_legend_of(dir: &std::path::Path) -> std::collections::BTreeMap<String, (String, bool)> {
    let (_, analyze) = reply_of(dir);
    let (_, map) = map_reply(dir);
    let mut out = legend_strings(&analyze);
    for (key, value) in legend_strings(&map) {
        out.insert(format!("module_map.{key}"), value);
    }
    out
}

/// A second tree that differs on BOTH axes — different files AND a different config.
///
/// 🔴 Both axes, because one of them is not enough and that was measured. `architecture.painMeaning`
/// is byte-identical across eight corpus trees in six languages, and moves 1,619 -> 1,847 bytes the
/// moment `scores.excludeTestFilesFromFileMetrics` is set. A sweep that varied only the TREE would
/// report it invariant and demand it be folded — and folding it is refused by output principle §1.5
/// and pinned against by `legends::tests`. "Invariant across trees" is not "invariant across runs".
fn other_tree(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("zzop-legend-other-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let config = zzop_config::template::CONFIG_TEMPLATE_JSONC.trim_end();
    let cut = config.rfind('}').expect("the template is a JSON object");
    fs::write(
        dir.join("zzop.config.jsonc"),
        format!(
            "{}  ,\"scores\": {{\"excludeTestFilesFromFileMetrics\": true}}\n{}",
            &config[..cut],
            &config[cut..]
        ),
    )
    .unwrap();
    fs::write(
        dir.join("svc.ts"),
        "export function login(u: string) {\n  return u;\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("svc.test.ts"),
        "import { login } from './svc';\nit('works', () => login('a'));\n",
    )
    .unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("git must be runnable for this test")
    };
    git(&["init", "-q", "."]);
    git(&["add", "-A"]);
    git(&[
        "-c",
        "user.email=t@example.com",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "seed",
    ]);
    dir
}

/// DIRECTION 4 — nothing that QUALIFIES for the fold is sitting outside it unaccounted for.
///
/// A legend qualifies when it is byte-identical across two runs that differ in tree AND config, and is
/// at least as large as the pointer that would replace it. Qualifying legends must be folded, or named
/// in [`NOT_FOLDED`] with the reason.
///
/// The size floor is DERIVED, never chosen: a legend smaller than `INLINE_POINTER` cannot be folded at
/// a profit, because the pointer is what would take its place. Its length is read off the shipped
/// pointer through `folded_string`, so the floor moves when the pointer does.
#[test]
fn every_run_invariant_legend_is_either_folded_or_carved_out_with_a_reason() {
    let dir_a = git_tree("pop-a");
    let dir_b = other_tree("pop-b");
    let (left, right) = (every_legend_of(&dir_a), every_legend_of(&dir_b));

    // FLOOR 1: both replies must actually carry legends, or every judgement below is about nothing.
    assert!(
        left.len() >= 8 && right.len() >= 8,
        "the two runs carry {} and {} legend-shaped keys across the shaped replies — this \
         population has stopped being derived from the replies",
        left.len(),
        right.len()
    );

    // The pointer's own length is the floor, read off a legend that really ships it.
    let pointer_len = {
        let (folded, _) = &left["findings.byRuleMeaning"];
        let marker = "This vocabulary is identical on every run";
        folded.len()
            - folded
                .find(marker)
                .expect("a folded legend ends with the pointer")
    };
    assert!(
        pointer_len > 200,
        "the pointer measured {pointer_len} bytes, which is too short to be the real one — the floor \
         below would then demand folding of legends that cannot profit from it"
    );

    let mut varying = 0usize;
    let mut unaccounted: Vec<String> = Vec::new();
    for (key, (text, folded)) in &left {
        let Some((other, _)) = right.get(key) else {
            continue;
        };
        if other != text {
            varying += 1;
            continue; // not run-invariant: the fold's entry condition fails, by measurement
        }
        if *folded {
            continue; // already folded, by either mechanism
        }
        if text.len() < pointer_len {
            continue; // folding it would make the reply bigger
        }
        if NOT_FOLDED.iter().any(|(k, _)| k == key) {
            continue;
        }
        unaccounted.push(format!("{key} ({} bytes)", text.len()));
    }

    // FLOOR 2: the config axis must actually separate something, or this test is a tree-only sweep
    // wearing two trees — the exact blind spot `other_tree`'s doc describes.
    assert!(
        varying > 0,
        "no legend differed between the two runs, so the config axis contributed nothing and a \
         run-VARYING legend would be demanded for folding"
    );

    assert!(
        unaccounted.is_empty(),
        "these legends are run-invariant and large enough to fold, and are in neither the fold list \
         nor NOT_FOLDED: {unaccounted:?}. Fold them, or add each to NOT_FOLDED with the reason it \
         stays inline. A fold list maintained by hand is how `architecture`'s two legends shipped \
         inline for thirteen days after they qualified."
    );

    // Every carve-out must be REAL — an entry for a key no reply carries is a claim about nothing, and
    // the list would then grow stale exactly the way the fold list did.
    for (key, _) in NOT_FOLDED {
        assert!(
            left.contains_key(*key) || right.contains_key(*key),
            "NOT_FOLDED names `{key}`, which neither reply carries — remove it, or it is a waiver for \
             something that no longer exists"
        );
    }
}
