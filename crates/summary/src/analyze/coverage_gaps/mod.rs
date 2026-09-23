//! `coverageGaps` — the shaped analyze reply's compact statement of which of this tree's PRINCIPAL
//! filetypes contribute nothing to the resolved dependency graph.
//!
//! # The gap this closes (measured 2026-08-20 on three public trees)
//! The facts that make `analyze`'s findings interpretable lived only in `zzop coverage`, a different
//! subcommand `analyze` never mentions. On directus @06027c83 (4,617 files) `analyze` emitted 306
//! `unimported-export` and 126 `dead-candidates` findings while `.vue` — 587 files, 12.7% of the tree
//! and 14.3% of its lines — had no structural projection at all, so the 587-file frontend was absent
//! from the very graph those judgments are computed over. On gogs @d460e50f, `.js` (173 files, 172 of
//! them parsed, 183 declared import specifiers) contributed ZERO resolved edges — a shape no channel
//! in the reply carried at any grain.
//!
//! # What earns a place here, and what deliberately does not
//! ONLY the two facts above, both keyed on the dep-graph population, both absent from the reply in
//! every other form:
//! - an extension with files and no structural projection (the directus shape), and
//! - an extension that parsed and still resolved no import (the gogs shape).
//!
//! The JOIN side does NOT get a row, on evidence: its counts (`ioProvides`, `ioConsumesKeyed`,
//! `ioConsumesUnresolved`) ALREADY ride this reply inside `coverage`, and a second copy of a number is
//! this repo's most reliable way to manufacture a stale one. `meaning` points at them instead.
//! `declaredImportsByExt` is likewise pointed at, never restated, for the same reason.
//!
//! # Why a share filter, and why ONE share — the FILE share
//! An extension earns a row only as a principal filetype: at least [`census::PRINCIPAL_SHARE_PCT`] of
//! the tree's FILES. That is a measurement, not an extension vocabulary, so nothing here needs a
//! language table to go stale. The parsed-but-unresolved kind is gated on the structural population
//! instead — it can only fire on files a parser already claimed.
//!
//! ## The LINE leg this cell shipped until 2026-09-01, and the measurement that removed it
//! There were two legs — the same share of the tree's files AND of its lines — and the pair was
//! argued in both directions at once on gogs: `.png` is 33.9% of the FILES and 6.6% of the lines (so
//! not the file axis alone), `.ini` is 12.7% of the LINES and 1.3% of the files (so not the line axis
//! alone). Only the second of those defends a leg that survives, and it defends THIS one. The first
//! argued for the line leg, and it stopped being that leg's work on 2026-08-20, when
//! `extraction_can_lose_facts` landed below the share test and excluded images, fonts, prose, media
//! and archives outright — `.png` has been dead twice over ever since, and a leg whose only case is
//! carried by something else is not paying for the rows it costs.
//!
//! What it cost is measured. On 9 public trees the two-leg test returned `[]` for the two largest
//! unread SOURCE populations in the corpus — immich `.svelte` (415 files, 12.1% of files) and nocodb
//! `.vue` (962 files, 21.2%) — both `structural: 0`, both named by `zzop coverage`'s
//! `unreadExtensions`, which asks the same question on the file share alone. They failed the LINE leg
//! against denominators made of what nobody writes: immich's line census is topped by `.ttf` at
//! 22.57% (19 files) and `.png` at 17.68%, nocodb's by a vendored `.sql` dump at 56.35% (27 files).
//! The premise the two legs rested on is inverted by that measurement — the noise this cell fears
//! (lock files, vendored dumps, generated bundles, fonts, `.pdb`) concentrates in LINES and not in
//! FILES, so the line leg was the leg letting noise in and the file leg the one keeping it out.
//! Removing it added exactly those two rows across the 9 trees and nothing else; `total`,
//! `bySeverity` and `byRule` did not move on any of them.
//!
//! Two alternatives were measured and are not coming back. Making the legs OR rather than AND adds
//! four trees' worth of `.json` plus mall's `.pdb` and `.pdm` — one-file populations that are pure
//! line-axis noise. Narrowing the line DENOMINATOR to extensions that can lose facts is a no-op:
//! that test is the complement of a closed hand-written list, so an unlisted vendored `.sql` or
//! `.pdb` stays in the denominator, and the 9 replies came back byte-identical.
//!
//! ## A largest-file exclusion was tried on that leg on 2026-08-20 and reverted the same day
//! Kept here because it is the reason a `package-lock.json`-shaped row is DISCLOSED rather than
//! erased, which is still this cell's policy and is now the whole of it. The exclusion killed the
//! lock shape (`corpus/oss/be-express` `.json`: 13 files, 10,229 lines, the lock alone 96.8%) and
//! also killed a real one: two trees with identical unread payload — 10 MyBatis mappers, ~2,990 lines
//! of `${}`-interpolated SQL — answered differently purely on whether the XML sat in one file or ten,
//! while `zzop coverage` kept reporting the concentrated one. An ERASING change resting on an
//! INFERENCE, in the direction that leaves the reader no trace. Every such row carries
//! `kind: "data-config"` instead, and this cell says such a row is a place to look rather than a
//! verdict. `coverage_gaps_tests.rs` pins that pair; with the line leg gone, line CONCENTRATION is
//! not a signal this cell can reach for at all, which is the strongest form of that answer.
//!
//! Everything below the bar stays where the full answer has always been: `zzop coverage <path>`, whose
//! per-extension table this module's rows are pinned against (`coverage_gaps_tests.rs`).
//!
//! # The seam with `queryCoverage`'s own capability cells, stated rather than hidden
//! That surface answers the PARSER half of this question (which principal filetypes no structural
//! parser read) and gates it on the engine's `extraction_can_lose_facts` test plus the engine's
//! principal-share floor. Both are reached here too — through `zzop-facade`'s re-export, which is what
//! keeps this crate's "nothing below the facade" layering (see `Cargo.toml`) intact without either
//! value being retyped. The subject sets are still deliberately different, and not because of
//! layering: this cell's membership rule is the DEP GRAPH (`inDepGraph == 0`), which covers the
//! parsed-but-unresolved kind that a parser-capability list structurally cannot. `meaning` says so on
//! the wire rather than letting a reader discover it by diffing two subcommands. What they must NOT
//! disagree on is which filetypes are even eligible, so both the share floor and the eligibility test
//! are the engine's own, borrowed through `zzop-facade`'s re-export rather than re-stated here.
//!
//! Dropping the line leg closed the one gap that was left. Until 2026-09-01 the two surfaces also
//! disagreed about the SHARE — that one gated on file share alone, this one on file share and line
//! share — and the disagreement was disclosed on the wire because it could not be defended, which is
//! how a `.vue` population of 962 files could be named there and absent here in the same reply's
//! neighbourhood. The no-parser halves now ask the identical question: `structural == 0`, the engine's
//! `extraction_can_lose_facts`, and at least `MIN_UNCOVERED_EXTENSION_SHARE_PCT` of the tree's walked
//! files. (`unread::extensions` floors the share to an integer before comparing and this predicate
//! cross-multiplies; for an integer floor those agree on every input.) What remains is not a margin
//! but a stated difference in SUBJECT — this list additionally carries extensions that parsed and
//! resolved nothing, which a parser-capability list has no way to hold.
//!
//! # Why the eligibility test is not `is_non_source_extension` (2026-08-20, reversed the same day)
//! It was, for one review cycle, and that is the sharper version of the same mistake: `zzop-engine`
//! ships TWO predicates over one table and they answer different questions — `is_non_source_extension`
//! answers "should this run ask the reader for a parser adapter", `extraction_can_lose_facts` answers
//! "could a dispatch-`None` here have cost facts". Using the first for the second suppressed the
//! largest extraction gap in the dogfood corpus: macrozheng/mall's 114 `.xml` MyBatis mappers — 15.8%
//! of its files, 11.5% of its lines, 906 SQL statements and 744 `${}` substitution sites, which is
//! essentially every SQL statement the project has — reported as an EMPTY list here AND an empty
//! `unreadExtensions`, in a reply whose `warnings` named `.pdb` and `.emmx` instead.
//! `zzop_engine::NonSourceKind` is that story's owner. The `locales/*.json` false positive that
//! prompted the first gate is real too, so the fix is not a revert: the row returns carrying `kind`,
//! and `MEANING` states that a `data-config` row is a place to look rather than a verdict.
//!
//! # The eligibility test that was proposed instead, and the measurement that killed it
//! A review of the row above proposed replacing "could a dispatch-`None` here have cost facts" with
//! **"does any LOADED rule put this extension in scope"** — derived from the run rather than from a
//! table, and correct on the two rows it was argued from (`.json` is in three bundled `file_pattern`s
//! and would be suppressed; `.xml` is in none and would survive). Re-measured over 23 trees it is not
//! close: `.svelte` and `.vue` sit in bundled `file_pattern`s too, so the SAME gate deletes
//! `fe-svelte` `.svelte` (24 files) and `koel` `.vue` (346) here — one real row for each noise row it
//! removes — and on `queryCoverage`'s `unreadExtensions`, which applies the identical test one leg
//! looser, it additionally takes `fe-vue` `.vue` (17% of that tree) and `fe-svelte` `.svelte` at 39%.
//! Recount: `grep -oh '"file_pattern": *"[^"]*"' rules/dsl/*/*.json | sort | uniq -c`, crossed with
//! `zzop analyze <tree>` over `corpus/oss`.
//!
//! It is also the wrong QUESTION, which is why the corpus and the reasoning agree. A `LineScan` rule
//! grepping a file's bytes for a secret is not the file's declarations reaching the analysis: had one
//! bundled pattern gained `xml`, mall's 906 MyBatis SQL statements would have gone silent again while
//! staying exactly as unread — a suppressor whose correctness depends on a regex someone else owns.
//!
//! And it does not reach here today even if it were right. The engine computes the language axis of
//! that census (`analyze::diagnostics::pack_scope`'s `ext_census`/`ext_rule_reach`) but builds it from
//! `crate::dispatch::dispatch(rel, …)?` (`pack_scope::rule_admission::ExtReach::new`), so an extension
//! NO frontend claims — `json` and `xml` both — is absent from it by construction, and no per-extension
//! projection of it rides the wire. Wiring it would mean a second census axis over the un-dispatched
//! files, a new field on `AnalyzeOutput`, and two readers. That is the cost of the road not taken; the
//! road taken tightens the share test instead, which needs nothing that is not already in `ir.loc`.

