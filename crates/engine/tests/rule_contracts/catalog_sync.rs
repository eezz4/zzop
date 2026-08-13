//! Contract 5: catalog sync — docs/rules/catalog.md must match the loaded reality, not a hand-updated
//! snapshot. Totals, id mentions, and (since 2026-07-31) the sightline surface: the set of catalog rows
//! carrying a sightline paragraph must equal the set of `RuleSightline` declarations the build composes.

use std::fs;
use std::path::{Path, PathBuf};

use zzop_core::Matcher;

use crate::{load_all_packs, native_ids};

fn catalog_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/rules/catalog.md")
}

fn catalog_text() -> String {
    let path = catalog_path();
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Parses `docs/rules/catalog.md`'s totals sentence (the `**Totals** (...): N DSL packs, N DSL rules, N
/// native analysis ids.` line near the top of the file) and asserts the three numbers match what
/// `load_dsl_packs`/`register_native_analyses` actually produce. The sentence is intentionally phrased in
/// one fixed, easily-`regex`-parsed shape (restructured for this test — see the doc's own totals line) so a
/// human editing prose around it doesn't accidentally break this test's ability to find the numbers; if you
/// legitimately need to reword that sentence, keep the `N DSL packs, N DSL rules, N native analysis ids`
/// clause's shape intact (or update this regex to match, deliberately, in the same commit).
#[test]
fn catalog_totals_match_loaded_rule_and_analysis_counts() {
    let text = catalog_text();
    let re =
        regex::Regex::new(r"(\d+)\s+DSL packs,\s*(\d+)\s+DSL rules,\s*(\d+)\s+native analysis ids")
            .expect("static regex");
    let caps = re.captures(&text).unwrap_or_else(|| {
        panic!(
            "docs/rules/catalog.md's totals sentence not found in the expected \"N DSL packs, N DSL \
             rules, N native analysis ids\" shape — either the doc's totals line was reworded (update it \
             back to that shape, or update this test's regex in the same commit) or the file moved"
        )
    });
    let stated_packs: usize = caps[1].parse().expect("digits");
    let stated_rules: usize = caps[2].parse().expect("digits");
    let stated_natives: usize = caps[3].parse().expect("digits");

    let packs = load_all_packs();
    let actual_rules: usize = packs.iter().map(|p| p.rules.len()).sum();
    let actual_natives = native_ids().len();

    assert_eq!(
        stated_packs,
        packs.len(),
        "catalog.md states {stated_packs} DSL packs, but rules/dsl/*.json loads {}",
        packs.len()
    );
    assert_eq!(
        stated_rules, actual_rules,
        "catalog.md states {stated_rules} DSL rules, but the loaded packs total {actual_rules}"
    );
    assert_eq!(
        stated_natives, actual_natives,
        "catalog.md states {stated_natives} native analysis ids, but register_native_analyses registers \
         {actual_natives}"
    );
}

#[test]
fn catalog_mentions_every_native_analysis_id() {
    let text = catalog_text();
    let missing: Vec<String> = native_ids()
        .into_iter()
        .filter(|id| !text.contains(id.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "native analysis ids registered but absent from docs/rules/catalog.md's text: {missing:?}"
    );
}

#[test]
fn catalog_mentions_every_dsl_pack_id() {
    let text = catalog_text();
    let packs = load_all_packs();
    let missing: Vec<&str> = packs
        .iter()
        .map(|p| p.id.as_str())
        .filter(|id| !text.contains(id))
        .collect();
    assert!(
        missing.is_empty(),
        "DSL pack ids loaded but absent from docs/rules/catalog.md's text: {missing:?}"
    );
}

/// The gap the v0.23.0 release audit found: the pins above cover TOTALS, native ids and PACK ids, so a
/// count-preserving rule-id RENAME — exactly what `df78842` did to 7 ids — would ship with the catalog
/// still naming the old ids and no test objecting. That matters more than an ordinary doc drift: the
/// catalog is `include_str!`-embedded as the MCP resource `zzop://contract/rule-catalog`
/// (`crates/summary/src/contracts.rs`), and `scripts/check-docs-rule-ids.sh` DERIVES its valid-id universe
/// from this same file — so one stale id here becomes a stale wire contract AND blesses stale ids in
/// every other doc the guard checks.
///
/// Matched as `` `<id>` `` (backticked), which is how every rule row spells its id — a bare `contains`
/// would let a rule id that appears only inside prose about a different rule count as present.
#[test]
fn catalog_mentions_every_dsl_rule_id() {
    let text = catalog_text();
    let packs = load_all_packs();
    let missing: Vec<String> = packs
        .iter()
        .flat_map(|p| p.rules.iter().map(move |r| (p.id.as_str(), r.id.as_str())))
        .filter(|(_, rule_id)| !text.contains(&format!("`{rule_id}`")))
        .map(|(pack_id, rule_id)| format!("{pack_id}/{rule_id}"))
        .collect();
    assert!(
        missing.is_empty(),
        "DSL rule ids loaded but absent from docs/rules/catalog.md as `<id>`: {missing:?} — a rule \
         rename must update the catalog in the same commit (it is the MCP rule-catalog resource and \
         check-docs-rule-ids.sh's id universe)"
    );
}

fn linter_overlap_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/rules/linter-overlap.md")
}

/// `docs/rules/linter-overlap.md` answers, per BUNDLED rule, "can you name the tool that already sees
/// this?" — the bar that decides whether a rule stays bundled or is an export candidate. It is an
/// inventory keyed by rule id, so it has exactly the failure mode its sibling above guards against: a
/// rule added to the bundle with no row here leaves the document silently claiming complete coverage of
/// a set it no longer covers.
///
/// That silence would be worse here than a plain stale count, because the document's whole purpose is
/// to be exhaustive — its closing claim is "the remaining rows are what zzop adds over the linters you
/// already have", and an uncovered rule makes that sentence false without changing a single visible
/// number. A count assertion would not catch it either, since the count lives in prose derived from the
/// same set; membership is the only honest check.
///
/// Deliberately NOT asserting a total: adding a rule is normal and must not break a test for arithmetic
/// reasons. The event worth failing on is a rule with no verdict.
#[test]
fn every_bundled_rule_has_a_linter_overlap_verdict() {
    let path = linter_overlap_path();
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let packs = load_all_packs();
    let missing: Vec<String> = packs
        .iter()
        .flat_map(|p| p.rules.iter().map(move |r| format!("{}/{}", p.id, r.id)))
        .filter(|qualified| !text.contains(&format!("`{qualified}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "bundled rules with no row in docs/rules/linter-overlap.md: {missing:?} — that document is an \
         EXHAUSTIVE per-rule inventory (it closes by claiming the rows are what zzop adds over the \
         linters a user already runs), so a rule missing from it makes that claim false while every \
         number in the file still reads correct. Add a row naming the nearest standard-linter \
         equivalent, or `none`, and a verdict. Note the ids there are written QUALIFIED (`pack/rule`), \
         unlike the catalog, which uses the bare rule id."
    );
}

/// The rule ids whose catalog row publishes a sightline claim. A row "claims" when it contains the
/// phrase `language sightline` or `evidence sightline` (case-insensitive) — deliberately the PHRASE,
/// not only the full `**Language sightline:**` / `**Evidence sightline:**` marker, because half the
/// claiming rows today claim by back-reference instead (`Same language sightline as
/// \`soft-delete-bypass\` above`, `the same **language sightline**:`, `Same evidence sightline as the
/// row above`) and a marker-only parse would read those four rows as claim-free. The claim is
/// attributed to the row's OWN id (first backticked cell) — never to ids the prose mentions.
fn catalog_sightline_row_ids(text: &str) -> std::collections::BTreeSet<String> {
    let phrase = regex::Regex::new(r"(?i)(language|evidence) sightline").expect("static regex");
    let row_id = regex::Regex::new(r"^\|\s*`([^`]+)`\s*\|").expect("static regex");
    let mut ids = std::collections::BTreeSet::new();
    for line in text.lines().filter(|l| phrase.is_match(l)) {
        let caps = row_id.captures(line).unwrap_or_else(|| {
            panic!(
                "docs/rules/catalog.md mentions a sightline outside a `| `<id>` | ...` rule row, so \
                 this guard cannot attribute the claim to a rule id — move the sentence into the \
                 owning rule's row, or extend catalog_sightline_row_ids deliberately: {line:?}"
            )
        });
        ids.insert(caps[1].to_string());
    }
    ids
}

/// Rule ids allowed to carry a catalog sightline paragraph WITHOUT a `RuleSightline` declaration —
/// `(id, reason)`, and an entry must actually exempt something (asserted below) so it cannot go stale.
///
/// EMPTY today, on purpose. The two candidates the sightline DECISIONS block records as deliberately
/// undeclared (`crates/engine/src/sightlines.rs`) need no entry because their catalog rows carry no
/// sightline paragraph either — both directions already agree — and the red a future marker on either
/// row would raise is CORRECT, not noise:
/// - `mutating-route-no-auth`: its trigger IS witnessed in every call-graph-covered extension (`.go`
///   routes included) — the gap is call-EDGE evidence, route-conditional, owned by the S8
///   framework-silence warning the coverage reply forwards per tree. A catalog sightline paragraph
///   would mis-type that per-run conditional disclosure as a build capability: on red, remove the
///   marker — unless the rule's disclosure has genuinely become extension-conditional, in which case
///   declare a `RuleSightline` next to the rule instead.
/// - `route-shadowing`: its language gate is an EXEMPTION on routing semantics (first-match vs
///   most-specific), not an evidence channel — semantic, not evidential, so an extension cross cannot
///   express it. Same red-on-marker reasoning.
const CATALOG_SIGHTLINE_EXEMPT: &[(&str, &str)] = &[];

/// The REVERSE guard of the sightline mechanism. `crates/engine/src/sightlines.rs` pins declared →
/// registered, but nothing pinned the prose SSOT against the machine half: a rule publishing a
/// sightline paragraph in docs/rules/catalog.md while declaring no `RuleSightline` failed silently —
/// exactly how `mutating-route-no-auth`'s (deliberate) absence reached stage-2 review with two owners
/// disagreeing about whether it was an omission. Set equality, both directions:
/// (a) every catalog sightline row is declared or exempted — a new rule shipping the paragraph
///     without the declaration goes red;
/// (b) every declaration's id has a catalog sightline row — the machine half must never claim more
///     than the prose SSOT.
#[test]
fn catalog_sightline_rows_and_declared_rule_sightlines_are_the_same_set() {
    let catalog = catalog_sightline_row_ids(&catalog_text());
    let declared: std::collections::BTreeSet<String> = zzop_engine::rule_sightlines()
        .iter()
        .map(|s| s.rule_id.to_string())
        .collect();
    assert!(
        !catalog.is_empty(),
        "sanity: no catalog sightline rows found at all"
    );
    assert!(
        !declared.is_empty(),
        "sanity: no RuleSightline declared at all"
    );

    let exempt: std::collections::BTreeSet<&str> =
        CATALOG_SIGHTLINE_EXEMPT.iter().map(|(id, _)| *id).collect();
    for (id, reason) in CATALOG_SIGHTLINE_EXEMPT {
        assert!(
            catalog.contains(*id) && !declared.contains(*id),
            "stale sightline exemption {id:?} ({reason}) — it no longer exempts anything (the row \
             dropped its sightline paragraph, or the rule now declares); delete the entry"
        );
    }

    let undeclared: Vec<&String> = catalog
        .iter()
        .filter(|id| !declared.contains(*id) && !exempt.contains(id.as_str()))
        .collect();
    assert!(
        undeclared.is_empty(),
        "docs/rules/catalog.md publishes a sightline paragraph for {undeclared:?}, but \
         zzop_engine::rule_sightlines() declares no RuleSightline for them — the coverage query would \
         stay silent about a blind spot the prose promises. Declare a RuleSightline next to the rule \
         (see the owning crate's rule_sightlines()), or add a documented exemption to \
         CATALOG_SIGHTLINE_EXEMPT with its reason"
    );

    let unpublished: Vec<&String> = declared
        .iter()
        .filter(|id| !catalog.contains(*id))
        .collect();
    assert!(
        unpublished.is_empty(),
        "RuleSightline declared for {unpublished:?}, but their docs/rules/catalog.md rows carry no \
         sightline paragraph — the machine half must never claim more than the prose SSOT; add the \
         row's sightline paragraph in the same commit (or drop the declaration)"
    );
}
/// The `file_pattern` of any matcher variant — the one field every variant carries, reached without
/// asking what kind of matcher it is. `capability_matrix`'s `required_channels` deliberately answers
/// `None` for `LineScan` (it is the universal channel and has no required IR channel to check), which
/// makes it the wrong accessor here: the claim below is about a pack that is nearly all line-scan.
fn file_pattern_of(matcher: &Matcher) -> &str {
    match matcher {
        Matcher::LineScan(m) => &m.file_pattern,
        Matcher::MethodScan(m) => &m.file_pattern,
        Matcher::SymbolScan(m) => &m.file_pattern,
        Matcher::IoScan(m) => &m.file_pattern,
        Matcher::CallScan(m) => &m.file_pattern,
        Matcher::LiteralScan(m) => &m.file_pattern,
    }
}

/// Contract 5's per-pack LANGUAGE-ADMISSION claim: a catalog sentence of the shape
/// `N rules in this pack admit `.<ext>`` must state the number the pack's own matchers produce.
///
/// ## Why this needs its own check rather than a `check-deploy-facts-prose.sh` claim shape
///
/// That guard owns every other inventory count in this repo's prose, and it could not own this one.
/// Its claim shapes are matched by `grep -HnoE` and its truths are derived in `awk`; the truth here
/// requires EVALUATING each rule's `file_pattern` regex, which needs the same regex engine the
/// engine itself uses (`(?i)` is not POSIX ERE, so awk cannot even parse the patterns). A shell
/// approximation would have to match the pattern's TEXT instead — and that approximation is exactly
/// the mistake made on 2026-08-12 while measuring the Python marker widening: scanning for `py` in
/// the pattern text missed `(?i)\.pyi?$`, and the blast radius was reported one rule short until the
/// full test suite caught it. Text-matching a regex is not the same question as running it.
///
/// ## Why the claim existed unguarded until now
///
/// `docs/rules/catalog.md` said "**Fifteen** rules in this pack admit `.java`" against a truth of 18
/// (fixed 96400b1). `check-deploy-facts-prose.sh` could not see it for a second reason on top of the
/// one above: its claim shapes are NUMERIC, and an English number word is structurally invisible to
/// them. The prose is a digit now, which is what makes a machine check possible at all.
///
/// The probe path is a bare `probe.<ext>` AND a deep one, and both must agree: a pattern that admits
/// the extension only under some directory layout (`alembic/versions/...`) would make the count a
/// function of the probe rather than of the language, and this claim is about the language.
#[test]
fn every_catalog_language_admission_claim_matches_what_the_packs_matchers_admit() {
    let text = catalog_text();
    let packs = load_all_packs();

    // The heading that owns each claim: `### `<pack>` (N rules)`.
    let heading_re = regex::Regex::new(r"(?m)^### `([a-z0-9-]+)` \(\d+ rules?\)").expect("static");
    let claim_re =
        regex::Regex::new(r"(\d+) rules in this pack admit `\.([a-z0-9]+)`").expect("static");

    let headings: Vec<(usize, String)> = heading_re
        .captures_iter(&text)
        .map(|c| (c.get(0).expect("group 0").start(), c[1].to_string()))
        .collect();
    assert!(
        !headings.is_empty(),
        "docs/rules/catalog.md has no `### `<pack>` (N rules)` headings — the claim-to-pack binding \
         lost its anchor, and every claim below would be judged against the wrong pack or none"
    );

    let mut checked = 0;
    for caps in claim_re.captures_iter(&text) {
        let at = caps.get(0).expect("group 0").start();
        let stated: usize = caps[1].parse().expect("digits");
        let ext = &caps[2];
        let pack_id = headings
            .iter()
            .rev()
            .find(|(pos, _)| *pos < at)
            .map(|(_, id)| id.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "a `N rules in this pack admit `.{ext}`` claim sits before the first pack heading \
                     — \"this pack\" names nothing, so the sentence cannot be true or false"
                )
            });
        let pack = packs.iter().find(|p| p.id == pack_id).unwrap_or_else(|| {
            panic!("catalog heading names pack `{pack_id}`, which no longer loads")
        });

        for probe in [format!("probe.{ext}"), format!("src/main/deep/probe.{ext}")] {
            let actual = pack
                .rules
                .iter()
                .filter(|r| {
                    regex::Regex::new(file_pattern_of(&r.matcher))
                        .is_ok_and(|re| re.is_match(&probe))
                })
                .count();
            assert_eq!(
                stated, actual,
                "docs/rules/catalog.md claims {stated} rule(s) in the `{pack_id}` pack admit `.{ext}`, \
                 but {actual} of its matchers' file_pattern accept {probe:?}. The catalog is baked into \
                 every binary and served over MCP as `rule-catalog`, so this sentence is quoted by \
                 agents that have no checkout to verify it against."
            );
        }
        checked += 1;
    }

    assert!(
        checked >= 1,
        "no `N rules in this pack admit `.<ext>`` claim was found in docs/rules/catalog.md — this \
         contract would pass vacuously. The `security` pack's Java-lane paragraph carries one; if it \
         was reworded, re-point this regex in the same commit rather than leaving a silent no-op."
    );
}

/// `SourceSymbolKind`'s per-language collapse table, held against the front ends themselves.
///
/// ## Why the table exists, and why writing it down created a new risk
///
/// The five kind names are TypeScript's, and every other front end folds its own vocabulary into
/// them — Rust `struct`/`enum`/`union` all become `Class`, a Rust `trait` becomes `Interface`, a
/// Prisma `model` becomes `Class`. Nothing said so anywhere until 2026-08-12, which is a real gap: a
/// `symbol-scan` rule filters on `kind`, so an author who reads `Class` as "a class" writes a rule
/// that silently also judges Rust structs.
///
/// Writing the table into `crates/core/src/ir/kinds.rs` fixed that and immediately created the defect
/// class this repo spends most of its guards on — prose stating an inventory, with nothing checking
/// it. This is the check. Both directions, because both are silent failures: a language emitting a
/// kind the table omits makes the table under-promise (a rule author rules out a query that would in
/// fact work), and a table naming a kind the parser never emits makes it over-promise (the author
/// writes the rule and gets nothing, with no way to tell that from "no matches").
///
/// The subject set is every `parser/parser-*` crate on disk, never a list here — a ninth front end
/// joins this check by existing. The extraction is textual (`SourceSymbolKind::<Variant>` in the
/// crate's shipped sources, test halves excluded, since a test asserting a kind is not the parser
/// emitting one), which is the same pragmatic proxy `recognizer_drift` and `contracts_tests` already
/// use for their own source scans.
#[test]
fn the_symbol_kind_collapse_table_matches_what_each_parser_emits() {
    use std::collections::{BTreeMap, BTreeSet};

    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    // --- what the table CLAIMS ------------------------------------------------------------------
    let kinds_src = fs::read_to_string(repo.join("crates/core/src/ir/kinds.rs"))
        .expect("crates/core/src/ir/kinds.rs is readable");
    let header = ["Class", "Interface", "Type", "Const", "Function"];
    let mut claimed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in kinds_src.lines() {
        let Some(row) = line.trim().strip_prefix("/// |") else {
            continue;
        };
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        // Header row, separator row, and the leading blank label cell of the header are all skipped
        // by requiring a language name in cell 0 and exactly five kind cells after it.
        if cells.len() < 6 || cells[0].is_empty() || cells[0].starts_with("---") {
            continue;
        }
        let language = cells[0].to_string();
        if language == "`Class`" || header.contains(&language.as_str()) {
            continue;
        }
        let mut set = BTreeSet::new();
        for (i, kind) in header.iter().enumerate() {
            let cell = cells[i + 1];
            if !cell.is_empty() && cell != "—" {
                set.insert((*kind).to_string());
            }
        }
        claimed.insert(language, set);
    }
    assert!(
        claimed.len() >= 6,
        "parsed {} row(s) out of the collapse table in crates/core/src/ir/kinds.rs — the table's \
         markdown shape changed and this pin would judge almost nothing. Re-point the parse in the \
         same commit as the reformat.",
        claimed.len()
    );

    // --- what the parsers DO -------------------------------------------------------------------
    // Table label -> crate directory. The one hand-written mapping here, and it is checked: a crate
    // with no label fails below rather than being skipped.
    let label_of = |crate_name: &str| -> Option<&'static str> {
        Some(match crate_name {
            "parser-typescript" => "TypeScript",
            "parser-java-21" => "Java",
            "parser-rust" => "Rust",
            "parser-go" => "Go",
            "parser-csharp" => "C#",
            "parser-python-3" => "Python",
            "parser-prisma" => "Prisma",
            // A front end that projects no symbols at all has no row to keep honest.
            "parser-sql" => return None,
            _ => return None,
        })
    };

    fn emitted(dir: &Path, out: &mut BTreeSet<String>) {
        // Built once per process rather than per source file: this walk visits every `.rs` under eight
        // parser crates, and recompiling the pattern at each was a `clippy::regex_creation_in_loops`
        // warning. `LazyLock` over a `fn`-local `static` keeps it beside its one use.
        static VARIANT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            regex::Regex::new(r"SourceSymbolKind::([A-Z][A-Za-z]*)").expect("static regex")
        });
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                emitted(&path, out);
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if !name.ends_with(".rs") || name == "tests.rs" || name.ends_with("_tests.rs") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            for (_, [variant]) in VARIANT.captures_iter(&text).map(|c| c.extract()) {
                out.insert(variant.to_string());
            }
        }
    }

    let mut unlabelled = Vec::new();
    let mut checked = 0;
    for entry in fs::read_dir(repo.join("parser"))
        .expect("parser/ is readable")
        .flatten()
    {
        let crate_name = entry.file_name().to_string_lossy().to_string();
        if !crate_name.starts_with("parser-") {
            continue;
        }
        let Some(label) = label_of(&crate_name) else {
            continue;
        };
        let mut actual = BTreeSet::new();
        emitted(&entry.path().join("src"), &mut actual);
        let Some(claim) = claimed.get(label) else {
            unlabelled.push(crate_name.clone());
            continue;
        };
        assert_eq!(
            claim, &actual,
            "the SourceSymbolKind collapse table in crates/core/src/ir/kinds.rs says {label} projects \
             {claim:?}, but {crate_name}'s shipped sources emit {actual:?}. A `symbol-scan` rule \
             filters on `kind`, so the table is what a rule author plans against — an omission rules \
             out a query that works, and an extra rules IN one that silently never matches."
        );
        checked += 1;
    }
    assert!(
        unlabelled.is_empty(),
        "these parser crates have no row in the collapse table: {unlabelled:?}. A front end with no \
         row is one whose kind vocabulary nobody has written down — add the row (or, if it projects \
         no symbols, exempt it in this test with that reason)."
    );
    assert!(
        checked >= 6,
        "only {checked} parser crate(s) were compared — the crate walk narrowed and this pin would \
         vouch for a table it barely read"
    );
}
