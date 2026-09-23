//! The sql-pack half of the §33/§37 unique-pair precondition pin. The constant, the family census and
//! the reasoning live in `rules/dsl/unique_pair_precondition_landing.rs`, which both this pack and the
//! `db` pack include — the two are separate test crates, so the one spelling cannot live in either.

use crate::{
    assert_landing_precedes_imperative, hits, scan, unique_pair_precondition_landing::*, TempDir,
};

/// The position pin, on a DELIVERED finding. Separate `#[test]` from this rule's §27-a pin in
/// `toctou.rs` on purpose: that one asserts a needle SET on the clause helper, which every one of the
/// family's three spellings satisfies; this one asserts the exact registered bytes, which only two of
/// them do.
#[test]
fn race_condition_toctou_unique_pair_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "api/createSubHandlers.ts",
        "declare const subStore: any;\nexport async function subscribe() {\n  const existing = await subStore.findOne((s: any) => s.id === \"x\");\n  if (!existing) {\n    await subStore.create({ id: \"y\" });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "race-condition-toctou");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "race-condition-toctou",
        &h[0].message,
        unique_pair_precondition_landing(),
        "Where the pair IS unique, add a unique constraint and then pick ONE of two ways",
    );
}
