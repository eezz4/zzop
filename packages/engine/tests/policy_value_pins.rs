//! T2 policy-value equality pins: two constants that live in different crates, encode the *same*
//! policy, and therefore cannot share a symbol (T1) across that crate boundary. A pin test here is
//! the substitute — if one constant changes, this test fails and forces the other to be
//! re-justified rather than silently drifting apart.

// MIN_FOREIGN_UNPROVIDED_GROUP (rules-http) and MIN_PREFIX_DRIFT_GROUP
// (rules-cross-layer) encode the same policy ("2 is coincidence, 3 is a
// pattern") across a crate boundary; if one changes, this pin forces the
// other to be re-justified.
#[test]
fn min_foreign_unprovided_group_matches_min_prefix_drift_group() {
    assert_eq!(
        zzop_rules_http::unprovided_consume::MIN_FOREIGN_UNPROVIDED_GROUP,
        zzop_rules_cross_layer::cross_layer::prefix_drift::MIN_PREFIX_DRIFT_GROUP,
        "MIN_FOREIGN_UNPROVIDED_GROUP (rules-http) and MIN_PREFIX_DRIFT_GROUP (rules-cross-layer) both \
         encode the same \"2 is coincidence, 3+ is a pattern\" fold-threshold policy for aggregating \
         same-cause findings; a crate boundary prevents T1 symbol sharing, so this equality pin is the \
         T2 substitute (rule-quality.md §6) — if one changes, re-justify the other and update this pin."
    );
}
