use super::*;

fn consumes(text: &str) -> Vec<IoConsume> {
    extract_rust_raw_sql_db_table_consumes("src/db.rs", text)
}

fn keys(text: &str) -> Vec<String> {
    consumes(text)
        .into_iter()
        .filter_map(|c| c.key)
        .collect::<Vec<_>>()
}

// --- POSITIVE: the shapes a Rust database stack actually writes --------------------------------------

#[test]
fn sqlx_query_macro_literal_is_read_out_of_the_opaque_token_stream() {
    // The whole reason `visit_macro` walks tokens: syn hands this body back as an opaque TokenStream,
    // so the default walk sees no literal at all.
    let out = consumes(
        "async fn load(p: &sqlx::PgPool) {\n    \
         sqlx::query!(\"SELECT id, email FROM users WHERE id = $1\", 1i64);\n}\n",
    );
    assert_eq!(out.len(), 1, "got: {out:?}");
    assert_eq!(out[0].key.as_deref(), Some("table:users"));
    assert_eq!(out[0].file, "src/db.rs");
    assert_eq!(out[0].line, 2);
    assert_eq!(out[0].kind, "db-table");
    // Keyed at extraction time — no engine-side entity resolution needed, unlike the ORM adapters.
    assert_eq!(out[0].raw, None);
}

#[test]
fn sqlx_query_as_macro_reads_the_sql_past_the_leading_type_argument() {
    let out = consumes(
        "async fn load(p: &sqlx::PgPool) {\n    \
         sqlx::query_as!(User, \"SELECT id FROM accounts\");\n}\n",
    );
    assert_eq!(out.len(), 1, "got: {out:?}");
    assert_eq!(out[0].key.as_deref(), Some("table:accounts"));
}

#[test]
fn a_plain_call_argument_is_read_by_the_ordinary_literal_walk() {
    // `sqlx::query(...)`, `tokio_postgres`'s `client.query(...)`, `diesel::sql_query(...)` — all the
    // non-macro forms land here.
    assert_eq!(
        keys("async fn f(c: &Client) {\n    c.query(\"SELECT * FROM orders\", &[]).await;\n}\n"),
        vec!["table:orders"]
    );
}

#[test]
fn rusqlite_execute_insert_names_its_table() {
    assert_eq!(
        keys("fn f(conn: &Connection) {\n    conn.execute(\"INSERT INTO sessions (id) VALUES (?1)\", []);\n}\n"),
        vec!["table:sessions"]
    );
}

#[test]
fn a_multi_line_raw_string_reaches_its_from() {
    // The normal shape for a hand-written query in Rust; the shared statement gate allows the leading
    // newline/indentation.
    let out = consumes(
        "fn q() -> &'static str {\n    r#\"\n        SELECT a.id\n        FROM articles a\n        JOIN comments c ON c.article_id = a.id\n    \"#\n}\n",
    );
    let mut got: Vec<&str> = out.iter().filter_map(|c| c.key.as_deref()).collect();
    got.sort_unstable();
    assert_eq!(
        got,
        vec!["table:articles", "table:comments"],
        "got: {out:?}"
    );
}

#[test]
fn a_hoisted_const_query_is_read_where_it_is_declared() {
    // Position-agnostic on purpose: `const SQL: &str = "..."; conn.execute(SQL, [])` is common, and
    // gating on "argument of a call" would miss it while adding no precision the shape gate lacks.
    let out = consumes("const SQL: &str = \"DELETE FROM tokens WHERE expired\";\n");
    assert_eq!(out.len(), 1, "got: {out:?}");
    assert_eq!(out[0].key.as_deref(), Some("table:tokens"));
    assert_eq!(out[0].line, 1);
}

#[test]
fn a_format_hole_outside_the_table_position_keeps_the_literal_table() {
    assert_eq!(
        keys(
            "fn f(id: i64) -> String {\n    format!(\"SELECT id FROM users WHERE id = {id}\")\n}\n"
        ),
        vec!["table:users"]
    );
}

#[test]
fn a_brace_that_is_not_a_placeholder_never_swallows_the_from_behind_it() {
    // An embedded JSON literal: the `{` body carries a quote, so it is left alone rather than masked —
    // otherwise the mask would run to the next `}` and delete the FROM clause with it.
    assert_eq!(
        keys("fn f() -> &'static str {\n    \"SELECT '{\\\"a\\\":1}' AS j FROM cfg\"\n}\n"),
        vec!["table:cfg"]
    );
}

// --- NEGATIVE: what must NOT produce a fact -----------------------------------------------------------

#[test]
fn a_rust_file_with_no_sql_at_all_yields_nothing() {
    // The invalidation baseline: without this, every assertion above is indistinguishable from
    // "always fires".
    assert!(consumes(
        "use std::collections::HashMap;\n\
         pub struct Config {\n    pub name: String,\n}\n\
         pub fn build(n: &str) -> Config {\n    Config { name: n.to_string() }\n}\n"
    )
    .is_empty());
}

