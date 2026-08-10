//! The CODE half of the native-rule io-channel seal — every crate's `NATIVE_ANALYSES` table against
//! the io each rule's own module actually reads.
//!
//! # The hole this closes
//! `zzop_core::rule_channels` moved a fact that lived only as scattered `kind == "http"` comparisons
//! onto the same row that registers the rule id. Moving a fact is not binding it: without this
//! contract the table is relocated hardcoding, and it would be WORSE than the scattered literals,
//! because a user-facing disclosure would now quote it. The rule this file enforces is the one
//! `recognizer_channels` enforces one seam over — a declaration equals what the declaring code does.
//!
//! # How a rule is attributed to its source, and why that way
//! By the rule id's own OWNERSHIP spelling in comment-stripped code, never by a name fold or an alias
//! map: the module that mints the finding (`rule_id: "…"`), binds the id to a `const`, or writes the
//! disable hint (`disable_hint("…")`) — see [`ownership_needles`], and contract 3 for the separate
//! guard that already holds those literals to the findings the file emits. The hit is widened to the
//! whole module UNIT (`foo.rs` plus `foo/`), because several rules mint their finding in a `message`
//! submodule while the io filtering sits in the unit root — attributing the file alone made
//! `unprovided-consume` read as io-blind while `unprovided_consume.rs` filtered `kind == "http"` four
//! times.
//!
//! # What the extractor can decide, and what it refuses to
//! A channel is (side, kind). The kind is the compared literal. The side is read from the nearest
//! preceding `provide`/`consume`/`edge` token, which in this codebase is always the receiver's own
//! name or field (`c.consume.kind`, `p.provide.kind`, `unconsumed_provides.iter().filter(|p| p.kind`)
//! — an `edge` receiver yields BOTH sides, since a `CrossLayerEdge` exists only where a provide and a
//! consume met. A site with no such token in reach fails loudly rather than guessing.
//!
//! It refuses `kind != "…"`. The two shipped uses mean OPPOSITE things —
//! `if p.kind != "http" { continue; }` narrows TO http, while
//! `.filter(|a| a.consume.kind != "db-table")` widens to everything else — and the difference is
//! control flow, not text. Such a site marks the module io-AWARE (so it cannot claim the empty
//! declaration) but contributes no channel; the rule's declaration is then carried by a pin.
//!
//! # What is NOT bound (stated, because a guard's silence gets read as coverage)
//! - **One link in the pre-filter hop is a human judgment.** Twelve declarations belong to rules the
//!   ENGINE hands a pre-filtered input (`partition::http_provide_sites`, `join_maps::http_consume_totals`,
//!   the callgraph pass's `ApiEndpoint` reconstruction), so their own module names no kind. The
//!   CHANNEL is still derived — [`CHANNEL_FROM_ENGINE_PREFILTER`] names the producing FUNCTION and its
//!   body goes through the same [`scan`], so a producer that stops filtering that kind goes red. What
//!   no code states, and a human therefore asserts once per pin, is that this producer's output really
//!   is that rule's input. The engine call site cannot supply it either: at
//!   `cross_layer_findings/mod.rs` the argument beside each `is_enabled(&gate, "cross-layer/…")` is a
//!   local (`&http_provides`, `&unprovided_filtered`, `&result.prefix_records`), and reading a channel
//!   off an argument NAME is prose-mining. So the answer to "does the call site bind the fact?" is no —
//!   it is where a human READS the link, and the pin points at the local's producer, which is code.
//! - **Direction is not a channel.** The declaration says the rule reads the channel, never that an
//!   empty channel silences it — see `zzop_core::rule_channels`' module doc.
//! - **Attribution granularity is the module unit, not the call site.** A unit backing two rules
//!   lends its channels to both. Nothing in the tree does that today — the one case that did,
//!   `all_consumes_unjoined/subsume.rs` naming the two rules it replaces, is what narrowed the needle
//!   from "the literal appears" to [`ownership_needles`] — but a future submodule minting two rules
//!   would inherit the union.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use zzop_core::rule_channels::reads;
use zzop_core::{RuleIoChannel, RULE_READ_IO_KINDS};

use crate::recognizer_channels::{code_only, unit_sources};

