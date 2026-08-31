//! The tree-level blindness fact `blindSpots` structurally cannot carry: the PRINCIPAL filetypes of
//! this tree that NO structural parser read at all.
//!
//! Measured 2026-08-20 on directus @ `06027c83`: `blindSpots: []` beside an extension row reading
//! `{"ext":"vue","files":587,"structural":0,"lexicalOnly":587}`, and a basis sentence naming only
//! "4 structural extension(s) crossed". A reader takes the empty array for "no blind spots"; the
//! largest blind spot on that tree was 587 files no structural parser opened, and the cost was
//! measured — `browser/unsafe-html-sink` reported 6 findings, all `.ts`, while an unsanitized
//! `innerHTML` sink sat in `app/src/components/v-template-input.vue`. The rule's DOMPurify veto works;
//! it never saw the file.
//!
//! ## Why this is a NEW cell and not a new entry inside `blindSpots`
//! Dropping a differently-shaped element into `blindSpots` would be the stronger read (one array, one
//! check) but it repurposes that array's element type: every entry there is a per-rule record keyed by
//! `ruleId`, and a consumer looping over it would produce a row with no rule. `VERSIONING.md`'s
//! compatibility surface covers CLI JSON output field NAMES AND TYPES — adding a field is minor,
//! repurposing an existing one is a major bump — so the heterogeneous array buys the reader nothing an
//! additive field cannot, at the price of a recorded break. What actually closes the measured
//! misreading is `blindSpotBasis`: it already exists for exactly this job (an empty `blindSpots` must
//! not be readable as a verdict), it is the array's own companion sentence, and it now names what was
//! EXCLUDED from the cross and where that population is listed. The per-rule cross itself is
//! deliberately unchanged — [`super::blind_spots`]'s module doc's reason for not crossing lexical-only
//! files per rule (it would drown the per-rule signal in a restatement of the `lexicalOnly` legend)
//! stands; that reasoning was never an argument for reporting the fact NOWHERE.
//!
//! ## What makes an extension worth naming — both gates are the engine's OWN judgements
//! A `.md` file no parser read is not a coverage gap, so "structural == 0" alone would ship a wall of
//! noise (on directus it names 14 extensions, 13 of them `.snap`/`.license`/`.editorconfig`-class).
//! Rather than invent a second answer, this cell reuses the two the engine already ships:
//! - [`zzop_engine::extraction_can_lose_facts`] — dispatch.rs's per-extension judgement of whether a
//!   dispatch-`None` CAN have cost anything. It deliberately keeps the SSR template dialects
//!   `.vue`/`.svelte`/`.jsp`/`.erb` in scope, and — since 2026-08-20 — also keeps DATA/CONFIG
//!   filetypes in scope, labelled. Which is a correction: this cell used to gate on
//!   [`zzop_engine::is_non_source_extension`], the answer to the DIFFERENT question "should this run
//!   ask for a parser adapter", and that answer is measurably wrong here. On macrozheng/mall, 114
//!   `.xml` files (15.8% of the tree) held 906 MyBatis SQL statements and 744 `${}` substitution
//!   sites — every SQL statement the project has — and this list came back EMPTY beside them.
//!   `zzop_engine::NonSourceKind`'s doc owns that measurement and the reasoning; the row this cell now
//!   emits carries `kind` so a reader can tell a MyBatis mapper directory from a locale bundle, which
//!   is a judgement this build cannot make for them and does not pretend to.
//! - [`zzop_engine::MIN_UNCOVERED_EXTENSION_SHARE_PCT`] — the 10%-of-the-tree floor that decides
//!   whether a filetype is one the tree is MADE OF, gating the engine's `NO loaded DSL rule targets …`
//!   and `THIN DSL rule reach` reports. Its own doc says that question "is one question and must not
//!   get two answers"; this is the third report to ask it, and it reads the same constant.
//!
//! Consequence on the measured trees: `.vue` at 587/4617 = 12% is named on directus and nothing else
//! is; on mall the change adds exactly one row (`.xml`), and across the four-tree dogfood corpus
//! (mall, eShop, koel, this repo) it adds exactly that one row and no other.

use serde_json::{json, Value};

