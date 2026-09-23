//! The db-pack half of the §33/§37 unique-pair precondition pin. The constant, the family census and
//! the reasoning live in `rules/dsl/unique_pair_precondition_landing.rs`, which both this pack and the
//! `sql` pack include — the two are separate test crates, so the one spelling cannot live in either.

use super::*;

/// The position pin, on a DELIVERED finding. Separate `#[test]` from this rule's §27-a pin in
/// `races.rs` on purpose: that one asserts a needle SET on the clause helper, which every one of the
/// family's three spellings satisfies; this one asserts the exact registered bytes, which only two of
/// them do.
#[test]
fn check_then_act_in_loop_unique_pair_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/seed.ts",
        "declare const prisma: any;\ndeclare const rows: { key: string }[];\nexport async function ensureAllMarked() {\n  for (const r of rows) {\n    const e = await prisma.item.findFirst({ where: { key: r.key } });\n    if (!e) {\n      await prisma.item.create({ data: r });\n    }\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "check-then-act-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "check-then-act-in-loop",
        &h[0].message,
        unique_pair_precondition_landing::unique_pair_precondition_landing(),
        "Where the pair IS unique, add a unique constraint;",
    );
}
