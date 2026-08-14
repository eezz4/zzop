//! Contracts 1-2: derived suppress-marker global uniqueness and the message "how to exclude" leg, plus
//! the published-surface leg (contract 2b) that pins WHICH rules the catalog pages may claim a marker for.
//!
//! Markers are no longer stored per rule — `RuleDef::suppress_marker()` DERIVES `zzop-<id>-ok` (see its doc; the `zzop-` TOOL PREFIX landed 2026-07-26 so a suppression comment can be grepped as a class and a reader can tell WHOSE checker it silenced).
//! That collapses three formerly-hand-guarded invariants into construction guarantees: every rule now has a
//! non-empty marker (ids are never empty), and every marker begins `zzop-` and ends `-ok` by definition. What derivation
//! does NOT guarantee is cross-pack uniqueness — two rules in different packs sharing an id would derive the
//! same marker and co-suppress — so that is the one presence/uniqueness invariant still worth a test.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use zzop_core::Matcher;

use crate::{load_all_packs, native_ids};

// ---------------------------------------------------------------------------------------------
// 0. The ENGINE-APPENDED suppress sentence — end to end
// ---------------------------------------------------------------------------------------------

/// A throwaway tree, same shape every `crates/engine/tests/analyze_*.rs` file builds (there is no shared
/// test-support crate — each `#[test]` binary is independent).
struct TempTree(PathBuf);

impl TempTree {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("zzop-marker-append-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp tree");
        TempTree(dir)
    }
    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(full, content).expect("write fixture");
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The engine — not the pack author — puts the suppress sentence into a line-scan finding's message.
///
/// The pack here deliberately says NOTHING about suppression, which is what makes this test a real one:
/// until the append existed, this rule's finding told a reader the problem and how to disable the rule
/// wholesale, and never that a one-line comment would do. 106 shipped rules used to buy that sentence by
/// typing it, byte-identically, into their own `message`.
///
/// The ORDER is asserted, not just the presence: the marker sentence goes in BEFORE
/// `zzop_core::disable_hint`'s fragment, because that is the order the 106 hand-written copies produced
/// and the append is pre-cache — a swap would rewrite every one of those messages and cold every warm
/// cache. `crates/engine/src/tests.rs`'s `dsl_finding_message_carries_the_config_disable_hint_for_its_own_id`
/// pins the disable hint's `ends_with` from the other side.
#[test]
fn the_engine_appends_the_suppress_sentence_to_a_line_scan_finding() {
    let pack: zzop_core::RulePackDef = serde_json::from_str(
        r#"{"id":"appendfix","rules":[{"id":"as-cast-probe","severity":"info",
            "message":"An `as` cast defeats the type checker. Prefer a type guard.",
            "matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"\\bas\\b"}}]}"#,
    )
    .expect("probe pack parses");

    let tree = TempTree::new();
    tree.write("src/a.ts", "export const x = y as Foo;\n");

    let out = zzop_engine::analyze_tree(
        &tree.0,
        &zzop_engine::EngineConfig {
            source_id: "marker-append-fixture".to_string(),
            packs: vec![pack],
            ..zzop_engine::EngineConfig::default()
        },
    );

    let hit = out
        .findings
        .iter()
        .find(|f| f.rule_id == "appendfix/as-cast-probe")
        .unwrap_or_else(|| {
            panic!(
                "the probe rule did not fire, so this test proved nothing about the append — findings: {:?}",
                out.findings.iter().map(|f| &f.rule_id).collect::<Vec<_>>()
            )
        });

    let expected_tail = format!(
        "Suppress a vetted case with `// zzop-as-cast-probe-ok`. {}",
        zzop_core::disable_hint("appendfix/as-cast-probe")
    );
    assert!(
        hit.message.ends_with(&expected_tail),
        "a line-scan finding's message must end with the engine-appended suppress sentence followed by \
         the disable hint.\n  expected tail: {expected_tail:?}\n  actual message: {:?}",
        hit.message
    );
}