#[test]
fn english_prose_that_reads_like_sql_yields_nothing() {
    // `Select a date from the list` is structurally identical to `SELECT <col> FROM <table> <alias>`;
    // letter case is the only discriminator, and it lives in the shared statement gate.
    assert!(consumes("const HELP: &str = \"Select a date from the list\";\n").is_empty());
    assert!(consumes("const HELP: &str = \"select id from users\";\n").is_empty());
}

#[test]
fn a_fully_interpolated_table_name_is_dropped_rather_than_fabricated() {
    assert!(
        consumes("fn f(t: &str) -> String {\n    format!(\"SELECT * FROM {t}\")\n}\n").is_empty()
    );
    assert!(
        consumes("fn f(t: &str) -> String {\n    format!(\"SELECT * FROM {}\", t)\n}\n").is_empty()
    );
}

#[test]
fn a_table_name_built_from_a_prefix_plus_a_hole_is_dropped() {
    // The case the sentinel exists for: without an identifier-SHAPED stand-in, `users_{}` would key
    // `table:users_` — a table that does not exist.
    assert!(
        consumes("fn f(s: &str) -> String {\n    format!(\"SELECT * FROM users_{s}\")\n}\n")
            .is_empty()
    );
    assert!(consumes(
        "fn f(a: &str, b: &str) -> String {\n    format!(\"SELECT * FROM {a}_{b}\")\n}\n"
    )
    .is_empty());
}

#[test]
fn a_doc_comment_describing_a_query_is_not_a_query() {
    // `///` is an `#[doc = "..."]` attribute in the AST — prose about SQL would otherwise mint a real
    // consume from a comment.
    assert!(
        consumes("/// Runs SELECT id FROM users and returns the ids.\npub fn load() {}\n")
            .is_empty()
    );
    assert!(consumes("//! Module notes: INSERT INTO audit_log on every write.\n").is_empty());
}

#[test]
fn an_inline_cfg_test_module_is_not_deployed_db_coupling() {
    let out = consumes(
        "pub fn ship() {}\n\
         #[cfg(test)]\nmod tests {\n    \
         const FIXTURE: &str = \"SELECT * FROM users\";\n}\n",
    );
    assert!(out.is_empty(), "got: {out:?}");
}

#[test]
fn a_test_annotated_function_is_skipped_whatever_the_runner() {
    for attr in ["#[test]", "#[tokio::test]", "#[sqlx::test]"] {
        let src = format!("{attr}\nfn t() {{\n    let _ = \"SELECT * FROM users\";\n}}\n");
        assert!(consumes(&src).is_empty(), "{attr} was not skipped");
    }
}

#[test]
fn cfg_not_test_code_still_ships_and_still_counts() {
    // The other side of the `#[cfg(test)]` gate: `cfg(not(test))` is compiled INTO the shipping build,
    // so skipping it would delete a real fact.
    assert_eq!(
        keys("#[cfg(not(test))]\nfn ship() {\n    let _ = \"SELECT * FROM users\";\n}\n"),
        vec!["table:users"]
    );
}

// --- the gate's node axis matches `lang::test_spans`, node for node -----------------------------------
// Every fixture below carried SQL that this adapter extracted as DEPLOYED db coupling while
// `extract_test_spans` was simultaneously reporting the very same lines as test-only: the two modules
// shared the predicate but not the visitor surface, so they answered one file two ways. The parity test
// at the end is the one that would catch the next such divergence; the named cases above it say which
// shape each one is, so a failure reads as a diagnosis rather than as a table row.

/// `(name, source)` — each holds SQL that is unreachable in a release build.
const TEST_GATED_SQL_FIXTURES: &[(&str, &str)] = &[
    (
        "file-level inner #![cfg(test)]",
        "#![cfg(test)]\n\nfn seed() {\n    sqlx::query!(\"INSERT INTO users (id) VALUES (1)\");\n}\n",
    ),
    (
        "#[cfg(test)] fn inside an impl block",
        "struct Repo;\nimpl Repo {\n    #[cfg(test)]\n    fn seed(&self, c: &C) {\n        c.execute(\"INSERT INTO users (id) VALUES (1)\", []);\n    }\n}\n",
    ),
    (
        "#[cfg(test)] const — an Item variant the old three-visitor gate never saw",
        "#[cfg(test)]\nconst FIXTURE_SQL: &str = \"SELECT id FROM orders\";\n",
    ),
    (
        "#[cfg(test)] default method body in a trait",
        "trait T {\n    #[cfg(test)]\n    fn seed(&self, c: &C) {\n        c.execute(\"SELECT id FROM orders\", []);\n    }\n}\n",
    ),
    (
        "#[cfg(all(test, not(miri)))] mod — the predicate half of the same defect",
        "#[cfg(all(test, not(miri)))]\nmod tests {\n    fn t(c: &C) {\n        c.execute(\"SELECT id FROM users\", []);\n    }\n}\n",
    ),
    (
        "#[cfg(test)] mod tests — the control that always worked",
        "#[cfg(test)]\nmod tests {\n    fn t(c: &C) {\n        c.execute(\"SELECT id FROM users\", []);\n    }\n}\n",
    ),
];

