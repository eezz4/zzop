//! Exercises `rules/dsl/perf/perf.json`'s `sqlalchemy-eager-relationship` line-scan rule: a
//! declaration-level SQLAlchemy eager loader strategy (`lazy="joined"/"subquery"/"selectin"/"immediate"`,
//! the legacy `lazy=False`, and the SQLModel `sa_relationship_kwargs` string-key spelling). Uses its own
//! `sqla_scan` helper (mirrors `perf.rs::scan`'s harness shape) since the shared `scan` helper filters to
//! `perf/api-in-loop` only.
//!
//! This is the FIRST line-scan rule in the shipped packs whose `file_pattern` targets `.py` (the two
//! existing `.py`-scoped rules in `http` are io-scan), so several tests below seal not just the rule but
//! the Python plumbing it rides — that `.py` files reach the per-file lexical pass at all, and the two
//! `//`-comment assumptions the line-scan interpreter makes that Python does not satisfy.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, Finding, RulePackDef};
use zzop_engine::{analyze_tree, EngineConfig};

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

fn perf_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "perf")
        .expect("perf.json pack present")
}

fn sqla_scan(rel: &str, content: &str) -> Vec<Finding> {
    let dir = TempDir::new("zzop-perf-sqla-eager");
    dir.write(rel, content);
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![perf_pack()],
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    out.findings
        .into_iter()
        .filter(|f| f.rule_id == "perf/sqlalchemy-eager-relationship")
        .collect()
}

const IMPORTS: &str = "from sqlalchemy.orm import relationship\n";

