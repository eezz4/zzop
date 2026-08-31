//! §33/§37 LANDING for the three cross-language "bind it instead" rules — `sql-format-interpolation`
//! (Rust), `sql-interpolated-statement` (Python/Go) and `sql-string-concat` (Java).
//!
//! WHAT THE READER'S CORRECT EDIT COSTS (`1.architecture/rules/rule-quality.md` §27 leg 3). All three
//! prescribe the same move — take the spliced expression out of the statement text and hand it to the
//! driver as a bound argument — and all three have a population for which that move DOES NOT EXIST. A
//! placeholder occupies a VALUE position; a table name, a per-tenant schema and an `ORDER BY` column
//! are identifiers, and no driver parameterizes an identifier. The two failures are not equally kind,
//! which is the half a reader cannot guess: the table-name case errors loudly, and the `ORDER BY` case
//! is ACCEPTED as a constant and returns unordered rows with nothing raised.
//!
//! ONE CONSTANT FOR THE THREE, byte-identical. The property belongs to the wire protocol, not to what
//! any one of these rules detects: `format!`, an f-string, `fmt.Sprintf` and Java `+` all reach the
//! same placeholder, so the cost is the same sentence in all three languages. The rules diverge only
//! in their EXIT, which is each rule's own imperative and stays in each rule's own message. Same split
//! `ROTATION_LANDING` uses for its eight, and the same reason: a position pin needs ONE spelling.
//!
//! WHY THIS IS NOT `IDENTIFIER_BINDING_LANDING`, the closest sibling and therefore the most dangerous
//! reuse (§37). That constant opens "THE TAGGED TEMPLATE PARAMETERIZES EVERY `${...}`, WHICH IS THE
//! SAME REASON THE UNSAFE CALL EXISTS" — a sentence about Prisma's `$queryRaw`, true of exactly one
//! rule and meaningless on a Java `PreparedStatement` or a Go `db.Query`. Its generic half could have
//! been lifted out and shared, but splitting a `critical` rule's shipped message to make a constant
//! reusable is a message change to that rule, and its own audit is recent; the two families stay
//! separate and this note is what keeps the next author from "merging" them by pasting either one
//! over the other.
//!
//! NOT A DISQUALIFIER, which is why these are pinned with the landing helper rather than the clause
//! one. A reader whose interpolation is a table name still has a real finding — that string still
//! reaches the planner as statement text. What changes is that the prescribed rewrite is unavailable
//! to them, and the message now says so before it hands them the rewrite.
//!
//! POSITION, not presence. The invalidation probe for every test below is to move this constant to the
//! tail of the message: every token stays present and spelled exactly once, and each pin must go red
//! on ORDER alone.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

const BOUND_PARAMETER_LANDING: &str = "A PLACEHOLDER CAN STAND ONLY WHERE THE DATABASE EXPECTS A VALUE, SO NOT EVERY INTERPOLATION HAS A BOUND FORM TO MOVE TO: a table name, a schema chosen per tenant, and the column or direction in an `ORDER BY` are IDENTIFIERS, and no driver parameterizes an identifier — an `IN (...)` list needs one placeholder per element rather than one for the list. The two failures are not equally kind. A table name put in a placeholder reaches the server as `SELECT * FROM ?` and comes back a syntax error you cannot miss; an `ORDER BY` column there is ACCEPTED as a constant, so the rows arrive in whatever order the plan happened to produce and nothing is raised. Sort the interpolations before you rewrite: values become placeholders plus bound arguments, and each identifier needs a fixed map from the request string to a literal you wrote — never the request string itself, escaped or quoted.";

/// Rust. The fixture is the dynamic-column-list shape, which is the population the landing is about:
/// this rule's own message already named it "the classic non-parameterizable vector" and then stopped,
/// so the shape was disclosed and the COST of following the remedy on it was not.
#[test]
fn sql_format_interpolation_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/queries.rs",
        "pub fn projection(cols: &str) -> String {\n    format!(\"SELECT {} FROM users\", cols)\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-format-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "sql-format-interpolation",
        &h[0].message,
        BOUND_PARAMETER_LANDING,
        "Bind the value as a query PARAMETER instead",
    );
}

/// Python. This rule's imperative sits in its SECOND sentence, so the landing had to go in front of it
/// rather than at the end of the disclosure block — the position, not the presence, is the claim.
#[test]
fn sql_interpolated_statement_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/repo.py",
        "def find(cur, uid):\n    cur.execute(f\"SELECT * FROM users WHERE id = {uid}\")\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-interpolated-statement");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "sql-interpolated-statement",
        &h[0].message,
        BOUND_PARAMETER_LANDING,
        "Bind the value as a query PARAMETER and keep",
    );
}

/// Java. The shortest of the three before this edit (567 characters), and the one whose remedy names
/// two concrete APIs — `PreparedStatement` and JPA `setParameter` — neither of which has an identifier
/// form either.
#[test]
fn sql_string_concat_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "C.java",
        "public class C {\n  void run(String login) {\n    Query q = em.createQuery(\"SELECT u FROM User u WHERE u.login = '\" + login + \"'\");\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-string-concat");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "sql-string-concat",
        &h[0].message,
        BOUND_PARAMETER_LANDING,
        "Use a parameterized/bound query instead",
    );
}

/// The facts the landing exists to carry, asserted once for the family rather than three times: the
/// silent arm is the load-bearing half. Told only that the rewrite "may not work", a reader assumes
/// they would see an error — and for an `ORDER BY` column they would not.
#[test]
fn the_bound_parameter_landing_keeps_both_failure_modes() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/repo.py",
        "def find(cur, uid):\n    cur.execute(f\"SELECT * FROM users WHERE id = {uid}\")\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-interpolated-statement");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    for needle in [
        "`SELECT * FROM ?`",
        "is ACCEPTED as a constant",
        "in whatever order the plan happened to produce",
        "a fixed map from the request string to a literal you wrote",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/sql-interpolated-statement: the landing lost {needle:?}: {}",
            h[0].message
        );
    }
}
