//! `trees: "auto"` workspace expansion — the port of `packages/cli/lib/workspaces.js`'s
//! `expandAutoTrees`. Only activates when `trees` is EXACTLY the string `"auto"`; every other shape
//! passes through untouched. The JS algorithm is hand-rolled and must be ported as-is, not replaced
//! with a general glob/YAML crate (a "smarter" matcher would change which packages get discovered):
//! - Manifest precedence: `pnpm-workspace.yaml` first (block list or inline flow list forms only),
//!   else `package.json` `workspaces` (array or `{packages: [...]}`); NEITHER present → a
//!   `ConfigError` telling the user to write an explicit `trees` array — never a silent single-tree
//!   fallback.
//! - Glob expansion: segment-by-segment against real directories, `*`/`?`/`**` (depth cap 40),
//!   `node_modules` and `.git` never descended, `!`-negatives applied as a whole-path anchored
//!   filter; a match is kept only if it contains a `package.json`; results sorted alphabetically
//!   (the determinism guarantee).
//! - `sourceId` = the package's own `name` field, else the relative dir; duplicate sourceIds are a
//!   WARNING (cross-source joins key on sourceId), not an error.
//! - Always emits an informational expansion warning; an extra one when only 1 tree resulted (the
//!   cross-layer join needs 2+).

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use serde_json::{json, Value};

use crate::ConfigError;

/// Directories never descended into while expanding a `**` glob, and never returned as workspace
/// packages: scanning them is both wasteful and wrong for workspace detection.
const SKIP_DIRS: [&str; 2] = ["node_modules", ".git"];

/// Hard cap on `**` recursion depth — a backstop against a pathological symlink cycle or an
/// absurdly deep tree. Far below any real monorepo nesting.
const MAX_GLOB_DEPTH: u32 = 40;

/// Expands `trees: "auto"` in `config` against `base_dir` (the config file's directory — the JS CLI
/// passes cwd, which is the same directory in normal CLI use). Returns the possibly-rewritten config
/// plus the expansion warnings. A config without `trees: "auto"` comes back unchanged.
pub fn expand_auto_trees(
    config: serde_json::Value,
    base_dir: &Path,
) -> Result<(serde_json::Value, Vec<String>), ConfigError> {
    let trees_is_auto = config
        .as_object()
        .and_then(|m| m.get("trees"))
        .and_then(Value::as_str)
        == Some("auto");
    if !trees_is_auto {
        return Ok((config, Vec::new()));
    }

    // `trees_is_auto` can only be true when `config.as_object()` above succeeded.
    let mut map = match config {
        Value::Object(m) => m,
        _ => unreachable!("trees_is_auto implies config is a JSON object"),
    };

    let (patterns, source): (Vec<String>, &str) = if let Some(p) =
        read_pnpm_workspace_packages(base_dir)
    {
        (p, "pnpm-workspace.yaml")
    } else if let Some(p) = read_npm_workspace_packages(base_dir) {
        (p, "package.json \"workspaces\"")
    } else {
        return Err(ConfigError(format!(
            "trees: \"auto\" found no workspace manifest in {} — expected a pnpm-workspace.yaml with a \"packages:\" list, or a package.json with a \"workspaces\" field. Write an explicit \"trees\": [{{ \"root\": ..., \"sourceId\": ... }}] array instead, or run zzop from the workspace root.",
            base_dir.display()
        )));
    };

    let dirs = resolve_workspace_dirs(base_dir, &patterns);
    if dirs.is_empty() {
        let joined = patterns.join(", ");
        let patterns_display = if joined.is_empty() {
            "(none)"
        } else {
            joined.as_str()
        };
        return Err(ConfigError(format!(
            "trees: \"auto\" matched no package directories from {source} (patterns: {patterns_display}). Each pattern must resolve to directories containing a package.json. Write an explicit \"trees\" array instead."
        )));
    }

    let mut warnings = Vec::new();
    let mut seen_source: HashMap<String, String> = HashMap::new();
    let mut trees: Vec<(String, String)> = Vec::with_capacity(dirs.len());
    for rel in &dirs {
        let name = read_package_name(&base_dir.join(rel));
        let source_id = name.unwrap_or_else(|| rel.clone());
        if let Some(prev_root) = seen_source.get(&source_id) {
            warnings.push(format!(
                "trees: \"auto\" derived a duplicate sourceId \"{source_id}\" for both \"{prev_root}\" and \"{rel}\". Cross-source joins key on sourceId; give one package a distinct \"name\" or use an explicit \"trees\" array to disambiguate."
            ));
        } else {
            seen_source.insert(source_id.clone(), rel.clone());
        }
        trees.push((rel.clone(), source_id));
    }

    let tree_desc = trees
        .iter()
        .map(|(root, source_id)| format!("{source_id} ({root})"))
        .collect::<Vec<_>>()
        .join(", ");
    warnings.push(format!(
        "trees: \"auto\" expanded to {} tree(s) from {source}: {tree_desc}.",
        trees.len()
    ));
    if trees.len() == 1 {
        warnings.push(
            "trees: \"auto\" resolved only one workspace package — the cross-layer join needs >= 2 trees with distinct sourceIds to fire, so this run behaves like a single-tree analysis."
                .to_string(),
        );
    }

    let trees_json: Vec<Value> = trees
        .into_iter()
        .map(|(root, source_id)| json!({ "root": root, "sourceId": source_id }))
        .collect();
    map.insert("trees".to_string(), Value::Array(trees_json));

    Ok((Value::Object(map), warnings))
}

