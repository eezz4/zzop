//! Shared helpers for the manifest scans (`package_json_entries` / `tsconfig_scan`): filename
//! predicates, POSIX join/normalize, `exports`-field walkers, and the JSONC comment strip.

use std::sync::OnceLock;

use regex::Regex;

/// Filename pattern matching a `package.json` at any depth — a monorepo has one per package (see
/// `package_json_entries`'s own doc).
pub(super) fn is_package_json_path(rel: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(^|/)package\.json$").unwrap())
        .is_match(rel)
}

/// True for a whitespace-delimited token that looks like a relative source-file path (matched against
/// the whole token, never a mid-token substring) — deliberately conservative, preferring to miss an
/// obscure script invocation over treating an unrelated flag/argument as a path.
pub(super) fn looks_like_script_path_token(tok: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\S+\.(?:ts|js|mjs|cjs)$").unwrap())
        .is_match(tok)
}

/// POSIX dirname of a rel path, `package_json_entries`-flavored: `""` (not `resolve::dirname`'s `"."`) for
/// a root-level `package.json`, so it can feed `join_and_normalize` below as the join-identity element
/// without an accidental `"./"` hop.
pub(super) fn package_json_dir(rel: &str) -> &str {
    match rel.rfind('/') {
        Some(i) => &rel[..i],
        None => "",
    }
}

/// POSIX join + `.`/`..`-segment normalize — a small local reimplementation of
/// `zzop_parser_typescript::resolve`'s private `normalize`/dirname-join logic, sized to exactly what
/// `package_json_entries` needs (that module's helpers are private, not importable from here).
pub(super) fn join_and_normalize(dir: &str, candidate: &str) -> String {
    let joined = if dir.is_empty() {
        candidate.to_string()
    } else {
        format!("{dir}/{candidate}")
    };
    let mut stack: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                if matches!(stack.last(), Some(&s) if s != "..") {
                    stack.pop();
                } else {
                    stack.push("..");
                }
            }
            s => stack.push(s),
        }
    }
    stack.join("/")
}

/// Recursively collects every string leaf of `v` that looks like a relative path (`./`/`../`-prefixed)
/// — the `exports` field walker: handles a single string, a conditional map, a subpath map, and
/// arbitrary nesting of the two. Only string values are collected, never object keys (subpath/condition
/// names), and the prefix filter excludes non-path values like a bare package specifier.
pub(super) fn collect_export_path_strings(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(s) => {
            if s.starts_with("./") || s.starts_with("../") {
                out.push(s.clone());
            }
        }
        serde_json::Value::Object(map) => {
            for val in map.values() {
                collect_export_path_strings(val, out);
            }
        }
        _ => {}
    }
}

/// The `exports` field's own `"."` (package-root) entry — unlike `collect_export_path_strings` (which
/// gathers every leaf including named sub-paths), a workspace bare-specifier import resolves only via
/// the `"."` condition (or `exports` being a bare string/condition-map, Node's shorthand for `{".":
/// ...}`). An `exports` map keyed entirely by sub-paths has no root entry; this conservatively falls
/// back to treating the whole object as a condition-map in that case.
pub(super) fn collect_exports_dot_entry(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(_) => collect_export_path_strings(v, out),
        serde_json::Value::Object(map) => match map.get(".") {
            Some(dot) => collect_export_path_strings(dot, out),
            None => collect_export_path_strings(v, out),
        },
        _ => {}
    }
}

/// Filename pattern matching a `tsconfig.json` at any depth — only this literal name is auto-discovered
/// (mirrors real `tsc` project discovery); an `extends` target is read only when referenced.
pub(super) fn is_tsconfig_json_path(rel: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(^|/)tsconfig\.json$").unwrap())
        .is_match(rel)
}

/// Strips `//` line comments and `/* ... */` block comments from `input`, respecting string literals
/// (a comment marker inside a JSON string is left alone). tsconfig.json commonly ships JSONC, which
/// `serde_json` rejects outright; this plus the trailing-comma strip in `parse_raw_tsconfig` is a small
/// tolerant preprocessor sized to real-world tsconfigs, not a general JSONC/JSON5 parser.
pub(super) fn strip_jsonc_comments(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut in_string = false;
    let mut escape = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}
