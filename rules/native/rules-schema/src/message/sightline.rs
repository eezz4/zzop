//! The two LANGUAGE/EVIDENCE SIGHTLINE sentences this crate publishes, split out of the parent
//! `message` module purely for size (the 300-line source cap) — the same convention
//! `zzop_rules_http`'s `mutating_route_no_auth/message.rs` follows.
//!
//! Both exist for one reason: a rule's verdict is only as wide as the structural fact it reads, and
//! neither fact here is produced for every language. Publishing the sightline IN the finding (and
//! pinning the published pages against it) is what keeps a zero — or, for `unreferenced-model-name`, a flood —
//! from reading as a verdict about the code.

/// The MARKUP-FREE claim `unreferenced-model-name`/`unreferenced-field-name` must publish, and that `docs/rules/catalog.md` /
/// `site/rules.html` are pinned against (`tests::the_field_usage_sightline_is_identical_in_the_finding_
/// and_the_published_docs`). Extension list quoted from [`crate::usage::FIELD_USAGE_SCAN_EXTENSIONS`],
/// so the one part most likely to go stale cannot.
///
/// Why these two findings, alone among this file's messages, need a sightline: they are the INVERSE of the
/// usual language-coverage gap. Every other language-limited rule goes SILENT where its evidence channel is
/// empty (a false all-clear); these two ASSERT — "never appears in source" is emitted precisely when the
/// evidence set is empty, so a tree with no `.ts`/`.tsx` at all (a schema-only tree, which this project's
/// own multi-tree topology advice recommends, or a Prisma schema consumed by a Python/Go/Rust client)
/// reports EVERY model dead. Nothing about the finding says which files were searched, so the reader has no
/// way to tell that verdict from a real one. Measured 2026-07-25: a directory holding one `schema.prisma`
/// and nothing else yields one `unreferenced-model-name` per model.
pub(super) fn field_usage_sightline_claim() -> String {
    format!(
        "identifier tokens scanned from this tree's {} files only",
        crate::usage::FIELD_USAGE_SCAN_EXTENSIONS
            .iter()
            .map(|e| format!(".{e}"))
            .collect::<Vec<_>>()
            .join("/")
    )
}

/// The full sentence both findings append — [`field_usage_sightline_claim`] plus what an empty channel
/// means. Markup-free-ness only has to hold for the CLAIM (the pinned part); this wrapper may use
/// backticks freely, since no page is compared against it.
pub(super) fn field_usage_sightline() -> String {
    format!(
        "EVIDENCE SIGHTLINE: \"in code\"/\"in source\" here means {claim} (`.d.ts` excluded) — no \
         Python, Go, Java, C# or Rust source is searched for the name, so a schema consumed from those \
         languages reads as dead. A tree holding the schema WITHOUT any such file (a schema-only tree, \
         a Prisma schema with a non-TypeScript client) supplies NO evidence at all, and then every \
         model reports here no matter how it is used — check the tree actually contains the consuming \
         code before acting, or inject a `bound-model` attribute on the model.",
        claim = field_usage_sightline_claim()
    )
}

/// The MARKUP-FREE claim all three JOIN rules must publish, pinned against `docs/rules/catalog.md` and
/// `site/rules.html` by `tests::the_query_call_site_sightline_is_identical_in_the_findings_and_the_docs`.
///
/// Names the PRODUCER rather than an extension list, deliberately: the fact
/// (`zzop_core::QueryCallSite`) has exactly one producer — `zzop_parser_typescript::
/// extract_query_call_sites`, threaded only on the `Some(Language::TypeScript)` arm of the engine's
/// per-file projection — so there is no list to keep in step and therefore no third copy to drift.
pub(super) fn query_call_site_sightline_claim() -> &'static str {
    "query call sites are extracted by the TypeScript parser only"
}