/// A declared channel the rule's own module cannot evidence because the ENGINE filtered its input
/// first — `(rule_id, channel label, repo-relative source, producer fn, why)`.
///
/// This is a second DERIVATION hop, not a string match: the named function's body is extracted and run
/// through the same [`scan`] the rule modules go through, so the channel a pin supplies comes from that
/// function's own code and follows it when it changes. What a human judged once, and this file does not
/// re-judge, is the one link no code states — that this producer's output really is that rule's input.
///
/// Why the pin points at the PRODUCER and not at the call site: at `cross_layer_findings/mod.rs` each
/// `is_enabled(&gate, "cross-layer/…")` does sit next to the argument it gates, but the argument is a
/// local (`&http_provides`, `&unprovided_filtered`, `&result.prefix_records`). Reading a channel off an
/// argument NAME is prose-mining; reading it off the function that built the local is code. The call
/// site is what a human reads to write the `why`, not something this file can derive from.
const CHANNEL_FROM_ENGINE_PREFILTER: &[(&str, &str, &str, &str, &str)] = &[
    // ---- fed `partition::http_provide_sites`, the run-wide http provide universe --------------
    (
        "cross-layer/method-mismatch",
        "io.provides:http",
        "crates/engine/src/cross_layer_findings/partition.rs",
        "http_provide_sites",
        "the provide side arrives pre-filtered as `&[HttpProvideSite]` — the engine derives that \
         universe from every tree's provides here, because `CrossLayerResult` alone does not expose \
         the unmatched ones this rule compares against.",
    ),
    (
        "cross-layer/version-skew",
        "io.provides:http",
        "crates/engine/src/cross_layer_findings/partition.rs",
        "http_provide_sites",
        "same pre-filtered `&[HttpProvideSite]` provide universe as `method-mismatch`; this rule's \
         own module only compares the consume side it walks.",
    ),
    (
        "cross-layer/path-near-miss",
        "io.provides:http",
        "crates/engine/src/cross_layer_findings/partition.rs",
        "http_provide_sites",
        "same pre-filtered `&[HttpProvideSite]` provide universe as `method-mismatch`; this rule's \
         own module only compares the consume side it walks.",
    ),
    (
        "cross-layer/route-near-miss",
        "io.provides:http",
        "crates/engine/src/cross_layer_findings/partition.rs",
        "http_provide_sites",
        "same pre-filtered `&[HttpProvideSite]` provide universe as `method-mismatch`; this rule's \
         own module only compares the consume side it walks.",
    ),
    (
        "cross-layer/external-shadow-internal",
        "io.provides:http",
        "crates/engine/src/cross_layer_findings/partition.rs",
        "http_provide_sites",
        "the whole point of the rule is that an EXTERNAL consume's normalized key matches a route an \
         analyzed tree provides, so it takes the same pre-filtered provide universe.",
    ),
    (
        "cross-layer/route-shadowing",
        "io.provides:http",
        "crates/engine/src/cross_layer_findings/partition.rs",
        "http_provide_sites",
        "cross-tree shadowing is provide-vs-provide: the rule's only input IS that pre-filtered \
         universe (`all_provides: &[HttpProvideSite]`), so its module has nothing left to compare.",
    ),
    (
        "cross-layer/unprovided-mutation-call",
        "io.provides:http",
        "crates/engine/src/cross_layer_findings/partition.rs",
        "http_provide_sites",
        "its provide-blindness gate is computed from per-source counts over that same universe, and \
         an \"unprovided\" consume is itself defined by the absence of a matching provide.",
    ),
    // ---- derived one hop further, from another rule's own filtering ---------------------------
    (
        "cross-layer/prefix-drift",
        "io.provides:http",
        "crates/engine/src/cross_layer_findings/partition.rs",
        "http_provide_sites",
        "a pure aggregation over `route-near-miss`'s prefix records, so it inherits both of that \
         rule's inputs — this is its provide half.",
    ),
    (
        "cross-layer/prefix-drift",
        "io.consumes:http",
        "rules/native/rules-cross-layer/src/cross_layer/route_near_miss.rs",
        "route_near_miss_results",
        "the consume half of the same inheritance: every prefix record this rule folds is built from \
         the consumes that function filtered.",
    ),
    // ---- the verb-unknown lift, and the consume-total denominator -----------------------------
    (
        "cross-layer/unknown-verb-route",
        "io.provides:http",
        "crates/engine/src/cross_layer_findings/partition.rs",
        "verb_unknown_sites",
        "the rule reports the verb-unknown routes the engine lifts OUT of the exact-key join; this is \
         the provide walk that finds those sentinels, and its output is the rule's whole input.",
    ),
    (
        "cross-layer/untraced-client-import-no-visible-consume",
        "io.consumes:http",
        "crates/engine/src/cross_layer_findings/join_maps.rs",
        "http_consume_totals",
        "the rule fires on a tree whose visible http-consume total sits below the ratio rules' floor \
         — that total is this map, and it is the rule's only io-derived input.",
    ),
    // ---- the callgraph pass reconstructs ApiEndpoint from the same provides --------------------
    (
        "unsafe-read-endpoint",
        "io.provides:http",
        "crates/engine/src/analyze/native_rules/callgraph/mod.rs",
        "run_callgraph_rules",
        "the rule takes `&[zzop_core::ApiEndpoint]`, which is not a third route pass: this function \
         rebuilds that list from the run's http provides, so no route extraction means no endpoint \
         for the BFS to start from.",
    ),
    (
        "non-idempotent-write",
        "io.provides:http",
        "crates/engine/src/analyze/native_rules/callgraph/mod.rs",
        "run_callgraph_rules",
        "same `ApiEndpoint` reconstruction as `unsafe-read-endpoint` — the two rules share the \
         callgraph pass's endpoint list, built in this function.",
    ),
];

