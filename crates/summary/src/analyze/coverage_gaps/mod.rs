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
//! # Why a share filter, and why TWO shares
//! An extension earns a row only as a principal filetype: at least [`census::PRINCIPAL_SHARE_PCT`] of the
//! tree's files AND of its lines (the line leg with one exclusion, two paragraphs down — read both
//! before quoting either). Neither share alone works, and the failure is measurable in both
//! directions — on gogs `.png` is 33.9% of the FILES and 6.6% of the lines (a file-share filter opens
//! every reply with an image row), while `.ini` is 12.7% of the LINES and 1.3% of the files (a
//! line-share filter reports the locale bundle). A filetype that is a principal share of BOTH is source
//! the tree is actually written in; that is a measurement, not an extension vocabulary, so nothing here
//! needs a language table to go stale. The parsed-but-unresolved kind is gated on the structural
//! population instead — it can only fire on files a parser already claimed.
//!
//! The line leg is asked of the extension's TOTAL. A largest-file exclusion was tried here on
//! 2026-08-20 to kill the `package-lock.json` shape and reverted the same day: measured against two
//! trees with identical unread payload, it answered differently on whether the lines sat in one file
//! or ten, which is an ERASING change resting on an INFERENCE. The noise it targeted is real but is
//! now disclosed instead — every such row carries `kind: "data-config"`, and this cell says such a
//! row is not a verdict. [`census::is_principal_population`] owns the reasoning and the reopening
//! condition.
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
//! parsed-but-unresolved kind that a parser-capability list structurally cannot — so where they
//! overlap they can still disagree at the margin. `meaning` says so on the wire rather than letting a
//! reader discover it by diffing two subcommands. What they must NOT disagree on is which filetypes are
//! even eligible, so both the share floor and the eligibility test are the engine's own, borrowed
//! through `zzop-facade`'s re-export rather than re-stated here.
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

use census::{by_extension, is_principal, is_principal_population};

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
    let total_lines: u64 = by_ext.values().map(|c| c.lines).sum();
    let total_structural: u64 = by_ext.values().map(|c| c.structural as u64).sum();
    let in_dep_graph: u64 = by_ext.values().map(|c| c.in_dep_graph as u64).sum();

    let extensions: Vec<Value> = by_ext
        .iter()
        .filter(|(ext, c)| {
            if c.in_dep_graph > 0 {
                return false;
            }
            if c.structural == 0 {
                // The no-parser kind: judged against the tree, on both shares (see the module doc) AND
                // on whether a dispatch-`None` here CAN have cost anything. The share test alone does
                // not generalize — it was argued from gogs' `.png` (33.9% of files, 6.6% of lines) and
                // `.ini` (12.7% of lines, 1.3% of files), both of which two shares do stop, but `.md`
                // is large in BOTH dimensions in ordinary trees. The classifier is the engine's,
                // reached through `zzop_facade`'s re-export because this crate must not depend on
                // `zzop-engine`; borrowing it is what keeps this cell and `unreadExtensions` from
                // disagreeing.
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
                zzop_facade::extraction_can_lose_facts(ext)
                    && is_principal(c.files as u64, total_files)
                    && is_principal_population(c, total_lines)
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

    let mut out = Map::new();
    out.insert("extensions".to_string(), Value::Array(extensions));
    out.insert(
        "basis".to_string(),
        json!(format!(
            "{} extension(s) crossed; {in_dep_graph} file(s) contribute at least one resolved import \
             edge",
            by_ext.len()
        )),
    );
    out.insert("meaning".to_string(), json!(MEANING));
    Value::Object(out)
}

/// The vocabulary rides INSIDE the object it describes — the same device `ruleTimings` and
/// `architecture.painMeaning` use, and for the same reason: a consumer that reads the rows cannot fail
/// to also have read what they omit. Every number it names lives somewhere else in THIS reply and is
/// pointed at rather than copied.
const MEANING: &str = "Extensions contributing ZERO resolved import edges while being a principal \
    filetype here (>=10% of this tree's files AND >=10% of its lines; a parsed extension is judged \
    against >=10% of the files with a structural projection instead). The LINE leg is a raw share, and \
    deliberately so: a largest-file exclusion was tried on it and reverted \
    the same day, because it erased a real row whenever one member of the extension dominated it, \
    which is exactly the MyBatis shape this cell exists for. The accepted cost is the other direction \
    of the same fact — one outsized member (a lock file, a generated bundle) can carry its filetype \
    over the line floor on its own. Read a row as a place to look rather than as a verdict; line \
    distribution is not evidence about whether anything read the files. Each row's kind field says \
    what a zero here can and cannot mean, and the two are not interchangeable. \
    Kind \"source\": a language this tree is written in that no frontend read. \
    Kind \"data-config\": a structured data or configuration filetype \
    (json/yaml/xml/toml/ini/properties/csv/lock/html) — nobody writes a language parser for those, so \
    the \"no native parser\" warnings deliberately never mention them, but they DO routinely declare \
    facts this engine's io channels carry: SQL inside MyBatis .xml mappers, services in a k8s \
    manifest, endpoints in an OpenAPI document. Whether this row cost you anything depends entirely \
    on what those files hold and THIS BUILD DID NOT READ THEM, so it is listed rather than judged — a \
    900-statement mapper directory and an i18n locales bundle are the same row until you look. \
    (Filetypes with no STRUCTURAL projection — prose, stylesheets, images, fonts, media, archives — \
    are excluded from this list entirely rather than shown as a cleared row. Structural, not total: a \
    `.md` page under a VitePress-style docs root can carry `import` lines inside a `<script setup>` \
    block, and since 2026-08-20 those ARE read — as dep-graph in-edges only, its symbols and io \
    unprojected, which is why the filetype still sits on the excluded side.) `structural: 0` has TWO \
    causes and they take opposite remedies, so check which before acting: no parser claimed the \
    extension (an adapter overlay is the on-ramp), or a parser claimed the files and bailed on them. \
    Neither `coverage.degraded` nor the `degraded` list tells those apart — both count every walked \
    file that got no structural projection, including files no frontend was ever going to read (an \
    oversized `.png` degrades and loses nothing by it), so on many trees the whole list is the FIRST \
    case. The CAUSE split rides in `warnings`, in the degraded-file self-report, which counts only \
    files a frontend actually dispatched to plus unreadable ones — so a degraded list with no such \
    warning beside it is itself the answer 'nothing claimed these', not a hole in the report. Either \
    way those files are absent from the resolved dependency graph — \
    the substrate every unimported/unreachable-export verdict, blast radius and fan-in/out is \
    computed over; read those findings as being about the REMAINING files, never about these. \
    `structural` above 0 with no edge means the files DID parse and their specifiers resolved to \
    nothing in-tree; the declared side is `coverage.declaredImportsByExt`. An empty list means the \
    cross ran and found nothing — `basis` says what was crossed, so it never means 'not measured'. \
    This is a share filter over extension counts, not a language judgment, and not the whole table — \
    the aggregate coverage surface carries every extension plus the capability crosses, which gate on \
    file share alone rather than on line share too, so the two lists can differ at \
    the margin. The cross-layer JOIN is a separate axis whose counts already ride this reply \
    (`coverage.ioProvides` / `ioConsumesKeyed` / `ioConsumesUnresolved` / `joinContributionZero`): a \
    near-zero provider count on a tree that serves routes makes an unprovided-consume finding a \
    statement about extraction, not about the code.";
