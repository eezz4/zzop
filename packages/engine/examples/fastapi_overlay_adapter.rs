//! End-to-end demo of the Mode B external-adapter injection path (`EngineConfig::adapter_overlays`)
//! against a FastAPI (Python) backend — a language zzop has no native in-process parser for, so
//! without an overlay it reports zero provides no matter how many `.py` files exist under a root.
//!
//! This example plays two roles at once:
//!  1. **External adapter author** — a lexical scanner over `.py` text (line/regex-based, no real
//!     Python AST) that recognizes FastAPI's router-registration idioms and projects them into the
//!     engine's `NormalizedEnvelope` / `FileProjection` fragment-channel contract (see
//!     `docs/NORMALIZED_AST.md`'s "Adapter overlays" section; shapes in `zzop_core::fragments`).
//!  2. **Engine caller** — feeds that envelope into `EngineConfig::adapter_overlays` and runs
//!     `analyze_tree` (Mode B: the overlay merges onto a native per-file pass over the same tree, so
//!     a mixed TypeScript-frontend/Python-backend tree still gets a native pass on its own half).
//!
//! A real external adapter would be a separate process/crate emitting this same JSON shape over a
//! socket, file, or pipe — every type used here (`NormalizedEnvelope`, `FileProjection`,
//! `RouterMountFragment`, `RouterMountEntry`) is the public contract any adapter author would consume.
//!
//! ## Adapter logic (deliberately a prototype — precision via structural anchors, never guessed)
//! Per `.py` file under `<root>/backend/app` (or `<root>` if that path is absent):
//!  - `router = APIRouter(prefix="/x", ...)` binds a router-mount fragment named after the binding
//!    identifier. A creation-time `prefix=` kwarg is resolved locally and prepended to every `Verb`
//!    path this identifier registers in the same file (the wire shape has no self-prefix field).
//!  - `@router.get("/items/{item_id}")` (and post/put/patch/delete) becomes a `Verb` entry: path
//!    verbatim, line = the decorator line, handler = the `def`/`async def` name on the next such
//!    line found scanning forward.
//!  - `x.include_router(y.router, prefix="/y")` becomes a `Mount` entry. The target `specifier` is
//!    resolved Python-module-style from this file's own imports (`from app.api.routes import items`
//!    binds `items` to submodule `app.api.routes.items`; a bare `include_router(api_router)`
//!    resolves the same way through its own import).
//!  - A non-literal `prefix=settings.API_V1_STR` kwarg is resolved one hop via a literal
//!    `IDENT: str = "..."` class-body assignment in `app/core/config.py`, if present; otherwise the
//!    mount honestly degrades to prefix `"/"` with a stderr warning rather than fabricating a value.
//!  - `app = FastAPI(...)` binds the root fragment named `app`; this app's own top-level mount is
//!    what makes `app` the sole un-mounted root once composed (any fragment named by a `Mount.ident`
//!    anywhere is excluded from being a root, resolved or not — under-reporting is honest, mis-keying
//!    is not).
//!
//! Usage: `cargo run --release -p zzop-engine --example fastapi_overlay_adapter -- <root>`

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use zzop_core::{
    FileProjection, IoFacts, NormalizedEnvelope, RouterMountEntry, RouterMountFragment,
    NORMALIZED_AST_FORMAT,
};
use zzop_engine::{analyze_tree, EngineConfig};