/// A declared channel whose evidence IS in the rule's own module, as a `kind != "…"` comparison whose
/// sense [`scan`] refuses to guess — `(rule_id, channel label, repo-relative source, needle, why)`.
///
/// The needle must still be present in that module's comment-stripped code, and the module must still
/// register as io-AWARE, so the pin cannot outlive the line it reads. What the human supplies is only
/// the SENSE, and the two shipped senses are opposite: an early-continue guard narrows TO the named
/// kind, a filter predicate widens to everything BUT it. Each `why` below states which one it is.
const CHANNEL_FROM_UNDECIDABLE_POLARITY: &[(&str, &str, &str, &str, &str)] = &[
    (
        "route-shadowing",
        "io.provides:http",
        "rules/native/rules-http/src/route_shadowing.rs",
        "p.kind != \"http\"",
        "NARROWING sense: an early-continue guard at the top of the grouping loop, so the rule sees \
         http provides and nothing else. Judged here rather than by the scanner because the sense is \
         control flow — the same text as a filter predicate would mean everything-but-http.",
    ),
    (
        "cross-layer/ambiguous-consume",
        "io.consumes:http",
        "rules/native/rules-cross-layer/src/cross_layer/ambiguous_consume.rs",
        "a.consume.kind != \"db-table\"",
        "WIDENING sense: a filter predicate, so the rule takes every ambiguous consume EXCEPT \
         db-table sharing (which has its own rule) and both remaining kinds stay in scope. This is \
         the consume the finding anchors on.",
    ),
    (
        "cross-layer/ambiguous-consume",
        "io.provides:http",
        "rules/native/rules-cross-layer/src/cross_layer/ambiguous_consume.rs",
        "a.consume.kind != \"db-table\"",
        "the provide side of the same widening: an ambiguity is by definition a consume whose key two \
         or more trees PROVIDE, and the finding enumerates those candidate providers.",
    ),
    (
        "cross-layer/ambiguous-consume",
        "io.consumes:trpc",
        "rules/native/rules-cross-layer/src/cross_layer/ambiguous_consume.rs",
        "a.consume.kind != \"db-table\"",
        "the second kind that exclusion leaves in scope — the join keys trpc procedures the same \
         exact-key way it keys http routes, so a trpc consume can be ambiguous too.",
    ),
    (
        "cross-layer/ambiguous-consume",
        "io.provides:trpc",
        "rules/native/rules-cross-layer/src/cross_layer/ambiguous_consume.rs",
        "a.consume.kind != \"db-table\"",
        "the provide side of the trpc half — same two reasons as the http pair above, and the only \
         declaration anywhere that reaches the trpc consume channel at all.",
    ),
];

