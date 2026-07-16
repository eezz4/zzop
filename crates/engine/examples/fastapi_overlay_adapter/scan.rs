//! Per-file scan: `scan_file` walks one Python file's text and returns its accumulated
//! `FileScan` router-mount state, plus the small text-manipulation helpers it uses
//! (balanced-paren call joining, import-line parsing, path joining). Split out of `overlay.rs`
//! since scanning ONE file is a coherent unit on its own — `overlay::build_overlay` walks the
//! tree and calls `scan_file` once per file.

use std::collections::HashMap;

use zzop_core::RouterMountEntry;

use super::overlay::FileScan;

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

pub(super) fn scan_file(
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
                    attr_keys: vec![],
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
                    attr_keys: vec![],
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
