//! e2e: the `examples/java-imports-adapter` reference overlay against a natively-parsed Java tree.
//!
//! History: this example was built against the v0.16-era lexical Java projector, which returned
//! `imports: None` for every `.java` file — a Java tree carried ZERO dependency edges natively, and
//! `envelope::merge::merge_projection_onto_artifact` adopted the overlay's `imports` because the
//! native artifact had none. `zzop-parser-java-21` (full CST, tree-sitter) closed that gap: `.java`
//! files now get native import extraction. What this test pins TODAY:
//!
//! 1. the committed envelope bytes (`test/expected-envelope.json`, produced by `adapter.mjs` over
//!    `test/fixture/`, the same bytes the example's node snapshot test pins) still validate through
//!    the REAL `zzop_core::validate_envelope`;
//! 2. the closed gap itself — the native Java parser yields real dep edges with no overlay at all,
//!    including edges the imports-only adapter could never see (leaf targets);
//! 3. merge precedence — an overlay's `imports` are adopted only when the native artifact has none,
//!    so on today's Java trees this overlay is a no-op: parsed facts are never overridden.
//!
//! The example stays as the minimal Mode B "one channel" teaching exhibit — the recipe applies to
//! any extension without native import extraction (see its README).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, EngineConfig};

/// A self-cleaning temp directory (same std-only pattern as `analyze_adapter_overlay.rs`).
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
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const APP: &str = "src/main/java/com/example/app/App.java";
const TEXT_UTIL: &str = "src/main/java/com/example/util/TextUtil.java";
const CONFIG: &str = "src/main/java/com/example/model/Config.java";

/// Copies the example's committed fixture tree (single-sourced there — the node snapshot test runs
/// over the same bytes) into a fresh temp dir.
fn write_fixture_tree() -> TempDir {
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/java-imports-adapter/test/fixture");
    let dir = TempDir::new("zzop-java-imports-overlay");
    for rel in [APP, TEXT_UTIL, CONFIG] {
        let full = dir.path().join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::copy(fixture_root.join(rel), full).unwrap();
    }
    dir
}

/// The adapter's committed output for the fixture tree, validated through the REAL
/// `zzop_core::validate_envelope` path — the same validator `apply_adapter_overlays` re-runs per
/// overlay, and the same one `zzop validate-envelope` / the MCP `validate_envelope` tool wrap.
fn example_envelope() -> zzop_core::NormalizedEnvelope {
    let json = include_str!("../../../examples/java-imports-adapter/test/expected-envelope.json");
    zzop_core::validate_envelope(json)
        .expect("examples/java-imports-adapter/test/expected-envelope.json must validate cleanly")
}

#[test]
fn committed_example_envelope_validates_against_the_real_contract() {
    let envelope = example_envelope();
    assert_eq!(envelope.parser, "java-imports-adapter/1");
    assert_eq!(envelope.files.len(), 2);
    // One channel only: every projection carries imports and NOTHING else the merge consumes.
    for file in &envelope.files {
        assert!(
            !file.imports.is_empty(),
            "{}: imports is the one channel",
            file.path
        );
        assert!(
            file.io.provides.is_empty() && file.io.consumes.is_empty(),
            "{}",
            file.path
        );
        assert!(file.symbols.is_empty() && !file.is_entry, "{}", file.path);
    }
}

#[test]
fn native_java_tree_now_has_full_dep_edges() {
    // The closed gap: `zzop-parser-java-21` extracts imports natively, so the bare tree — no
    // overlay — carries the full dep graph, including the two edges the imports-only reference
    // adapter could never see (Config.java is a leaf with no projection of its own, yet edges INTO
    // it resolve, because the native resolution set is built from every parsed file, not just
    // fact-carrying overlay projections).
    let dir = write_fixture_tree();
    let out = analyze_tree(dir.path(), &EngineConfig::default());
    assert_eq!(out.file_count, 3);

    let app_edges = out.ir.ir.dep.get(APP).cloned().unwrap_or_default();
    // Both the plain `import ...TextUtil;` and the `import static ...TextUtil.trimAll;` bindings
    // point at the same file — one deduped edge, not two.
    assert_eq!(
        app_edges,
        vec![TEXT_UTIL.to_string(), CONFIG.to_string()],
        "{:?}",
        out.ir.ir.dep
    );
    let text_util_edges = out.ir.ir.dep.get(TEXT_UTIL).cloned().unwrap_or_default();
    assert_eq!(
        text_util_edges,
        vec![CONFIG.to_string()],
        "{:?}",
        out.ir.ir.dep
    );
}

#[test]
fn overlay_imports_never_override_native_imports() {
    // Merge precedence, pinned from the overlay side: `merge_projection_onto_artifact` adopts an
    // overlay's `imports` exactly when the native artifact has none. The native Java parser now
    // populates them, so attaching the example overlay changes NOTHING — the dep graph is
    // byte-identical to the bare-tree baseline (parsed facts are never overridden, even where the
    // overlay's recall is a strict subset of the native parser's).
    let dir = write_fixture_tree();
    let baseline = analyze_tree(dir.path(), &EngineConfig::default());
    let cfg = EngineConfig {
        adapter_overlays: vec![example_envelope()],
        ..EngineConfig::default()
    };
    let overlaid = analyze_tree(dir.path(), &cfg);

    assert_eq!(overlaid.ir.ir.dep, baseline.ir.ir.dep);
}