/// Crates whose every rule declares the empty channel set, and the fact that makes that checkable.
/// A family-level claim (`zzop_rules_schema` declares 17 rows through two label lists) can only be
/// trusted if the crate provably has no io in it at all — which is what
/// [`no_io_free_crate_names_any_io`] re-measures rather than assumes.
const IO_FREE_CRATES: &[(&str, &str)] = &[
    (
        "rules-graph",
        "dependency/dead-code graph and symbol call-graph rules — their evidence is import edges and \
         symbol reachability, extracted per file and never entered into the cross-layer io join.",
    ),
    (
        "rules-schema",
        "Prisma model IR plus per-file usage tokens. This is the crate whose 12 per-issue ids are \
         registered from label lists rather than tabled one by one, so their shared empty declaration \
         rests entirely on this measurement.",
    ),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn native_root() -> PathBuf {
    repo_root().join("rules/native")
}

// ---------------------------------------------------------------------------------------------
// Source reading
// ---------------------------------------------------------------------------------------------

/// Every non-test `.rs` file under `rules/native`, EXCLUDING each crate's `lib.rs`. The declaration
/// table lives in `lib.rs` and spells every id in the crate, so counting it would attribute every rule
/// to the crate root at once — the table would then be checking itself.
fn rule_source_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(&native_root(), &mut out);
    out.retain(|p| p.file_name().is_some_and(|n| n != "lib.rs"));
    out.sort();
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "target" && name != "tests" {
                collect(&path, out);
            }
            continue;
        }
        if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            out.push(path);
        }
    }
}

fn read_code(path: &Path) -> String {
    code_only(&std::fs::read_to_string(path).unwrap_or_default())
}

/// The module UNIT a file belongs to, as `(root directory, unit name)` — the pair
/// [`unit_sources`] takes. Climbs out of every `foo/` whose sibling `foo.rs` exists, so
/// `…/route_near_miss/dimensions.rs` resolves to the `route_near_miss` unit rather than to itself.
fn unit_of(file: &Path) -> (PathBuf, String) {
    let mut dir = file.parent().unwrap_or(Path::new("")).to_path_buf();
    let mut unit = file
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    if unit == "mod" {
        unit = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        dir = dir.parent().unwrap_or(Path::new("")).to_path_buf();
    }
    loop {
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let Some(parent) = dir.parent().map(Path::to_path_buf) else {
            break;
        };
        if name.is_empty() || !parent.join(format!("{name}.rs")).is_file() {
            break;
        }
        unit = name;
        dir = parent;
    }
    (dir, unit)
}

// ---------------------------------------------------------------------------------------------
// Channel evidence
// ---------------------------------------------------------------------------------------------

/// What one module unit's code says about io: the channels it compares, and whether it named any io
/// kind at all through a comparison this scanner declines to interpret.
#[derive(Default)]
struct Evidence {
    channels: BTreeSet<RuleIoChannel>,
    /// `kind != "…"` sites — io-aware, direction undecided (see this file's module doc).
    undetermined: BTreeSet<String>,
}

impl Evidence {
    fn is_io_blind(&self) -> bool {
        self.channels.is_empty() && self.undetermined.is_empty()
    }
}

/// How far back a `kind == "…"` site may look for the token that names its receiver's side. Wide
/// enough to clear a multi-line iterator chain (`s.io.provides\n.iter()\n.filter(|p| p.kind == …`),
/// narrow enough that it cannot reach the previous statement's subject.
const SIDE_LOOKBACK: usize = 200;

/// PROVIDES / CONSUMES / both, from the nearest preceding receiver token.
fn sides_at(code: &str, at: usize) -> Option<Vec<&'static str>> {
    let start = at.saturating_sub(SIDE_LOOKBACK);
    let window = code[..at]
        .char_indices()
        .filter(|(i, _)| *i >= start)
        .map(|(_, c)| c)
        .collect::<String>()
        .to_ascii_lowercase();
    let pick = |needle: &str| window.rfind(needle);
    let candidates = [
        (pick("provide"), "provide"),
        (pick("consume"), "consume"),
        (pick("edge"), "edge"),
    ];
    let (_, winner) = candidates
        .into_iter()
        .filter_map(|(pos, name)| pos.map(|p| (p, name)))
        .max_by_key(|(p, _)| *p)?;
    Some(match winner {
        // An edge exists only where a provide and a consume met, so an edge-keyed comparison reads
        // both sides of the join at once.
        "edge" => vec!["provide", "consume"],
        other => vec![other],
    })
}