/// One entry per principal filetype no structural parser read, ext-ordered because the caller feeds a
/// `BTreeMap`'s iteration order (determinism is a shipped contract). `per_ext` is
/// `(ext, files, structural)` — the same two columns the `extensions` table renders, projected by the
/// caller so this module never learns that table's tuple layout.
///
/// Entries carry the extension, the share that QUALIFIED it, and its content `kind`, and nothing else:
/// the file counts are measured once, in the extension's own row, and the per-extension "no native
/// parser" entry with its sample paths and adapter on-ramp is already in this tree's `warnings`. A
/// second copy of either is a number that goes stale.
///
/// `kind` is [`zzop_engine::extension_content_kind`]'s token, never one spelled here, and it is what
/// makes a `data-config` row readable rather than alarming: it says which of the two remedies applies
/// (`source` — an adapter or parser is missing; `data-config` — look at what these files hold before
/// concluding anything). Extensions with nothing to lose never reach the row at all, so the token
/// `"no-facts-to-lose"` cannot appear here; that is a property of the filter, stated in [`legend`].
pub(super) fn extensions<'a>(per_ext: impl Iterator<Item = (&'a str, usize, usize)>) -> Vec<Value> {
    let rows: Vec<(&str, usize, usize)> = per_ext.collect();
    let total: usize = rows.iter().map(|(_, files, _)| files).sum();
    if total == 0 {
        return Vec::new();
    }
    rows.iter()
        .filter(|(ext, _, structural)| {
            *structural == 0 && zzop_engine::extraction_can_lose_facts(ext)
        })
        .map(|(ext, files, _)| (ext, files * 100 / total))
        .filter(|(_, share)| *share >= zzop_engine::MIN_UNCOVERED_EXTENSION_SHARE_PCT)
        .map(|(ext, share)| {
            json!({ "ext": ext, "sharePct": share, "kind": zzop_engine::extension_content_kind(ext) })
        })
        .collect()
}

/// The one sentence the vocabulary needs to be self-describing, shipped top-level next to
/// `blindSpotMeaning` — same discipline, same reason.
pub(super) fn legend() -> Value {
    json!(format!(
        "CAPABILITY×MEASURED, and the population the per-rule `blindSpots` cross deliberately does NOT \
         cover: extensions holding at least {floor}% of this tree's walked files that no structural \
         parser read — every file of that extension landed in the `lexicalOnly` or `degraded` column of \
         its `extensions` row, so no symbol, import or io fact was extracted from any of them. A zero \
         finding count over those files is absence of a parser, not a clean bill of health, and it is \
         absence for EVERY rule at once — not only the ones with a declared sightline, which is why an \
         empty `blindSpots` says nothing about them. Two gates, both the engine's own judgement rather \
         than a second one invented here: filetypes with NOTHING a structural parser could ever \
         project (prose, stylesheets, images, fonts, media, archives, certificates) are dropped. \
         That test is about a STRUCTURAL projection and not about every fact: a `.md` page under a \
         VitePress-style docs root can carry `import` lines inside a `<script setup>` block, and \
         since 2026-08-20 those ARE read — as dep-graph in-edges only, which is why the filetype \
         still belongs on this side of the gate. Its symbols and io stay unprojected. The \
         {floor}% floor is the same \
         principal-filetype line the engine's `NO loaded DSL rule targets …` and `THIN DSL rule reach` \
         self-reports use, so \"is this a language the tree is made of\" keeps one answer. Each row's \
         kind field says which of TWO remedies applies and they are not interchangeable. \
         kind \"source\": no frontend read a language this tree is written in (SSR template dialects \
         like .vue/.svelte are deliberately in this class) — bring a parser adapter. \
         kind \"data-config\": a structured data or configuration filetype, where whether anything was \
         lost DEPENDS ON WHAT THE FILES HOLD and this build \
         cannot tell without reading them: a MyBatis .xml mapper directory is 900 SQL statements this \
         run never saw, an i18n locales/*.json bundle is nothing at all, and they are the same row \
         until you look. It is listed rather than judged because the reverse — suppressing it — was \
         measured to hide the largest extraction gap in the dogfood corpus. `sharePct` is \
         integer-floored against the summed `files` column. Counts and paths are NOT repeated here: \
         read the extension's own `extensions` row for its file counts, and this tree's `warnings` for \
         the per-extension \"no native parser\" entry, its sample paths, and the adapter on-ramp that \
         closes a source-kind gap — that warning channel answers the OTHER question (\"is a parser \
         adapter worth asking for\") and deliberately stays silent about structured-data filetypes, so \
         the two lists differ and neither is the other's summary. An empty \
         list means every qualifying filetype ABOVE that share was read structurally — a measured \
         statement rather than an omission, and one that is only as strong as the share and the \
         nothing-to-lose test it rests on: a language under the floor, or one this build classes as \
         having no extractable facts, is ABSENT from this list rather than cleared by it, and a tree \
         with no structural file at all empties the list by having nothing to weigh rather than by \
         being covered.",
        floor = zzop_engine::MIN_UNCOVERED_EXTENSION_SHARE_PCT
    ))
}