fn main() {
    let root = match std::env::args().nth(1) {
        Some(r) => PathBuf::from(r),
        None => {
            eprintln!("usage: fastapi_overlay_adapter <root>");
            std::process::exit(2);
        }
    };

    let scan_root_backend = root.join("backend").join("app");
    let (scan_root, scan_prefix) = if scan_root_backend.is_dir() {
        (scan_root_backend, "backend/app".to_string())
    } else {
        (root.clone(), String::new())
    };

    let envelope = build_overlay(&scan_root, &scan_prefix);
    let (verb_count, mount_count) = envelope.files.iter().fold((0usize, 0usize), |acc, f| {
        f.router_mount_fragments
            .iter()
            .flat_map(|frag| &frag.entries)
            .fold(acc, |(v, m), entry| match entry {
                RouterMountEntry::Verb { .. } => (v + 1, m),
                RouterMountEntry::Mount { .. } => (v, m + 1),
            })
    });
    eprintln!(
        "adapter: scanned {} python file(s) under {} -> {verb_count} Verb entr{}, \
         {mount_count} Mount entr{} extracted",
        envelope.files.len(),
        scan_root.display(),
        if verb_count == 1 { "y" } else { "ies" },
        if mount_count == 1 { "y" } else { "ies" },
    );
    // ZZOP_DUMP_FRAGMENTS=1: dump every extracted fragment/entry, to distinguish an adapter bug
    // (wrong extraction) from an engine composition bug (dropped a correct extraction).
    if std::env::var("ZZOP_DUMP_FRAGMENTS").is_ok() {
        for f in &envelope.files {
            for frag in &f.router_mount_fragments {
                eprintln!("  fragment '{}' @ {}", frag.name, f.path);
                for entry in &frag.entries {
                    eprintln!("    {entry:?}");
                }
            }
        }
    }

    let source_id = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());

    let base_config = EngineConfig {
        source_id: source_id.clone(),
        ..EngineConfig::default()
    };
    let before = analyze_tree(&root, &base_config);

    let mut overlay_config = EngineConfig {
        source_id,
        ..EngineConfig::default()
    };
    overlay_config.adapter_overlays = vec![envelope];
    let after = analyze_tree(&root, &overlay_config);

    print_summary("BEFORE (native only, no overlay)", &before);
    print_summary("AFTER (native + fastapi-overlay-prototype)", &after);

    if !after.warnings.is_empty() {
        println!("--- after.warnings: {} ---", after.warnings.len());
        for w in &after.warnings {
            println!("  {w}");
        }
    }

    let before_provides = before.ir.ir.io.as_ref().map_or(0, |io| io.provides.len());
    let after_io = after.ir.ir.io.as_ref();
    let after_provides = after_io.map_or(0, |io| io.provides.len());
    println!("provides before={before_provides} after={after_provides}");

    println!("--- sample provide keys after (up to 10) ---");
    if let Some(io) = after_io {
        for p in io.provides.iter().take(10) {
            println!("  {} [{}:{}]", p.key, p.file, p.line);
        }
    }
}

fn print_summary(label: &str, out: &zzop_engine::AnalyzeOutput) {
    let (provides, consumes) = out
        .ir
        .ir
        .io
        .as_ref()
        .map_or((0, 0), |io| (io.provides.len(), io.consumes.len()));
    println!(
        "{label}: files={} provides={} consumes={}",
        out.file_count, provides, consumes
    );
}

// ---------------------------------------------------------------------------------------------
// Adapter: lexical .py scan -> NormalizedEnvelope
// ---------------------------------------------------------------------------------------------

/// One file's accumulated router-mount state while scanning, keyed by the local binding identifier
/// (`router`, `api_router`, `app`, ...) — mirrors `RouterMountFragment` as a mutable builder, plus
/// this file's own creation-prefix map for the adapter-side join described in the module doc.
#[derive(Default)]
struct FileScan {
    fragments: HashMap<String, Vec<RouterMountEntry>>,
    /// ident -> its `APIRouter(prefix="...")` creation-time prefix (empty if none was given).
    creation_prefix: HashMap<String, String>,
    /// local import alias -> dotted module path it came from (the `from X import a, b` module `X`,
    /// not `X.a`); resolved further at mount-resolution time.
    imports: HashMap<String, String>,
}

