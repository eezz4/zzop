//! Adapter: lexical `.py` scan -> `NormalizedEnvelope`. Tree-walk + per-file orchestration
//! (`build_overlay`) and the `FileScan` accumulator it and `scan::scan_file` share; the actual
//! per-file scan logic lives in `scan.rs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use zzop_core::{
    FileProjection, IoFacts, NormalizedEnvelope, RouterMountEntry, RouterMountFragment,
    NORMALIZED_AST_FORMAT,
};

use super::scan::scan_file;

/// One file's accumulated router-mount state while scanning, keyed by the local binding identifier
/// (`router`, `api_router`, `app`, ...) — mirrors `RouterMountFragment` as a mutable builder, plus
/// this file's own creation-prefix map for the adapter-side join described in the module doc.
#[derive(Default)]
pub(super) struct FileScan {
    pub(super) fragments: HashMap<String, Vec<RouterMountEntry>>,
    /// ident -> its `APIRouter(prefix="...")` creation-time prefix (empty if none was given).
    pub(super) creation_prefix: HashMap<String, String>,
    /// local import alias -> dotted module path it came from (the `from X import a, b` module `X`,
    /// not `X.a`); resolved further at mount-resolution time.
    pub(super) imports: HashMap<String, String>,
}

pub(super) fn build_overlay(scan_root: &Path, scan_prefix: &str) -> NormalizedEnvelope {
    let py_files = walk_py_files(scan_root);

    // One-hop constant substrate: `IDENT: str = "literal"` class-body assignments from
    // `core/config.py`, if present (see module doc).
    let config_consts = read_config_consts(scan_root);

    let mut files = Vec::new();
    for abs in &py_files {
        let rel_in_scan = to_rel_forward_slash(scan_root, abs);
        let text = match std::fs::read_to_string(abs) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("adapter: skipping unreadable file {}: {e}", abs.display());
                continue;
            }
        };
        let full_path = if scan_prefix.is_empty() {
            rel_in_scan.clone()
        } else {
            format!("{scan_prefix}/{rel_in_scan}")
        };
        let loc = text.lines().count() as u32;

        let scan = scan_file(&full_path, &text, &config_consts, scan_prefix);
        let router_mount_fragments: Vec<RouterMountFragment> = scan
            .fragments
            .into_iter()
            .map(|(name, entries)| RouterMountFragment { name, entries })
            .collect();

        files.push(FileProjection {
            class_shape_fragments: Vec::new(),
            path: full_path,
            loc,
            symbols: Vec::new(),
            imports: zzop_core::ImportMap::new(),
            re_exports: Vec::new(),
            dynamic_imports: Vec::new(),
            used_names: Vec::new(),
            const_map_fragment: HashMap::new(),
            procedure_router_fragments: Vec::new(),
            router_mount_fragments,
            io: IoFacts::default(),
            degraded: false,
            is_entry: false,
            attributes: Vec::new(),
            loop_spans: Vec::new(),
        });
    }

    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "fastapi-overlay-prototype/1".to_string(),
        source: scan_root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| scan_root.display().to_string()),
        files,
    }
}

/// Recursively collects `*.py` files under `root`, skipping `tests`/`test`/`__pycache__`/`alembic`
/// directories (not route-registration surface).
fn walk_py_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_py_files_inner(root, &mut out);
    out.sort();
    out
}

fn walk_py_files_inner(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if matches!(name.as_ref(), "tests" | "test" | "__pycache__" | "alembic") {
                continue;
            }
            walk_py_files_inner(&path, out);
        } else if file_type.is_file() && path.extension().and_then(|e| e.to_str()) == Some("py") {
            out.push(path);
        }
    }
}

fn to_rel_forward_slash(root: &Path, abs: &Path) -> String {
    let rel = abs.strip_prefix(root).unwrap_or(abs);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Reads `<scan_root>/core/config.py` for top-level `Settings` class-body assignments of the shape
/// `IDENT: str = "literal"` — the only kind of constant this adapter ever folds.
fn read_config_consts(scan_root: &Path) -> HashMap<String, String> {
    let mut consts = HashMap::new();
    let config_path = scan_root.join("core").join("config.py");
    let Ok(text) = std::fs::read_to_string(&config_path) else {
        return consts;
    };
    let re = regex::Regex::new(r#"^\s*(\w+)\s*:\s*str\s*=\s*"([^"]*)"\s*$"#).unwrap();
    for line in text.lines() {
        if let Some(caps) = re.captures(line) {
            consts.insert(caps[1].to_string(), caps[2].to_string());
        }
    }
    consts
}
