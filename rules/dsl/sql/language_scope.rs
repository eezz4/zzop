//! The LANGUAGE-SCOPE half of the five quote-anchored `sql` rules — `delete-no-where`,
//! `update-no-where`, `truncate-in-app-code`, `select-star`, `like-leading-wildcard`.
//!
//! None of the five carries one character of host-language syntax: each is a quote character, a SQL
//! keyword, a table name and a closing quote. All five were nonetheless gated to `.ts/.tsx/.js/.mjs/.cjs`
//! and (since 2026-08-02) `.rs`, so `conn.execute("DELETE FROM users")` was CRITICAL in TypeScript and
//! SILENT in Python, Go, C# and Java — measured byte-for-byte before the widening. The sibling modules in
//! this directory own each rule's matcher semantics; this one owns only the question "does the file's
//! LANGUAGE change the verdict", in both directions.
//!
//! The corpus half of the same claim is `cases/trees/sql-langs/` (four production/test pairs plus an
//! Alembic migration twin). These unit pins exist because that gate needs a release build and a 25-tree
//! run, and because a `file_pattern` narrowed back by one extension should fail in seconds, by name.

use crate::{hits, scan, TempDir};

/// One production file per newly-admitted language, each holding the SAME five statements. The tuple is
/// (path, source), and every source is a whole file so the parser dispatch is realistic rather than a
/// bare fragment.
const PRODUCTION: [(&str, &str); 4] = [
    (
        "services/queries.py",
        "def q():\n    a = \"DELETE FROM users\"\n    b = \"UPDATE users SET active = 0\"\n    c = \"TRUNCATE TABLE sessions\"\n    d = \"SELECT * FROM users\"\n    e = \"SELECT id FROM users WHERE name LIKE '%term'\"\n    return (a, b, c, d, e)\n",
    ),
    (
        "services/queries.go",
        "package services\n\nconst A = \"DELETE FROM users\"\nconst B = \"UPDATE users SET active = 0\"\nconst C = \"TRUNCATE TABLE sessions\"\nconst D = \"SELECT * FROM users\"\nconst E = \"SELECT id FROM users WHERE name LIKE '%term'\"\n",
    ),
    (
        "Api/Queries.cs",
        "public class Queries\n{\n    public const string A = \"DELETE FROM users\";\n    public const string B = \"UPDATE users SET active = 0\";\n    public const string C = \"TRUNCATE TABLE sessions\";\n    public const string D = \"SELECT * FROM users\";\n    public const string E = \"SELECT id FROM users WHERE name LIKE '%term'\";\n}\n",
    ),
    (
        "src/main/java/com/example/Queries.java",
        "public class Queries {\n    static final String A = \"DELETE FROM users\";\n    static final String B = \"UPDATE users SET active = 0\";\n    static final String C = \"TRUNCATE TABLE sessions\";\n    static final String D = \"SELECT * FROM users\";\n    static final String E = \"SELECT id FROM users WHERE name LIKE '%term'\";\n}\n",
    ),
];

/// The test-path twin of each entry in [`PRODUCTION`], at that language's own convention. Same bytes for
/// the statements; only the PATH differs, which is what makes the silence below attributable.
const TEST_TWINS: [(&str, &str); 4] = [
    ("services/test_queries.py", PRODUCTION[0].1),
    ("services/queries_test.go", PRODUCTION[1].1),
    ("Api.Tests/QueriesTests.cs", PRODUCTION[2].1),
    (
        "src/test/java/com/example/QueriesTest.java",
        PRODUCTION[3].1,
    ),
];

const FIVE: [&str; 5] = [
    "delete-no-where",
    "update-no-where",
    "truncate-in-app-code",
    "select-star",
    "like-leading-wildcard",
];

#[test]
fn every_newly_admitted_language_fires_all_five_quote_anchored_rules() {
    for (path, src) in PRODUCTION {
        let dir = TempDir::new("zzop-sql");
        dir.write(path, src);
        let out = scan(&dir);
        for rule in FIVE {
            assert_eq!(
                hits(&out, rule).len(),
                1,
                "{rule} did not fire exactly once on {path}: {:?}",
                out.findings
            );
        }
    }
}

#[test]
fn every_newly_admitted_language_stays_silent_on_its_own_test_path() {
    // The prerequisite, asserted rather than trusted: the shared test-path vocabulary learned `_test.go`,
    // `test_*.py`, `*Tests.cs`/`*.Tests/` and the Java spellings on 2026-08-10. Without it, admitting
    // these extensions to three CRITICAL rules would pour them into every test module holding SQL text.
    for (path, src) in TEST_TWINS {
        let dir = TempDir::new("zzop-sql");
        dir.write(path, src);
        let out = scan(&dir);
        for rule in FIVE {
            assert!(
                hits(&out, rule).is_empty(),
                "{rule} fired on the test path {path}: {:?}",
                out.findings
            );
        }
    }
}

