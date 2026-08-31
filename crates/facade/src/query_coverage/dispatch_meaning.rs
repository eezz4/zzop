//! `dispatchMeaning` — one sentence per dispatch class, shipped in the reply so the per-extension
//! table's vocabulary is self-describing rather than documented somewhere the reader is not.
//!
//! Split out of `query_coverage.rs` on 2026-08-20 for the repo's per-file line cap, and it is the same
//! seam every other cell on this reply already uses: `blind_spots`, `unread` and `recognizers` each own
//! their own `legend()` beside the rows they describe. This one was the outlier, sitting inline in the
//! assembler.

use serde_json::{json, Value};

/// One sentence per dispatch class (plus the per-row `inDepGraph` derived count), shipped in the
/// reply so the vocabulary is self-describing — the same discipline as `query_file`'s
/// `verdictMeaning`, sharing its semantics: `structural` here is `analyzed` there, per file.
pub(super) fn legend() -> Value {
    json!({
        "structural": "a parser frontend read the file and projected at least one fact — a symbol, a \
                       dep-graph entry, or an io provide/consume. All THREE channels count, because a \
                       frontend need not fill all three: the SQL frontend projects io only (no \
                       symbols, no imports), so counting the first two alone reported a parsed \
                       `.sql` file as `lexicalOnly` beside a census that said a parser had been \
                       dispatched to it. NOT a per-rule claim: which declared rules still lack their \
                       evidence channel on this tree is `blindSpots`' axis, so read an empty findings \
                       list against that list, not as clean outright",
        "lexicalOnly": "walked and line-scanned only — no parser in this build projected any symbol, \
                        import or io fact from the file, so everything needing those was never \
                        evaluated; an empty findings list does NOT mean clean. Its usual cause is \
                        that no frontend claims the extension at all, which `unreadExtensions` names \
                        for the filetypes large enough to matter",
        "degraded": "a parser tried and bailed (syntax error or over sizeCap) — text rules ran, \
                     structural ones did not",
        "inDepGraph": "files of this extension contributing at least one RESOLVED outgoing import \
                       edge (a non-empty source entry in the dep graph). A LOW count against `files` \
                       on a structural extension is the import-resolution blindness signal: the \
                       files were parsed, but their imports did not resolve to in-tree files. NOT a \
                       declared-imports count — the dep graph carries only resolved edges; the \
                       declared side is `declaredImports`' axis, so read the two together",
        "declaredImports": "sum over this extension's parsed files of each file's DISTINCT declared \
                            import specifiers (import/use/using bindings, re-exports, dynamic \
                            import()), counted BEFORE resolution — package imports and specifiers \
                            no resolver could map are still in it, so `declaredImports` high with \
                            `inDepGraph` low is the import-resolution blindness signal read directly. \
                            NOT 1:1 with the census's `resolvedImportEdges`: a declaration is a \
                            specifier and an edge is a resolved (importer, file) pair — several \
                            specifiers can land on one file, and one glob import can fan out to \
                            several edges. `null` means NEVER MEASURED for this extension (its \
                            parser projects no import channel — e.g. prisma/sql — or the tree was \
                            ingested as a Mode A envelope, which measures nothing here): absence of \
                            data, not 0. A degraded file counts 0 declared — read the row's \
                            `degraded` column beside it"
    })
}
