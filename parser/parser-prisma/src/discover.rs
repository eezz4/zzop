//! find_prisma_schemas — generic schema.prisma discovery.

use std::fs;
use std::path::{Path, PathBuf};

use zzop_core::SchemaModel;

use crate::parse::parse_schema;

const SKIP_DIRS: [&str; 4] = ["node_modules", "dist", "build", "coverage"];

/// Collect + parse every `schema.prisma` under `app_dir` into a flat `SchemaModel` list. `domain` is set to the
/// directory name above the schema file when it sits under a `prisma/` folder (best-effort grouping), else `None`.
/// Convention-based (not a specific repo layout): finds the conventional `prisma/schema.prisma` and any other
/// `schema.prisma` under the tree (multi-file / domain-split schemas), skipping node_modules/dist/dot-dirs.
pub fn find_prisma_schemas(app_dir: &Path) -> Vec<SchemaModel> {
    let mut models = Vec::new();
    for file in walk_schema_files(app_dir) {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let rel = relative_slash_path(app_dir, &file);
        let domain = domain_of(&file);
        models.extend(parse_schema(&text, Some(&rel), domain.as_deref()));
    }
    models
}

fn walk_schema_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_schema_files_into(dir, &mut out);
    out
}

fn walk_schema_files_into(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) || name.starts_with('.') {
                continue;
            }
            walk_schema_files_into(&path, out);
        } else if path.is_file() && name == "schema.prisma" {
            out.push(path);
        }
    }
}

/// Domain hint: the dir name enclosing the `prisma/` folder (e.g. `.../domains/billing/prisma/schema.prisma` ->
/// "billing"); `None` when the schema is not under a `prisma/` dir.
fn domain_of(file: &Path) -> Option<String> {
    let parent = file.parent()?.file_name()?.to_str()?;
    if parent != "prisma" {
        return None;
    }
    let grandparent = file.parent()?.parent()?.file_name()?.to_str()?;
    if grandparent.is_empty() {
        None
    } else {
        Some(grandparent.to_string())
    }
}

fn relative_slash_path(base: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(base).unwrap_or(file);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
