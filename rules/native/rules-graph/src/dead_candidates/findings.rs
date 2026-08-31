//! Finding-shaping for `"dead-candidates"` — split out of the parent for the 300-line cap, along the
//! same seam `dead_exports/findings.rs` uses: the parent keeps the ANALYSIS and this file keeps the
//! sentence the analysis prints.

use zzop_core::{disable_hint, DepGraph, FileNode, Finding, Severity};

use super::{find_dead_candidates, DEAD_MAX_CHANGES};

/// One `Finding` per dead-candidate file (native analysis id `"dead-candidates"`, matching
/// `register_native_analyses`), gated at `DEAD_MAX_CHANGES`. See `find_dead_candidates`'s doc for
/// `extra_entries`.
pub fn dead_candidate_findings(
    nodes: &[FileNode],
    dep: &DepGraph,
    extra_entries: &std::collections::HashSet<String>,
) -> Vec<Finding> {
    find_dead_candidates(nodes, dep, DEAD_MAX_CHANGES, extra_entries)
        .into_iter()
        .map(|n| Finding {
            rule_id: "dead-candidates".to_string(),
            severity: Severity::Info,
            file: n.path,
            line: 1,
            // The framework-convention clause comes BEFORE the imperative on purpose
            // (`rule-quality.md` §27): a reader who acts on the first instruction never reaches a
            // caveat placed after it, and the act this rule prescribes is deleting a WHOLE FILE.
            // Before this clause the only hedge was the rule's own off switch — offset 1139 against
            // `Delete the file` at 738, and it named a custom bundler entry and a template-string
            // dynamic import, neither of which a reader holding a path-routed framework file would
            // try on for size. MEASURED (cal.com, 2026-08-26, full 334-finding enumeration): 64
            // findings told a reader to delete files Next.js routes by path, including every tRPC
            // namespace endpoint and the whole NextAuth handler.
            //
            // Those 64 are now GATED in `find_dead_candidates`, and that is why this sentence does
            // NOT tell their story: a message must describe the population it still reaches. The
            // clause's whole job is the residue the gate cannot name, so it names the SHAPE — a
            // whole file loaded from its own path — with the two MEASURED instances that are still
            // reported: a Docusaurus doc route (typeorm `docs/src/pages/maintainers.tsx`, whose
            // anchor is `docusaurus.config.ts`) and Nextra's `_meta` (cal.com `apps/docs/content/`,
            // 3). Nuxt's `composables/` was CONSIDERED for this list and rejected on measurement:
            // that class is already resolved engine-side (`nuxt_auto_import::scan` +
            // `merge_auto_import_fan_in`, anchored on `nuxt.config.*`), and nocodb's 6 surviving
            // `composables/` findings are the genuinely unreferenced ones that mechanism
            // deliberately keeps — verified 2026-08-26, zero references to any of the six anywhere
            // in the Nuxt app. Naming them here would teach a reader to disbelieve six true findings.
            //
            // The HALF-A-PRODUCT clause (2026-08-27) sits beside it, also BEFORE the imperative, and
            // it is NOT the sibling sentence copied. `unimported-export` closes with "if this is
            // public API consumed outside this repo (e.g. published to npm)" — trailing its own
            // imperative, scoped to PUBLIC API, and about a SYMBOL. Ported verbatim it would have
            // missed the case that produced it: nocodb's `packages/nocodb/src/models/CommentReaction.ts`
            // is a deep internal model in a package whose manifest DOES declare `main`, so a reader
            // answers "not public API, not the published entry" and deletes. The out-of-tree consumer
            // there is the closed-source enterprise half of the same product, importing the MODULE
            // PATH from a repository that is not on disk — `document-comments.service.ts:58`
            // `toggleReaction` is the CE stub of exactly that call site, whole body `return null`.
            // So the clause is (a) moved ahead of the verb, because this rule deletes a whole FILE,
            // (b) widened past "public API"/npm to any separate repository, and (c) given the
            // detection procedure and the three exits, since no gate covers this class at all —
            // see `ce_ee_consumer_clause_precedes_the_imperative`.
            //
            // NOT gated, deliberately (`rule-quality.md` §24/§35): every signal a tree writes about
            // its own missing half is CONVENTION grade — a prose comment, a one-statement method
            // body, a migration that outlived its reader — and this direction (silencing a deletion
            // candidate) is the one §24 requires a DECLARATION for. §26 (b)'s severity escape is
            // already spent: this rule is `Info`. The one declaration-grade mechanism that exists,
            // `PackageJsonScan::all_entry_paths` (`main`/`module`/`bin`/`exports` seeded into
            // `extra_entries`, `exports` subpaths included), reaches only what a manifest
            // DECLARES, which is why the clause says that rather than "published to npm".
            message: format!(
                "no importers found in this tree (candidate dead file — scoped to files that \
                 participate in the dep graph: a `dep`-map key or edge target, or a TS-dispatch \
                 extension ts/tsx/js/jsx/mjs/cjs/mts/cts as a fallback; dev-tool config files, \
                 ambient `.d.ts` declarations and package.json-referenced entry files are excluded, \
                 being loaded by a tool/runtime directly rather than imported; so is a path a tool \
                 config spells out as a quoted literal WITH its extension that resolves, relative to \
                 that config's own directory, to a file this tree has — usually a bundler's entry \
                 list, but the KEY is not read, so a path sitting in an `exclude`/`ignores` array is \
                 exempted the same way, and an entry written without its extension is not exempted \
                 at all). A framework may load a whole file from its own path rather than through an \
                 import — a Docusaurus `src/pages/` route, a Nextra `_meta` file, the next \
                 framework's next reserved directory — and there zero in-repo importers is the \
                 convention working, not a dead file: deleting it drops runtime behavior with no \
                 import left to break. Zero in-repo importers is also what HALF A PRODUCT looks \
                 like: if a closed-source enterprise edition, a downstream fork, or any separate \
                 repository imports this module by path, those consumers are invisible to this \
                 in-repo import graph — and the package.json exemption above reaches only what a \
                 manifest DECLARES (`main`/`module`/`bin`/`exports`, subpaths included), never every \
                 module under it. Such a tree usually says so about itself, and these are examples \
                 rather than a list to match yourself against: a stub \
                 commented as the community-edition no-op of a paid feature, a service method whose \
                 whole body is `return null` at the call site this file exists for, a migration \
                 still maintaining a table nothing here reads, a model left out of its package's \
                 barrel whose id prefix is still registered. The question that decides the rest is \
                 about the PRODUCT, not this file: is any shipping part of it built from a \
                 repository you are not looking at? If it might be, search that repository for this \
                 module path first, or mark the file deprecated for one release instead of deleting \
                 it, or leave it. If it is not, no importers means what it says. \
                 Delete the \
                 file if it is genuinely unused, or wire it up if it should be reachable. A file \
                 carrying a machine-generated banner in its first 8 lines is skipped already \
                 (`vocabulary.generatedFileMarkers` picks the banner vocabulary); a generator that \
                 stamps NO banner is invisible to that, and the answer for it is an `exclude` entry \
                 for its path — deleting the file is undone by the next regeneration. {} if your \
                 build loads files this graph can't see (e.g. a custom bundler entry, a \
                 template-string dynamic import).",
                disable_hint("dead-candidates")
            ),
            evidence_paths: Vec::new(),
            data: None,
        })
        .collect()
}
