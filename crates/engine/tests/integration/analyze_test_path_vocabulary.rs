//! The test-path axis of the convention vocabulary — the ONE additive key under `vocabulary.*`, and the
//! ONE whose built-in default applies to a run that declares nothing. Both exceptions are argued at
//! `VocabularyConfig::extra_test_path_patterns`; these tests are what makes them true rather than
//! documented.
//!
//! ## The defect these exist to keep closed (measured 2026-08-10)
//! The DSL's shared `${test-paths}` vocabulary knew only TypeScript's spellings — directories plus the
//! `.test.`/`.spec.` dot-infix — while `zzop_core::is_test_file` separately knew `_test.go`,
//! `test_*.py`, `*Tests.cs` and `FooTest.java`. 132 of the 144 bundled rules consult the fragment (measured 2026-08-10), so a
//! tree of nothing but idiomatic Go/Python/C# test files scored 14 findings, all false positives, against
//! 1 for the identical bytes under `tests/`. Two owners of one fact; the one with the consumers had
//! rotted. `cases/trees/test-conventions` is the corpus half of this pin (production twins that DO fire,
//! test twins in `benign`); the tests below are the unit half, plus the only coverage of the config key.
//!
//! Loads the REAL shipped packs, not stubs: the claim is about what a user's run actually does, and a
//! stub pack would let the fragment rot while these stayed green.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig, VocabularyConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn all_shipped_packs() -> Vec<RulePackDef> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl");
    let result = load_dsl_packs(&dir);
    assert!(result.errors.is_empty(), "pack load errors: {result:?}");
    result.packs.into_iter().map(|(_, p)| p).collect()
}

fn scan(dir: &TempDir, vocabulary: VocabularyConfig) -> AnalyzeOutput {
    analyze_tree(
        dir.path(),
        &EngineConfig {
            packs: all_shipped_packs(),
            vocabulary,
            ..EngineConfig::default()
        },
    )
}

/// The Go/Python/C# defect content, written to whatever paths the caller names. `services/` is
/// load-bearing: `reliability/console-in-be` is gated on a backend path segment, so a fixture outside
/// one would silently drop that rule from what is being measured.
const GO_BODY: &str = "package services\n\nimport \"fmt\"\n\nfunc FanOut(rows []string) {\n\tfor _, row := range rows {\n\t\tgo func() { fmt.Println(row) }()\n\t\tfmt.Println(row)\n\t}\n}\n";
const PY_BODY: &str =
    "import hashlib\n\n\ndef audit(ids):\n    for i in ids:\n        print(i)\n\n\ndef digest(password):\n    return hashlib.md5(password.encode()).hexdigest()\n";
const CS_BODY: &str = "using System;\nusing System.Security.Cryptography;\n\npublic class Thing\n{\n    public void Audit(int[] ids)\n    {\n        foreach (var i in ids)\n        {\n            Console.WriteLine($\"{i}\");\n        }\n    }\n}\n";

/// THE PIN. The same bytes at a production path and at each language's own test path: the production
/// side must report, the test side must be silent, and BOTH halves are asserted because either alone is
/// satisfiable by an accident. A silent-only assertion passes when the content stops triggering; a
/// firing-only assertion passes when the path gate is gone.
#[test]
fn each_language_test_path_convention_is_declined_while_its_production_twin_reports() {
    for (production, test_path, body) in [
        ("services/handler.go", "services/handler_test.go", GO_BODY),
        ("services/login.py", "services/test_login.py", PY_BODY),
        ("services/login2.py", "services/login_test.py", PY_BODY),
        ("services/Thing.cs", "services/ThingTests.cs", CS_BODY),
        ("services/Other.cs", "Api.Tests/Other.cs", CS_BODY),
        ("services/Third.cs", "services/ThirdTest.cs", CS_BODY),
    ] {
        let dir = TempDir::new("zzop-test-paths");
        dir.write(production, body);
        let produced = scan(&dir, VocabularyConfig::built_in());
        let on_production: Vec<&str> = produced
            .findings
            .iter()
            .map(|f| f.rule_id.as_str())
            .collect();
        assert!(
            !on_production.is_empty(),
            "{production} must report — otherwise the silence at {test_path} proves nothing"
        );

        let dir = TempDir::new("zzop-test-paths");
        dir.write(test_path, body);
        let declined = scan(&dir, VocabularyConfig::built_in());
        assert!(
            declined.findings.is_empty(),
            "{test_path} is an idiomatic test path but was judged as production code — the shared \
             `${{test-paths}}` vocabulary has lost this language's convention. Its production twin \
             {production} reported {on_production:?} for the same bytes. Findings: {:?}",
            declined.findings
        );
    }
}

