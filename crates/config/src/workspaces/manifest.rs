//! Manifest readers for `trees: "auto"` — pure move from `workspaces.rs` (300-line source cap).
//! Reads the workspace glob patterns from `pnpm-workspace.yaml` (minimal hand-rolled reader) or
//! `package.json` `workspaces`, plus a package's own `name` for sourceId derivation. See the
//! `workspaces` module doc for why this is a faithful JS port, not a general YAML/JSON parser.

use std::path::Path;

use serde_json::Value;

/// MINIMAL pnpm-workspace.yaml reader: returns the `packages:` glob list, or `None` if the file is
/// absent (or unreadable). Supports the two forms real pnpm workspaces use — a block list:
/// ```yaml
/// packages:
///   - 'packages/*'
///   - "apps/*"
/// ```
/// and an inline flow list: `packages: ['packages/*', 'apps/*']`. Comments (`#`) and blank lines are
/// ignored. This is NOT a general YAML parser; an exotic file that doesn't match these forms yields
/// an empty list (surfaced to the user as "no packages found", not a crash).
pub(super) fn read_pnpm_workspace_packages(base_dir: &Path) -> Option<Vec<String>> {
    let raw = std::fs::read_to_string(base_dir.join("pnpm-workspace.yaml")).ok()?;
    let mut patterns = Vec::new();
    let mut in_packages = false;
    for line in raw.lines() {
        let no_comment = strip_yaml_comment(line);

        // Top-level `packages:` key (no leading indentation).
        if let Some(inline) = match_packages_key(no_comment) {
            let inline = inline.trim();
            if let Some(rest) = inline.strip_prefix('[') {
                // Flow list on one line: packages: ['a', 'b']
                let inner = {
                    let trimmed_end = rest.trim_end();
                    trimmed_end.strip_suffix(']').unwrap_or(rest)
                };
                for part in inner.split(',') {
                    let v = unquote(part);
                    if !v.is_empty() {
                        patterns.push(v);
                    }
                }
                in_packages = false;
            } else {
                in_packages = true;
            }
            continue;
        }

        if !in_packages {
            continue;
        }

        // A block-list item: `  - 'packages/*'`
        if let Some(item) = match_block_item(no_comment) {
            let v = unquote(item);
            if !v.is_empty() {
                patterns.push(v);
            }
            continue;
        }

        // A blank/comment-only line stays inside the block; anything else (a new top-level key)
        // ends it.
        if no_comment.trim().is_empty() {
            continue;
        }
        if no_comment
            .chars()
            .next()
            .is_some_and(|c| !c.is_whitespace())
        {
            in_packages = false;
        }
    }
    Some(patterns)
}

/// npm/yarn workspace globs from a `package.json` `workspaces` field (array, or
/// `{ packages: [...] }`). Returns `None` if there is no package.json, it isn't valid JSON, or it has
/// no `workspaces` field in either supported shape.
pub(super) fn read_npm_workspace_packages(base_dir: &Path) -> Option<Vec<String>> {
    let raw = std::fs::read_to_string(base_dir.join("package.json")).ok()?;
    let pkg: Value = serde_json::from_str(&raw).ok()?;
    let ws = pkg.get("workspaces")?;
    if let Some(arr) = ws.as_array() {
        return Some(
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
        );
    }
    // `ws.get("packages")` safely yields `None` when `ws` isn't a JSON object.
    ws.get("packages")
        .and_then(Value::as_array)
        .map(|packages| {
            packages
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
}

/// Reads a workspace package's declared name from its package.json, or `None` when absent/unnamed —
/// the caller falls back to the relative directory path so a nameless package still gets a distinct
/// sourceId.
pub(super) fn read_package_name(pkg_dir_abs: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(pkg_dir_abs.join("package.json")).ok()?;
    let pkg: Value = serde_json::from_str(&raw).ok()?;
    pkg.get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Strips one layer of matching surrounding single/double quotes from a trimmed scalar.
fn unquote(s: &str) -> String {
    let t = s.trim();
    let chars: Vec<char> = t.chars().collect();
    if chars.len() >= 2 {
        let first = chars[0];
        let last = chars[chars.len() - 1];
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return chars[1..chars.len() - 1].iter().collect();
        }
    }
    t.to_string()
}

/// Strips a trailing `\s+#...` comment from a single line, mirroring the JS
/// `line.replace(/\s+#.*$/, '')` — a `#` is only treated as a comment start when directly preceded
/// by whitespace (so a `#` glued to non-whitespace content is left alone).
fn strip_yaml_comment(line: &str) -> &str {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if chars[i].1.is_whitespace() {
            let run_start_byte = chars[i].0;
            let mut j = i;
            while j < n && chars[j].1.is_whitespace() {
                j += 1;
            }
            if j < n && chars[j].1 == '#' {
                return &line[..run_start_byte];
            }
            i = j;
        } else {
            i += 1;
        }
    }
    line
}

/// Matches a top-level (unindented) `packages:` key, returning everything after the colon
/// (un-trimmed) — mirrors `/^packages\s*:(.*)$/`.
fn match_packages_key(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("packages")?;
    let rest = rest.trim_start();
    rest.strip_prefix(':')
}

/// Matches a YAML block-list item (`  - 'value'`), returning the trimmed value between the dash and
/// end of line — mirrors `/^\s*-\s*(.+?)\s*$/`, which requires at least one non-trailing-whitespace
/// char, so a dash with nothing but whitespace after it does not match.
fn match_block_item(line: &str) -> Option<&str> {
    let rest = line.trim_start();
    let rest = rest.strip_prefix('-')?;
    let rest = rest.trim_start();
    let trimmed = rest.trim_end();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
