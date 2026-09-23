//! §33/§37 LANDING for the check-then-act family's shared precondition — the sentence that says what
//! "add a unique constraint" costs a project whose column pair legitimately repeats.
//!
//! NOT A PACK DIRECTORY, and the second constant in this tree that is not (the first is
//! `sanitizer_subtraction_landing.rs`, whose module doc gives the mechanics at length). The carriers
//! span TWO packs, each pack root is its own `[[test]]` target — a separate crate root — so a constant
//! declared in either pack cannot be reached from the other, and duplicating it is the one thing §37
//! forbids. It is reached through the accessor below because the coverage guard's registry scanner
//! reads only a BARE single-line `const NAME: &str = "...";`.
//!
//! WHAT THE READER'S CORRECT EDIT COSTS (`1.architecture/rules/rule-quality.md` §27 leg 3). All four
//! rules of the check-then-act family end in the same move: make the read's columns unique. That move
//! is a MIGRATION, and it is refused outright by any table that already holds a duplicate of the pair —
//! then, forced through, it rejects the second legitimate row at runtime and the feature that creates
//! it stops working. The finding itself stays true where that happens (the race is real whether or not
//! the pair is unique), which is why this is a landing and not a disqualifier: what changes is that the
//! remedy has a precondition the rule cannot check, not that the report was wrong.
//!
//! WHAT THIS FILE REGISTERS, AND WHAT IT DELIBERATELY DOES NOT. Four rules say this one thing and they
//! say it in THREE spellings, measured byte-for-byte on 2026-09-01:
//!
//! | rule | spelling |
//! |---|---|
//! | `db/check-then-act-in-loop` | the 405-byte sentence below |
//! | `sql/race-condition-toctou` | byte-identical to it |
//! | `sql/raw-sql-check-then-write` | the same sentence with `takes no UNIQUE constraint` for `takes no SUCH constraint`, and a trailing clause |
//! | `db/find-then-create-no-unique` | an independent 765-byte long form (`any table` / `of the pair` / `at runtime`, plus a "query the live table first" instruction) |
//!
//! Registration cannot fold the last two, because folding them would REWRITE two shipped messages and
//! this change rewrites none — the two byte-identical carriers take the constant, the other two keep
//! their own bytes, and the divergence is asserted below rather than left to be discovered. Whether the
//! four should collapse to one spelling is a message decision and belongs to whoever makes it; what is
//! closed here is the silent case, where one of the four is edited and the other three are not.
//!
//! POSITION, not presence. A reader who acts on the first instruction never reaches a caveat placed
//! behind it, so `contains` proves nothing. The invalidation probe for both pins is to move this
//! constant to the tail of that rule's message: every token stays present and spelled exactly once,
//! and the pin must go red on ORDER alone.

/// The unique-pair precondition, spliced ahead of the "add a unique constraint" imperative in the two
/// rules that carry it byte-identically. Not a `convention` value — no project declares it; it is
/// zzop's own sentence about what the reader's own migration does to a table that already holds the
/// duplicate the constraint would forbid.
const UNIQUE_PAIR_PRECONDITION_LANDING: &str = "FIRST RULE OUT A COLUMN PAIR THAT IS NOT MEANT TO BE UNIQUE — a pair one owner may legitimately hold twice (two linked accounts at the same external provider, two installs of one integration) takes no such constraint: the migration FAILS OUTRIGHT against a table that already holds a duplicate of it, and forced through, the second legitimate row is rejected and the feature that creates it stops working.";

/// The one spelling, handed to the two pack roots that carry it.
///
/// An accessor rather than a `pub(crate) const` on purpose: the axis-B registry in
/// `message_order_verdicts.rs` scans source for a bare `const NAME: &str = "...";` and skips anything
/// with a visibility prefix, so making the constant itself reachable would take it out of the registry
/// and make both carriers read as carrying no landing at all.
pub(crate) fn unique_pair_precondition_landing() -> &'static str {
    UNIQUE_PAIR_PRECONDITION_LANDING
}

/// The family census: which rules carry this spelling, and which carry one of the two that drifted
/// from it.
///
/// The two position pins (one per pack) each prove the order inside ONE message. Neither can see that a
/// sibling says the same thing in different bytes, and that is exactly how this family got to three
/// spellings: every existing pin on all four rules passes a needle set — `NOT MEANT TO BE UNIQUE`,
/// `legitimately hold twice`, `migration FAILS OUTRIGHT` — that all three spellings satisfy, so the
/// divergence was invisible to every check in the tree. This test reads the shipped JSON and names it.
///
/// The two drifted rows are asserted to STILL be drifted, the same ratchet `OPEN_ORDER_VIOLATIONS`
/// uses: collapsing them onto the constant is a message change, so it must turn this red and be
/// declared, rather than passing quietly as an incidental edit.
#[test]
fn the_unique_pair_precondition_is_one_spelling_in_two_rules_and_drifted_in_two_more() {
    let landing = unique_pair_precondition_landing();
    let db: serde_json::Value =
        serde_json::from_str(include_str!("db/db.json")).expect("db.json parses");
    let sql: serde_json::Value =
        serde_json::from_str(include_str!("sql/sql.json")).expect("sql.json parses");

    let message = |pack: &serde_json::Value, id: &str| -> String {
        pack["rules"]
            .as_array()
            .expect("rules array")
            .iter()
            .find(|r| r["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("rule {id} present"))["message"]
            .as_str()
            .expect("message")
            .to_string()
    };

    let mut carriers: Vec<String> = Vec::new();
    for (pack_id, pack) in [("db", &db), ("sql", &sql)] {
        for rule in pack["rules"].as_array().expect("rules array") {
            let id = rule["id"].as_str().expect("rule id");
            if rule["message"]
                .as_str()
                .is_some_and(|m| m.contains(landing))
            {
                carriers.push(format!("{pack_id}/{id}"));
            }
        }
    }
    carriers.sort();
    assert_eq!(
        carriers,
        ["db/check-then-act-in-loop", "sql/race-condition-toctou"],
        "the unique-pair landing is carried by a different rule set than the two it was measured for. \
         A rule that acquires this remedy and pastes the sentence rather than taking the constant is \
         a fourth spelling; a carrier that drops it has lost the precondition its migration needs."
    );

    // The two that say the same thing in different bytes. Each fragment is what makes that spelling
    // DIFFERENT from the constant, so a silent re-spelling of either turns this red.
    for (id, message, fragment) in [
        (
            "sql/raw-sql-check-then-write",
            message(&sql, "raw-sql-check-then-write"),
            "takes no unique constraint: the migration FAILS OUTRIGHT",
        ),
        (
            "db/find-then-create-no-unique",
            message(&db, "find-then-create-no-unique"),
            "against any table that already holds a duplicate of the pair",
        ),
    ] {
        assert_eq!(
            message.matches(fragment).count(),
            1,
            "{id}: this rule's own spelling of the unique-pair precondition changed. It says the same \
             thing as the constant above in different bytes, deliberately left that way because \
             folding it would rewrite a shipped message — so an edit here is either that fold (take \
             the constant and drop this row) or a fourth spelling (do not). Looked for {fragment:?}."
        );
        assert!(
            !message.contains(landing),
            "{id} now carries the shared spelling byte-identically, so the fold happened. Move it \
             into the carrier list above and give it a landing pin."
        );
    }
}
