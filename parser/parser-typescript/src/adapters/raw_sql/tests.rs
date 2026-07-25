use super::extract_raw_sql_db_table_consumes;
use zzop_core::IoConsume;

fn run(rel: &str, src: &str) -> Vec<IoConsume> {
    extract_raw_sql_db_table_consumes(rel, src)
}

fn keys(out: &[IoConsume]) -> Vec<String> {
    out.iter()
        .map(|c| c.key.clone().unwrap_or_default())
        .collect()
}

// --- corpus-derived --------------------------------------------------------------------------
// Statement text and call shape copied byte-for-byte from the Cloudflare-D1 dogfood corpus
// (`settle-hub-be/src/createLedger.ts`, `ping-hub-be/src/{createGroup,sweepExpired}.ts`,
// `settle-hub-be/src/getRates.ts`); surrounding scaffolding is minimal.

#[test]
fn d1_prepare_select_then_batch_insert_yields_both_tables() {
    let src = concat!(
        "export async function postRevision(env: Env, ledgerId: string) {\n",
        "  const latest = await env.DB.prepare(\n",
        "    \"SELECT revision_no FROM ledger_revision WHERE ledger_id = ? ORDER BY revision_no DESC LIMIT 1\",\n",
        "  )\n",
        "    .bind(ledgerId)\n",
        "    .first<{ revision_no: number }>();\n",
        "  await env.DB.batch([\n",
        "    env.DB.prepare(\n",
        "      \"INSERT INTO ledger_revision (id, ledger_id, revision_no, snapshot, created_at) VALUES (?, ?, ?, ?, ?)\",\n",
        "    ).bind(1),\n",
        "    env.DB.prepare(\"UPDATE ledger SET updated_at = ? WHERE id = ?\").bind(2),\n",
        "  ]);\n",
        "}\n",
    );
    let out = run("src/createLedger.ts", src);
    assert_eq!(
        keys(&out),
        vec![
            "table:ledger_revision",
            "table:ledger_revision",
            "table:ledger"
        ]
    );
    // The read anchors at the line the STATEMENT STRING starts on (3), not the `prepare(` line (2) —
    // the string is what carries the fact.
    assert_eq!(out[0].line, 3);
    assert_eq!(out[0].kind, "db-table");
    assert_eq!(out[0].file, "src/createLedger.ts");
    assert!(out[0].raw.is_none() && out[0].method.is_none());
}

#[test]
fn count_check_and_insert_in_one_file_both_project() {
    let src = concat!(
        "export async function joinGroup(env: Env, id: string, nickname: string) {\n",
        "  const memberCount = await env.DB.prepare(\"SELECT COUNT(*) as cnt FROM sessions WHERE group_id = ?\")\n",
        "    .bind(id)\n",
        "    .first<{ cnt: number }>();\n",
        "  await env.DB.prepare(\"INSERT INTO sessions (token_hash, group_id, nickname, created_at) VALUES (?, ?, ?, ?)\").run();\n",
        "}\n",
    );
    assert_eq!(
        keys(&run("src/createGroup.ts", src)),
        vec!["table:sessions", "table:sessions"]
    );
}

#[test]
fn a_statement_hoisted_into_a_file_local_const_still_projects() {
    // `getRates.ts` holds its query in a module constant and passes the IDENTIFIER to `prepare()`.
    // Gating on "argument of a call" would lose this; the shape gate does not.
    let src = concat!(
        "const SELECT_ROW = \"SELECT base, rates, fetched_at FROM fx_rates WHERE id = 1\";\n",
        "export async function getRates(env: Env) {\n",
        "  return env.DB.prepare(SELECT_ROW).first<FxRatesRow>();\n",
        "}\n",
    );
    let out = run("src/getRates.ts", src);
    assert_eq!(keys(&out), vec!["table:fx_rates"]);
    assert_eq!(out[0].line, 1);
}

#[test]
fn a_template_literal_with_an_interpolated_predicate_keeps_its_literal_table() {
    let src = concat!(
        "export function sweep(db: D1Database, expiredGroupIds: string, now: number) {\n",
        "  return db.batch([\n",
        "    db.prepare(`DELETE FROM sessions WHERE group_id IN (${expiredGroupIds})`).bind(now),\n",
        "    db.prepare(`DELETE FROM locations WHERE group_id IN (${expiredGroupIds})`).bind(now),\n",
        "  ]);\n",
        "}\n",
    );
    assert_eq!(
        keys(&run("src/sweepExpired.ts", src)),
        vec!["table:sessions", "table:locations"]
    );
}

// --- never-guess boundary (synthetic, NOT corpus-derived) ------------------------------------

#[test]
fn an_interpolated_table_name_is_dropped_not_guessed() {
    let src = "const q = (table: string) => db.prepare(`SELECT * FROM ${table} WHERE id = ?`);\n";
    assert!(run("src/q.ts", src).is_empty());
}

#[test]
fn a_table_name_partly_built_from_interpolation_is_dropped() {
    let src =
        "const q = (env: string) => db.prepare(`SELECT * FROM sessions_${env} WHERE id = ?`);\n";
    assert!(
        run("src/q.ts", src).is_empty(),
        "a prefix that only LOOKS literal must not mint table:sessions"
    );
}

#[test]
fn adjacent_interpolations_cannot_leave_a_bare_separator_behind() {
    let src = "const q = (a: string, b: string) => db.prepare(`SELECT * FROM ${a}_${b}`);\n";
    assert!(run("src/q.ts", src).is_empty());
}

// --- precision gates (synthetic, NOT corpus-derived) -----------------------------------------

#[test]
fn ui_prose_beginning_with_a_sql_keyword_is_not_a_table_access() {
    // Found by this very test during development: "Select a date from the list" IS a structurally
    // valid `SELECT <col> FROM <table> <alias>`, so only the uppercase-keyword gate stops it.
    let src = concat!(
        "export const labels = {\n",
        "  pick: \"Select a date from the list\",\n",
        "  wipe: \"Delete this item?\",\n",
        "  more: \"Update your profile from settings\",\n",
        "};\n",
    );
    assert!(run("src/labels.ts", src).is_empty());
}

#[test]
fn a_test_file_is_skipped() {
    let src = "it(\"reads\", () => db.prepare(\"SELECT id FROM users WHERE id = ?\"));\n";
    assert!(run("src/db.test.ts", src).is_empty());
}

#[test]
fn a_ddl_string_is_not_a_consume() {
    let src = "await db.exec(\"CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY)\");\n";
    assert!(run("src/migrate.ts", src).is_empty());
}

#[test]
fn a_tagged_template_query_projects_like_any_other_template() {
    let src = "const rows = await sql`SELECT id, email FROM accounts WHERE id = ${id}`;\n";
    assert_eq!(keys(&run("src/accounts.ts", src)), vec!["table:accounts"]);
}

#[test]
fn a_file_with_no_sql_yields_nothing() {
    assert!(run("src/util.ts", "export const a = 1;\n").is_empty());
}

#[test]
fn an_unparseable_file_yields_nothing_instead_of_panicking() {
    assert!(run("src/broken.ts", "function ( {{{ ").is_empty());
}