use std::collections::HashSet;

use serde_json::{json, Map, Value};

mod census;
mod meaning;

use census::{by_extension, is_principal, withheld_by_share};

/// The FULL vocabulary sentence, for the reply-legends contract document. The reply itself now carries
/// the short note (`crate::output::legends::coverage_gaps_note`) plus a pointer at that document
/// instead of these 4,416 bytes, which were byte-identical on every corpus tree measured. One owner,
/// two views — `meaning`'s own doc records what the last second copy of a value in it cost.
pub(super) fn full_meaning() -> String {
    meaning::meaning()
}

/// Builds the reply's `coverageGaps` object from an `AnalyzeOutputView`-shaped output.
///
/// ALWAYS returned, never `Option` — following `queryCoverage`'s `blindSpots`/`blindSpotBasis` pair
/// rather than this shaper's `architecture`/`cache`/`ruleTimings` "absent, and the presence IS the
/// signal" convention. The two conventions answer different questions, and `queryCoverage` states the
/// rule: a field is conditional when its absence is UNAMBIGUOUS. `architecture` is absent exactly when
/// git signals did not run, which the reader can already tell. This one cannot be: a reader who does
/// not see a gap statement has no way to tell "measured, the graph holds this tree's code" from
/// "nobody looked", and telling those two apart is the entire reason the field exists. So the empty
/// list ships, and `basis` says what produced it — without that sentence an empty array reads the same
/// for "crossed, nothing missing" and "no walked file to cross".
pub(super) fn coverage_gaps(output_view: &Value) -> Value {
    let degraded: HashSet<&str> = output_view
        .get("degraded")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let by_ext = by_extension(output_view.get("ir").unwrap_or(&Value::Null), &degraded);
    // The declared side is MEASURED BY THE ENGINE and already on this reply — read, never recomputed
    // and never restated: an extension with a declared specifier is one whose parser projects an import
    // channel at all, which is what separates "resolved nothing" from "was never asked".
    let declared = output_view
        .pointer("/coverage/declaredImportsByExt")
        .and_then(Value::as_object);
    let total_files: u64 = by_ext.values().map(|c| c.files as u64).sum();
    let total_structural: u64 = by_ext.values().map(|c| c.structural as u64).sum();
    let in_dep_graph: u64 = by_ext.values().map(|c| c.in_dep_graph as u64).sum();

    let extensions: Vec<Value> = by_ext
        .iter()
        .filter(|(ext, c)| {
            if c.in_dep_graph > 0 {
                return false;
            }
            if c.structural == 0 {
                // The no-parser kind: judged against the tree on its FILE share AND on whether a
                // dispatch-`None` here CAN have cost anything. The share test alone does not
                // generalize — `.md` is a principal share of an ordinary tree's files and there is
                // nothing structural to lose in it — which is what the second condition, not a second
                // share, is for. A LINE share sat here as a third condition until 2026-09-01; the
                // module doc holds what it cost and why its own argument had already moved out from
                // under it. The classifier is the engine's, reached through `zzop_facade`'s re-export
                // because this crate must not depend on `zzop-engine`; borrowing it is what keeps
                // this cell and `unreadExtensions` from disagreeing, and with the line share gone
                // these two conditions ARE that cell's two, over the same denominator.
                //
                // It is `extraction_can_lose_facts`, NOT `is_non_source_extension`, and the difference
                // is the whole point: those are two questions and this cell asks the second one.
                // Gating on the first was measured on 2026-08-20 in both directions on the same day —
                // it fixed a tree of 31 `.ts` plus a 12-file `locales/` directory reporting `json` as
                // a gap while `unreadExtensions` said nothing was held back, and it created a far more
                // expensive silence: macrozheng/mall's 114 `.xml` MyBatis mappers (15.8% of files,
                // 11.5% of lines, 906 SQL statements, 744 `${}` sites) produced an EMPTY list on both
                // surfaces. The locale complaint was real too, so the answer is not to revert: the row
                // comes back carrying `kind`, which says a data/config filetype's cost depends on what
                // the files hold and that this build did not read them to find out.
                // THE FLOOR STAYS ON THE ROWS, AND WHAT IT WITHHELD IS NAMED IN `basis` (2026-09-04).
                //
                // The floor cannot separate signal from noise — one project's 114 `.xml` MyBatis
                // mappers clear it at 15.8% and are named, while another app's mappers at 7.7%,
                // holding every SQL statement that app has, are deleted. Same filetype, same content,
                // opposite answer. But dropping the floor was measured wrong in the other direction
                // within the hour: a twelve-file fixture with ONE `.jsonc` emitted a row, which is
                // exactly the locale noise the floor was built for and which `coverage_gaps_tests`
                // pins against.
                //
                // Both are true because they are about different channels. The ROW list is a
                // shortlist — it earns its floor. The `basis` sentence is the companion that exists so
                // an empty list cannot read as a verdict, and it was the one lying: it said what was
                // crossed and never that anything had been held back. So the floor keeps deciding
                // ROWS, and `basis` now names its exclusions. Nothing is deleted from the reply; a
                // shortlist stays a shortlist.
                zzop_facade::extraction_can_lose_facts(ext)
                    && is_principal(c.files as u64, total_files)
            } else {
                // The parsed-but-unresolved kind: judged against the STRUCTURAL population, and only
                // for an extension the engine measured a declared import on — without that, a resolved
                // count of zero is a parser that projects no import channel, not a resolution failure.
                declared
                    .and_then(|m| m.get(ext.as_str()))
                    .and_then(Value::as_u64)
                    .is_some_and(|declared| declared > 0)
                    && is_principal(c.structural as u64, total_structural)
            }
        })
        .map(|(ext, c)| {
            json!({ "ext": ext, "files": c.files, "structural": c.structural,
                    "kind": zzop_facade::extension_content_kind(ext) })
        })
        .collect();

    let withheld = withheld_by_share(&by_ext, total_files);

    let mut out = Map::new();
    out.insert("extensions".to_string(), Value::Array(extensions));
    let held_back = if withheld.is_empty() {
        String::new()
    } else {
        format!(
            ". HELD BACK from the rows above by the {floor}%-of-files shortlist bar, though this build \
             judges each able to have lost facts: {}. Their absence from `extensions` is a size \
             judgement, never a measurement that they cost nothing — this build did not read them",
            withheld
                .iter()
                .map(|(ext, files)| format!("{ext} ({files} file(s))"))
                .collect::<Vec<_>>()
                .join(", "),
            floor = zzop_facade::MIN_UNCOVERED_EXTENSION_SHARE_PCT,
        )
    };
    out.insert(
        "basis".to_string(),
        json!(format!(
            "{} extension(s) crossed; {in_dep_graph} file(s) contribute at least one resolved import \
             edge{held_back}",
            by_ext.len()
        )),
    );
    // FOLDED, not the full vocabulary (2026-09-01): the short note that carries what a row's zero can
    // and cannot mean, plus a pointer at the reply-legends document for the rest. The rows and `basis`
    // above are THIS RUN's measurement and are untouched — see `crate::output::legends` for the rule,
    // and for the three neighbouring legends it deliberately leaves inline.
    out.insert(
        "meaning".to_string(),
        json!(crate::output::legends::folded_string(
            "coverageGaps.meaning"
        )),
    );
    Value::Object(out)
}
