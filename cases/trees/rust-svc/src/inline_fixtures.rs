// The corpus half of the test-region gate — and, since 2026-08-02, of its ONE declared opt-out. The
// gated module below holds four literals and the benchmark scores them in TWO different directions.
//
// Every literal in that module is a copy of a shape this tree's OTHER files carry as a LABELED anchor
// (`sql/select-star`, `sql/delete-no-where`, `egress/localhost-url-literal-committed`,
// `security/hardcoded-secret`). The one difference is that these sit under the two Rust test attributes
// — the convention that puts unit tests INSIDE the shipping file, where no path pattern can see them.
//
// WHAT MUST STAY SILENT (`over_fetch`, `whole_table_delete`, `dev_endpoint`).
// `sql/select-star`, `sql/delete-no-where` and
// `egress/localhost-url-literal-committed` judge code that RUNS, and this code does not ship. The gate
// (`zzop_parser_rust::extract_test_spans` -> `SourceFile::test_spans` -> `dsl::eval`) is what keeps them
// out; those lines carry no entry in `cases/EXPECTED.jsonc`, so a gate that stops working turns them
// into FALSE POSITIVES here rather than going quiet somewhere nobody reads.
//
// WHAT MUST FIRE (`api_key`). `security/hardcoded-secret` declares `scan_test_regions` — a credential at
// rest is leaked by the COMMIT, and this one is in git history, in every fork and every clone, whether
// or not the module compiles into the shipping build. It IS labeled in `cases/EXPECTED.jsonc`. Before
// that flag existed the gate deleted this finding and the run reported clean, which is what put the
// line here unlabeled in the first place.
//
// The pair is the point. One region, one span, one pass — three rules silenced and one not — so neither
// half can be satisfied by the other's failure: a gate that stopped working shows up as three FPs, and
// an opt-out that stopped working shows up as one FN.
//
// SEEN RED, and the drill is worth stating because the first attempt at it was VACUOUS: replacing only
// the module's `cfg` attribute leaves the inner function's own test attribute standing, the region is
// still gated, and the drill reads green while proving nothing. Removing BOTH attributes fires all four
// findings, one per literal line — i.e. it adds the three that must stay silent.
//
// Its name is deliberately NOT `tests.rs` or `test_*.rs`, and it is not under a `tests/` directory: a
// path-shaped name would be excluded by `${test-paths}` before any span was consulted, and the file
// would then prove nothing about the span.
//
// The secret-shaped literal uses the same non-vendor shape `credentials.rs` uses. A contiguous
// `sk_live_`-style token cannot be committed here at all (`scripts/check-vendor-token-literals.sh`
// refuses it, which is why the detection gate SYNTHESIZES its one such fixture at run time) — measured,
// after a first draft of this file carried one.

/// Shipped code, in the same file — the control that keeps "silent" from meaning "the rules died here".
/// This one IS labeled in `cases/EXPECTED.jsonc`.
pub fn shipped_report_sql() -> &'static str {
    "SELECT * FROM svc_reports"
}

#[cfg(test)]
mod tests {
    #[test]
    fn fixture_queries_are_not_shipped_code() {
        let over_fetch = "SELECT * FROM svc_users";
        let whole_table_delete = "DELETE FROM sessions";
        let dev_endpoint = "http://localhost:4010/internal";
        let api_key = "a7Fk29QmZx41Lp08Wd";
        assert!(!over_fetch.is_empty());
        assert!(!whole_table_delete.is_empty());
        assert!(!dev_endpoint.is_empty());
        assert!(!api_key.is_empty());
    }
}