#[test]
fn relationship_lazy_joined_is_flagged() {
    // The plumbing seal as much as the rule's: a `.py` file reaches the per-file line-scan pass at all.
    let f = sqla_scan(
        "models.py",
        &format!("{IMPORTS}\n\nclass User(Base):\n    photos = relationship(\"Photo\", lazy=\"joined\")\n"),
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn relationship_lazy_selectin_is_flagged() {
    // `selectin` is the strategy most often reached for as an "N+1 fix" and is the one most likely to be
    // left on a declaration permanently; it must not be treated as the safe member of the family.
    let f = sqla_scan(
        "models.py",
        &format!("{IMPORTS}\n\nclass User(Base):\n    photos = relationship(\"Photo\", lazy=\"selectin\")\n"),
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn relationship_lazy_subquery_on_its_own_continuation_line_is_flagged() {
    // The realistic multi-line `relationship(...)` call form — the pattern is per-LINE, so the option
    // must be recognized on the continuation line without the `relationship(` token beside it.
    let f = sqla_scan(
        "models.py",
        &format!(
            "{IMPORTS}\n\nclass User(Base):\n    photos = relationship(\n        \"Photo\",\n        back_populates=\"user\",\n        lazy=\"subquery\",\n    )\n"
        ),
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn legacy_lazy_false_synonym_for_joined_is_flagged() {
    // `lazy=False` is SQLAlchemy's legacy spelling of `lazy="joined"` — same eager join, no quotes, so it
    // needs its own arm of the alternation or it reads as clean.
    let f = sqla_scan(
        "models.py",
        &format!(
            "{IMPORTS}\n\nclass User(Base):\n    photos = relationship(\"Photo\", lazy=False)\n"
        ),
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn sqlmodel_sa_relationship_kwargs_string_key_is_flagged() {
    // SQLModel routes the same option through a dict, so the key is a STRING (`"lazy": "selectin"`) and
    // the `lazy=` keyword-argument arm cannot see it.
    let f = sqla_scan(
        "models.py",
        "from sqlmodel import Field, Relationship, SQLModel\n\n\nclass User(SQLModel, table=True):\n    photos: list[\"Photo\"] = Relationship(sa_relationship_kwargs={\"lazy\": \"selectin\"})\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn pyi_stub_extension_is_in_scope() {
    // `file_pattern` is `\.pyi?$` — the stub extension is deliberately included, so a declaration that
    // lives only in a `.pyi` is not silently out of scope.
    let f = sqla_scan(
        "models.pyi",
        &format!("{IMPORTS}\n\nclass User(Base):\n    photos = relationship(\"Photo\", lazy=\"joined\")\n"),
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn lazy_dynamic_and_default_select_are_not_flagged() {
    // The negative half of the vocabulary: `dynamic` returns a query and `select` IS the lazy default —
    // neither declares an eager load, and flagging either would make the rule fire on ordinary models.
    let f = sqla_scan(
        "models.py",
        &format!(
            "{IMPORTS}\n\nclass User(Base):\n    photos = relationship(\"Photo\", lazy=\"dynamic\")\n    tags = relationship(\"Tag\", lazy=\"select\")\n"
        ),
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn query_time_joinedload_opt_in_is_not_flagged() {
    // The FIX this rule recommends must not itself fire — per-query `joinedload(...)` is the opt-in the
    // message points at, and it carries no `lazy` option at all.
    let f = sqla_scan(
        "queries.py",
        "from sqlalchemy.orm import joinedload\n\n\ndef load_users(session):\n    return session.query(User).options(joinedload(User.photos)).all()\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_python_file_with_no_orm_import_is_not_flagged() {
    // `require_file` is the whole reason a bare `lazy=` keyword is an acceptable line pattern; without
    // the ORM signal the same line must stay silent.
    let f = sqla_scan(
        "loader.py",
        "def build(lazy=\"joined\"):\n    return {\"lazy\": \"joined\"}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn commented_out_declaration_on_a_full_line_hash_comment_is_not_flagged() {
    // `skip_comment_lines` only knows `//`/`*`/`/*`, so this rule carries `exclude_pattern` `^\s*#`
    // instead — without it every commented-out Python declaration would fire.
    let f = sqla_scan(
        "models.py",
        &format!(
            "{IMPORTS}\n\nclass User(Base):\n    # photos = relationship(\"Photo\", lazy=\"joined\")\n    photos = relationship(\"Photo\")\n"
        ),
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_declaration_under_a_tests_directory_is_not_flagged() {
    // The shared `${test-paths-stories}` exclusion reaches the usual Python layout (`tests/`); a model
    // defined for a test fixture is not a production read path.
    let f = sqla_scan(
        "tests/test_models.py",
        &format!("{IMPORTS}\n\nclass User(Base):\n    photos = relationship(\"Photo\", lazy=\"joined\")\n"),
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_top_level_test_prefixed_module_still_fires_documented_residual() {
    // Residual, pinned rather than fixed: `${test-paths-stories}` matches DIRECTORIES (`tests/`) and the
    // JS `.test.`/`.spec.` infixes, not pytest's `test_*.py` FILENAME convention outside such a
    // directory. Widening it would be a change to a shared vocabulary every pack rides, not to this
    // rule. If the fragment ever grows the Python filename clause, this test goes red and is the place
    // that records why the expectation flipped.
    let f = sqla_scan(
        "test_models.py",
        &format!("{IMPORTS}\n\nclass User(Base):\n    photos = relationship(\"Photo\", lazy=\"joined\")\n"),
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn a_bare_hash_suppress_marker_does_not_suppress_but_the_documented_spelling_does() {
    // THE Python plumbing gap, pinned in both directions because the rule's message documents exactly
    // this: `crates/core/src/dsl/markers.rs`'s `compile_marker` anchors a line-scan marker as
    // `//\s*<marker>\b`, and only `.sql` files get a second leader (`--`). Python's `#` is not among
    // them, so the idiomatic spelling is INERT and the message tells the reader to write `# // <marker>`
    // instead. If line-scan ever gains `#` (io-scan's `compile_marker_line_comment` already has it), the
    // first assertion flips and the message's suppression paragraph must be rewritten in that change.
    let bare = sqla_scan(
        "models.py",
        &format!(
            "{IMPORTS}\n\nclass User(Base):\n    # zzop-sqlalchemy-eager-relationship-ok: tiny fixed lookup table\n    photos = relationship(\"Photo\", lazy=\"joined\")\n"
        ),
    );
    assert_eq!(bare.len(), 1, "bare `#` marker must NOT suppress: {bare:?}");

    let documented = sqla_scan(
        "models.py",
        &format!(
            "{IMPORTS}\n\nclass User(Base):\n    # // zzop-sqlalchemy-eager-relationship-ok: tiny fixed lookup table\n    photos = relationship(\"Photo\", lazy=\"joined\")\n"
        ),
    );
    assert!(
        documented.is_empty(),
        "the spelling the message documents must suppress: {documented:?}"
    );
}
