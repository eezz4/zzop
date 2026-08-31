//! Extension CLASSIFICATION — the half of dispatch that answers "what KIND of file is this", as
//! opposed to `dispatch.rs`'s "which parser frontend routes it". Split out on 2026-08-20 for the repo's
//! per-file line cap, and the seam is real: nothing here reads a path, a glob or a `Language`.
//!
//! The whole module exists because ONE list was being asked TWO questions — see [`NonSourceKind`],
//! which owns that story and the measurement behind it.

/// What a dispatch-`None` on a non-source extension costs — the axis [`NON_SOURCE_EXTENSIONS`] is keyed
/// on, and the reason that table is a table of pairs rather than a bare name list.
///
/// **This enum exists because ONE list was answering TWO questions and only one of the answers was
/// right.** The questions are:
/// 1. *"Should this run tell the reader to bring a parser adapter for this filetype?"* —
///    [`is_non_source_extension`], read by `analyze::diagnostics::unparsed_extension_warning`'s
///    collection site. "No" for every member of this table: nobody writes a language frontend for
///    `.png`, and a per-extension nag about `.md`/`.json` in every reply is the noise wall that
///    filter exists to prevent.
/// 2. *"Was anything LOST by having no structural projection here?"* — [`extraction_can_lose_facts`],
///    read by the two coverage-gap surfaces (`zzop_facade`'s `unreadExtensions` and `zzop-summary`'s
///    `coverageGaps`). Answering it with question 1's list shipped a measured false negative:
///    macrozheng/mall carries 114 `.xml` files (15.8% of its files, 11.5% of its lines) holding 906
///    MyBatis SQL statements and 744 `${}` substitution sites — essentially every SQL statement in the
///    project — and BOTH surfaces reported an empty list beside them, because `.xml` sat in the
///    "no adapter nag" table. `.png` and `.xml` are not the same kind of silence and one boolean
///    cannot say so.
///
/// The split runs exactly along the groups [`NON_SOURCE_EXTENSIONS`]'s own comments already drew — no
/// new vocabulary is introduced here, the existing groups are given a machine-readable identity so the
/// two questions can never again be answered by the same bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonSourceKind {
    /// Nothing a structural parser could ever project: prose, stylesheets, images, fonts, media,
    /// archives, binaries, certificates. `structural: 0` on these is not a gap in any sense — there is
    /// no symbol, import or io fact in a `.png` to lose. (Line-scan DSL rules still run over whatever
    /// text they hold; that is a different channel and unaffected by this classification.)
    ///
    /// **`.md` and `.mdx` are in this group and their membership is CONDITIONAL on a second channel
    /// existing** — see `zzop_parser_typescript::PRESCAN_IMPORT_HOSTS`, which owns that roster and pairs
    /// each extension with the reader that finds its imports, and `dispatch::tests` for the assertion
    /// nailing the two facts together. A markdown page compiles to a Vue SFC, so its `<script setup>`
    /// block holds real `import` statements; an MDX page compiles to a component too and carries its
    /// imports as BARE top-level ESM instead. While nothing read either, this variant's sentence was
    /// measurably false for both: koel's `docs/config.ts` was reported `dead-candidates` with a live
    /// importer sitting in a `.md` page, and 175 of the corpus's 218 `.mdx` files carry a top-level
    /// `^import` that nothing counted (recount: `find corpus ( -name .git -o -name node_modules )
    /// -prune -o -type f -iname '*.mdx' -print0 | xargs -0 grep -l '^import ' | wc -l`).
    ///
    /// The repair for BOTH is the pre-scan reading them — i.e. `assemble::helpers::is_prescan_ext`
    /// delegating to that roster, which is what makes this variant's claim true for `.md`/`.mdx` and
    /// where to look if it ever reads false again — and NOT a reclassification. `.md` and `.mdx` sit on
    /// DIFFERENT arms of it (`ScriptBlocks` and `BareEsm`), and that difference is invisible from here:
    /// what this table needs to know is only that the imports are read, not by which reader.
    /// Reclassifying to [`Self::DataConfig`] is not the repair either (neither is structured data, and
    /// the row's "go look" remedy answers a question nobody asked here). Measured, flipping
    /// [`extraction_can_lose_facts`] for `md` adds a coverage-gap row on be-gin (6 of 40 files, zero
    /// `<script>` blocks between them) and adds NONE on koel, where `.md` is 47 of 2773 files and falls
    /// under the principal-share floor. The disclosure would fire everywhere the phenomenon is absent
    /// and stay silent where it is present.
    ///
    /// The RESIDUAL, named here rather than shipped as a per-tree row: the pre-scan reads IMPORTS only.
    /// Symbols and io declared inside a `.md` `<script>` block or anywhere in an `.mdx` page stay
    /// unprojected, so a function defined in an MDX page is invisible to every symbol-keyed rule.
    NoFactsToLose,
    /// Structured data and configuration. No LANGUAGE frontend is a plausible ask (question 1 stays
    /// "no"), but these formats routinely DECLARE facts this engine's io/symbol channels are built to
    /// carry — SQL in a MyBatis mapper, services and routes in a k8s manifest, endpoints in an OpenAPI
    /// document, the dependency set in a lockfile — and they equally routinely hold nothing but data
    /// (an i18n `locales/*.json` bundle). **This build cannot tell those two apart without reading the
    /// files, and it does not claim to**: the coverage-gap surfaces carry a `data-config` row and say
    /// on the wire that the reader must look, rather than deciding for them in either direction.
    /// Suppressing the row is the expensive error (the mall measurement above); asserting a gap is the
    /// cheap one, which is why the disclosure is a labelled row and not a verdict.
    DataConfig,
}