/// The other half of the same contract, and the reason the append had to learn what matcher a rule uses:
/// io-scan findings must NOT get the sentence. Envelope Mode A carries no source text, so an io-scan
/// marker has no anchor line to be read off — telling a reader to add the comment would be the engine
/// lying about its own capability. Excluded by MATCHER KIND (`zzop_core::dsl::marker_channel`), never by
/// relying on today's two io-scan rules happening to spell the limitation out themselves.
#[test]
fn the_engine_appends_no_suppress_sentence_to_an_io_scan_finding() {
    let pack: zzop_core::RulePackDef = serde_json::from_str(
        r#"{"id":"appendfix","rules":[{"id":"io-probe","severity":"warning",
            "message":"An admin route with no auth evidence.",
            "matcher":{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","kind":"http","key_pattern":"(?i)/admin"}}]}"#,
    )
    .expect("probe pack parses");

    let tree = TempTree::new();
    tree.write(
        "src/routes.ts",
        "import express from 'express';\nconst app = express();\napp.get('/admin/users', (req, res) => res.json([]));\n",
    );

    let out = zzop_engine::analyze_tree(
        &tree.0,
        &zzop_engine::EngineConfig {
            source_id: "marker-append-fixture".to_string(),
            packs: vec![pack],
            ..zzop_engine::EngineConfig::default()
        },
    );

    let hit = out
        .findings
        .iter()
        .find(|f| f.rule_id == "appendfix/io-probe")
        .unwrap_or_else(|| {
            panic!(
                "the io-scan probe rule did not fire, so the exclusion below would be vacuous — findings: {:?}",
                out.findings.iter().map(|f| &f.rule_id).collect::<Vec<_>>()
            )
        });

    assert!(
        !hit.message.contains("Suppress a vetted case"),
        "an io-scan finding must never be told to add a suppress comment — its anchor line is re-read \
         through a callback envelope mode answers with None. Message was: {:?}",
        hit.message
    );
    assert!(
        hit.message
            .ends_with(&zzop_core::disable_hint("appendfix/io-probe")),
        "the io-scan exclusion must drop only the marker sentence, never the disable hint: {:?}",
        hit.message
    );
}

// ---------------------------------------------------------------------------------------------
// 1. Derived-marker global uniqueness
// ---------------------------------------------------------------------------------------------

/// No two shipped rules — in the same pack OR across packs — may derive the same suppress marker. Since the
/// marker is `zzop-<id>-ok`, this is exactly "rule ids are globally unique". It matters because a `// zzop-x-ok`
/// comment a reader placed to vet ONE rule's finding would silently also suppress any OTHER rule that
/// derives `zzop-x-ok` wherever their line/lookback windows overlap — the reader never opted into that. The
/// within-pack case was the old contract; deriving from the id widened the blast radius to every pack, so
/// the guard widens with it.
#[test]
fn derived_suppress_markers_are_globally_unique() {
    let packs = load_all_packs();
    let mut by_marker: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pack in &packs {
        for rule in &pack.rules {
            by_marker
                .entry(rule.suppress_marker())
                .or_default()
                .push(format!("{}/{}", pack.id, rule.id));
        }
    }
    let offenders: Vec<String> = by_marker
        .into_iter()
        .filter(|(_, rules)| rules.len() > 1)
        .map(|(marker, rules)| {
            format!("marker `{marker}` shared by rules {rules:?} (co-suppression risk)")
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "rules that derive a duplicate suppress marker: {offenders:#?}"
    );
}

/// Uniqueness above compares markers for EQUALITY, which is not the whole aliasing surface: `compile_marker`
/// anchors the marker as `//\s*<marker>\b`, and `\b` fires at a word/non-word boundary — so rule `x`'s marker
/// `zzop-x-ok` also matches inside rule `x-ok-y`'s marker `zzop-x-ok-y-ok` (the boundary sits between `k` and `-`).
/// A reader annotating a `x-ok-y` finding would silently suppress `x` on that line too, having opted into
/// neither. Zero shipped ids have this shape today (it needs an id containing `-ok-` or ending `-ok`), which
/// is exactly why it is worth pinning now — nothing else stops the first such id from being authored.
#[test]
fn no_derived_marker_is_a_word_boundary_prefix_of_another() {
    let packs = load_all_packs();
    let ids: Vec<String> = packs
        .iter()
        .flat_map(|pack| pack.rules.iter().map(|rule| rule.id.clone()))
        .collect();
    let offenders: Vec<String> = ids
        .iter()
        .flat_map(|shorter| {
            let prefix = format!("{shorter}-ok");
            ids.iter()
                .filter(move |longer| longer.as_str() != shorter && longer.starts_with(&prefix))
                .map(move |longer| {
                    format!(
                        "rule `{shorter}` (marker `{shorter}-ok`) also fires inside rule `{longer}`'s marker \
                         `{longer}-ok` (co-suppression risk)"
                    )
                })
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "rule ids whose derived markers alias by word boundary: {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Message triple — problem + fix + exclude (this leg)
// ---------------------------------------------------------------------------------------------

/// Every DSL rule's message, AS A READER RECEIVES IT, names its own derived suppress marker
/// (`zzop-<id>-ok`) OR the literal `disabled_rules`/`disabledRules` string — the "how to exclude" leg of
/// zzop's finding contract (every finding must tell the reader the problem, the fix, AND how to turn it
/// off; see docs/rules/authoring-guide.md's quality bar). A rule that legitimately has no per-finding
/// marker still passes via the `disabled_rules` leg — this accepts EITHER, not just the marker.
///
/// ## Why this reads the RENDERED message, not `rule.message`
/// It used to read the pack field, and that was sound only while every author typed the sentence
/// themselves. 106 of 143 rules did — with the SAME 44 bytes, `Suppress a vetted case with
/// `// zzop-<id>-ok`.`, carrying nothing a reader could not derive from the rule id. That sentence now
/// comes from `zzop_core::dsl::suppress_hint` at finding-construction time
/// (`zzop-engine`'s `pipeline::findings::append_hints`), so judging the pack field alone would fail all
/// 106 while every emitted finding was perfectly correct — and, worse, the same check on a future rule
/// would demand an author hand-write text the engine is about to add anyway.
///
/// The composition below mirrors the engine's own, minus `disable_hint` (which this contract must not
/// accept as the marker leg, and which `dsl_messages.rs`'s contract 17 forbids in a pack message
/// anyway). `append_hints`'s ORDER — suppress sentence, then disable hint — is pinned end-to-end by
/// [`the_engine_appends_the_suppress_sentence_to_a_line_scan_finding`] instead of here, because this
/// contract is about presence and that one is about bytes.
///
/// The kinds that get NO appended sentence (symbol-scan has no anchor line, io-scan's anchor line is
/// re-read through a callback envelope mode answers with `None`) are therefore held to the ORIGINAL bar:
/// their author must still say how to exclude, in their own words. Both shipped io-scan rules do.
#[test]
fn every_dsl_rule_message_documents_how_to_exclude_it() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    let mut appended_for = 0usize;
    for pack in &packs {
        for rule in &pack.rules {
            let marker = rule.suppress_marker();
            let rendered = match zzop_core::dsl::suppress_hint(rule) {
                Some(sentence) => {
                    appended_for += 1;
                    format!("{} {sentence}", rule.message)
                }
                None => rule.message.clone(),
            };
            let marker_leg = rendered.contains(&marker);
            let disabled_leg =
                rendered.contains("disabled_rules") || rendered.contains("disabledRules");
            if !(marker_leg || disabled_leg) {
                offenders.push(format!(
                    "{}/{} (derived marker `{marker}`) — the message a reader receives mentions neither \
                     its own marker nor disabled_rules/disabledRules",
                    pack.id, rule.id
                ));
            }
        }
    }
    // NON-VACUITY, in the direction this contract just became able to fail silently. If `suppress_hint`
    // ever returned `None` for everything — a matcher arm flipped, the opt-out widened — every rule
    // would be judged on its pack text alone and the 106 folded rules would go red LOUDLY, which is
    // fine. The dangerous direction is the opposite: nothing here would notice the engine having
    // stopped appending, because the pack text of the 33 self-describing rules would still carry the
    // marker and mask it. So the count of rules relying on the append is asserted to be non-zero.
    assert!(
        appended_for > 0,
        "not one of the {} loaded rules gets an engine-appended suppress sentence. Either every rule \
         hand-writes its own again (the duplication this fold removed), or `suppress_hint` stopped \
         returning one — and in the second case this contract is now passing on pack text while every \
         finding a user reads has lost its suppress sentence.",
        packs.iter().map(|p| p.rules.len()).sum::<usize>()
    );
    assert!(
        offenders.is_empty(),
        "rule messages missing the \"how to exclude\" leg: {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 2b. Published-surface leg — the catalog pages may only claim a marker for a rule that has one
// ---------------------------------------------------------------------------------------------

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The pages whose ROWS are checked — every `.md` and `.html` in the repository, DERIVED from git the
/// way `scripts/check-docs-rule-ids.sh` derives its own subject set, and for the identical reason its
/// header records: that script began as two hard-typed paths under a comment claiming to list "the four
/// real user-facing surfaces", and a violation planted in a third surface left it green.
///
/// This constant had the same shape and the same hole until 2026-07-29 — `["../../docs/rules/catalog.md",
/// "../../site/rules.html"]`, hand-typed, with no mechanism that could notice a third page growing rule
/// rows. Measured before the fix: a `docs/rules/planted-surface.md` carrying one row that told readers
/// to suppress `cross-layer/prefix-drift` (a rule that honors NO marker) with an inline marker left this
/// file's contract green.
///
/// Scanning EVERY doc rather than a rules-only subset is deliberate and costs nothing in precision: the
/// contract below only judges a row whose id it recognizes ([`rule_rows`] + the `known` filter), so a
/// document with no rule rows contributes nothing. There is therefore no discriminator to get wrong —
/// the row shape IS the discriminator, which is the property the shell guard had to approximate with a
/// `"severity"` prefilter because grep has no cheaper way to ask.
///
/// UNTRACKED-but-not-ignored files are included alongside tracked ones, matching
/// `scripts/check-max-file-lines.sh`'s two-call `list_rs_files`. A page added in the working tree is
/// exactly the page whose claims nobody has reviewed yet, and a guard that only sees committed bytes
/// reports on the previous commit rather than on the one being made.
fn row_surfaces() -> Vec<(String, String)> {
    let root = workspace_root();
    let list = |extra: &[&str]| -> Vec<String> {
        let mut cmd = std::process::Command::new("git");
        cmd.arg("-C").arg(&root).arg("ls-files");
        cmd.args(extra);
        let out = cmd
            .args(["--", "*.md", "*.html"])
            .output()
            .unwrap_or_else(|e| {
                panic!(
                    "could not run `git ls-files` in {} ({e}) — this contract DERIVES the pages it \
                     checks from git, so without it there is no subject set and a green result would \
                     mean nothing was read",
                    root.display()
                )
            });
        assert!(
            out.status.success(),
            "`git ls-files {extra:?}` failed in {}: {}",
            root.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    };

    // No exclusion list. `--exclude-standard` already applies every ignore file git itself honors —
    // repo, per-user global, and `.git/info/exclude` — so build output and local-only tooling state are
    // filtered by the SAME rules that decide what this repo would ever publish, rather than by a set of
    // path prefixes typed here. That is the point: a hand-typed exclusion list is the identical defect,
    // one level down. The sibling shell guards spell three prefixes out because `git ls-files` is only
    // half of what they enumerate; here it is all of it, and the census below prints what was read.
    let mut rels: Vec<String> = list(&[])
        .into_iter()
        .chain(list(&["--others", "--exclude-standard"]))
        .collect();
    rels.sort();
    rels.dedup();

    assert!(
        !rels.is_empty(),
        "`git ls-files` matched ZERO .md/.html files in {} — the surface enumeration is broken, not the \
         repo empty. This repo ships docs/rules/catalog.md and site/rules.html at minimum; a zero here \
         would let every page claim any marker it liked",
        root.display()
    );

    rels.into_iter()
        .filter_map(|rel| {
            // A tracked-but-deleted path stays listed until the deletion is committed — skip it rather
            // than panicking, the same degradation `check-max-file-lines.sh`'s `existing_only` applies.
            let text = std::fs::read_to_string(root.join(&rel)).ok()?;
            Some((rel, text))
        })
        .collect()
}

/// The hand-authored native marker literal, spelled once here and asserted to still exist in the scanner.
const HAND_AUTHORED_NATIVE_MARKER: &str = "idempotent-ok";

/// The one function in `rules/native/` that actually READS a source line looking for an inline marker.
/// Everything below is derived from its definition and its callers — see [`native_marker_honoring_ids`]
/// for why that is the only sound derivation path.
const MARKER_WINDOW_FN: &str = "scan_marker_window";

/// `(path, comment-free text)` for every SHIPPED `.rs` file under `rules/native/`.
///
/// Line comments (`//`, `///`, `//!`) are dropped whole, for the same reason `surface_parity`'s
/// `emission_text` drops them: prose that MENTIONS a function is not a place that calls it. That is not
/// a hypothetical here — `http_scan.rs`'s own doc writes ``[`scan_marker_window`] is the definition``
/// and `http_scan/tests.rs` writes ``while `scan_marker_window` reads the body-start line``, and a
/// bare-substring derivation would read both as call sites. Test sources are dropped outright
/// ([`crate::is_test_source`]): a marker-suppression unit test necessarily calls the marker reader, and
/// counting it would make every rule in a crate look marker-honoring because one of its tests exercised
/// the scanner.
fn native_rule_sources() -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    crate::collect_rs_files(&crate::native_dir(), &mut files);
    files.retain(|path| !crate::is_test_source(path));
    let sources: Vec<(PathBuf, String)> = files
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let code = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            Some((path, code))
        })
        .collect();
    assert!(
        !sources.is_empty(),
        "no shipped .rs file found under {} — the native-rule scan root is gone or became test-only, \
         and every derivation below would return an EMPTY set while reading nothing",
        crate::native_dir().display()
    );
    sources
}

/// Whether `code` contains a CALL to `name` — `name` immediately followed by `(`, so a `use super::{..,
/// name, ..}` import line (no parenthesis) and a `fn name<T>(` definition (a generic parameter list
/// between the two) are both excluded by shape rather than by a special case.
fn calls(code: &str, name: &str) -> bool {
    regex::Regex::new(&format!(r"\b{}\s*\(", regex::escape(name)))
        .expect("static shape, escaped name")
        .is_match(code)
}

/// The names of the functions that CALL [`MARKER_WINDOW_FN`] — i.e. the marker-reading API every
/// rule-level honor has to go through. Derived by tracking the enclosing `fn` while walking each file:
/// the last `fn <name>` opened before the call line owns it.
///
/// The definition itself must exist exactly once and must be called at least once; both are asserted,
/// because either failing turns every derivation below into a silent empty set.
fn marker_window_reader_fns(sources: &[(PathBuf, String)]) -> BTreeSet<String> {
    let fn_re =
        regex::Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([a-z_][a-z0-9_]*)")
            .expect("static regex");
    let mut definitions = 0usize;
    let mut readers = BTreeSet::new();
    for (_path, code) in sources {
        let mut current: Option<String> = None;
        for line in code.lines() {
            if let Some(c) = fn_re.captures(line) {
                let name = c[1].to_string();
                if name == MARKER_WINDOW_FN {
                    definitions += 1;
                }
                current = Some(name);
            }
            if calls(line, MARKER_WINDOW_FN) {
                if let Some(name) = current.as_deref() {
                    if name != MARKER_WINDOW_FN {
                        readers.insert(name.to_string());
                    }
                }
            }
        }
    }
    assert_eq!(
        definitions, 1,
        "expected exactly ONE `fn {MARKER_WINDOW_FN}` definition under rules/native/, found \
         {definitions}. Zero means it was renamed or retired and every derivation below silently \
         returns an empty set; more than one means two independent marker readers exist and this \
         single-seed derivation no longer describes them."
    );
    assert!(
        !readers.is_empty(),
        "`fn {MARKER_WINDOW_FN}` is defined but never called from any shipped native source — no rule \
         can honor an inline marker through it, so this derivation would report NOTHING honoring while \
         the catalog pages keep promising a marker"
    );
    readers
}

/// The native analysis ids whose emitting file reads the inline marker window — the DERIVED replacement
/// for what was, until 2026-07-29, a hand-written `NATIVE_MARKER_HONORING_IDS` pair.
///
/// ## Why this derivation path and not a grep for the marker literal
/// The obvious route — grep `rules/native/` for `idempotent-ok` — is a trap, and a measured one. Two
/// files spell that literal in order to DENY it: `rules-cross-layer/src/cross_layer/mod.rs` and
/// `rules-schema/src/message.rs` both write out the marker in a "this crate honors none" sentence. A
/// literal grep therefore reports the two crates that most explicitly do NOT honor a marker as honoring
/// one — the same shape as the `cli_only_vocabulary()` defect this repo hit the day before, where a
/// discriminator-free scan matched prose instead of behaviour. Anchoring on the CALL removes the class
/// outright: prose cannot call a function.
///
/// ## What "honoring" means here, stated precisely
/// A file qualifies when it calls one of [`marker_window_reader_fns`], which today is
/// `is_whitelisted` (the marker SUPPRESSES the finding) or `with_ok_marker_near_miss` (a marker-shaped
/// comment that did not suppress is DISCLOSED in the message). Both are counted, and the second is not
/// a looseness: the near-miss text it appends says, in the finding the user reads, "the marker this
/// rule honors is `idempotent-ok:`". A file making that claim is exactly a file whose catalog row may
/// repeat it, which is the contract this set feeds.
///
/// ## Granularity, stated honestly
/// FILE-level: every `rule_id: "<id>"` literal in a marker-reading file is taken as honoring. A file
/// emitting two rule ids where only one is marker-gated would over-report. No such file exists (each
/// native rule owns its own module), and the failure would be LOUD rather than silent — an id wrongly
/// in this set makes any catalog row that correctly DENIES a marker fail the contract below. A file
/// that reads the marker window but exposes no `rule_id` literal at all (an id built from a `const` or
/// threaded through a struct field, both of which exist elsewhere in `rules/native/`) is a real blind
/// spot, so it is asserted against rather than skipped.
fn native_marker_honoring_ids() -> BTreeSet<String> {
    let sources = native_rule_sources();
    let readers = marker_window_reader_fns(&sources);
    let id_re = regex::Regex::new("rule_id:\\s*\"([a-z0-9][a-z0-9/_-]*)\"").expect("static regex");
    let mut ids = BTreeSet::new();
    for (path, code) in &sources {
        if !readers.iter().any(|reader| calls(code, reader)) {
            continue;
        }
        let found: Vec<String> = id_re
            .captures_iter(code)
            .map(|c| c[1].to_string())
            .collect();
        // The file that DEFINES the readers is shared plumbing, not a rule — it emits no finding, so
        // yielding no id there is correct rather than a blind spot.
        let defines_a_reader = readers.iter().any(|reader| {
            regex::Regex::new(&format!(r"fn\s+{}\b", regex::escape(reader)))
                .expect("static shape, escaped name")
                .is_match(code)
        });
        assert!(
            !found.is_empty() || defines_a_reader,
            "{} reads the inline marker window but exposes no `rule_id: \"<id>\"` literal, so this \
             derivation cannot tell WHICH rule honors the marker — its findings would be missing from \
             the honoring set and the catalog pages could deny a marker this file honors. Spell the id \
             as a literal at the `Finding` construction site, or teach this derivation the new shape.",
            path.display()
        );
        ids.extend(found);
    }
    assert!(
        !ids.is_empty(),
        "NO native rule reads the inline marker window. If the hand-authored `{HAND_AUTHORED_NATIVE_MARKER}` \
         exception was deliberately retired, that is correct — but then docs/rules/catalog.md, \
         site/rules.html and docs/getting-started.md must stop promising it in the SAME commit, and this \
         assertion is where you find out."
    );
    ids
}

/// Every rule id (bare and, for DSL rules, pack-qualified) whose findings an inline marker can actually
/// suppress — read from the same data the engine loads, never a hand-copied list. DSL: every matcher
/// except `symbol-scan`, whose findings have no source line to anchor a comment against
/// (`RuleDef::suppress_marker` still derives a string for it, but nothing ever consults the result — see
/// `crates/facade/src/explain/render.rs`'s `suppress_marker_str`). Native:
/// [`native_marker_honoring_ids`], derived from the marker reader's own call sites.
fn marker_honoring_ids() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for pack in &load_all_packs() {
        for rule in &pack.rules {
            if matches!(rule.matcher, Matcher::SymbolScan(_)) {
                continue;
            }
            ids.insert(rule.id.clone());
            ids.insert(format!("{}/{}", pack.id, rule.id));
        }
    }
    ids.extend(native_marker_honoring_ids());
    ids
}

/// `(rule id, whole row text)` for every rule row on a catalog surface. Both surfaces put one row on one
/// line, opened by the id: Markdown as ``| `id` |``, HTML as `<tr><td><code>id</code></td>`. Anchored to
/// the row OPENER, not a bare id mention, for the same reason `scripts/check-rules-catalog-sync.sh` is:
/// rows cross-reference each other by id, and a loose match would let one rule's prose testify for another.
fn rule_rows(text: &str) -> Vec<(String, &str)> {
    let md = regex::Regex::new(r"^\| `([a-z0-9][a-z0-9/_-]*)`").expect("static regex");
    let html = regex::Regex::new(r"^\s*<tr><td><code>([a-z0-9][a-z0-9/_-]*)</code></td>")
        .expect("static regex");
    text.lines()
        .filter_map(|line| {
            md.captures(line)
                .or_else(|| html.captures(line))
                .map(|c| (c[1].to_string(), line))
        })
        .collect()
}

/// Prose that AFFIRMS a per-site marker: a suppress/disable word and the word "marker" inside one
/// sentence. Sentence-bounded (`[^.]`) so an unrelated later sentence cannot supply the second half, and
/// direction-agnostic so both "suppress ... with the marker" and "the ... marker remains the escape hatch"
/// are caught. Deliberately NOT triggered by "marker" used as domain vocabulary (`soft-delete-bypass`'s
/// `deletedAt` marker FIELD), which carries no suppress/disable word in the same sentence.
fn affirms_a_marker(row: &str) -> bool {
    static AFFIRM: std::sync::OnceLock<[regex::Regex; 2]> = std::sync::OnceLock::new();
    let res = AFFIRM.get_or_init(|| {
        [
            regex::Regex::new(r"(?i)(?:suppress\w*|disable\w*)[^.]{0,80}?\bmarker\b")
                .expect("static regex"),
            regex::Regex::new(r"(?i)\bmarker\b[^.]{0,80}?(?:suppress\w*|escape hatch)")
                .expect("static regex"),
        ]
    });
    res.iter().any(|re| re.is_match(row))
}

/// The one canonical way a row says a rule has NO marker — spelled identically on both surfaces today.
fn denies_a_marker(row: &str) -> bool {
    row.to_lowercase().contains("no inline suppression marker")
}

/// Contract 2b: on `docs/rules/catalog.md` and `site/rules.html`, a rule row may claim a per-site
/// suppression marker only if that rule actually honors one, and may deny one only if it does not.
///
/// Why it needs a pin: this exact contradiction shipped. `site/rules.html`'s
/// `cross-layer/retrying-write-no-idempotency` row said "the per-site disable marker remains the escape
/// hatch" while `docs/rules/catalog.md`'s row for the same rule said it "honors NO inline suppression
/// marker" — and the code honors none. A user trusting the site would have pasted a comment that does
/// nothing and read the resulting silence as safety. Nothing was red: the suppression contract is prose,
/// and `scripts/check-rules-catalog-sync.sh` compares only ids and `.rs` paths between the two pages.
///
/// Both sides come from what ships — the honoring set from the loaded packs and the native registry's
/// documented exception, the claims from the pages' own bytes — so this file holds no third copy to drift.
/// Scope, stated honestly: ROWS only. A page-level blanket claim in intro prose ("native analyses do not
/// support inline suppression", which was also live and also false) is not keyed by a rule id and cannot
/// be checked this way without pinning wording; the durable fix for that half is unifying the contract so
/// there is no per-family exception left to state.
#[test]
fn catalog_surfaces_claim_a_suppression_marker_only_for_rules_that_honor_one() {
    let honoring = marker_honoring_ids();
    let known: BTreeSet<String> = honoring.iter().cloned().chain(native_ids()).collect();
    let mut offenders = Vec::new();
    let mut rows_judged = 0usize;

    for (rel, text) in row_surfaces() {
        for (id, row) in rule_rows(&text) {
            if !known.contains(&id) {
                continue; // not a rule row (matchers, config keys, ... share the table shape)
            }
            rows_judged += 1;
            let honors = honoring.contains(&id);
            if !honors && affirms_a_marker(row) && !denies_a_marker(row) {
                offenders.push(format!(
                    "{rel}: `{id}` honors NO inline marker, but its row claims one — row reads: {row}"
                ));
            }
            if honors && denies_a_marker(row) {
                offenders.push(format!(
                    "{rel}: `{id}` DOES honor an inline marker, but its row denies one — row reads: {row}"
                ));
            }
        }
    }
    // NON-VACUITY GUARD, the necessary companion to a derived subject set. `row_surfaces` already fails
    // on a zero-file enumeration, but that is the weaker bar: every page could be readable and yet no
    // ROW recognized (the table markup changed, `rule_rows`' anchors went stale), and this contract
    // would pass having judged nothing at all — a green that means "found no claims to check", read as
    // "every claim checks out". That is the same false green the hand list produced, arrived at from
    // the other direction, so it is asserted rather than assumed.
    assert!(
        rows_judged > 0,
        "scanned {} .md/.html page(s) and recognized ZERO rule rows keyed by a known id — this repo \
         publishes a rule catalog, so either `rule_rows`' Markdown/HTML row anchors no longer match \
         what the pages emit, or the pages stopped carrying rows. Nothing was verified.",
        row_surfaces().len()
    );
    assert!(
        offenders.is_empty(),
        "published rule rows whose suppression claim contradicts the code: {offenders:#?}"
    );
}

/// Keeps [`native_marker_honoring_ids`] honest from the other end: the hand-authored `// idempotent-ok:`
/// literal the whole exception exists for must still be in the scanner, and every id the derivation
/// yields must be a REGISTERED native analysis — an id that ships no analysis cannot honor anything, and
/// would let the contract above vouch for a page row about a rule that does not run.
///
/// The scanner is LOCATED, not hardcoded: it is the file defining the marker readers
/// [`marker_window_reader_fns`] found. The path `../../rules/native/rules-http/src/http_scan.rs` used
/// to be typed here, which meant relocating the scanner turned this pin into a `cannot read` panic
/// blaming the wrong thing, and splitting it across two files would have left the pin watching whichever
/// half kept the old name.
#[test]
fn the_hand_authored_native_marker_still_exists_in_the_scanner() {
    let sources = native_rule_sources();
    let readers = marker_window_reader_fns(&sources);
    let scanners: Vec<&PathBuf> = sources
        .iter()
        .filter(|(_, code)| {
            readers.iter().any(|reader| {
                regex::Regex::new(&format!(r"fn\s+{}\b", regex::escape(reader)))
                    .expect("static shape, escaped name")
                    .is_match(code)
            })
        })
        .map(|(path, _)| path)
        .collect();
    assert!(
        !scanners.is_empty(),
        "found no shipped native source DEFINING any of the marker readers {readers:?} — they are \
         called from somewhere but defined nowhere this scan can see, so the derivation's seed is broken"
    );
    let carrying: Vec<&&PathBuf> = scanners
        .iter()
        .filter(|path| {
            std::fs::read_to_string(path)
                .unwrap_or_default()
                .contains(HAND_AUTHORED_NATIVE_MARKER)
        })
        .collect();
    assert!(
        !carrying.is_empty(),
        "the hand-authored `{HAND_AUTHORED_NATIVE_MARKER}` marker is gone from every file defining a \
         marker reader ({scanners:?}) — if that exception was retired, update docs/rules/catalog.md, \
         site/rules.html and docs/getting-started.md in the same commit"
    );

    let registered = native_ids();
    for id in native_marker_honoring_ids() {
        assert!(
            registered.contains(&id),
            "the marker-honoring derivation yielded `{id}`, which is not a registered native analysis \
             id — either the rule was renamed on one side only, or the `rule_id:` literal scan picked \
             up something that is not a shipped id"
        );
    }
}