/// Every io channel one source file's code compares, plus the kinds it names without a decidable
/// sense. `unresolved` collects sites whose side could not be read at all — those are a scanner
/// failure, not a rule property, and the caller turns them into a red.
fn scan(code: &str, ev: &mut Evidence, unresolved: &mut Vec<String>) {
    for (op, decidable) in [("kind == \"", true), ("kind != \"", false)] {
        for (i, _) in code.match_indices(op) {
            let rest = &code[i + op.len()..];
            let Some(end) = rest.find('"') else { continue };
            let kind = &rest[..end];
            let Some(kind) = RULE_READ_IO_KINDS.iter().find(|k| **k == kind) else {
                continue;
            };
            if !decidable {
                ev.undetermined.insert((*kind).to_string());
                continue;
            }
            let Some(sides) = sides_at(code, i) else {
                unresolved.push(format!(
                    "`{op}{kind}\"` at byte {i}: no provide/consume/edge token within \
                     {SIDE_LOOKBACK} bytes before it, so its side is unreadable"
                ));
                continue;
            };
            for side in sides {
                ev.channels.insert(if side == "provide" {
                    RuleIoChannel::provides(kind)
                } else {
                    RuleIoChannel::consumes(kind)
                });
            }
        }
    }
}

/// Rule id -> the io evidence of every module unit whose code names that id.
fn evidence_by_rule() -> (BTreeMap<String, Evidence>, Vec<String>) {
    let declared: Vec<String> = zzop_engine::native_rule_channels()
        .into_iter()
        .map(|r| r.rule_id)
        .collect();
    let files: Vec<(PathBuf, String)> = rule_source_files()
        .into_iter()
        .map(|p| {
            let code = read_code(&p);
            (p, code)
        })
        .collect();

    let mut unresolved = Vec::new();
    let mut unit_cache: BTreeMap<PathBuf, Evidence> = BTreeMap::new();
    let mut out: BTreeMap<String, Evidence> = BTreeMap::new();
    for id in declared {
        let mut ev = Evidence::default();
        for (path, code) in &files {
            if !owns_rule(code, &id) {
                continue;
            }
            let (root, unit) = unit_of(path);
            let key = root.join(&unit);
            if !unit_cache.contains_key(&key) {
                let mut unit_ev = Evidence::default();
                for src in unit_sources(&root, &unit) {
                    scan(&src, &mut unit_ev, &mut unresolved);
                }
                unit_cache.insert(key.clone(), unit_ev);
            }
            let cached = &unit_cache[&key];
            ev.channels.extend(cached.channels.iter().copied());
            ev.undetermined.extend(cached.undetermined.iter().cloned());
        }
        out.insert(id, ev);
    }
    (out, unresolved)
}

/// The three ways a module OWNS a rule id in code: it mints the finding, it binds the id to a
/// `const`, or it tells the user how to disable it. Deliberately narrower than "the literal appears
/// somewhere" — `all_consumes_unjoined/subsume.rs` lists the two rules it REPLACES in a plain array,
/// and the wide needle handed that unit's channels to both of them. Those channels happened to be
/// right, which is the worst kind of wrong: a user-facing claim resting on a sibling module's
/// suppression list.
fn ownership_needles(id: &str) -> [String; 3] {
    [
        format!("rule_id: \"{id}\""),
        format!("= \"{id}\""),
        format!("disable_hint(\"{id}\")"),
    ]
}

fn owns_rule(code: &str, id: &str) -> bool {
    ownership_needles(id).iter().any(|n| code.contains(n))
}

fn label_of(c: &RuleIoChannel) -> String {
    c.label()
}

fn channel_from_label(label: &str) -> Option<RuleIoChannel> {
    reads::ALL.iter().copied().find(|c| c.label() == label)
}