/// Extensions this engine deliberately never names in the "bring an adapter" per-extension disclosure
/// (`analyze::diagnostics::unparsed_extension_warning`) — non-source file types where a dispatch-`None`
/// result is not a request for a parser frontend. A mechanism list (like `DEFAULT_SKIP_DIRS` above), not
/// rule vocabulary — nothing here names a rule or pack id.
///
/// Each entry carries its [`NonSourceKind`], which is what separates "there was nothing here to read"
/// from "no frontend claims this, and whether that cost anything depends on the file". The groups below
/// are the ones this table has always been commented with; the kind column makes them readable by code
/// instead of only by a person:
/// - docs/text: prose, never source. → [`NonSourceKind::NoFactsToLose`]. `md` AND `mdx` are
///   additionally import PRE-SCAN hosts (`zzop_parser_typescript::PRESCAN_IMPORT_HOSTS`, on its
///   `ScriptBlocks` and `BareEsm` arms respectively) — a third axis that cuts ACROSS this table rather
///   than along it, and the reason that roster is not a projection of this one. `astro` is on that
///   roster too and is deliberately absent from THIS table, so it keeps earning its coverage-gap row
///   and its "bring an adapter" warning: the frontmatter pre-scan reads its imports, nothing more.
/// - data/config: structured data. → [`NonSourceKind::DataConfig`] — see that variant for why this
///   group used to be described as data "a DSL `IoScan`/`SymbolScan` matcher has no symbols/io to key
///   on", and why that sentence was measured false.
/// - styles: presentation, not logic. → [`NonSourceKind::NoFactsToLose`]
/// - markup-as-asset: plain `.html`/`.htm` are static assets in most trees this engine analyzes (SSR
///   template dialects are the exception — `.jsp`/`.erb`/`.vue`/`.svelte` are deliberately NOT listed
///   here at all, since those ARE plausible adapter targets and should still warn). Classed
///   [`NonSourceKind::DataConfig`] rather than inert: markup can carry forms, urls and inline script,
///   so an unread `.html` majority is a fact worth a labelled row even though nobody will write an
///   HTML language frontend.
/// - images/fonts/media/binaries+archives: no text to parse at all. → [`NonSourceKind::NoFactsToLose`]
/// - misc: certificates — data, not code, and nothing structural to project. →
///   [`NonSourceKind::NoFactsToLose`]
///
/// The group boundaries are kept WHOLE on purpose: `csv`/`tsv`/`lock` are the members of the data/config
/// group with the weakest case for carrying declarations, and splitting them out would be this file
/// inventing a boundary no measurement supports (none of them clears the principal-share floor on any
/// tree in the dogfood corpus, so the split would be untestable as well as unfounded).
pub(super) const NON_SOURCE_EXTENSIONS: &[(&str, NonSourceKind)] = &[
    // docs/text
    ("md", NonSourceKind::NoFactsToLose),
    ("mdx", NonSourceKind::NoFactsToLose),
    ("txt", NonSourceKind::NoFactsToLose),
    ("rst", NonSourceKind::NoFactsToLose),
    ("adoc", NonSourceKind::NoFactsToLose),
    // data/config
    ("json", NonSourceKind::DataConfig),
    ("jsonc", NonSourceKind::DataConfig),
    ("json5", NonSourceKind::DataConfig),
    ("yaml", NonSourceKind::DataConfig),
    ("yml", NonSourceKind::DataConfig),
    ("toml", NonSourceKind::DataConfig),
    ("xml", NonSourceKind::DataConfig),
    ("csv", NonSourceKind::DataConfig),
    ("tsv", NonSourceKind::DataConfig),
    ("ini", NonSourceKind::DataConfig),
    ("properties", NonSourceKind::DataConfig),
    ("lock", NonSourceKind::DataConfig),
    // styles
    ("css", NonSourceKind::NoFactsToLose),
    ("scss", NonSourceKind::NoFactsToLose),
    ("sass", NonSourceKind::NoFactsToLose),
    ("less", NonSourceKind::NoFactsToLose),
    ("styl", NonSourceKind::NoFactsToLose),
    // markup-as-asset
    ("html", NonSourceKind::DataConfig),
    ("htm", NonSourceKind::DataConfig),
    // images
    ("png", NonSourceKind::NoFactsToLose),
    ("jpg", NonSourceKind::NoFactsToLose),
    ("jpeg", NonSourceKind::NoFactsToLose),
    ("gif", NonSourceKind::NoFactsToLose),
    ("webp", NonSourceKind::NoFactsToLose),
    ("svg", NonSourceKind::NoFactsToLose),
    ("ico", NonSourceKind::NoFactsToLose),
    ("bmp", NonSourceKind::NoFactsToLose),
    ("avif", NonSourceKind::NoFactsToLose),
    // fonts
    ("woff", NonSourceKind::NoFactsToLose),
    ("woff2", NonSourceKind::NoFactsToLose),
    ("ttf", NonSourceKind::NoFactsToLose),
    ("otf", NonSourceKind::NoFactsToLose),
    ("eot", NonSourceKind::NoFactsToLose),
    // media
    ("mp3", NonSourceKind::NoFactsToLose),
    ("mp4", NonSourceKind::NoFactsToLose),
    ("webm", NonSourceKind::NoFactsToLose),
    ("wav", NonSourceKind::NoFactsToLose),
    ("ogg", NonSourceKind::NoFactsToLose),
    ("mov", NonSourceKind::NoFactsToLose),
    // binaries/archives
    ("zip", NonSourceKind::NoFactsToLose),
    ("gz", NonSourceKind::NoFactsToLose),
    ("tar", NonSourceKind::NoFactsToLose),
    ("pdf", NonSourceKind::NoFactsToLose),
    ("wasm", NonSourceKind::NoFactsToLose),
    ("exe", NonSourceKind::NoFactsToLose),
    ("dll", NonSourceKind::NoFactsToLose),
    ("so", NonSourceKind::NoFactsToLose),
    ("dylib", NonSourceKind::NoFactsToLose),
    ("node", NonSourceKind::NoFactsToLose),
    ("jar", NonSourceKind::NoFactsToLose),
    ("map", NonSourceKind::NoFactsToLose),
    // misc
    ("pem", NonSourceKind::NoFactsToLose),
    ("crt", NonSourceKind::NoFactsToLose),
];

