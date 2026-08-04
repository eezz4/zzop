//! End-to-end coverage for `vocabulary.pythonPackageRoots` (U56) — the declared extra roots Python
//! ABSOLUTE imports resolve from, on top of the built-in tree root and `src/`.
//!
//! Three axes, and the third is the safety argument the key was judged on, measured rather than
//! asserted in prose:
//! - ⓐ undeclared = the pre-key behavior, byte-for-byte (the dep graph does not move);
//! - ⓑ a correct declaration makes real edges appear (both entry forms: the editable-install
//!   package mapping `"tml="` and the interposed-directory extra root `"backend"`);
//! - ⓒ a WRONG declaration changes NOTHING — candidates are filtered against the paths that actually
//!   exist, so a bad entry is one more failed lookup and can never invent an edge.
//!
//! Same `TempDir` fixture pattern as `analyze_vocabulary_config.rs`, which owns the general
//! "a declarable knob must move the output" coverage for the rest of the vocabulary.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

fn scan(dir: &TempDir, roots: &[&str]) -> AnalyzeOutput {
    analyze_tree(
        dir.path(),
        &EngineConfig {
            vocabulary: VocabularyConfig {
                python_package_roots: roots.iter().map(|s| s.to_string()).collect(),
                ..VocabularyConfig::default()
            },
            ..EngineConfig::default()
        },
    )
}

fn dep_edges<'a>(out: &'a AnalyzeOutput, rel: &str) -> &'a [String] {
    out.ir
        .ir
        .dep
        .get(rel)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// The `twitter/the-algorithm-ml` shape that motivated the key: every internal import is absolute
/// under a package name (`tml.*`) that no tree directory carries — the venv symlinks the name to the
/// tree root (`ln -s $(pwd) site-packages/tml`), so only a declaration can say so.
fn editable_install_tree() -> TempDir {
    let dir = TempDir::new("zzop-py-pkg-roots-tml");
    dir.write("projects/home/model.py", "def build():\n    return 1\n");
    dir.write(
        "core/train.py",
        "from tml.projects.home import model\n\ndef run():\n    return model.build()\n",
    );
    dir
}

#[test]
fn a_undeclared_roots_leave_the_symlinked_package_unresolved() {
    let dir = editable_install_tree();
    let out = scan(&dir, &[]);
    assert_eq!(
        dep_edges(&out, "core/train.py"),
        &[] as &[String],
        "without a declaration there is no `tml/` directory to resolve against — the import stays \
         unresolved, which is the measured -ml silence this key exists to end"
    );
}

#[test]
fn b_a_package_mapping_declaration_makes_the_edge_appear() {
    let dir = editable_install_tree();
    let out = scan(&dir, &["tml="]);
    assert_eq!(
        dep_edges(&out, "core/train.py"),
        &["projects/home/model.py".to_string()],
        "declaring `tml=` (import name tml == tree root) must resolve the absolute import to the real \
         file"
    );
}

#[test]
fn b_an_extra_root_declaration_resolves_an_interposed_directory() {
    // The `backend/` shape from the same judgment: `app.api.main` lives at `backend/app/api/main.py`,
    // one directory below every root the resolver tries built-in.
    let dir = TempDir::new("zzop-py-pkg-roots-backend");
    dir.write("backend/app/core/config.py", "API_V1 = \"/api/v1\"\n");
    dir.write(
        "backend/app/api/main.py",
        "from app.core import config\n\ndef base():\n    return config.API_V1\n",
    );

    let undeclared = scan(&dir, &[]);
    assert_eq!(
        dep_edges(&undeclared, "backend/app/api/main.py"),
        &[] as &[String]
    );

    let declared = scan(&dir, &["backend"]);
    assert_eq!(
        dep_edges(&declared, "backend/app/api/main.py"),
        &["backend/app/core/config.py".to_string()],
        "declaring the interposed `backend` root must resolve the absolute import"
    );
}

#[test]
fn c_a_wrong_declaration_adds_zero_edges_and_invents_none() {
    // The safety argument, measured: candidates are questions, not claims. A declaration naming a
    // directory that does not exist, or mapping the package to the wrong place, must leave the WHOLE
    // dep graph identical to the undeclared run — no new edges anywhere, no false edge to any file.
    let dir = editable_install_tree();
    let baseline = scan(&dir, &[]);
    let wrong = scan(&dir, &["frontend", "tml=lib/tml", "nosuch="]);
    assert_eq!(
        wrong.ir.ir.dep, baseline.ir.ir.dep,
        "a wrong declaration must be indistinguishable from no declaration — anything else means a \
         candidate stopped being filtered by real paths and the no-invented-edges property broke"
    );
    assert_eq!(dep_edges(&wrong, "core/train.py"), &[] as &[String]);
}