/// One function's body text, by brace matching from the `{` that follows `fn <name>(`.
///
/// The input is already comment-stripped; a brace inside a string literal would still miscount, and
/// none of the pinned producers contains one. Miscounting long would over-extend the body and could
/// only ADMIT a channel the pin already claims, which the sibling `no longer derives` leg then catches
/// — the direction that matters is that it never silently shortens to nothing, and a `None` here is a
/// red rather than an excuse.
fn fn_body(code: &str, name: &str) -> Option<String> {
    let at = code.find(&format!("fn {name}("))?;
    let chars: Vec<char> = code[at..].chars().collect();
    let open = chars.iter().position(|c| *c == '{')?;
    let mut depth = 0usize;
    for (i, c) in chars.iter().enumerate().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(chars[open..=i].iter().collect());
                }
            }
            _ => {}
        }
    }
    None
}

/// Every `(rule_id, channel)` a pin of either class supplies — the set the binding below accepts in
/// place of a module derivation.
fn pinned_pairs() -> BTreeSet<(&'static str, &'static str)> {
    CHANNEL_FROM_ENGINE_PREFILTER
        .iter()
        .chain(CHANNEL_FROM_UNDECIDABLE_POLARITY)
        .map(|(id, label, ..)| (*id, *label))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The floor (working-agreements §5.5) — an extractor that sees nothing answers "consistent".
// ---------------------------------------------------------------------------------------------

/// The scanner must be reading real code: most io-bearing rules have to derive channels from their
/// own module, every channel constant has to be reachable by derivation somewhere, and no site may be
/// left with an unreadable side. A green run over an extractor that found nothing would bless every
/// declaration in the table at once.
#[test]
fn the_module_scan_is_not_vacuous() {
    let (evidence, unresolved) = evidence_by_rule();
    assert!(
        unresolved.is_empty(),
        "io kind comparison(s) whose side this scanner could not read:\n{}\n\
         Either the receiver stopped naming its side, or the chain grew past the lookback. Fix the \
         scanner — a site it cannot read is a channel it silently drops.",
        unresolved.join("\n")
    );

    let derived: Vec<&String> = evidence
        .iter()
        .filter(|(_, e)| !e.channels.is_empty())
        .map(|(id, _)| id)
        .collect();
    assert!(
        derived.len() >= 18,
        "only {} rule(s) derive any channel from their own module {derived:?} — the attribution \
         stopped finding rule sources, not the rules stopped reading io",
        derived.len()
    );

    let seen: BTreeSet<RuleIoChannel> = evidence
        .values()
        .flat_map(|e| e.channels.iter())
        .copied()
        .collect();
    for c in [
        reads::HTTP_PROVIDES,
        reads::HTTP_CONSUMES,
        reads::DB_TABLE_PROVIDES,
        reads::DB_TABLE_CONSUMES,
        reads::TRPC_PROVIDES,
    ] {
        assert!(
            seen.contains(&c),
            "no rule module anywhere evidences {} — that channel's declarations are unchecked",
            c.label()
        );
    }
}

/// Every declared rule must resolve to at least one source file, or its declaration is checked
/// against an empty evidence set and passes for the wrong reason. The io-free crates are exempt:
/// their rows are covered by [`no_io_free_crate_names_any_io`] instead, and `zzop_rules_schema`'s
/// per-issue ids are composed strings that appear in no module.
#[test]
fn every_declared_rule_with_an_io_claim_resolves_to_source() {
    let files: Vec<String> = rule_source_files().iter().map(|p| read_code(p)).collect();
    let mut orphans = Vec::new();
    for row in zzop_engine::native_rule_channels() {
        if row.reads.is_empty() {
            continue;
        }
        if !files.iter().any(|c| owns_rule(c, &row.rule_id)) {
            orphans.push(row.rule_id);
        }
    }
    assert!(
        orphans.is_empty(),
        "rule(s) declaring io channels whose id appears in no rules/native source: {orphans:?} — \
         the id was renamed in the table without the module following, and every channel check below \
         would skip it silently"
    );
}

// ---------------------------------------------------------------------------------------------
// The contract
// ---------------------------------------------------------------------------------------------

/// THE BINDING — a rule's declared channels equal what its own module compares, plus what a pin
/// names. Both directions: a declared channel with neither is an over-claim, and a compared channel
/// the table omits is the silent under-claim a disclosure would then never mention.
#[test]
fn every_declaration_matches_the_io_its_module_reads_or_a_pin_supplies() {
    let (evidence, _) = evidence_by_rule();
    let pinned = pinned_pairs();

    let mut offenders = Vec::new();
    for row in zzop_engine::native_rule_channels() {
        let empty = Evidence::default();
        let ev = evidence.get(&row.rule_id).unwrap_or(&empty);
        let declared: BTreeSet<RuleIoChannel> = row.reads.iter().copied().collect();

        let missing: Vec<String> = ev.channels.difference(&declared).map(label_of).collect();
        if !missing.is_empty() {
            offenders.push(format!(
                "{} compares {missing:?} but its NATIVE_ANALYSES row does not declare it — a rule \
                 reading a channel nothing says it reads is exactly the fact this table exists to \
                 publish",
                row.rule_id
            ));
        }
        let unbacked: Vec<String> = declared
            .difference(&ev.channels)
            .map(label_of)
            .filter(|c| !pinned.contains(&(row.rule_id.as_str(), c.as_str())))
            .collect();
        if !unbacked.is_empty() {
            offenders.push(format!(
                "{} declares {unbacked:?} with no comparison in its own module and no pin — either \
                 the row over-claims, or the channel arrives through an engine pre-filter (then pin \
                 it in CHANNEL_FROM_ENGINE_PREFILTER naming the producing function)",
                row.rule_id
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "NATIVE_ANALYSES disagrees with the io its rules read:\n{}",
        offenders.join("\n")
    );
}

/// Both pin tables share four legs, checked here once: the pin names a real rule, names a channel
/// that rule really declares, names a channel its own module really cannot derive (else the pin is
/// excusing a check that would pass), and carries a reason thick enough to be a judgment.
fn check_pin_frame(
    rule_id: &str,
    label: &str,
    why: &str,
    declared: &BTreeMap<String, BTreeSet<RuleIoChannel>>,
    evidence: &BTreeMap<String, Evidence>,
    stale: &mut Vec<String>,
) -> Option<RuleIoChannel> {
    assert!(
        why.len() > 40,
        "{rule_id}/{label}'s pin reason is too thin to be a judgment: {why:?}"
    );
    let channel = match channel_from_label(label) {
        Some(c) => c,
        None => {
            stale.push(format!("{rule_id}: {label:?} is not a named channel"));
            return None;
        }
    };
    let Some(row) = declared.get(rule_id) else {
        stale.push(format!("{rule_id} is not a declared rule id"));
        return None;
    };
    if !row.contains(&channel) {
        stale.push(format!(
            "{rule_id} no longer declares {label} — delete the pin with the declaration"
        ));
        return None;
    }
    if evidence
        .get(rule_id)
        .is_some_and(|e| e.channels.contains(&channel))
    {
        stale.push(format!(
            "{rule_id}'s own module now compares {label} — delete the pin so the binding checks it"
        ));
        return None;
    }
    Some(channel)
}

/// A pin is only as good as the code it points at.
///
/// For an engine pre-filter pin the check is a DERIVATION: the named function's body goes through the
/// same [`scan`] the rule modules do, and must itself evidence the claimed channel. So a pin cannot
/// assert a channel its producer does not build, and it goes red when the producer is renamed,
/// deleted, or stops filtering that kind — the human's judgment is narrowed to the single link no
/// code states, "this producer's output is that rule's input".
///
/// For an undecidable-polarity pin the check is the exact site: the needle must still be present in
/// the rule module's comment-stripped code (so prose cannot satisfy it), and the module must still
/// register as io-aware. The SENSE stays the human's, stated in the reason.
#[test]
fn every_pin_still_points_at_code_that_supplies_its_channel() {
    let (evidence, _) = evidence_by_rule();
    let declared: BTreeMap<String, BTreeSet<RuleIoChannel>> = zzop_engine::native_rule_channels()
        .into_iter()
        .map(|r| (r.rule_id, r.reads.iter().copied().collect()))
        .collect();

    let mut stale = Vec::new();
    for &(rule_id, label, source, producer, why) in CHANNEL_FROM_ENGINE_PREFILTER {
        let Some(channel) = check_pin_frame(rule_id, label, why, &declared, &evidence, &mut stale)
        else {
            continue;
        };
        let path = repo_root().join(source);
        let Ok(text) = std::fs::read_to_string(&path) else {
            stale.push(format!("{rule_id}/{label}: {source} does not exist"));
            continue;
        };
        let Some(body) = fn_body(&code_only(&text), producer) else {
            stale.push(format!(
                "{rule_id}/{label}: {source} has no `fn {producer}(` with a matchable body — the \
                 producer this pin rests on was renamed or removed, so the declaration is unbacked"
            ));
            continue;
        };
        let mut ev = Evidence::default();
        let mut unresolved = Vec::new();
        scan(&body, &mut ev, &mut unresolved);
        if !ev.channels.contains(&channel) {
            stale.push(format!(
                "{rule_id}/{label}: {source}::{producer} builds {:?} (undecidable {:?}, unreadable \
                 {unresolved:?}) — it no longer supplies the channel this pin credits it with",
                ev.channels.iter().map(label_of).collect::<Vec<_>>(),
                ev.undetermined
            ));
        }
    }
    for &(rule_id, label, source, needle, why) in CHANNEL_FROM_UNDECIDABLE_POLARITY {
        let Some(_) = check_pin_frame(rule_id, label, why, &declared, &evidence, &mut stale) else {
            continue;
        };
        let path = repo_root().join(source);
        let Ok(text) = std::fs::read_to_string(&path) else {
            stale.push(format!("{rule_id}/{label}: {source} does not exist"));
            continue;
        };
        if !code_only(&text).contains(needle) {
            stale.push(format!(
                "{rule_id}/{label}: {source} no longer contains {needle:?} in code — the comparison \
                 whose sense this pin decides is gone, so the declaration is unbacked"
            ));
        }
        if evidence
            .get(rule_id)
            .is_some_and(|e| e.undetermined.is_empty())
        {
            stale.push(format!(
                "{rule_id}'s module no longer registers any undecidable io comparison — this pin \
                 class exists only for that shape; re-derive the channel or move the pin"
            ));
        }
    }
    assert!(
        stale.is_empty(),
        "a channel pin is stale:\n{}",
        stale.join("\n")
    );
}

/// The empty declaration, checked as a claim rather than accepted as a default. A crate listed here
/// must contain no io comparison and construct no io at all — that measurement is what lets
/// `zzop_rules_schema` state one channel set for the 12 ids it registers from label lists.
#[test]
fn no_io_free_crate_names_any_io() {
    let mut offenders = Vec::new();
    for &(krate, why) in IO_FREE_CRATES {
        assert!(
            why.len() > 40,
            "{krate}'s io-free reason is too thin to be a judgment: {why:?}"
        );
        let dir = native_root().join(krate);
        assert!(
            dir.is_dir(),
            "{krate} is not a crate under rules/native — this claim guards nothing"
        );
        let mut files = Vec::new();
        collect(&dir, &mut files);
        assert!(
            !files.is_empty(),
            "{krate} contributes zero source files to this scan"
        );
        for path in files {
            let code = read_code(&path);
            let mut ev = Evidence::default();
            let mut unresolved = Vec::new();
            scan(&code, &mut ev, &mut unresolved);
            if !ev.is_io_blind() || !unresolved.is_empty() {
                offenders.push(format!(
                    "{} compares an io kind ({:?} / undecidable {:?} / unreadable {unresolved:?})",
                    path.display(),
                    ev.channels.iter().map(label_of).collect::<Vec<_>>(),
                    ev.undetermined
                ));
            }
            for ty in ["IoProvide", "IoConsume"] {
                if code.contains(ty) {
                    offenders.push(format!("{} names {ty}", path.display()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a crate declared io-free now touches io:\n{}\n\
         Give the affected rule its own row with real channels — the family-level empty declaration \
         rests on this measurement and nothing else.",
        offenders.join("\n")
    );

    // ...and the crates that claim it must actually be the ones declaring nothing, so the list cannot
    // quietly stop covering a crate that went empty for the wrong reason.
    let io_free: BTreeSet<&str> = IO_FREE_CRATES.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        io_free,
        ["rules-graph", "rules-schema"].into_iter().collect(),
        "the io-free crate list changed — a crate joining it needs the measurement above run against \
         it, and a crate leaving it needs per-rule rows"
    );
}