/// This extension's [`NonSourceKind`], or `None` when the extension is not in [`NON_SOURCE_EXTENSIONS`]
/// at all — i.e. SOURCE, which includes every extension a parser frontend claims and every template
/// dialect deliberately left out of the table. Case-insensitive, mirroring `dispatch_by_extension`'s own
/// `to_ascii_lowercase` normalization (the caller is not required to pre-lowercase `ext`).
///
/// The single reader of the table; [`is_non_source_extension`] and [`extraction_can_lose_facts`] are both
/// projections of it, so the two questions in [`NonSourceKind`]'s doc cannot drift apart the way they did
/// while one boolean answered both.
pub fn non_source_kind(ext: &str) -> Option<NonSourceKind> {
    let lower = ext.to_ascii_lowercase();
    NON_SOURCE_EXTENSIONS
        .iter()
        .find(|(name, _)| *name == lower)
        .map(|(_, kind)| *kind)
}

/// True if `ext` names a non-source file type — the filter `unparsed_extension_warning`'s collection
/// step applies before asking the reader to bring a parser adapter for a dispatch-`None` extension.
/// Question 1 of [`NonSourceKind`]'s doc; unchanged in meaning and membership since this table was a
/// bare name list.
///
/// **Not the "was anything lost" test** — that is [`extraction_can_lose_facts`], and using this one for
/// it is the exact defect [`NonSourceKind`] records.
pub fn is_non_source_extension(ext: &str) -> bool {
    non_source_kind(ext).is_some()
}

/// True when a dispatch-`None` on `ext` CAN mean facts were lost — question 2 of [`NonSourceKind`]'s
/// doc, and the gate the coverage-gap surfaces (`unreadExtensions`, `coverageGaps`) apply instead of
/// [`is_non_source_extension`].
///
/// `true` for source extensions (absent from the table) and for [`NonSourceKind::DataConfig`]; `false`
/// only for [`NonSourceKind::NoFactsToLose`], where the claim "nothing was lost" is safe to make
/// unconditionally. A surface reading this must still LABEL a `DataConfig` row as such — the predicate
/// says "this could have cost something", never "this did".
pub fn extraction_can_lose_facts(ext: &str) -> bool {
    non_source_kind(ext) != Some(NonSourceKind::NoFactsToLose)
}

/// The wire spelling every surface that publishes an extension's content class must use, so the
/// vocabulary has ONE owner instead of one per reply shaper. Three tokens, closed set:
/// `"source"` (not in [`NON_SOURCE_EXTENSIONS`] at all — a language a frontend could read),
/// `"data-config"` ([`NonSourceKind::DataConfig`]) and `"no-facts-to-lose"`
/// ([`NonSourceKind::NoFactsToLose`]).
///
/// A surface filtering on [`extraction_can_lose_facts`] only ever emits the first two — the third is
/// spelled here anyway so the token set is complete at its definition rather than at each call site,
/// and so a test can name what was filtered out.
pub fn extension_content_kind(ext: &str) -> &'static str {
    match non_source_kind(ext) {
        None => "source",
        Some(NonSourceKind::DataConfig) => "data-config",
        Some(NonSourceKind::NoFactsToLose) => "no-facts-to-lose",
    }
}