// --- quote-form evidence: which spellings the line-scan can and cannot reach -------------------------

#[test]
fn a_csharp_verbatim_literal_fires_because_the_quote_is_still_adjacent_to_the_keyword() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "Api/Q.cs",
        "public class Q\n{\n    public const string A = @\"DELETE FROM users\";\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "delete-no-where").len(), 1, "{:?}", out.findings);
}

#[test]
fn a_go_raw_backtick_literal_fires_because_backtick_is_already_a_quote_kind() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "services/q.go",
        "package services\n\nconst A = `DELETE FROM users`\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "delete-no-where").len(), 1, "{:?}", out.findings);
}

#[test]
fn a_single_line_python_triple_quoted_literal_fires() {
    let dir = TempDir::new("zzop-sql");
    dir.write("q.py", "A = \"\"\"DELETE FROM users\"\"\"\n");
    let out = scan(&dir);
    assert_eq!(hits(&out, "delete-no-where").len(), 1, "{:?}", out.findings);
}

#[test]
fn every_multi_line_statement_form_is_the_disclosed_residual_and_stays_silent() {
    // Pinned, not merely documented: each rule's message tells the reader a multi-line statement is
    // invisible, and a message is a claim this repo checks. The statement line carries no quote at all in
    // any of these four, so a line-scan has nothing to anchor on.
    let cases: [(&str, &str); 4] = [
        (
            "src/main/java/com/example/Q.java",
            "public class Q {\n    static final String A = \"\"\"\n        DELETE FROM users\n        \"\"\";\n}\n",
        ),
        ("q.py", "A = \"\"\"\n    DELETE FROM users\n\"\"\"\n"),
        (
            "services/q.go",
            "package services\n\nconst A = `\n\tDELETE FROM users\n`\n",
        ),
        (
            "Api/Q.cs",
            "public class Q\n{\n    public const string A = @\"\n        DELETE FROM users\n    \";\n}\n",
        ),
    ];
    for (path, src) in cases {
        let dir = TempDir::new("zzop-sql");
        dir.write(path, src);
        let out = scan(&dir);
        assert!(
            hits(&out, "delete-no-where").is_empty(),
            "the multi-line form in {path} is the disclosed residual and must stay silent: {:?}",
            out.findings
        );
    }
}

// --- the printf exclusion on like-leading-wildcard, both directions ----------------------------------

#[test]
fn a_printf_conversion_after_like_is_a_placeholder_not_a_wildcard_and_is_excluded() {
    // `%s` here is substituted at runtime, so the pattern this rule would be describing is text it never
    // sees. Go's Sprintf, Java's String.format and Python's `%`-formatting all write it this way; only the
    // `.ts`/`.rs` lanes were free of the shape, which is why the widening is what surfaced it.
    let cases: [(&str, &str); 3] = [
        (
            "services/q.go",
            "package services\n\nimport \"fmt\"\n\nfunc Q(t string) string {\n\treturn fmt.Sprintf(\"SELECT id FROM users WHERE name LIKE '%s'\", t)\n}\n",
        ),
        (
            "src/main/java/com/example/Q.java",
            "public class Q {\n    static String q(String t) {\n        return String.format(\"SELECT id FROM users WHERE name LIKE '%s'\", t);\n    }\n}\n",
        ),
        (
            "q.py",
            "def q(t):\n    return \"SELECT id FROM users WHERE name LIKE '%(term)s'\" % {\"term\": t}\n",
        ),
    ];
    for (path, src) in cases {
        let dir = TempDir::new("zzop-sql");
        dir.write(path, src);
        let out = scan(&dir);
        assert!(
            hits(&out, "like-leading-wildcard").is_empty(),
            "the printf placeholder in {path} must not be read as a leading wildcard: {:?}",
            out.findings
        );
    }
}

#[test]
fn the_printf_exclusion_does_not_become_a_blanket_veto_on_percent() {
    // The other direction, which is what keeps the exclusion from silently eating the rule in the printf
    // languages: an ESCAPED `%%` really is a wildcard, and a wildcard followed by more pattern text
    // (`'%sale%'`) only starts with a conversion letter by coincidence. The exclusion requires the quote
    // to close IMMEDIATELY after the letter, which is exactly what separates these from the case above.
    let cases: [(&str, &str); 2] = [
        (
            "services/q.go",
            "package services\n\nimport \"fmt\"\n\nfunc Q(t string) string {\n\treturn fmt.Sprintf(\"SELECT id FROM users WHERE name LIKE '%%%s%%'\", t)\n}\n",
        ),
        (
            "services/q2.go",
            "package services\n\nconst Q2 = \"SELECT id FROM users WHERE name LIKE '%sale%'\"\n",
        ),
    ];
    for (path, src) in cases {
        let dir = TempDir::new("zzop-sql");
        dir.write(path, src);
        let out = scan(&dir);
        assert_eq!(
            hits(&out, "like-leading-wildcard").len(),
            1,
            "the genuine leading wildcard in {path} must still fire: {:?}",
            out.findings
        );
    }
}