fn build_overlay(scan_root: &Path, scan_prefix: &str) -> NormalizedEnvelope {
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
            path: full_path,
            loc,
            symbols: Vec::new(),
            imports: zzop_core::ImportMap::new(),
            re_exports: Vec::new(),
            dynamic_imports: Vec::new(),
            used_names: Vec::new(),
            const_map_fragment: HashMap::new(),
            trpc_router_fragments: Vec::new(),
            router_mount_fragments,
            io: IoFacts::default(),
            degraded: false,
            is_entry: false,
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

/// Converts a dotted Python module path (e.g. `"app.api.routes.items"`) into a `path` relative to
/// the tree root passed to `analyze_tree`: strips the leading `app` package-root segment (the
/// scanned directory itself is that package) and re-prefixes with `scan_prefix`.
fn dotted_to_rel_path(dotted: &str, scan_prefix: &str) -> String {
    let mut parts: Vec<&str> = dotted.split('.').collect();
    if parts.first() == Some(&"app") {
        parts.remove(0);
    }
    let joined = parts.join("/");
    if scan_prefix.is_empty() {
        format!("{joined}.py")
    } else {
        format!("{scan_prefix}/{joined}.py")
    }
}

fn scan_file(
    full_path: &str,
    text: &str,
    config_consts: &HashMap<String, String>,
    scan_prefix: &str,
) -> FileScan {
    let mut scan = FileScan::default();
    let lines: Vec<&str> = text.lines().collect();

    // Pass 1: imports (`from X import a, b, c`, multi-line-parenthesized form included).
    let mut i = 0;
    while i < lines.len() {
        if let Some(rest) = strip_from_import(lines[i]) {
            let (module, names_part, consumed) = if rest.trim_start().starts_with('(') {
                let mut joined = rest.to_string();
                let mut j = i;
                while !joined.contains(')') && j + 1 < lines.len() {
                    j += 1;
                    joined.push(' ');
                    joined.push_str(lines[j]);
                }
                let module = current_from_module(lines[i]);
                (module, joined, j - i)
            } else {
                (current_from_module(lines[i]), rest.to_string(), 0)
            };
            if let Some(module) = module {
                for raw_name in names_part.trim_matches(|c| c == '(' || c == ')').split(',') {
                    let name = raw_name.trim();
                    if name.is_empty() {
                        continue;
                    }
                    let local = name.split_whitespace().last().unwrap_or(name);
                    scan.imports.insert(local.to_string(), module.clone());
                }
            }
            i += consumed;
        }
        i += 1;
    }

    // Pass 2: router/app creation + verb decorators + mounts.
    let re_create = regex::Regex::new(r"^\s*(\w+)\s*=\s*(?:APIRouter|FastAPI)\(").unwrap();
    let re_verb = regex::Regex::new(r"^\s*@(\w+)\.(get|post|put|patch|delete)\(").unwrap();
    let re_mount = regex::Regex::new(r"(\w+)\.include_router\(").unwrap();
    let re_prefix_literal = regex::Regex::new(r#"prefix\s*=\s*"([^"]*)""#).unwrap();
    let re_prefix_expr = regex::Regex::new(r"prefix\s*=\s*([\w.]+)").unwrap();
    let re_first_string = regex::Regex::new(r#""([^"]*)""#).unwrap();
    let re_def = regex::Regex::new(r"^\s*(?:async\s+)?def\s+(\w+)").unwrap();

    for (idx, line) in lines.iter().enumerate() {
        if let Some(caps) = re_create.captures(line) {
            let ident = caps[1].to_string();
            let joined = join_balanced_call(&lines, idx);
            let prefix = re_prefix_literal
                .captures(&joined)
                .map(|c| c[1].to_string())
                .unwrap_or_default();
            scan.creation_prefix.insert(ident.clone(), prefix);
            scan.fragments.entry(ident).or_default();
            continue;
        }

        if let Some(caps) = re_verb.captures(line) {
            let ident = caps[1].to_string();
            let method = caps[2].to_uppercase();
            // Path literal: same line if present, else scan forward to where the call closes.
            let path_snippet = if line.contains('"') {
                line.to_string()
            } else {
                join_balanced_call(&lines, idx)
            };
            let raw_path = re_first_string
                .captures(&path_snippet)
                .map(|c| c[1].to_string())
                .unwrap_or_else(|| "/".to_string());

            let mut handler = None;
            for l in lines.iter().skip(idx + 1) {
                if let Some(c) = re_def.captures(l) {
                    handler = Some(c[1].to_string());
                    break;
                }
            }

            let prefix = scan
                .creation_prefix
                .get(&ident)
                .cloned()
                .unwrap_or_default();
            let full_path = join_path(&prefix, &raw_path);

            scan.fragments
                .entry(ident)
                .or_default()
                .push(RouterMountEntry::Verb {
                    method,
                    path: full_path,
                    handler,
                    line: (idx + 1) as u32,
                });
            continue;
        }

        if let Some(caps) = re_mount.captures(line) {
            let receiver = caps[1].to_string();
            let joined = join_balanced_call(&lines, idx);

            let Some(first_arg) = first_positional_arg(&joined) else {
                continue;
            };

            let (target_ident, module_dotted) =
                if let Some((module_alias, attr)) = first_arg.rsplit_once('.') {
                    // `items.router` — module_alias is imported as a submodule.
                    let dotted = scan
                        .imports
                        .get(module_alias)
                        .map(|m| format!("{m}.{module_alias}"));
                    (attr.to_string(), dotted)
                } else {
                    // Bare `api_router` — imported directly as the object itself.
                    let dotted = scan.imports.get(&first_arg).cloned();
                    (first_arg.clone(), dotted)
                };

            let specifier = module_dotted.map(|d| dotted_to_rel_path(&d, scan_prefix));
            if specifier.is_none() {
                eprintln!(
                    "adapter: {full_path}:{}: could not resolve mount target '{first_arg}' \
                     (no matching import) — emitting without a specifier",
                    idx + 1
                );
            }

            let prefix = if let Some(caps) = re_prefix_literal.captures(&joined) {
                caps[1].to_string()
            } else if let Some(caps) = re_prefix_expr.captures(&joined) {
                let expr = &caps[1];
                let const_name = expr.rsplit('.').next().unwrap_or(expr);
                match config_consts.get(const_name) {
                    Some(v) => v.clone(),
                    None => {
                        eprintln!(
                            "adapter: {full_path}:{}: non-literal prefix '{expr}' did not \
                             resolve via one-hop config-constant lookup — degrading to \"/\"",
                            idx + 1
                        );
                        "/".to_string()
                    }
                }
            } else {
                "/".to_string()
            };

            scan.fragments
                .entry(receiver)
                .or_default()
                .push(RouterMountEntry::Mount {
                    prefix,
                    ident: target_ident,
                    specifier,
                });
        }
    }

    scan
}

/// `from <module> import ...` — returns everything after `import ` on this line, or `None` if this
/// line is not such a statement.
fn strip_from_import(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("from ") {
        return None;
    }
    let idx = trimmed.find(" import ")?;
    Some(&trimmed[idx + " import ".len()..])
}

fn current_from_module(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("from ")?;
    let module = rest.split(" import").next()?.trim();
    Some(module.to_string())
}

/// Joins `lines[start..]` until parenthesis depth (from the first `(` on `lines[start]`) returns to
/// 0 — a small "balanced call" reader for decorator/constructor/method calls spanning multiple lines.
fn join_balanced_call(lines: &[&str], start: usize) -> String {
    let mut depth: i32 = 0;
    let mut started = false;
    let mut out = String::new();
    for line in lines.iter().skip(start) {
        for ch in line.chars() {
            if ch == '(' {
                depth += 1;
                started = true;
            } else if ch == ')' {
                depth -= 1;
            }
        }
        out.push_str(line);
        out.push('\n');
        if started && depth <= 0 {
            break;
        }
    }
    out
}

/// Extracts the first positional argument of an `include_router(...)` call already joined by
/// [`join_balanced_call`]: text up to the first top-level comma or the closing paren, trimmed.
fn first_positional_arg(joined: &str) -> Option<String> {
    let open = joined.find("include_router(")? + "include_router(".len();
    let mut depth: i32 = 1;
    let mut end = None;
    let bytes = &joined[open..];
    for (i, ch) in bytes.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            ',' if depth == 1 => {
                end = Some(i);
                break;
            }
            _ => {}
        }
    }
    let end = end?;
    let arg = bytes[..end].trim();
    if arg.is_empty() {
        None
    } else {
        Some(arg.to_string())
    }
}

/// Joins a router's creation prefix with one of its own verb paths — same semantics as the engine's
/// `compose_router_mount_provides::join_prefix` (trims a redundant `/` at the seam; `path == "/"`
/// collapses onto the prefix alone).
fn join_path(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        return path.to_string();
    }
    if path == "/" || path.is_empty() {
        return prefix.to_string();
    }
    let base = prefix.trim_end_matches('/');
    if let Some(rest) = path.strip_prefix('/') {
        format!("{base}/{rest}")
    } else {
        format!("{base}/{path}")
    }
}