#[test]
fn every_test_gated_shape_yields_no_deployed_db_coupling() {
    for (name, src) in TEST_GATED_SQL_FIXTURES {
        assert!(
            keys(src).is_empty(),
            "{name}: a fixture's SQL was extracted as deployed db coupling — got {:?}",
            keys(src)
        );
    }
}

#[test]
fn the_same_sql_outside_a_test_gate_still_yields_its_table() {
    // The BIDIRECTIONAL half. Without it, the assertions above are equally satisfied by an extractor
    // that stopped working: each source here is the gated fixture with only its gate removed.
    let ungated = [
        "fn seed() {\n    sqlx::query!(\"INSERT INTO users (id) VALUES (1)\");\n}\n",
        "struct Repo;\nimpl Repo {\n    fn seed(&self, c: &C) {\n        c.execute(\"INSERT INTO users (id) VALUES (1)\", []);\n    }\n}\n",
        "const FIXTURE_SQL: &str = \"SELECT id FROM orders\";\n",
        "trait T {\n    fn seed(&self, c: &C) {\n        c.execute(\"SELECT id FROM orders\", []);\n    }\n}\n",
        "#[cfg(any(test, feature = \"testkit\"))]\nmod helpers {\n    fn t(c: &C) {\n        c.execute(\"SELECT id FROM users\", []);\n    }\n}\n",
        "mod tests {\n    fn t(c: &C) {\n        c.execute(\"SELECT id FROM users\", []);\n    }\n}\n",
    ];
    for src in ungated {
        assert_eq!(
            keys(src).len(),
            1,
            "shipped SQL must still mint its consume — got {:?} for {src:?}",
            keys(src)
        );
    }
}

#[test]
fn suppression_here_and_a_test_span_there_are_the_same_answer() {
    // The seam pin. This adapter SKIPS and `lang::test_spans` RECORDS — deliberately different uses of
    // one predicate — but they must agree on WHICH lines are test-only. When they did not, a rule pack
    // subtracting spans could not undo a fact this adapter had already minted.
    for (name, src) in TEST_GATED_SQL_FIXTURES {
        let spans = crate::extract_test_spans("src/db.rs", src);
        assert!(
            !spans.is_empty(),
            "{name}: no test span, so a rule pack could not subtract this region either"
        );
        assert!(
            keys(src).is_empty(),
            "{name}: test_spans calls these lines test-only but this adapter minted a fact from them"
        );
    }
}

#[test]
fn a_test_path_file_yields_nothing_before_parsing() {
    assert!(extract_rust_raw_sql_db_table_consumes(
        "tests/db_it.rs",
        "fn t() {\n    let _ = \"SELECT * FROM users\";\n}\n"
    )
    .is_empty());
}

#[test]
fn query_file_names_a_path_not_a_table() {
    // The argument is a `.sql` FILE path; it fails the statement gate, and this adapter does not go
    // read that file (stated in the module doc as out of scope).
    assert!(consumes(
        "async fn f(p: &sqlx::PgPool) {\n    sqlx::query_file!(\"queries/users.sql\");\n}\n"
    )
    .is_empty());
}

#[test]
fn create_table_is_the_provide_sides_business_not_a_consume() {
    assert!(consumes("const DDL: &str = \"CREATE TABLE users (id BIGINT)\";\n").is_empty());
}

#[test]
fn an_unparseable_file_degrades_to_empty_rather_than_guessing() {
    assert!(consumes("fn f(: \"SELECT * FROM users\"\n").is_empty());
}

// --- the masking helper, pinned directly --------------------------------------------------------------

#[test]
fn mask_format_holes_replaces_only_real_placeholders() {
    assert_eq!(mask_format_holes("a {} b"), format!("a {PLACEHOLDER} b"));
    assert_eq!(mask_format_holes("{name}"), PLACEHOLDER);
    assert_eq!(mask_format_holes("{0:>8.3}"), PLACEHOLDER);
    // Escapes and non-placeholders survive untouched.
    assert_eq!(mask_format_holes("{{literal}}"), "{{literal}}");
    assert_eq!(mask_format_holes("{ spaced }"), "{ spaced }");
    assert_eq!(mask_format_holes("{unterminated"), "{unterminated");
    assert_eq!(mask_format_holes("{\"a\":1}"), "{\"a\":1}");
    // A body longer than the cap is prose, not a spec.
    let long = "x".repeat(MAX_PLACEHOLDER_BODY + 1);
    assert_eq!(
        mask_format_holes(&format!("{{{long}}}")),
        format!("{{{long}}}")
    );
    // Multi-byte text on both sides of a hole is copied intact (no byte-index slicing bug).
    assert_eq!(
        mask_format_holes("café{}né"),
        format!("café{PLACEHOLDER}né")
    );
}