/// The additive half: a project's own spelling is DECLARED, and declaring it costs nothing that was
/// already built in. The second assertion is the one that matters — replacement semantics (which every
/// other `vocabulary.*` key has) would silently drop `_test.go` the moment a user declared `it/`, and the
/// loss would surface as findings rather than as an error.
#[test]
fn a_declared_extra_test_path_adds_to_the_built_in_conventions_instead_of_replacing_them() {
    let dir = TempDir::new("zzop-test-paths-extra");
    dir.write("services/it/checkout.go", GO_BODY);
    dir.write("services/handler_test.go", GO_BODY);
    dir.write("services/handler.go", GO_BODY);

    let undeclared = scan(&dir, VocabularyConfig::built_in());
    let undeclared_files: Vec<&str> = undeclared
        .findings
        .iter()
        .map(|f| f.file.as_str())
        .collect();
    assert!(
        undeclared_files.contains(&"services/it/checkout.go"),
        "`it/` is nobody's language convention, so an undeclared run must judge it: {undeclared_files:?}"
    );
    assert!(
        undeclared_files.contains(&"services/handler.go"),
        "the production twin must report in both runs: {undeclared_files:?}"
    );
    assert!(
        !undeclared_files.contains(&"services/handler_test.go"),
        "the Go convention applies with nothing declared: {undeclared_files:?}"
    );

    let declared = scan(
        &dir,
        VocabularyConfig {
            extra_test_path_patterns: vec!["(^|/)it/".to_string()],
            ..VocabularyConfig::built_in()
        },
    );
    let declared_files: Vec<&str> = declared.findings.iter().map(|f| f.file.as_str()).collect();
    assert!(
        !declared_files.contains(&"services/it/checkout.go"),
        "the declared arm must take effect — a knob that changes nothing is worse than no knob: \
         {declared_files:?}"
    );
    assert!(
        !declared_files.contains(&"services/handler_test.go"),
        "ADDITIVE, not replacing: declaring `it/` must not cost this project the Go convention it never \
         mentioned. This is the trap the key's doc exists to refuse: {declared_files:?}"
    );
    assert!(
        declared_files.contains(&"services/handler.go"),
        "production code stays judged: {declared_files:?}"
    );
}

/// An unusable declaration is DROPPED and NAMED, never spliced into the exclusions of the 132 rules that reference the shared vocabulary. Splicing it
/// would make every one of those rules fail to compile its `file_exclude_pattern`, which the DSL treats
/// as "skip the whole rule" — one bad character in a config file turning into a silent, green, empty run.
#[test]
fn an_uncompilable_extra_test_path_is_dropped_by_name_and_leaves_the_valid_arms_working() {
    let dir = TempDir::new("zzop-test-paths-bad");
    dir.write("services/handler.go", GO_BODY);
    dir.write("services/handler_test.go", GO_BODY);
    dir.write("services/it/checkout.go", GO_BODY);

    let out = scan(
        &dir,
        VocabularyConfig {
            extra_test_path_patterns: vec!["(^|/)it/".to_string(), "([unclosed".to_string()],
            ..VocabularyConfig::built_in()
        },
    );
    let files: Vec<&str> = out.findings.iter().map(|f| f.file.as_str()).collect();
    assert!(
        files.contains(&"services/handler.go"),
        "the rules must still run at all — an invalid arm must not silence them: {files:?}"
    );
    assert!(
        !files.contains(&"services/handler_test.go") && !files.contains(&"services/it/checkout.go"),
        "the built-in conventions and the VALID declared arm must both survive a bad sibling: {files:?}"
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("extraTestPathPatterns") && w.contains("([unclosed")),
        "the dropped pattern must be named — its failure mode (fewer exclusions) is indistinguishable \
         from a wrong directory: {:?}",
        out.warnings
    );
}

/// PER TREE. Two roots, byte-identical files at the identical relative path, one root declaring `it/` and
/// the other declaring nothing — the multi-repo polyglot shape the key was asked for. A declaration is
/// carried on the tree's own `EngineConfig`, so this is structural rather than a coincidence of ordering;
/// the test exists because "structural" is the claim that gets made and then quietly stops being true.
#[test]
fn one_trees_declared_test_paths_never_reach_another_tree_in_the_same_run() {
    let a = TempDir::new("zzop-test-paths-tree-a");
    let b = TempDir::new("zzop-test-paths-tree-b");
    for dir in [&a, &b] {
        dir.write("services/it/checkout.go", GO_BODY);
    }

    let trees = vec![
        (
            a.path().to_path_buf(),
            EngineConfig {
                source_id: "a".to_string(),
                packs: all_shipped_packs(),
                vocabulary: VocabularyConfig {
                    extra_test_path_patterns: vec!["(^|/)it/".to_string()],
                    ..VocabularyConfig::built_in()
                },
                ..EngineConfig::default()
            },
        ),
        (
            b.path().to_path_buf(),
            EngineConfig {
                source_id: "b".to_string(),
                packs: all_shipped_packs(),
                vocabulary: VocabularyConfig::built_in(),
                ..EngineConfig::default()
            },
        ),
    ];

    let out = zzop_engine::analyze_trees(&trees);
    let counts: Vec<(&str, usize)> = out
        .trees
        .iter()
        .map(|(_, source_id, output)| (source_id.as_str(), output.findings.len()))
        .collect();
    assert_eq!(
        counts.iter().find(|(id, _)| *id == "a").map(|(_, n)| *n),
        Some(0),
        "tree a declared `it/` and must be silent: {counts:?}"
    );
    assert!(
        counts
            .iter()
            .find(|(id, _)| *id == "b")
            .is_some_and(|(_, n)| *n > 0),
        "tree b declared nothing and must still judge the same path — a declaration that leaked would \
         show up here as a second zero: {counts:?}"
    );
}
