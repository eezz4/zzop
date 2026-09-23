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

/// True when a `scripts` KEY is one of npm's own RUN-lifecycle keys — the ones `npm start` / `npm restart`
/// execute. A path token named by one of these is the package's RUN ENTRY (what this package IS when you
/// run it), not a tool that operates on the package, so `package_json_entries` routes it to
/// `PackageJsonScan::entry_paths` beside `main`/`bin`/`exports` rather than to `script_paths`.
///
/// ## Why this set, and why EXACT keys only (measured 2026-08-24, 820 manifests across both corpora)
/// The set is npm's documented lifecycle, not a taste list: `npm start` runs `prestart`, `start`,
/// `poststart`; `npm restart` runs the `restart` triple. Taking all six rather than `start` alone is the
/// same derivation — which of the three an author writes is not evidence about the file. The corpus
/// contains `start` only (10 sites); the other five are included because the ecosystem defines them as
/// the same invocation, not because a tree showed them.
///
/// EXACT match is the measured part. Prefix-matching `start*` would also admit grafana's `start:swagger`
/// (-> `scripts/webpack/webpack.swagger.ts`) and `start:rspack` (-> `scripts/rspack/rspack.dev.ts`), which
/// are genuinely build scripts; those colon-suffixed names are project convention that npm never runs as
/// `npm start`, so they stay demoted.
///
/// `serve` and `preview` were considered and REJECTED: zero sites in either corpus name a path token from
/// them, so admitting them would be speculation. Re-run the census before adding one — the key list is
/// only as good as the last measurement.
///
/// ## The residual, stated
/// This is a RESCUE, so it fails toward not demoting, which is the safe direction. It is still imperfect
/// in both directions and both were observed: grafana's `start` names `scripts/webpack/webpack.dev.ts`, a
/// real build script that is now rescued (over-rescue, harmless — it merely sorts where it always did);
/// and `corpus/x/xai-cookbook/.../webrtc/server` names its source only from `dev` while its `start` points
/// at an uncompiled `dist/index.js`, so that server stays demoted (under-rescue — the one shape this
/// mechanism cannot see, because the manifest's own run key names a file that does not exist).
pub(super) fn is_run_lifecycle_script_key(key: &str) -> bool {
    matches!(
        key,
        "prestart" | "start" | "poststart" | "prerestart" | "restart" | "postrestart"
    )
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

/// POSIX join + `.`/`..`-segment normalize, re-exported so the `pipeline` modules that already import
/// `super::manifest::join_and_normalize` keep one spelling. The implementation moved to
/// `zzop_core::posix_path` on 2026-09-07 (review ledger V93 ⑵) — `config_entries.rs` carried a
/// byte-identical second copy, and each was justified by the other original being a private helper.
pub(super) use zzop_core::posix_path::join_and_normalize;

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
/// (mirrors real `tsc` project discovery). A config reached by a LINK is read without being discovered,
/// and there are TWO such links, each followed one level: `extends` and `references` — so a
/// `tsconfig.app.json` never matches this pattern yet its `paths` still reach the map (`tsconfig_scan`
/// owns both links, and the heuristic the `references` one carries). `jsconfig.json` matches neither
/// this pattern nor any link, so a JS project's path mapping is unreachable — a gap, not a decision.
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
