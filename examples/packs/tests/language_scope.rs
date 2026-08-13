//! The LANGUAGE-SCOPE half of this pack's two quote-anchored rules — `select-star` and
//! `like-leading-wildcard` — split out of `rules/dsl/sql/language_scope.rs` when they left the bundle.
//!
//! Neither carries one character of host-language syntax: each is a quote character, a SQL keyword and
//! what follows it. Both were nonetheless gated to `.ts/.tsx/.js/.mjs/.cjs` and `.rs` until 2026-08-10,
//! so `"SELECT * FROM users"` was reported in TypeScript and SILENT in Python, Go, C# and Java. The
//! fixtures below are the same whole files the bundled side keeps, deliberately unchanged: a fixture
//! trimmed to only this pack's statements would stop being evidence that the two rules pick their own
//! lines out of a file holding five candidate statements.
//!
//! `select_like.rs` in this directory owns each rule's matcher semantics; this file owns only the
//! question "does the file's LANGUAGE change the verdict", in both directions. The corpus half is
//! `cases/trees/sql-langs/`.

use crate::{hits, scan, TempDir};

/// One production file per language, each holding the SAME five statements — three of them the bundled
/// `sql` pack's subjects, two of them this pack's.
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

const EXPORTED_TWO: [&str; 2] = ["select-star", "like-leading-wildcard"];

#[test]
fn every_newly_admitted_language_fires_both_quote_anchored_rules() {
    for (path, src) in PRODUCTION {
        let dir = TempDir::new("zzop-sql-preferences");
        dir.write(path, src);
        let out = scan(&dir);
        for rule in EXPORTED_TWO {
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
    // Both rules exclude test paths through the shared `${test-paths}` vocabulary, which learned
    // `_test.go`, `test_*.py`, `*Tests.cs`/`*.Tests/` and the Java spellings on 2026-08-10. Without it,
    // admitting these extensions would pour findings into every test module holding SQL text.
    for (path, src) in TEST_TWINS {
        let dir = TempDir::new("zzop-sql-preferences");
        dir.write(path, src);
        let out = scan(&dir);
        for rule in EXPORTED_TWO {
            assert!(
                hits(&out, rule).is_empty(),
                "{rule} fired on the test path {path}: {:?}",
                out.findings
            );
        }
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
        let dir = TempDir::new("zzop-sql-preferences");
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
        let dir = TempDir::new("zzop-sql-preferences");
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

// --- the Python comment leader, one axis at a time --------------------------------------------------
//
// Both rules' messages tell a Python reader two things that come out of DIFFERENT engine tables, and
// until 2026-08-12 both said it with one fused word ("the comment/marker leader") — a phrase that can be
// true of at most one of the two, and that each message then contradicted by offering a `#` marker a few
// clauses later. `scripts/check-marker-claims.sh` refuses the fusion; these three fixtures decide the
// same question by behaviour, so the corrected prose cannot drift back without a red test.
//
// Same two statements throughout, so the control and the two claims cannot end up testing three
// different files — which is the failure mode a suppression test without a matching control has.
const PY_BOTH: &str = "def q():\n    d = \"SELECT * FROM users\"\n    e = \"SELECT id FROM users WHERE name LIKE '%term'\"\n    return (d, e)\n";

#[test]
fn the_python_control_fires_both_rules_once_each() {
    let dir = TempDir::new("zzop-sql-preferences");
    dir.write("services/hash_axis.py", PY_BOTH);
    let out = scan(&dir);
    for rule in EXPORTED_TWO {
        assert_eq!(
            hits(&out, rule).len(),
            1,
            "the control for the two `#` tests below — {rule}: {:?}",
            out.findings
        );
    }
}

#[test]
fn a_hash_marker_suppresses_both_rules_in_python() {
    // The MARKER axis: `py` is in `HASH_COMMENT_EXTENSIONS`, so `# <marker>` on the line above suppresses
    // exactly as `// <marker>` does in the TypeScript fixtures of `select_like.rs`. Each rule needs its
    // OWN marker, so this also pins that suppressing one does not silence the other.
    let dir = TempDir::new("zzop-sql-preferences");
    dir.write(
        "services/hash_axis.py",
        "def q():\n    # zzop-select-star-ok: internal debug dump, columns intentionally unbounded\n    d = \"SELECT * FROM users\"\n    # zzop-like-leading-wildcard-ok: tiny fixed lookup table, offline batch job\n    e = \"SELECT id FROM users WHERE name LIKE '%term'\"\n    return (d, e)\n",
    );
    let out = scan(&dir);
    for rule in EXPORTED_TWO {
        assert!(
            hits(&out, rule).is_empty(),
            "a `#` marker must suppress {rule} in a `.py` file: {:?}",
            out.findings
        );
    }
}

#[test]
fn a_hash_commented_out_query_still_fires_both_rules_in_python() {
    // The SKIP axis, deliberately NOT widened with the marker axis: `skip_comment_lines` still reads
    // `//` alone outside `.sql` and config files, so a `#`-commented-out query is judged as live code.
    // This is the half a reader is likeliest to get backwards — the same `#` that silences a finding
    // from the line above does NOT make the line it prefixes invisible.
    let dir = TempDir::new("zzop-sql-preferences");
    dir.write(
        "services/hash_axis.py",
        "def q():\n    # d = \"SELECT * FROM users\"\n    # e = \"SELECT id FROM users WHERE name LIKE '%term'\"\n    return None\n",
    );
    let out = scan(&dir);
    for rule in EXPORTED_TWO {
        assert_eq!(
            hits(&out, rule).len(),
            1,
            "a `#`-commented-out query is still live code to the skip axis — {rule}: {:?}",
            out.findings
        );
    }
}