// --- manifest readers ----------------------------------------------------------------------

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
fn read_pnpm_workspace_packages(base_dir: &Path) -> Option<Vec<String>> {
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
fn read_npm_workspace_packages(base_dir: &Path) -> Option<Vec<String>> {
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
fn read_package_name(pkg_dir_abs: &Path) -> Option<String> {
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

// --- glob expansion (positive patterns -> real directories) --------------------------------

/// Resolve workspace-package glob patterns to the set of relative directories that both match a
/// positive pattern, survive every `!`-negated pattern, and contain a `package.json`.
/// Deterministically sorted.
fn resolve_workspace_dirs(base_dir: &Path, patterns: &[String]) -> Vec<String> {
    let mut positives: Vec<String> = Vec::new();
    let mut negatives: Vec<String> = Vec::new();
    for raw in patterns {
        let p = raw.trim();
        if p.is_empty() {
            continue;
        }
        if let Some(neg) = p.strip_prefix('!') {
            negatives.push(neg.to_string());
        } else {
            positives.push(p.to_string());
        }
    }

    let mut matched: BTreeSet<String> = BTreeSet::new();
    for pattern in &positives {
        let segments = split_pattern(pattern);
        for rel in expand_segments(base_dir, "", &segments, 0) {
            if !rel.is_empty() {
                matched.insert(rel);
            }
        }
    }

    let neg_ops: Vec<Vec<GlobOp>> = negatives
        .iter()
        .map(|p| compile_negation_pattern(p))
        .collect();
    let mut kept: Vec<String> = Vec::new();
    for rel in &matched {
        if neg_ops.iter().any(|ops| glob_ops_match(ops, rel)) {
            continue;
        }
        if base_dir.join(rel).join("package.json").exists() {
            kept.push(rel.clone());
        }
    }
    kept.sort();
    kept
}

fn split_pattern(pattern: &str) -> Vec<String> {
    pattern
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .map(str::to_string)
        .collect()
}

/// List immediate subdirectory names of `abs_dir`, excluding `SKIP_DIRS`. Returns `[]` for a
/// missing/unreadable directory (a glob pattern pointing at a non-existent path simply matches
/// nothing). Does not follow symlinks (mirrors Node's `Dirent.isDirectory()`, which reflects the raw
/// directory-entry type, not the symlink target).
fn list_subdirs(abs_dir: &Path) -> Vec<String> {
    let entries = match std::fs::read_dir(abs_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        if !is_dir {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        dirs.push(name);
    }
    dirs
}

/// Expand one glob pattern's remaining `segments` from `current_rel` (a "/"-joined relative dir
/// under `base_dir`) into every matching relative directory path. `**` matches zero or more
/// directory levels. Depth-guarded by `MAX_GLOB_DEPTH`.
fn expand_segments(
    base_dir: &Path,
    current_rel: &str,
    segments: &[String],
    depth: u32,
) -> Vec<String> {
    if segments.is_empty() {
        return vec![current_rel.to_string()];
    }
    if depth > MAX_GLOB_DEPTH {
        return Vec::new();
    }

    let seg = &segments[0];
    let rest = &segments[1..];
    let current_abs = if current_rel.is_empty() {
        base_dir.to_path_buf()
    } else {
        base_dir.join(current_rel)
    };
    let mut results = Vec::new();

    if seg == "**" {
        // Zero levels: apply the rest right here. One-or-more: recurse into each subdir keeping `**`.
        results.extend(expand_segments(base_dir, current_rel, rest, depth));
        for sub in list_subdirs(&current_abs) {
            let next_rel = join_rel(current_rel, &sub);
            results.extend(expand_segments(base_dir, &next_rel, segments, depth + 1));
        }
        return results;
    }

    let is_wild = seg.contains('*') || seg.contains('?');
    for sub in list_subdirs(&current_abs) {
        let is_match = if is_wild {
            segment_matches(seg, &sub)
        } else {
            sub == seg.as_str()
        };
        if is_match {
            let next_rel = join_rel(current_rel, &sub);
            results.extend(expand_segments(base_dir, &next_rel, rest, depth + 1));
        }
    }
    results
}

fn join_rel(current_rel: &str, sub: &str) -> String {
    if current_rel.is_empty() {
        sub.to_string()
    } else {
        format!("{current_rel}/{sub}")
    }
}

// --- glob matching engine (shared by segment wildcards and whole-path negations) ------------
//
// A tiny hand-rolled matcher standing in for the JS source's `new RegExp(...)` calls: this crate has
// no regex dependency, and the grammar these patterns ever produce (literal chars, `[^/]*`, `[^/]`,
// and — for negations only — an optional `(?:/.*)?` group per interior `**` segment) is small enough
// to match directly without building real regex text. (The `.*` inside that optional group is
// modeled as "any char", not JS's "any char but a line terminator" — real directory names never
// contain newlines, so the two are equivalent for this use.)

enum GlobOp {
    Char(char),
    AnyNonSlash,
    AnyNonSlashRun,
    /// An interior `**` path segment: matches nothing, or a `/` followed by any run of characters
    /// (which may itself contain `/`). Mirrors `(?:/.*)?` in the JS source's whole-path regex.
    OptionalSlashAny,
}

fn compile_segment(seg: &str) -> Vec<GlobOp> {
    seg.chars()
        .map(|c| match c {
            '*' => GlobOp::AnyNonSlashRun,
            '?' => GlobOp::AnyNonSlash,
            other => GlobOp::Char(other),
        })
        .collect()
}

fn segment_matches(pattern: &str, name: &str) -> bool {
    glob_ops_match(&compile_segment(pattern), name)
}

/// Compiles a WHOLE glob path (may contain `/` and `**`) for negation matching. `**` matches across
/// directory separators only when it is an entire segment by itself; `*`/`?` do not cross `/`.
///
/// Faithfully reproduces a quirk of the ported JS `globToFullRegExp`: a **leading** `**` segment
/// (i.e. the pattern's very first segment) contributes nothing at all — including no separator — so
/// the segment that follows it still expects a literal leading `/` before its own content, which no
/// relative path ever has. A pattern like `!**/examples/**` therefore never matches anything; this
/// was verified against the JS source directly and is carried over as-is per the port mandate (see
/// module doc), not "fixed".
fn compile_negation_pattern(pattern: &str) -> Vec<GlobOp> {
    let segs = split_pattern(pattern);
    let mut ops: Vec<GlobOp> = Vec::new();
    for (i, seg) in segs.iter().enumerate() {
        if seg == "**" {
            if i > 0 {
                ops.push(GlobOp::OptionalSlashAny);
            }
        } else {
            if i > 0 {
                ops.push(GlobOp::Char('/'));
            }
            ops.extend(compile_segment(seg));
        }
    }
    ops
}

fn glob_ops_match(ops: &[GlobOp], candidate: &str) -> bool {
    let chars: Vec<char> = candidate.chars().collect();
    match_ops_at(ops, 0, &chars, 0)
}

fn match_ops_at(ops: &[GlobOp], oi: usize, s: &[char], si: usize) -> bool {
    if oi == ops.len() {
        return si == s.len();
    }
    match &ops[oi] {
        GlobOp::Char(c) => si < s.len() && s[si] == *c && match_ops_at(ops, oi + 1, s, si + 1),
        GlobOp::AnyNonSlash => si < s.len() && s[si] != '/' && match_ops_at(ops, oi + 1, s, si + 1),
        GlobOp::AnyNonSlashRun => {
            let mut end = si;
            while end < s.len() && s[end] != '/' {
                end += 1;
            }
            (si..=end).rev().any(|k| match_ops_at(ops, oi + 1, s, k))
        }
        GlobOp::OptionalSlashAny => {
            if match_ops_at(ops, oi + 1, s, si) {
                return true;
            }
            si < s.len()
                && s[si] == '/'
                && (si + 1..=s.len())
                    .rev()
                    .any(|k| match_ops_at(ops, oi + 1, s, k))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    fn pkg_json(name: Option<&str>) -> String {
        match name {
            Some(n) => format!(r#"{{"name": "{n}"}}"#),
            None => "{}".to_string(),
        }
    }

    fn auto_config() -> Value {
        json!({ "trees": "auto" })
    }

    // --- manifest precedence & minimal YAML forms -------------------------------------------

    #[test]
    fn pnpm_block_list_expands_to_matching_directories() {
        let dir = TempDir::new("zzop-ws-pnpm-block");
        dir.write(
            "pnpm-workspace.yaml",
            "packages:\n  - 'packages/*'\n  - 'apps/*'\n",
        );
        dir.write("packages/a/package.json", &pkg_json(Some("pkg-a")));
        dir.write("apps/x/package.json", &pkg_json(Some("app-x")));

        let (config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        assert_eq!(trees.len(), 2);
        let roots: Vec<&str> = trees.iter().map(|t| t["root"].as_str().unwrap()).collect();
        assert_eq!(roots, vec!["apps/x", "packages/a"]); // sorted
        assert!(warnings
            .iter()
            .any(|w| w.contains("expanded to 2 tree(s) from pnpm-workspace.yaml")));
    }

    #[test]
    fn pnpm_inline_flow_list_is_parsed() {
        let dir = TempDir::new("zzop-ws-pnpm-flow");
        dir.write("pnpm-workspace.yaml", "packages: ['pkgs/*']\n");
        dir.write("pkgs/one/package.json", &pkg_json(Some("one")));

        let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        assert_eq!(trees.len(), 1);
        assert_eq!(trees[0]["root"].as_str().unwrap(), "pkgs/one");
        assert_eq!(trees[0]["sourceId"].as_str().unwrap(), "one");
    }

    #[test]
    fn pnpm_workspace_yaml_takes_precedence_over_package_json() {
        let dir = TempDir::new("zzop-ws-precedence");
        dir.write("pnpm-workspace.yaml", "packages:\n  - 'from-pnpm/*'\n");
        dir.write("package.json", r#"{"workspaces": ["from-npm/*"]}"#);
        dir.write("from-pnpm/a/package.json", &pkg_json(Some("a")));
        dir.write("from-npm/b/package.json", &pkg_json(Some("b")));

        let (config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        assert_eq!(trees.len(), 1);
        assert_eq!(trees[0]["root"].as_str().unwrap(), "from-pnpm/a");
        assert!(warnings
            .iter()
            .any(|w| w.contains("from pnpm-workspace.yaml")));
    }

    #[test]
    fn package_json_array_form_is_read_when_no_pnpm_manifest() {
        let dir = TempDir::new("zzop-ws-npm-array");
        dir.write("package.json", r#"{"workspaces": ["packages/*"]}"#);
        dir.write("packages/a/package.json", &pkg_json(Some("a")));

        let (config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        assert_eq!(trees.len(), 1);
        assert!(warnings
            .iter()
            .any(|w| w.contains("package.json \"workspaces\"")));
    }

    #[test]
    fn package_json_object_form_with_packages_field_is_read() {
        let dir = TempDir::new("zzop-ws-npm-object");
        dir.write(
            "package.json",
            r#"{"workspaces": {"packages": ["packages/*"]}}"#,
        );
        dir.write("packages/a/package.json", &pkg_json(Some("a")));

        let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        assert_eq!(trees.len(), 1);
        assert_eq!(trees[0]["root"].as_str().unwrap(), "packages/a");
    }

    // --- negation, recursion, skip-dirs -------------------------------------------------------

    #[test]
    fn negative_patterns_exclude_matching_directories() {
        let dir = TempDir::new("zzop-ws-negation");
        dir.write(
            "pnpm-workspace.yaml",
            "packages:\n  - 'packages/*'\n  - '!packages/legacy'\n",
        );
        dir.write("packages/a/package.json", &pkg_json(Some("a")));
        dir.write("packages/legacy/package.json", &pkg_json(Some("legacy")));

        let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        let roots: Vec<&str> = trees.iter().map(|t| t["root"].as_str().unwrap()).collect();
        assert_eq!(roots, vec!["packages/a"]);
    }

    #[test]
    fn double_star_recurses_and_skips_node_modules_and_git() {
        let dir = TempDir::new("zzop-ws-recursion");
        dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/**'\n");
        dir.write("packages/package.json", &pkg_json(Some("root-pkg")));
        dir.write(
            "packages/deep/nested/dir/package.json",
            &pkg_json(Some("deep-pkg")),
        );
        dir.write(
            "packages/node_modules/should-be-skipped/package.json",
            &pkg_json(Some("skip-me")),
        );
        dir.write(
            "packages/.git/fake/package.json",
            &pkg_json(Some("skip-me-too")),
        );

        let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        let roots: Vec<&str> = trees.iter().map(|t| t["root"].as_str().unwrap()).collect();
        assert_eq!(roots, vec!["packages", "packages/deep/nested/dir"]);
    }

    #[test]
    fn a_matched_directory_without_package_json_is_excluded() {
        let dir = TempDir::new("zzop-ws-requires-pkg-json");
        dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
        dir.write("packages/has-pkg/package.json", &pkg_json(Some("has-pkg")));
        dir.mkdir("packages/no-pkg");

        let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        let roots: Vec<&str> = trees.iter().map(|t| t["root"].as_str().unwrap()).collect();
        assert_eq!(roots, vec!["packages/has-pkg"]);
    }

    #[test]
    fn results_are_sorted_regardless_of_directory_creation_order() {
        let dir = TempDir::new("zzop-ws-sort");
        dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
        // Create out of alphabetical order on purpose.
        dir.write("packages/zeta/package.json", &pkg_json(Some("zeta")));
        dir.write("packages/alpha/package.json", &pkg_json(Some("alpha")));
        dir.write("packages/mid/package.json", &pkg_json(Some("mid")));

        let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        let roots: Vec<&str> = trees.iter().map(|t| t["root"].as_str().unwrap()).collect();
        assert_eq!(
            roots,
            vec!["packages/alpha", "packages/mid", "packages/zeta"]
        );
    }

    // --- errors --------------------------------------------------------------------------------

    #[test]
    fn no_workspace_manifest_is_a_config_error_with_the_exact_text() {
        let dir = TempDir::new("zzop-ws-no-manifest");
        let err = expand_auto_trees(auto_config(), dir.path()).unwrap_err();
        let expected = format!(
            "trees: \"auto\" found no workspace manifest in {} — expected a pnpm-workspace.yaml with a \"packages:\" list, or a package.json with a \"workspaces\" field. Write an explicit \"trees\": [{{ \"root\": ..., \"sourceId\": ... }}] array instead, or run zzop from the workspace root.",
            dir.path().display()
        );
        assert_eq!(err.0, expected);
    }

    #[test]
    fn no_matching_package_directories_is_a_config_error_with_the_exact_text() {
        let dir = TempDir::new("zzop-ws-no-match");
        dir.write("pnpm-workspace.yaml", "packages:\n  - 'nonexistent/*'\n");
        let err = expand_auto_trees(auto_config(), dir.path()).unwrap_err();
        assert_eq!(
            err.0,
            "trees: \"auto\" matched no package directories from pnpm-workspace.yaml (patterns: nonexistent/*). Each pattern must resolve to directories containing a package.json. Write an explicit \"trees\" array instead."
        );
    }

    #[test]
    fn no_matching_package_directories_falls_back_to_none_placeholder_with_empty_pattern_list() {
        let dir = TempDir::new("zzop-ws-no-match-empty");
        // A `packages:` key with no list items at all yields an empty pattern list.
        dir.write("pnpm-workspace.yaml", "packages:\n");
        let err = expand_auto_trees(auto_config(), dir.path()).unwrap_err();
        assert_eq!(
            err.0,
            "trees: \"auto\" matched no package directories from pnpm-workspace.yaml (patterns: (none)). Each pattern must resolve to directories containing a package.json. Write an explicit \"trees\" array instead."
        );
    }

    // --- sourceId derivation ---------------------------------------------------------------

    #[test]
    fn source_id_comes_from_package_json_name_when_present() {
        let dir = TempDir::new("zzop-ws-sourceid-name");
        dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
        dir.write("packages/a/package.json", &pkg_json(Some("my-pkg-name")));

        let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        assert_eq!(trees[0]["sourceId"].as_str().unwrap(), "my-pkg-name");
    }

    #[test]
    fn source_id_falls_back_to_relative_dir_when_name_is_absent() {
        let dir = TempDir::new("zzop-ws-sourceid-fallback");
        dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
        dir.write("packages/nameless/package.json", "{}");

        let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        assert_eq!(trees[0]["sourceId"].as_str().unwrap(), "packages/nameless");
    }

    #[test]
    fn duplicate_source_ids_produce_a_warning_not_an_error() {
        let dir = TempDir::new("zzop-ws-duplicate");
        dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
        dir.write("packages/a/package.json", &pkg_json(Some("dup")));
        dir.write("packages/b/package.json", &pkg_json(Some("dup")));

        let (config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let trees = config["trees"].as_array().unwrap();
        assert_eq!(trees.len(), 2);
        let expected_warning = "trees: \"auto\" derived a duplicate sourceId \"dup\" for both \"packages/a\" and \"packages/b\". Cross-source joins key on sourceId; give one package a distinct \"name\" or use an explicit \"trees\" array to disambiguate.";
        assert!(warnings.iter().any(|w| w == expected_warning));
    }

    #[test]
    fn single_resolved_tree_gets_an_extra_warning() {
        let dir = TempDir::new("zzop-ws-single-tree");
        dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
        dir.write("packages/only/package.json", &pkg_json(Some("only")));

        let (_config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        assert!(warnings.iter().any(|w| w.contains("expanded to 1 tree(s)")));
        assert!(warnings.iter().any(|w| {
            w == "trees: \"auto\" resolved only one workspace package — the cross-layer join needs >= 2 trees with distinct sourceIds to fire, so this run behaves like a single-tree analysis."
        }));
    }

    #[test]
    fn exact_expansion_warning_text_composition() {
        let dir = TempDir::new("zzop-ws-warning-text");
        dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
        dir.write("packages/a/package.json", &pkg_json(Some("name-a")));
        dir.write("packages/b/package.json", &pkg_json(Some("name-b")));

        let (_config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
        let expected = "trees: \"auto\" expanded to 2 tree(s) from pnpm-workspace.yaml: name-a (packages/a), name-b (packages/b).";
        assert!(warnings.iter().any(|w| w == expected));
    }

    // --- pass-through for non-"auto" configs ------------------------------------------------

    #[test]
    fn explicit_trees_array_passes_through_untouched() {
        let config = json!({ "trees": [{ "root": ".", "sourceId": "x" }] });
        let (out, warnings) =
            expand_auto_trees(config.clone(), Path::new("/nonexistent/zzop-test-base")).unwrap();
        assert_eq!(out, config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn config_without_trees_key_passes_through_untouched() {
        let config = json!({ "roots": ["."] });
        let (out, warnings) =
            expand_auto_trees(config.clone(), Path::new("/nonexistent/zzop-test-base")).unwrap();
        assert_eq!(out, config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn non_object_config_passes_through_untouched() {
        let null_config = Value::Null;
        let (out, warnings) = expand_auto_trees(
            null_config.clone(),
            Path::new("/nonexistent/zzop-test-base"),
        )
        .unwrap();
        assert_eq!(out, null_config);
        assert!(warnings.is_empty());

        let arr_config = json!(["trees", "auto"]);
        let (out2, warnings2) =
            expand_auto_trees(arr_config.clone(), Path::new("/nonexistent/zzop-test-base"))
                .unwrap();
        assert_eq!(out2, arr_config);
        assert!(warnings2.is_empty());
    }
}