/// Extensions whose files the TypeScript parser — the ONE producer of `zzop_core::QueryCallSite`
/// (see [`query_call_site_sightline_claim`]) — is dispatched for. The prose claims above deliberately
/// name the producer instead of a list; the machine-readable declarations below cannot (a coverage
/// consumer crosses extensions, not producer names), so this is that producer's dispatch arm spelled
/// out once.
///
/// POLICY VALUE, T2: duplicates `zzop_engine`'s TypeScript dispatch arm (this crate depends on
/// `zzop_core` only, so it cannot import the predicate) — pinned both directions by
/// `zzop_engine::sightlines`' `ts_witness_extension_lists_match_the_dispatch_table`, the same
/// arrangement as `zzop_rules_http::http_scan::WRITE_SITE_COVERED_EXTENSIONS`.
pub const QUERY_CALL_SITE_EXTENSIONS: &[&str] =
    &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"];

/// This crate's machine-readable sightline declarations (`zzop_core::RuleSightline` — see that module's
/// doc for the mechanism and the direction of the claim), the structured form of the two prose
/// sentences this file already owns, built FROM the same pinned claim functions so the two halves
/// cannot drift. Two classes, opposite consequences, each entry wording its own:
/// - the three JOIN rules go SILENT where blind (no call site, no finding — a zero that reads clean);
/// - the two usage rules ASSERT where blind ("never appears in source" is emitted precisely when the
///   evidence set is empty — a flood that reads as deadness).
pub fn rule_sightlines() -> Vec<zzop_core::RuleSightline> {
    vec![
        join_sightline("soft-delete-bypass"),
        join_sightline("orderby-unindexed"),
        join_sightline("enum-string-drift"),
        usage_sightline("schema/unreferenced-model-name"),
        usage_sightline("schema/unreferenced-field-name"),
    ]
}

/// One JOIN rule's declaration — the silent-when-blind class.
fn join_sightline(rule_id: &'static str) -> zzop_core::RuleSightline {
    zzop_core::RuleSightline {
        rule_id,
        trigger_extensions: QUERY_CALL_SITE_EXTENSIONS,
        outside_meaning: format!(
            "{claim}, so a Prisma client driven from a file outside these extensions contributes no \
             call site — ZERO findings of this rule on those files means NOT ANALYZED, never \"no \
             such call site\"",
            claim = query_call_site_sightline_claim()
        ),
        assert_when_blind: false,
    }
}

/// One usage rule's declaration — the assert-when-blind class (`assert_when_blind: true`, the only
/// two declarations in this build that set it), so its consequence sentence is the inverse of
/// [`join_sightline`]'s.
fn usage_sightline(rule_id: &'static str) -> zzop_core::RuleSightline {
    zzop_core::RuleSightline {
        rule_id,
        trigger_extensions: crate::usage::FIELD_USAGE_SCAN_EXTENSIONS,
        outside_meaning: format!(
            "\"unreferenced\" here means {claim} — a file outside these extensions is never searched \
             for the name, so a model or field consumed only from such files still reports here: \
             findings on a tree whose consumers live outside them are evidence-channel blindness, \
             not deadness",
            claim = field_usage_sightline_claim()
        ),
        assert_when_blind: true,
    }
}

/// The full sightline sentence all three JOIN rules splice in.
///
/// Why: these rules JOIN a Prisma schema against call sites, and the schema half is language-neutral
/// while the call-site half is not. A Prisma schema driven by `prisma-client-py` or `prisma-client-go`
/// therefore produces a fully-parsed schema, zero call sites, and zero findings — which reads exactly
/// like a clean codebase. Same class as `zzop_rules_http`'s `write_site_sightline`.
pub(super) fn query_call_site_sightline() -> String {
    format!(
        "LANGUAGE SIGHTLINE: {claim} (`zzop_parser_typescript::extract_query_call_sites`), so this \
         check only ever sees a Prisma client called from TypeScript/JavaScript — the same schema \
         driven by `prisma-client-py`, `prisma-client-go`, or any other non-TypeScript client \
         contributes no call site at all, and this rule then reports nothing no matter what those \
         calls do. ZERO findings of this rule in such a repo means NOT ANALYZED, never \"no such \
         call site\".",
        claim = query_call_site_sightline_claim()
    )
}
