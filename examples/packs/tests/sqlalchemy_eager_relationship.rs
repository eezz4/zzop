//! Exercises `examples/packs/orm-eager.json`'s `sqlalchemy-eager-relationship` line-scan rule: a
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
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/packs");
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
        .find(|p| p.id == "orm-eager")
        .expect("orm-eager.json pack present")
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
        .filter(|f| f.rule_id == "orm-eager/sqlalchemy-eager-relationship")
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
fn a_top_level_test_prefixed_module_is_not_flagged() {
    // FLIPPED 2026-08-10, at the invitation of the version of this test it replaces. That one asserted
    // `len() == 1` and called itself a "documented residual": `${test-paths-stories}` matched
    // DIRECTORIES (`tests/`) and the JS `.test.`/`.spec.` infixes but not pytest's `test_*.py` FILENAME
    // convention outside such a directory, and it said in as many words that widening the shared
    // fragment was the fix and that this test was "the place that records why the expectation flipped".
    //
    // The fragment now carries every language's own convention — Python's `test_*.py`/`*_test.py`, Go's
    // `_test.go`, C#'s `*Tests.cs`, Java's `FooTest.java` — because it and `zzop_core::is_test_file` were
    // merged into one owner, and the half with 131 consumers had been the TypeScript-only one. This
    // assertion is the same claim the sibling above makes for `tests/`: a model defined for a test
    // fixture is not a production read path, and pytest names that file `test_models.py` whether or not
    // it sits in a directory.
    let f = sqla_scan(
        "test_models.py",
        &format!("{IMPORTS}\n\nclass User(Base):\n    photos = relationship(\"Photo\", lazy=\"joined\")\n"),
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn the_idiomatic_hash_suppress_marker_works_and_so_does_the_old_sandwich() {
    // FLIPPED 2026-08-12, exactly as the version of this test it replaces invited: "If line-scan ever
    // gains `#` (io-scan's `compile_marker_line_comment` already has it), the first assertion flips and
    // the message's suppression paragraph must be rewritten in that change." `.py` joined the marker
    // axis's hash family that day, the paragraph was rewritten in the same commit, and the two
    // assertions below are the pin that the prescribed spelling and the old one both land.
    //
    // The old test was doing real work while it stood — it is what caught the widening on the first
    // full-suite run, because a rule matching `\.pyi?$` is invisible to a "which file_pattern names
    // `py`" text scan of the pack JSONs, which is how the change's blast radius was first (wrongly)
    // measured at five sql rules.
    let idiomatic = sqla_scan(
        "models.py",
        &format!(
            "{IMPORTS}\n\nclass User(Base):\n    # zzop-sqlalchemy-eager-relationship-ok: tiny fixed lookup table\n    photos = relationship(\"Photo\", lazy=\"joined\")\n"
        ),
    );
    assert!(
        idiomatic.is_empty(),
        "the spelling the message now prescribes must suppress: {idiomatic:?}"
    );

    // The sandwich the message used to prescribe keeps working, because it CONTAINS the `//` form —
    // additive, like every other widening on this axis. Pinned so the flip above cannot be read as
    // "the old advice was revoked": a reader who already wrote it is not broken by this change.
    let sandwich = sqla_scan(
        "models.py",
        &format!(
            "{IMPORTS}\n\nclass User(Base):\n    # // zzop-sqlalchemy-eager-relationship-ok: tiny fixed lookup table\n    photos = relationship(\"Photo\", lazy=\"joined\")\n"
        ),
    );
    assert!(
        sandwich.is_empty(),
        "the previously documented spelling must keep working: {sandwich:?}"
    );
}

#[test]
fn the_hash_marker_does_not_suppress_in_a_pyi_stub() {
    // The paired residual of `the_idiomatic_hash_suppress_marker_works_...` above, and the reason this
    // rule's message now scopes its offer to `.py` explicitly. The hash family is spelled BY EXTENSION
    // (`HASH_COMMENT_EXTENSIONS` lists `py`, not `pyi`), while this rule's `file_pattern` is `\.pyi?$` —
    // so the one file type where the marker silently does nothing is one this rule genuinely reads.
    // `pyi_stub_extension_is_in_scope` above is the control: the same declaration without the marker
    // fires, so this is a statement about the MARKER, not about scope.
    //
    // Read as a claim about the engine rather than about this rule: whether `pyi` should join the hash
    // family is a separate question with its own cost (`marker_widening_prose` is printed to users, and
    // `zzop explain` plus `docs/rules/dsl-reference.md` carry the roster). This test is what keeps the
    // message and the engine saying the same thing.
    //
    // ⚠ This is only HALF the residual, and the message said the other half wrong until 2026-08-13:
    // it concluded "no inline marker is writable in a stub at all" and sent the reader to the config
    // lever. That does not follow from the line below — see the sibling test, which measures what a
    // stub DOES accept. Getting one half of a split right is what made the wrong half look measured.
    let f = sqla_scan(
        "models.pyi",
        &format!(
            "{IMPORTS}\n\nclass User(Base):\n    # zzop-sqlalchemy-eager-relationship-ok: tiny fixed lookup table\n    photos = relationship(\"Photo\", lazy=\"joined\")\n"
        ),
    );
    assert_eq!(
        f.len(),
        1,
        "a `#` marker must NOT suppress in a `.pyi` stub — `pyi` is not in HASH_COMMENT_EXTENSIONS: {f:?}"
    );
}

#[test]
fn the_sandwich_marker_still_suppresses_in_a_pyi_stub() {
    // The half the message got wrong. Because `pyi` is outside the hash family, the marker axis grants
    // `//` alone there — and `# // <marker>` is a Python comment whose BODY carries the `//` form, so
    // the very sandwich that stopped being necessary in `.py` is the one spelling that still works in a
    // stub. The message's old conclusion ("no inline marker is writable in a stub at all") did not
    // follow from the sibling test above; it inferred a total absence from one spelling's failure.
    //
    // So the sandwich did not die when `.py` joined the hash family — it MOVED. Pinned in both
    // directions on purpose: if `pyi` ever joins the family, the sibling above goes red and this one
    // stays green, which is exactly the signal that the message's `.pyi` paragraph can be simplified
    // rather than deleted.
    let f = sqla_scan(
        "models.pyi",
        &format!(
            "{IMPORTS}\n\nclass User(Base):\n    # // zzop-sqlalchemy-eager-relationship-ok: stub mirrors a tiny lookup table\n    photos = relationship(\"Photo\", lazy=\"joined\")\n"
        ),
    );
    assert!(
        f.is_empty(),
        "the `# //` sandwich must suppress in a `.pyi` stub — it carries the `//` form the marker axis \
         grants there, and it is the only inline spelling that does: {f:?}"
    );
}
