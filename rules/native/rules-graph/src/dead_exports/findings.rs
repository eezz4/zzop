//! Finding-shaping for the `"unimported-export"` native analysis — see the parent module doc's
//! "Engine wiring" section.

use std::collections::HashMap;

use zzop_core::{disable_hint, Finding, Severity, SourceSymbolKind};

use super::{DeadExport, DeadExportReason};

/// Converts every `find_dead_exports` result into a `Finding` at its symbol's declaration line.
pub fn dead_export_findings(
    dead: Vec<DeadExport>,
    symbol_lines: &HashMap<(&str, &str), u32>,
) -> Vec<Finding> {
    dead.into_iter()
        .map(|d| dead_export_to_finding(symbol_lines, d))
        .collect()
}

fn dead_export_to_finding(symbol_lines: &HashMap<(&str, &str), u32>, d: DeadExport) -> Finding {
    let line = symbol_lines
        .get(&(d.file.as_str(), d.name.as_str()))
        .copied()
        .unwrap_or(1);
    // The framework-convention clause comes BEFORE the imperative on purpose (`rule-quality.md` §27):
    // a reader who acts on the first instruction never reaches a caveat placed after it, and the act
    // this rule prescribes is deleting an export. MEASURED (cal.com, 2026-08-26, full 1162-finding
    // enumeration): five payment webhooks under pages/api were told to delete the very export that
    // keeps their raw body — and therefore their HMAC signature check — intact.
    //
    // Those five are now GATED in `find_dead_exports`, and that is why this sentence does NOT tell
    // their story: a message must describe the population it still reaches. The clause's whole job
    // is the residue the gate cannot name — the next framework, the next reserved name — so it names
    // the SHAPE (a name read from the path) with three recognizable instances and stops. Spelling
    // out the webhook consequence cost 538 characters against §27's measured ~50-per-rule band while
    // illustrating a case the reader can no longer be looking at.
    //
    // A SECOND shape was added 2026-08-27, at the same level rather than as a fourth example, because
    // it is a different KIND: shape (1) reserves a NAME, shape (2) nominates a DIRECTORY and reserves
    // nothing. A Nuxt reader met three path-convention examples, matched none of them, and proceeded —
    // measured on nocodb, four prescriptions that break the build or the running app, and 644 findings
    // (78% of the tree's total for this rule) sitting under one app's auto-import directories.
    // Those with a `nuxt.config.*` are now RESOLVED name by name in the engine, so — same rule as
    // above — this clause does not retell them; it names the shape and hands the reader the
    // discriminating question, then says exactly which spelling is already handled and which is not
    // (`unplugin-auto-import`, a layer's inherited table). Corpus harvest for those two spellings is
    // ZERO, which is why they are named in prose here rather than gated in `find_dead_exports`
    // (`rule-quality.md` §26 (3)).
    //
    // What this clause deliberately does NOT say: that a `.vue`/`.svelte`/`.astro` consumer is
    // invisible. It is not — `zzop_parser_typescript::PRESCAN_IMPORT_HOSTS` reads their `<script>`
    // imports and feeds them here as `prescan_import_pairs`, VERIFIED on nocodb (exports whose only
    // importer is a `.vue` file report nothing). Saying otherwise would teach a reader to doubt true
    // findings, which `rule-quality.md` §32 rejects for a disclosure sentence in exactly these words.
    // The generated-file escape hatch is named here, not only in the catalog: a reader holding 15 of
    // these on one machine-written file does not open the catalog, and the only other hatch this
    // message offers ("turn the rule off") costs them every other finding in the tree. This rule
    // already skips files carrying a generated banner; the sentence exists because the case that
    // reaches a user is the one with NO banner, where `exclude` is the answer and nothing said so.
    let message = format!(
        "exported {} '{}' is {} ({}). Zero in-repo importers is BY DESIGN for two shapes, and these \
         are examples rather than a list to match yourself against: in both, deleting the export \
         changes runtime behavior with no import left to break. (1) A framework reads this name from \
         the file's own path rather than importing it — `export const config` in a Next.js Pages \
         Router route, `export const prerender` in an Astro page, `export function load` in a \
         SvelteKit route. (2) A build tool injects a whole DIRECTORY's exports as globals, so every \
         consumer writes the bare identifier and no import line exists anywhere — Nuxt auto-imports \
         the directories its `nuxt.config.*` nominates, and `unplugin-auto-import` does the same from \
         a Vite or Webpack setup. Nothing is reserved about the NAME in (2), \
         so the question to ask is about the PATH: does a build tool in this tree nominate the \
         directory this symbol lives in as an injection source? A `nuxt.config.*` that nominates it \
         is already read and resolved here, name by name; a table declared anywhere else, or \
         inherited by a Nuxt layer through `extends`, is not. {} A file carrying a machine-generated banner in its first 8 \
         lines is skipped by this rule already (`vocabulary.generatedFileMarkers` picks the banner \
         vocabulary); a generator that stamps NO banner is invisible to that, and the answer for it \
         is an `exclude` entry for its path — deleting the `export` there is undone by the next \
         regeneration. {} if this is public API consumed outside this repo (e.g. \
         published to npm) — such consumers are invisible to this in-repo import graph.",
        kind_label(d.kind),
        d.name,
        match d.reason {
            DeadExportReason::Unused => "never imported anywhere",
            DeadExportReason::InFileOnly => "only referenced within its own file",
        },
        reason_label(d.reason),
        match d.reason {
            DeadExportReason::Unused => "Delete it, or export it from somewhere it's actually consumed.",
            DeadExportReason::InFileOnly => "Drop the `export` keyword to make the un-used-elsewhere status explicit.",
        },
        disable_hint("unimported-export"),
    );
    Finding {
        rule_id: "unimported-export".to_string(),
        severity: Severity::Info,
        file: d.file.clone(),
        line,
        message,
        evidence_paths: Vec::new(),
        data: serde_json::to_value(&d).ok(),
    }
}

fn kind_label(kind: SourceSymbolKind) -> &'static str {
    match kind {
        SourceSymbolKind::Function => "function",
        SourceSymbolKind::Class => "class",
        SourceSymbolKind::Const => "const",
        SourceSymbolKind::Type => "type",
        SourceSymbolKind::Interface => "interface",
    }
}

fn reason_label(reason: DeadExportReason) -> &'static str {
    match reason {
        DeadExportReason::Unused => "deletion candidate",
        DeadExportReason::InFileOnly => "un-export candidate",
    }
}
