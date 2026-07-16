use crate::{hits, scan, TempDir};

// --- query-logic-density ---

#[test]
fn counts_case_when_branches_in_multiline_sql_template_over_threshold() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "report.ts",
        "export const q = `\n  SELECT id,\n    CASE\n      WHEN tier = 'gold' THEN price * 0.8\n      WHEN tier = 'silver' THEN price * 0.9\n      ELSE price\n    END AS final\n  FROM orders WHERE active = true\n`;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "query-logic-density");
    assert_eq!(h.len(), 1, "expected 1 hit, got: {:?}", out.findings);
    assert_eq!(h[0].file, "report.ts");
}

#[test]
fn does_not_count_aggregation_only_sql() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "agg.ts",
        "export const q = `\n  SELECT customer_id, SUM(amount) AS total, COUNT(*) AS n\n  FROM orders GROUP BY customer_id\n`;\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "query-logic-density").is_empty());
}

#[test]
fn single_case_when_is_below_threshold() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "one.ts",
        "export const q = `SELECT CASE WHEN active THEN 1 ELSE 0 END AS flag FROM users`;\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "query-logic-density").is_empty());
}

#[test]
fn ignores_ordinary_js_switch_case() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "code.ts",
        "export function f(x: number) {\n  if (x > 0) return 1;\n  switch (x) { case 1: return 2; default: return 3; }\n  return 0;\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "query-logic-density").is_empty());
}

// --- TypeScript `switch/case` clauses must never satisfy query-logic-density: a bare `\bcase\b` line
// pattern would fire on `case 'sum':` labels whenever the file also contains incidental "when"/"from"/"set"
// words, so the SQL-shaped pattern requires `CASE WHEN ...` or a bare line-ending `CASE`. ---

#[test]
fn typescript_switch_cases_do_not_fire_query_logic_density() {
    let dir = TempDir::new("zzop-sql");
    // Gate bait on purpose: "when"/"from"/"values" appear as ordinary prose/identifiers.
    dir.write(
        "aggregate.ts",
        "// chooses the aggregator when values come from the grid; picks when needed\nexport function createAggregator(type: string) {\n  switch (type) {\n    case 'sum':\n      return 1;\n    case 'count':\n      return 2;\n    default:\n      return 0;\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "sql/query-logic-density"),
        "{:?}",
        out.findings
    );
}

/// The SQL shapes still fire: single-line `CASE WHEN` and the multiline bare `CASE` both anchor.
#[test]
fn single_line_case_when_still_fires_query_logic_density() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "pricing.ts",
        "export const q = `SELECT id, CASE WHEN tier = 'gold' THEN 1 WHEN tier = 'silver' THEN 2 ELSE 0 END FROM orders`;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "query-logic-density");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}
