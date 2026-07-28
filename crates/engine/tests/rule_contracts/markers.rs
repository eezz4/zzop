//! Contracts 1-2: derived suppress-marker global uniqueness and the message "how to exclude" leg, plus
//! the published-surface leg (contract 2b) that pins WHICH rules the catalog pages may claim a marker for.
//!
//! Markers are no longer stored per rule — `RuleDef::suppress_marker()` DERIVES `zzop-<id>-ok` (see its doc; the `zzop-` TOOL PREFIX landed 2026-07-26 so a suppression comment can be grepped as a class and a reader can tell WHOSE checker it silenced).
//! That collapses three formerly-hand-guarded invariants into construction guarantees: every rule now has a
//! non-empty marker (ids are never empty), and every marker begins `zzop-` and ends `-ok` by definition. What derivation
//! does NOT guarantee is cross-pack uniqueness — two rules in different packs sharing an id would derive the
//! same marker and co-suppress — so that is the one presence/uniqueness invariant still worth a test.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use zzop_core::Matcher;

use crate::{load_all_packs, native_ids};

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

/// Every DSL rule's `message` names its own derived suppress marker (`zzop-<id>-ok`) OR the literal
/// `disabled_rules`/`disabledRules` string somewhere in the text — the "how to exclude" leg of zzop's
/// finding contract (every finding must tell the reader the problem, the fix, AND how to turn it off; see
/// docs/rules/authoring-guide.md's quality bar). A rule that legitimately has no per-finding marker still
/// passes via the `disabled_rules` leg — this test accepts EITHER, not just the marker.
#[test]
fn every_dsl_rule_message_documents_how_to_exclude_it() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        for rule in &pack.rules {
            let marker = rule.suppress_marker();
            let marker_leg = rule.message.contains(&marker);
            let disabled_leg =
                rule.message.contains("disabled_rules") || rule.message.contains("disabledRules");
            if !(marker_leg || disabled_leg) {
                offenders.push(format!(
                    "{}/{} (derived marker `{marker}`) — message mentions neither its own marker nor \
                     disabled_rules/disabledRules",
                    pack.id, rule.id
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "rule messages missing the \"how to exclude\" leg: {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 2b. Published-surface leg — the catalog pages may only claim a marker for a rule that has one
// ---------------------------------------------------------------------------------------------

/// The pages whose ROWS are checked, relative to this crate's manifest dir. Both are hand-authored prose
/// keyed by rule id, and neither can reference a Rust value, so the relationship is sealed here instead.
const ROW_SURFACES: [&str; 2] = ["../../docs/rules/catalog.md", "../../site/rules.html"];

/// The ONLY native analyses that honor an inline comment marker: `rules-http`'s two call-graph scanners
/// read a HAND-WRITTEN `// idempotent-ok:` literal (`rules/native/rules-http/src/http_scan.rs`) that is
/// derived from no rule id. Every other native id — all 25 `cross-layer/*`, the graph rules, the schema
/// rules, the metrics ids — honors none (`rules-cross-layer/src/cross_layer/mod.rs`'s "Suppression"
/// section). Census-visible on purpose: retiring the hand-authored marker (the id-derivation unification
/// on the backlog) empties this list, and `the_hand_authored_native_marker_still_exists_in_the_scanner`
/// below goes red the moment the literal leaves the scanner, forcing this constant and the pages to move
/// together.
const NATIVE_MARKER_HONORING_IDS: [&str; 2] = ["unsafe-read-endpoint", "non-idempotent-write"];

/// The hand-authored native marker literal, spelled once here and asserted to still exist in the scanner.
const HAND_AUTHORED_NATIVE_MARKER: &str = "idempotent-ok";

fn read_repo_file(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Every rule id (bare and, for DSL rules, pack-qualified) whose findings an inline marker can actually
/// suppress — read from the same data the engine loads, never a hand-copied list. DSL: every matcher
/// except `symbol-scan`, whose findings have no source line to anchor a comment against
/// (`RuleDef::suppress_marker` still derives a string for it, but nothing ever consults the result — see
/// `crates/facade/src/explain/render.rs`'s `suppress_marker_str`). Native: `NATIVE_MARKER_HONORING_IDS`.
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
    for id in NATIVE_MARKER_HONORING_IDS {
        ids.insert(id.to_string());
    }
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

    for rel in ROW_SURFACES {
        let text = read_repo_file(rel);
        for (id, row) in rule_rows(&text) {
            if !known.contains(&id) {
                continue; // not a rule row (matchers, config keys, ... share the table shape)
            }
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
    assert!(
        offenders.is_empty(),
        "published rule rows whose suppression claim contradicts the code: {offenders:#?}"
    );
}

/// Keeps `NATIVE_MARKER_HONORING_IDS` honest: the hand-authored `// idempotent-ok:` literal it exists for
/// must still be in the scanner. Retiring it (unifying onto derived `zzop-<id>-ok`, or dropping the exception
/// entirely) empties the exception and must empty this list in the same commit — otherwise the pages could
/// keep promising a marker that no longer exists and the test above would still read green.
#[test]
fn the_hand_authored_native_marker_still_exists_in_the_scanner() {
    let scanner = read_repo_file("../../rules/native/rules-http/src/http_scan.rs");
    assert!(
        scanner.contains(HAND_AUTHORED_NATIVE_MARKER),
        "rules-http's hand-authored `{HAND_AUTHORED_NATIVE_MARKER}` marker is gone from http_scan.rs — \
         empty NATIVE_MARKER_HONORING_IDS and update docs/rules/catalog.md, site/rules.html, \
         site/usage.html and docs/getting-started.md in the same commit"
    );
    let registered = native_ids();
    for id in NATIVE_MARKER_HONORING_IDS {
        assert!(
            registered.iter().any(|registered_id| registered_id == id),
            "NATIVE_MARKER_HONORING_IDS names `{id}`, which is not a registered native analysis id"
        );
    }
}
