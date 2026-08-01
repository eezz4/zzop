//! This crate's machine-readable sightline declarations (`zzop_core::RuleSightline` — see that
//! module's doc for the mechanism and the direction of the claim). Both entries are the structured
//! form of the SAME fact the two scanners' finding messages and catalog rows already publish in prose
//! ([`super::write_site_sightline_claim`] + `docs/rules/catalog.md`'s Language sightline paragraphs):
//! `SourceSymbol::write_sites` is produced by the TypeScript parser alone, so outside
//! [`super::WRITE_SITE_COVERED_EXTENSIONS`] the BFS has no write site to reach and neither rule can
//! fire at all. Structuring existing truth, not asserting new capability — the extension list and the
//! claim sentence are both quoted from the constants/functions the prose pins already guard.

use zzop_core::RuleSightline;

use super::{write_site_sightline_claim, WRITE_SITE_COVERED_EXTENSIONS};

/// The two write-site-gated rules' declarations. A `fn`, not a `const`, because `outside_meaning`
/// splices in [`write_site_sightline_claim`] (which itself quotes the extension constant) — reusing
/// the pinned claim keeps this the second READER of one fact rather than a second copy of it.
pub fn rule_sightlines() -> Vec<RuleSightline> {
    let claim = write_site_sightline_claim();
    let entry = |rule_id: &'static str, silence_reads_as: &str| RuleSightline {
        rule_id,
        trigger_extensions: WRITE_SITE_COVERED_EXTENSIONS,
        outside_meaning: format!(
            "this check {claim}, so a handler in a file outside these extensions has no write site \
             the call-graph BFS could reach — ZERO findings of this rule on those files means NOT \
             ANALYZED, never {silence_reads_as:?}"
        ),
        assert_when_blind: false,
    };
    vec![
        entry("unsafe-read-endpoint", "no unsafe read"),
        entry("non-idempotent-write", "no non-idempotent write"),
    ]
}
