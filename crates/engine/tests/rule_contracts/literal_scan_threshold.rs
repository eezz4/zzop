//! Crate↔pack policy pin for the entropy floor — `policy_pins`' boundary ("a JSON pack cannot
//! reference a Rust constant") applied to a NUMBER instead of a pattern.
//!
//! `security/high-entropy-secret`'s `entropy_min` is a measured threshold
//! (`zzop_core::HIGH_ENTROPY_SECRET_MIN_BITS` — its doc carries the separation measurement and the
//! regeneration command). The value the engine EVALUATES is the pack's, because a pack cannot name a
//! Rust constant; the constant exists so the number has a census home (`scripts/policy-census.txt`
//! tracks Rust constants; the inline-value census walks only PATTERN fields, so an f32 matcher field
//! is invisible to it — `def::pattern_fields`' `LiteralScan` arm says so at the field). This pin is
//! what keeps the two spellings one number: retune the threshold and only one side, and this goes red
//! with the measurement doc in hand.

use zzop_core::Matcher;

use crate::load_all_packs;

#[test]
fn the_shipped_entropy_floor_equals_the_censused_constant() {
    let packs = load_all_packs();
    let pack = packs
        .iter()
        .find(|p| p.id == "security")
        .expect("security pack loaded");
    let rule = pack
        .rules
        .iter()
        .find(|r| r.id == "high-entropy-secret")
        .expect(
            "security/high-entropy-secret shipped — if the rule was renamed or removed, this pin \
                 must follow it (or be deleted deliberately), never silently stop matching",
        );
    let Matcher::LiteralScan(m) = &rule.matcher else {
        panic!(
            "security/high-entropy-secret must be a literal-scan rule, got {:?}",
            rule.matcher
        );
    };
    let shipped = m.entropy_min.expect(
        "high-entropy-secret must declare an entropy_min — an unset floor turns the rule \
                 into 'flag every secret-named literal', which is not the judged design",
    );
    assert_eq!(
        shipped.to_bits(),
        zzop_core::HIGH_ENTROPY_SECRET_MIN_BITS.to_bits(),
        "security/high-entropy-secret ships entropy_min={shipped} but \
         zzop_core::HIGH_ENTROPY_SECRET_MIN_BITS={} — the pack value is what the engine evaluates and \
         the constant is the censused, measurement-documented spelling; they must move together (the \
         constant's doc carries the separation measurement a retune has to redo)",
        zzop_core::HIGH_ENTROPY_SECRET_MIN_BITS,
    );
}
