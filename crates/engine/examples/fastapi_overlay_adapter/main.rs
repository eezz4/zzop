//! End-to-end demo of the Mode B external-adapter injection path (`EngineConfig::adapter_overlays`)
//! against a FastAPI (Python) backend.
//!
//! NOTE: zzop now ships a native, full-AST Python parser (`zzop-parser-python-3`, ruff-based) that
//! extracts FastAPI route provides directly for the common literal shapes — decorator-based routes,
//! `APIRouter` literal prefixes, and cross-file `include_router` composition — with no overlay
//! required (see `docs/ARCHITECTURE.md`'s "Language support" section). This example remains the
//! Mode-B reference for what native v1 deliberately does not cover: non-literal `APIRouter` prefixes,
//! Flask/Django routes, and other custom/per-project conventions a native recognizer can't generalize.
//!
//! This example plays two roles at once:
//!  1. **External adapter author** — a lexical scanner over `.py` text (line/regex-based, no real
//!     Python AST) that recognizes FastAPI's router-registration idioms and projects them into the
//!     engine's `NormalizedEnvelope` / `FileProjection` fragment-channel contract (see
//!     `docs/NORMALIZED_AST.md`'s "Adapter overlays" section; shapes in `zzop_core::fragments`). This
//!     half lives in `overlay.rs` (tree-walk + per-file orchestration) and `scan.rs` (the per-file
//!     scan itself).
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

use std::path::PathBuf;

use zzop_core::RouterMountEntry;
use zzop_engine::{analyze_tree, EngineConfig};

mod overlay;
mod scan;

use overlay::build_overlay;

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
                // This adapter never emits a producer-judged attribute (`ScopedAttr` —
                // `express-middleware-v1`, a native-TypeScript-recognizer concern) — not counted
                // in either tally.
                RouterMountEntry::ScopedAttr { .. } => (v, m),
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