#[test]
fn a_bound_percent_s_between_set_and_the_closing_quote_is_a_value_not_an_open_statement() {
    // Deliberately NOT treated the way `{}`/`${}` are. A psycopg `%s` is the bound-parameter spelling —
    // the same role `?` plays in the TypeScript lane, where this rule has always fired — so the statement
    // IS complete and it does touch every row. The disclosed cost is in the rule's message: a `%s` that
    // splices a whole clause reads identically, and `{`/`}` can be excluded only because those spellings
    // cannot mean a bound parameter.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "services/q.py",
        "def q(cur):\n    return cur.execute(\"UPDATE users SET active = %s\", (0,))\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "update-no-where").len(), 1, "{:?}", out.findings);
}

// --- the Alembic arm: both halves of the migration handoff -------------------------------------------

#[test]
fn an_alembic_versions_backfill_is_destructive_migration_turf_not_critical() {
    // Real-corpus calibration (corpus/oss, 2026-08-10): the ONLY lines in 277 Python files matching any of
    // the five widened patterns were Alembic backfills under `alembic/versions/`, which is Python's
    // dominant migration layout and is NOT spelled `migrations/`. Without this arm the widening ships
    // three CRITICAL findings on deliberate one-time writes.
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "alembic/versions/0001_backfill.py",
        "def upgrade():\n    op.execute(\"UPDATE accounts SET migrated = 1\")\n    op.execute(\"DELETE FROM staging\")\n    op.execute(\"TRUNCATE TABLE scratch\")\n",
    );
    let out = scan(&dir);
    for rule in ["delete-no-where", "update-no-where", "truncate-in-app-code"] {
        assert!(
            hits(&out, rule).is_empty(),
            "{rule} must not fire under alembic/versions/: {:?}",
            out.findings
        );
    }
    // The half a silence-only fixture cannot assert: the disclosure the critical rules' messages PROMISE
    // is actually emitted. Drop `.py` from `destructive-migration` and the three lines above go from
    // "reported at info" to "reported nowhere", with every assertion above still green.
    let h = hits(&out, "destructive-migration");
    assert_eq!(h.len(), 3, "{:?}", out.findings);
    for f in h {
        assert_eq!(f.severity, zzop_core::Severity::Info);
    }
}

#[test]
fn destructive_migration_admits_every_extension_its_critical_siblings_exclude() {
    // The invariant behind the handoff, read off the pack itself rather than restated: the three critical
    // rules EXCLUDE migration paths and tell the reader `sql/destructive-migration` covers them, so an
    // extension one of them admits in app code and the other does not admit under `migrations/` is a
    // message promising a disclosure nobody emits. That asymmetry shipped for `.rs` from 2026-08-02 and
    // would have shipped again for `.py`/`.cs` today; this is the check that would have caught both.
    let pack = crate::sql_pack_uncached();
    let extensions = |id: &str| -> Vec<String> {
        let rule = pack
            .rules
            .iter()
            .find(|r| r.id == id)
            .unwrap_or_else(|| panic!("no rule {id} in the sql pack"));
        let zzop_core::dsl::Matcher::LineScan(line_scan) = &rule.matcher else {
            panic!("{id} is not a line-scan rule; this invariant is about their file_pattern")
        };
        let pattern = &line_scan.file_pattern;
        // Every `(a|b|c)$`-shaped extension group in the pattern, flattened. Both spellings in this pack
        // put the extension list in the LAST parenthesised group before the `$`.
        let tail = pattern
            .rsplit_once("\\.(")
            .unwrap_or_else(|| panic!("{id}'s file_pattern has no extension group: {pattern}"))
            .1;
        let group = tail
            .split_once(")$")
            .unwrap_or_else(|| panic!("{id}'s extension group is not closed: {pattern}"))
            .0;
        group.split('|').map(str::to_owned).collect()
    };

    let disclosure = extensions("destructive-migration");
    for id in ["delete-no-where", "update-no-where", "truncate-in-app-code"] {
        for ext in extensions(id) {
            assert!(
                disclosure.contains(&ext),
                "`sql/{id}` admits `.{ext}` in app code and excludes migration paths, but \
                 `sql/destructive-migration` does not admit `.{ext}` — so that rule's message promises a \
                 disclosure that is never emitted for `.{ext}` migrations. Disclosure extensions: {:?}",
                disclosure,
            );
        }
    }
}
