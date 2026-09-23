//! The `coverageGaps.meaning` sentence — the run-invariant half of the cell, shipped inside the object
//! it describes.
//!
//! Split out of the parent on 2026-09-01, on the seam the parent's own split already uses: `census.rs`
//! is the pass that fills the buckets, `mod.rs` is the filter that judges them, and this is what the
//! reply SAYS about that judgment. Nothing here reads a bucket and nothing there spells a sentence.

/// The vocabulary rides INSIDE the object it describes — the same device `ruleTimings` and
/// `architecture.painMeaning` use, and for the same reason: a consumer that reads the rows cannot fail
/// to also have read what they omit. Every number it names lives somewhere else in THIS reply and is
/// pointed at rather than copied.
///
/// A function rather than a `const` since 2026-09-01, for the reason `census::PRINCIPAL_SHARE_PCT`'s
/// own doc gives about the value it used to copy: the share this sentence PUBLISHES has to be the
/// share the filter APPLIED. It was spelled `10` here while the predicate read the engine's constant,
/// so moving that constant would have shipped a reply stating a floor it did not use — the exact
/// failure that doc records, one layer further out, where a reader rather than a test would find it.
/// "Sealing a copy with a test was the alternative; deleting the copy is strictly better" is that
/// doc's own conclusion, and interpolation is how a sentence deletes its copy.
///
/// It names no subcommand, and that is a contract rather than a style preference: the same string
/// reaches an MCP client that has no argv (`rule_contracts::host_vocabulary`, which caught exactly
/// this in the edit that removed the line leg — a parenthetical `zzop coverage <path>` beside the
/// field name it was already pointing at).
pub(super) fn meaning() -> String {
    format!(
        "Extensions contributing ZERO resolved import edges while being a principal \
    filetype here (>={floor}% of this tree's walked files; a parsed extension is judged \
    against >={floor}% of the files with a structural projection instead). The measuring stick is the \
    FILE count and only the file count: line counts are not read here at all. How a filetype spreads \
    its lines, and how many lines the tree's OTHER filetypes carry, are not evidence about whether \
    anything read THESE files — a tree whose line total is led by fonts, images or a vendored SQL \
    dump would otherwise hide the language it is written in. The accepted cost runs the other way and \
    is disclosed rather than erased: one outsized member (a lock file, a generated bundle) can carry \
    its filetype into this list, and so can a lone build manifest in a tree small enough that one \
    file is a tenth of it. \
    Read a row as a place to look rather than as a verdict. Each row's kind field says \
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
    nothing in-tree; the declared side is `coverage.declaredImportsByExt`. \
    THE WORD ZERO ABOVE IS LOAD-BEARING AND IT IS ALSO THIS LIST'S BLIND SPOT, stated here rather \
    than left to be discovered: the test is ALL-OR-NOTHING, so an extension where even ONE file \
    contributes a resolved edge is dropped WHOLE, however many of its other files resolved nothing. \
    PARTIAL resolution loss — the ordinary shape of a broken path alias, where most imports land and \
    a subtree's do not — can therefore never appear here, and its absence from these rows is a \
    property of the test, not a measurement that it did not happen. The unit-correct place to see it \
    is the aggregate coverage surface, which carries `structural` and `inDepGraph` as FILE counts on \
    one row per extension: parsed files minus files contributing a resolved edge is the partial \
    figure, at a grain where the subtraction is legal. \
    DO NOT SUBTRACT `coverage.resolvedImportEdges` FROM `coverage.declaredImportsByExt` to get it. \
    Those are different units and the difference is not a count of anything: a declaration is a \
    SPECIFIER and an edge is a resolved (importer, file) PAIR, so several specifiers can land on one \
    file and one glob import can fan out to several edges. The remainder of that subtraction is not \
    'unresolved imports'; it has no name. \
    An empty list means the \
    cross ran and found nothing ABOVE these gates — `basis` says what was crossed, so it never means \
    'not measured', and the paragraph above says which population the gates put out of reach. \
    This is a share filter over extension counts, not a language judgment, and not the whole table — \
    the aggregate coverage surface carries every extension plus the capability crosses. Its \
    `unreadExtensions` cell applies the SAME floor to the SAME eligibility test over the SAME \
    denominator, so the no-parser rows of the two are one answer rather than two that disagree at \
    the margin. This list is the wider of the two, by a difference in subject rather than in \
    threshold: it additionally carries extensions that PARSED and still resolved nothing, which a \
    parser-capability list has no way to hold. The cross-layer JOIN is a separate axis whose \
    counts already ride this reply \
    (`coverage.ioProvides` / `ioConsumesKeyed` / `ioConsumesUnresolved` / `joinContributionZero`): a \
    near-zero provider count on a tree that serves routes makes an unprovided-consume finding a \
    statement about extraction, not about the code.",
        floor = zzop_facade::MIN_UNCOVERED_EXTENSION_SHARE_PCT
    )
}
