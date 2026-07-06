//! Import specifier resolution + dep-graph projection: relative (`./` `../`) and `@/` alias specifiers
//! resolve to an internal file path; workspace-package specifiers (`WorkspacePkg`) resolve to that
//! package's entry/subpath file; specifiers matching a governing tsconfig's `paths`/`baseUrl`
//! (`TsconfigPaths`) resolve to the mapped target (tried before the `@/` and workspace fallbacks). Bare
//! npm packages and node builtins are external -> `None`, dropped from the dep graph.

use std::collections::{BTreeMap, HashMap, HashSet};

use zzop_core::{DepGraph, ImportMap, ReExport};

/// Extensions / index files tried in order when resolving a specifier base.
pub const RESOLVE_EXTS: &[&str] = &[
    "",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".mjs",
    ".cjs",
    "/index.ts",
    "/index.tsx",
    "/index.js",
    "/index.jsx",
];

/// Resolve a specifier to an internal file path within `all_paths`, or `None`. Relative (`.`/`..`) is
/// joined against `from_file`'s dir; `@/` is a repo-root alias; everything else is external -> `None`.
pub fn resolve_file(
    specifier: &str,
    from_file: &str,
    all_paths: &HashSet<String>,
) -> Option<String> {
    if specifier.starts_with('.') {
        let joined = normalize(&format!("{}/{}", dirname(from_file), specifier));
        return try_ext(&joined, all_paths);
    }
    if let Some(rest) = specifier.strip_prefix("@/") {
        // Root-relative first (tsconfig maps `@/*` to the analysis root), then `src/`-relative — the
        // dominant convention is `"@/*": ["./src/*"]`, so without this fallback every `@/` import
        // breaks and dead-exports/unreachable analysis misreports the whole `src/` tree as orphaned.
        // tsconfig `paths` isn't read here (yet); this covers the two conventional mappings, root first.
        return try_ext(rest, all_paths).or_else(|| try_ext(&format!("src/{rest}"), all_paths));
    }
    None
}

/// NodeNext-style literal extension -> real TypeScript source extension(s): `.js`/`.mjs`/`.cjs` imports
/// commonly name compiled output while the real source is `.ts`/`.tsx`, `.mts`, or `.cts`.
const EXTENSION_FALLBACKS: &[(&str, &[&str])] = &[
    (".js", &[".ts", ".tsx"]),
    (".mjs", &[".mts"]),
    (".cjs", &[".cts"]),
];

/// Try each extension/index suffix against `all_paths` (see `EXTENSION_FALLBACKS` for the NodeNext
/// `.js`/`.mjs`/`.cjs` -> real-source fallback).
pub fn try_ext(base: &str, all_paths: &HashSet<String>) -> Option<String> {
    for ext in RESOLVE_EXTS {
        let candidate = format!("{base}{ext}");
        if all_paths.contains(&candidate) {
            return Some(candidate);
        }
        if ext.is_empty() {
            for (literal, reals) in EXTENSION_FALLBACKS {
                let Some(stem) = base.strip_suffix(literal) else {
                    continue;
                };
                for real in *reals {
                    let c = format!("{stem}{real}");
                    if all_paths.contains(&c) {
                        return Some(c);
                    }
                }
            }
        }
    }
    None
}

/// One directory's effective TypeScript path-mapping config: `compilerOptions.baseUrl` (POSIX dir
/// relative to the analysis root, `""` for root) and `compilerOptions.paths` (alias pattern -> ordered
/// target list, joined against `base_url` only at resolution time). Built from tsconfig.json (+ one
/// local `extends` level) by `zzop-engine`'s `pipeline::tsconfig_scan`; stays pure/filesystem-free.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TsconfigPaths {
    pub base_url: String,
    pub paths: BTreeMap<String, Vec<String>>,
}

/// POSIX dirname, `""`-for-root (matches `TsconfigPaths` keys) — distinct from `dirname` below, which
/// returns `"."` for a no-slash path.
fn dirname_posix(p: &str) -> String {
    match p.rfind('/') {
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

/// Nearest-ancestor lookup: walks up from `from_file`'s directory to the analysis root, returning the
/// first registered `TsconfigPaths` (mirrors how `tsc` finds the governing tsconfig).
fn governing_tsconfig<'a>(
    from_file: &str,
    tsconfigs: &'a BTreeMap<String, TsconfigPaths>,
) -> Option<&'a TsconfigPaths> {
    let mut dir = dirname_posix(from_file);
    loop {
        if let Some(cfg) = tsconfigs.get(&dir) {
            return Some(cfg);
        }
        if dir.is_empty() {
            return None;
        }
        dir = dirname_posix(&dir);
    }
}

/// Resolves `specifier` against one tsconfig's `paths` map, TS semantics: longest-matching-prefix wins;
/// within the winning pattern, targets are tried in declared order and the first that resolves via
/// `try_ext` wins. Returns `None` if no pattern matches or all targets fail — mirrors `tsc`, which never
/// falls back to a shorter-prefix pattern.
fn resolve_via_paths(
    specifier: &str,
    tsconfig: &TsconfigPaths,
    all_paths: &HashSet<String>,
) -> Option<String> {
    let mut candidates: Vec<(&str, &str, &Vec<String>)> = Vec::new();
    for (pattern, targets) in &tsconfig.paths {
        match pattern.find('*') {
            None => {
                if specifier == pattern.as_str() {
                    candidates.push((pattern.as_str(), "", targets));
                }
            }
            Some(star) => {
                let prefix = &pattern[..star];
                let suffix = &pattern[star + 1..];
                if specifier.len() >= prefix.len() + suffix.len()
                    && specifier.starts_with(prefix)
                    && specifier.ends_with(suffix)
                {
                    candidates.push((prefix, suffix, targets));
                }
            }
        }
    }
    // Longest prefix first; ties broken by `paths`' BTreeMap-deterministic alphabetical order.
    candidates.sort_by_key(|c| std::cmp::Reverse(c.0.len()));
    for (prefix, suffix, targets) in candidates {
        let captured = &specifier[prefix.len()..specifier.len() - suffix.len()];
        for target in targets {
            let filled = match target.find('*') {
                Some(i) => format!("{}{captured}{}", &target[..i], &target[i + 1..]),
                None => target.clone(),
            };
            let base = if tsconfig.base_url.is_empty() {
                filled
            } else {
                format!("{}/{filled}", tsconfig.base_url)
            };
            if let Some(hit) = try_ext(&normalize(&base), all_paths) {
                return Some(hit);
            }
        }
    }
    None
}

/// TS also resolves a non-relative specifier plainly against `baseUrl` (e.g. `'foo/bar'` ->
/// `<baseUrl>/foo/bar`), tried after any `paths` match. No separate "is baseUrl configured" guard
/// needed — an unconfigured directory is never registered.
fn resolve_via_base_url(
    specifier: &str,
    tsconfig: &TsconfigPaths,
    all_paths: &HashSet<String>,
) -> Option<String> {
    let base = if tsconfig.base_url.is_empty() {
        specifier.to_string()
    } else {
        format!("{}/{specifier}", tsconfig.base_url)
    };
    try_ext(&normalize(&base), all_paths)
}

/// A workspace (monorepo) package as seen by the import resolver, resolving both a bare `<name>` and a
/// `<name>/subpath` specifier. Built by `zzop-engine`'s `pipeline::package_json_entries` from `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePkg {
    /// The package's own directory (POSIX, relative to the analysis root; `""` for root-level) — resolves
    /// `<name>/subpath` specifiers via `try_ext` the same way a relative import resolves.
    pub dir: String,
    /// The package's resolved bare-specifier entry file (`main`/`module`/`exports["."]`, falling back to
    /// `index.ts`/`src/index.ts`). `None` if nothing resolves — `<name>/subpath` still resolves via `dir`.
    pub entry: Option<String>,
}

/// Checks whether an import specifier matches a workspace package name (exact or sub-path) against
/// `workspace_pkgs`' keys, generic over the value type. Examples:
///  - `@base/utils-fe`           -> `Some(("@base/utils-fe", None))`
///  - `@base/utils-fe/auth/hash` -> `Some(("@base/utils-fe", Some("auth/hash")))`
///  - `@/foo/bar`                -> `None` (`@/` is the path alias, handled separately)
///  - `react`                    -> `None`
pub fn match_workspace_pkg<'a, V>(
    specifier: &'a str,
    workspace_pkgs: &'a HashMap<String, V>,
) -> Option<(&'a str, Option<&'a str>)> {
    if let Some((k, _)) = workspace_pkgs.get_key_value(specifier) {
        return Some((k.as_str(), None));
    }
    // `@/foo` (no second '/') falls through below; unscoped packages (`lodash/fp`) use text after the first '/'.
    let (pkg_part, rest_start) = if !specifier.starts_with('@') {
        let slash = specifier.find('/')?;
        (&specifier[..slash], slash + 1)
    } else {
        let first_slash = specifier.find('/')?;
        let second_slash = specifier[first_slash + 1..]
            .find('/')
            .map(|i| i + first_slash + 1)?;
        (&specifier[..second_slash], second_slash + 1)
    };
    let (k, _) = workspace_pkgs.get_key_value(pkg_part)?;
    let rest = &specifier[rest_start..];
    Some((k.as_str(), (!rest.is_empty()).then_some(rest)))
}

/// `resolve_file`, aware of workspace packages and tsconfig `paths`/`baseUrl`. Non-relative specifiers
/// try, in order: the governing tsconfig's `paths`, then its `baseUrl`, then the `@/` convention, then
/// workspace packages; a relative specifier always resolves exactly as `resolve_file` (`paths` never
/// remaps one). A bare `<name>` resolves to `WorkspacePkg::entry`; `<name>/subpath` resolves to
/// `dir/subpath` via `try_ext`. Workspace packages win over a same-named npm dependency since
/// `all_paths` never contains `node_modules`.
pub fn resolve_file_with_workspace(
    specifier: &str,
    from_file: &str,
    all_paths: &HashSet<String>,
    workspace_pkgs: &HashMap<String, WorkspacePkg>,
    tsconfigs: &BTreeMap<String, TsconfigPaths>,
) -> Option<String> {
    if specifier.starts_with('.') {
        return resolve_file(specifier, from_file, all_paths);
    }
    if let Some(cfg) = governing_tsconfig(from_file, tsconfigs) {
        if let Some(hit) = resolve_via_paths(specifier, cfg, all_paths) {
            return Some(hit);
        }
        if let Some(hit) = resolve_via_base_url(specifier, cfg, all_paths) {
            return Some(hit);
        }
    }
    if specifier.starts_with("@/") {
        return resolve_file(specifier, from_file, all_paths);
    }
    let (pkg_name, subpath) = match_workspace_pkg(specifier, workspace_pkgs)?;
    let pkg = &workspace_pkgs[pkg_name];
    match subpath {
        None => pkg.entry.clone(),
        Some(sub) => {
            let base = if pkg.dir.is_empty() {
                sub.to_string()
            } else {
                format!("{}/{sub}", pkg.dir)
            };
            try_ext(&base, all_paths)
        }
    }
}

/// Shared implementation behind `build_dep`/`build_dep_with_workspace`. Per file: resolves each
/// non-deferred import binding via `resolve`, keeping deduped internal edges (external/deferred
/// specifiers excluded) — feeding circular detection and fan-in/out. Also merges in each file's
/// non-type-only re-export specifiers (`export {x} from './y'` / `export * from './y'`) as the same kind
/// of edge, resolved+deduped into the same vector (Defect A — a bare re-export used to be invisible to
/// the dep graph, undercounting a barrel file's fan-in and false-positiving `dead-candidates`); a
/// type-only re-export (`export type {X} from './y'`) is skipped entirely — erased by TS at compile time,
/// it is not a runtime edge at all, not even one excluded from circular only.
///
/// Separately (Defect B) returns an ephemeral `(from, to)` exclusion set: pairs where EVERY binding/
/// re-export contributing that edge is type-only (`import type`/per-specifier `{ type X }`). The returned
/// `DepGraph` still includes these edges (fan-in/dead-exports/every metric legitimately count a type
/// import as a "use" of the target); only `zzop_core::circular_from_dep_excluding` consults the exclusion
/// set, and only for the duration of one analysis run — it is never cached or serialized. A pair with
/// BOTH a type-only and a value binding to the same target is not excluded (a real runtime edge exists).
fn build_dep_impl<F>(
    files: &[(String, ImportMap)],
    re_exports: &[(String, Vec<ReExport>)],
    mut resolve: F,
) -> (DepGraph, HashSet<(String, String)>)
where
    F: FnMut(&str, &str) -> Option<String>,
{
    let re_export_map: HashMap<&str, &Vec<ReExport>> = re_exports
        .iter()
        .map(|(rel, rs)| (rel.as_str(), rs))
        .collect();
    let mut dep = DepGraph::new();
    let mut type_only_edges = HashSet::new();
    for (rel, imports) in files {
        let mut seen = HashSet::new();
        let mut resolved = Vec::new();
        // target -> true iff every binding/re-export resolving to it so far is type-only.
        let mut target_type_only: HashMap<String, bool> = HashMap::new();
        for binding in imports.values() {
            if binding.deferred {
                continue; // lazy require/import: no module-load edge
            }
            if let Some(target) = resolve(&binding.specifier, rel) {
                target_type_only
                    .entry(target.clone())
                    .and_modify(|all_type_only| *all_type_only &= binding.type_only)
                    .or_insert(binding.type_only);
                if seen.insert(target.clone()) {
                    resolved.push(target);
                }
            }
        }
        if let Some(res) = re_export_map.get(rel.as_str()) {
            for re in res.iter() {
                if re.type_only {
                    continue; // erased at compile time: not a runtime edge, not a cycle edge either
                }
                if let Some(target) = resolve(&re.specifier, rel) {
                    target_type_only.insert(target.clone(), false); // always a real (non-type-only) edge
                    if seen.insert(target.clone()) {
                        resolved.push(target);
                    }
                }
            }
        }
        for (target, all_type_only) in target_type_only {
            if all_type_only {
                type_only_edges.insert((rel.clone(), target));
            }
        }
        dep.insert(rel.clone(), resolved);
    }
    (dep, type_only_edges)
}

/// `build_dep`, aware of workspace packages and tsconfig `paths`/`baseUrl`: resolves each binding/
/// re-export via `resolve_file_with_workspace`. Behaviorally equivalent to `build_dep` when both maps are
/// empty. See `build_dep_impl`'s doc for the merged-re-export-edge/type-only-exclusion-set behavior both
/// `build_dep`/`build_dep_with_workspace` share.
pub fn build_dep_with_workspace(
    files: &[(String, ImportMap)],
    re_exports: &[(String, Vec<ReExport>)],
    all_paths: &HashSet<String>,
    workspace_pkgs: &HashMap<String, WorkspacePkg>,
    tsconfigs: &BTreeMap<String, TsconfigPaths>,
) -> (DepGraph, HashSet<(String, String)>) {
    build_dep_impl(files, re_exports, |specifier, rel| {
        resolve_file_with_workspace(specifier, rel, all_paths, workspace_pkgs, tsconfigs)
    })
}

/// Build a file-level dep graph: per file, resolve each non-deferred import (and non-type-only
/// re-export) and keep deduped internal edges (external/deferred/type-only-re-export specifiers
/// excluded), feeding circular detection and fan-in/out. See `build_dep_impl`'s doc for the full
/// re-export-merge/type-only-exclusion-set behavior (the second return value).
pub fn build_dep(
    files: &[(String, ImportMap)],
    re_exports: &[(String, Vec<ReExport>)],
    all_paths: &HashSet<String>,
) -> (DepGraph, HashSet<(String, String)>) {
    build_dep_impl(files, re_exports, |specifier, rel| {
        resolve_file(specifier, rel, all_paths)
    })
}

/// POSIX dirname: text before the last '/', or "." when there is no '/'.
fn dirname(p: &str) -> String {
    match p.rfind('/') {
        Some(i) => p[..i].to_string(),
        None => ".".to_string(),
    }
}

/// POSIX normalize: resolve "." and ".." segments (relative paths; leading ".." is preserved).
fn normalize(p: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for seg in p.split('/') {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_imports;

    fn paths(xs: &[&str]) -> HashSet<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn resolves_relative_to_ts() {
        let all = paths(&["features/x/bar.ts"]);
        assert_eq!(
            resolve_file("./bar", "features/x/useFoo.ts", &all).as_deref(),
            Some("features/x/bar.ts")
        );
    }

    #[test]
    fn resolves_index_file() {
        let all = paths(&["a/shared/index.ts"]);
        assert_eq!(
            resolve_file("./shared", "a/b.ts", &all).as_deref(),
            Some("a/shared/index.ts")
        );
    }

    #[test]
    fn maps_js_specifier_to_ts_source() {
        let all = paths(&["a/bar.ts"]);
        assert_eq!(
            resolve_file("./bar.js", "a/b.ts", &all).as_deref(),
            Some("a/bar.ts")
        );
    }

    #[test]
    fn maps_mjs_specifier_to_mts_source() {
        let all = paths(&["a/bar.mts"]);
        assert_eq!(
            resolve_file("./bar.mjs", "a/b.ts", &all).as_deref(),
            Some("a/bar.mts")
        );
    }

    #[test]
    fn maps_cjs_specifier_to_cts_source() {
        let all = paths(&["a/bar.cts"]);
        assert_eq!(
            resolve_file("./bar.cjs", "a/b.ts", &all).as_deref(),
            Some("a/bar.cts")
        );
    }

    #[test]
    fn barrel_index_js_specifier_reexport_chain_resolves_end_to_end() {
        // a.ts -> ./b/index.js (real: b/index.ts) -> ./c.js (real: c.ts), both hops using a literal
        // `.js` extension (NodeNext style), chained through `build_dep` to prove both hops resolve.
        // `b/index.ts` uses an import-then-local-export barrel (an `import` + a local, from-less
        // `export { c }`), not a bare `export { c } from './c.js'` re-export — that shape is covered
        // separately by `bare_named_re_export_creates_dep_edge` below, now that `build_dep` merges
        // `parse_re_exports` output into the graph too.
        let a = parse_imports("a.ts", "import { c } from './b/index.js';\n");
        let b = parse_imports(
            "b/index.ts",
            "import { c } from '../c.js';\nexport { c };\n",
        );
        let all = paths(&["a.ts", "b/index.ts", "c.ts"]);
        let (dep, _type_only) = build_dep(
            &[("a.ts".to_string(), a), ("b/index.ts".to_string(), b)],
            &[],
            &all,
        );
        assert_eq!(dep["a.ts"], vec!["b/index.ts".to_string()]);
        assert_eq!(dep["b/index.ts"], vec!["c.ts".to_string()]);
    }

    // --- Defect A: bare re-exports merge into the dep graph as runtime edges ---

    #[test]
    fn bare_named_re_export_creates_dep_edge() {
        // `export { x } from './b'` alone (no local import) used to be invisible to `build_dep` — the
        // whole point of Defect A's fix.
        let re_exports = vec![(
            "barrel.ts".to_string(),
            vec![zzop_core::ReExport {
                specifier: "./b".to_string(),
                original: "x".to_string(),
                local_alias: "x".to_string(),
                type_only: false,
            }],
        )];
        let all = paths(&["barrel.ts", "b.ts"]);
        let (dep, _type_only) = build_dep(
            &[("barrel.ts".to_string(), ImportMap::new())],
            &re_exports,
            &all,
        );
        assert_eq!(dep["barrel.ts"], vec!["b.ts".to_string()]);
    }

    #[test]
    fn bare_star_re_export_creates_dep_edge() {
        // `export * from './z'` alone — same fix, the star-form re-export.
        let re_exports = vec![(
            "barrel.ts".to_string(),
            vec![zzop_core::ReExport {
                specifier: "./z".to_string(),
                original: "*".to_string(),
                local_alias: "*".to_string(),
                type_only: false,
            }],
        )];
        let all = paths(&["barrel.ts", "z.ts"]);
        let (dep, _type_only) = build_dep(
            &[("barrel.ts".to_string(), ImportMap::new())],
            &re_exports,
            &all,
        );
        assert_eq!(dep["barrel.ts"], vec!["z.ts".to_string()]);
    }

    #[test]
    fn type_only_re_export_creates_no_dep_edge() {
        // `export type { X } from './y'` is erased by TS at compile time — no edge at all, not even one
        // excluded from circular only (unlike a type-only import binding — see Defect B tests below).
        let re_exports = vec![(
            "barrel.ts".to_string(),
            vec![zzop_core::ReExport {
                specifier: "./y".to_string(),
                original: "X".to_string(),
                local_alias: "X".to_string(),
                type_only: true,
            }],
        )];
        let all = paths(&["barrel.ts", "y.ts"]);
        let (dep, type_only_edges) = build_dep(
            &[("barrel.ts".to_string(), ImportMap::new())],
            &re_exports,
            &all,
        );
        assert!(dep["barrel.ts"].is_empty());
        assert!(type_only_edges.is_empty());
    }

    #[test]
    fn re_export_target_gains_fan_in_via_reverse_dep_edge() {
        // The dep-graph consumer side of Defect A: a barrel-only re-export gives its target an inbound
        // edge, which is exactly what `fan_in` (reverse-edge count, `analyze.rs`'s `dep_stats_from_dep`)
        // counts to avoid `dead-candidates` false-positiving a re-exported-only file.
        let re_exports = vec![(
            "barrel.ts".to_string(),
            vec![zzop_core::ReExport {
                specifier: "./impl".to_string(),
                original: "x".to_string(),
                local_alias: "x".to_string(),
                type_only: false,
            }],
        )];
        let all = paths(&["barrel.ts", "impl.ts"]);
        let (dep, _type_only) = build_dep(
            &[("barrel.ts".to_string(), ImportMap::new())],
            &re_exports,
            &all,
        );
        let fan_in = dep
            .values()
            .filter(|tos| tos.contains(&"impl.ts".to_string()))
            .count();
        assert_eq!(fan_in, 1);
    }

    #[test]
    fn resolves_at_alias() {
        let all = paths(&["features/x.ts"]);
        assert_eq!(
            resolve_file("@/features/x", "anywhere/deep.ts", &all).as_deref(),
            Some("features/x.ts")
        );
    }

    #[test]
    fn resolves_at_alias_through_src_fallback() {
        let all = paths(&["src/core/blocklist.ts"]);
        assert_eq!(
            resolve_file("@/core/blocklist", "src/background/recording.ts", &all).as_deref(),
            Some("src/core/blocklist.ts")
        );
    }

    #[test]
    fn at_alias_prefers_root_match_over_src_fallback() {
        let all = paths(&["features/x.ts", "src/features/x.ts"]);
        assert_eq!(
            resolve_file("@/features/x", "a/b.ts", &all).as_deref(),
            Some("features/x.ts")
        );
    }

    #[test]
    fn normalizes_parent_segments() {
        let all = paths(&["features/shared/y.ts"]);
        assert_eq!(
            resolve_file("../shared/y", "features/x/useFoo.ts", &all).as_deref(),
            Some("features/shared/y.ts")
        );
    }

    #[test]
    fn external_specifier_is_none() {
        assert_eq!(resolve_file("react", "a/b.ts", &paths(&["a/b.ts"])), None);
    }

    #[test]
    fn unresolvable_relative_is_none() {
        assert_eq!(resolve_file("./missing", "a/b.ts", &paths(&[])), None);
    }

    #[test]
    fn build_dep_keeps_internal_drops_external() {
        let imports = parse_imports("a.ts", "import { x } from './b';\nimport 'react';\n");
        let all = paths(&["a.ts", "b.ts"]);
        let (dep, _type_only) = build_dep(&[("a.ts".to_string(), imports)], &[], &all);
        assert_eq!(dep["a.ts"], vec!["b.ts".to_string()]);
    }

    #[test]
    fn build_dep_excludes_deferred() {
        use zzop_core::ImportBinding;
        let mut imports = ImportMap::new();
        imports.insert(
            "Y".to_string(),
            ImportBinding {
                specifier: "./y".into(),
                original: "*".into(),
                deferred: true,
                type_only: false,
            },
        );
        let all = paths(&["x.js", "y.ts"]);
        let (dep, _type_only) = build_dep(&[("x.js".to_string(), imports)], &[], &all);
        assert!(dep["x.js"].is_empty());
    }

    // --- Defect B: type-only bindings stay in the DepGraph, but excluded from circular only ---

    #[test]
    fn type_only_binding_stays_in_dep_graph_but_is_flagged_excludable() {
        use zzop_core::ImportBinding;
        let mut imports = ImportMap::new();
        imports.insert(
            "T".to_string(),
            ImportBinding {
                specifier: "./y".into(),
                original: "T".into(),
                deferred: false,
                type_only: true,
            },
        );
        let all = paths(&["x.ts", "y.ts"]);
        let (dep, type_only_edges) = build_dep(&[("x.ts".to_string(), imports)], &[], &all);
        // Fan-in/metrics still see the edge — a type import is still a "use" of the target.
        assert_eq!(dep["x.ts"], vec!["y.ts".to_string()]);
        // But circular detection's exclusion set flags it.
        assert!(type_only_edges.contains(&("x.ts".to_string(), "y.ts".to_string())));
    }

    #[test]
    fn value_and_type_only_binding_to_same_target_is_not_excluded() {
        // A real value import to the same target as a type-only one means a genuine runtime edge exists
        // — the pair must not be excluded from circular even though a type-only binding also targets it.
        use zzop_core::ImportBinding;
        let mut imports = ImportMap::new();
        imports.insert(
            "T".to_string(),
            ImportBinding {
                specifier: "./y".into(),
                original: "T".into(),
                deferred: false,
                type_only: true,
            },
        );
        imports.insert(
            "v".to_string(),
            ImportBinding {
                specifier: "./y".into(),
                original: "v".into(),
                deferred: false,
                type_only: false,
            },
        );
        let all = paths(&["x.ts", "y.ts"]);
        let (dep, type_only_edges) = build_dep(&[("x.ts".to_string(), imports)], &[], &all);
        assert_eq!(dep["x.ts"], vec!["y.ts".to_string()]);
        assert!(!type_only_edges.contains(&("x.ts".to_string(), "y.ts".to_string())));
    }

    #[test]
    fn import_type_only_pair_does_not_form_a_circular_dependency() {
        // Two files linked ONLY by `import type` (both directions) must not read as a cycle; a value
        // import between the same two files still must.
        use zzop_core::{circular_from_dep_excluding, ImportBinding};
        let mut a_imports = ImportMap::new();
        a_imports.insert(
            "B".to_string(),
            ImportBinding {
                specifier: "./b".into(),
                original: "B".into(),
                deferred: false,
                type_only: true,
            },
        );
        let mut b_imports = ImportMap::new();
        b_imports.insert(
            "A".to_string(),
            ImportBinding {
                specifier: "./a".into(),
                original: "A".into(),
                deferred: false,
                type_only: true,
            },
        );
        let all = paths(&["a.ts", "b.ts"]);
        let (dep, type_only_edges) = build_dep(
            &[
                ("a.ts".to_string(), a_imports),
                ("b.ts".to_string(), b_imports),
            ],
            &[],
            &all,
        );
        assert!(circular_from_dep_excluding(&dep, &type_only_edges).is_empty());
    }

    #[test]
    fn value_import_pair_still_forms_a_circular_dependency() {
        // Same shape as above, but with plain value imports both ways — must still be a cycle.
        use zzop_core::{circular_from_dep_excluding, ImportBinding};
        let mut a_imports = ImportMap::new();
        a_imports.insert(
            "B".to_string(),
            ImportBinding {
                specifier: "./b".into(),
                original: "B".into(),
                deferred: false,
                type_only: false,
            },
        );
        let mut b_imports = ImportMap::new();
        b_imports.insert(
            "A".to_string(),
            ImportBinding {
                specifier: "./a".into(),
                original: "A".into(),
                deferred: false,
                type_only: false,
            },
        );
        let all = paths(&["a.ts", "b.ts"]);
        let (dep, type_only_edges) = build_dep(
            &[
                ("a.ts".to_string(), a_imports),
                ("b.ts".to_string(), b_imports),
            ],
            &[],
            &all,
        );
        assert_eq!(circular_from_dep_excluding(&dep, &type_only_edges).len(), 1);
    }

    #[test]
    fn per_specifier_type_only_import_is_also_excluded_from_circular() {
        // `import { type X } from './y'` (per-specifier, not a whole `import type` clause) — parsed via
        // `parse_imports`, proving the exclusion set works end-to-end from real TS source, not just a
        // hand-built `ImportBinding`.
        let a = parse_imports("a.ts", "import { type B } from './b';\n");
        let b = parse_imports("b.ts", "import { type A } from './a';\n");
        let all = paths(&["a.ts", "b.ts"]);
        let (dep, type_only_edges) = build_dep(
            &[("a.ts".to_string(), a), ("b.ts".to_string(), b)],
            &[],
            &all,
        );
        assert!(zzop_core::circular_from_dep_excluding(&dep, &type_only_edges).is_empty());
    }

    // --- matchWorkspacePkg ---

    /// `HashMap<String, ()>`: `match_workspace_pkg` is generic over the value type; these tests don't
    /// need `WorkspacePkg`'s `dir`/`entry` fields.
    fn pkgs() -> HashMap<String, ()> {
        ["@acme/utils-core", "@acme/utils-shared", "lodash"]
            .into_iter()
            .map(|s| (s.to_string(), ()))
            .collect()
    }

    #[test]
    fn workspace_pkg_exact_returns_as_is() {
        assert_eq!(
            match_workspace_pkg("@acme/utils-core", &pkgs()),
            Some(("@acme/utils-core", None))
        );
    }

    #[test]
    fn workspace_pkg_scoped_sub_path_matches_up_to_second_slash() {
        assert_eq!(
            match_workspace_pkg("@acme/utils-core/auth/hash", &pkgs()),
            Some(("@acme/utils-core", Some("auth/hash")))
        );
        assert_eq!(
            match_workspace_pkg("@acme/utils-shared/types", &pkgs()),
            Some(("@acme/utils-shared", Some("types")))
        );
    }

    #[test]
    fn workspace_pkg_scoped_trailing_slash_only_same_as_exact() {
        assert_eq!(
            match_workspace_pkg("@acme/utils-core/", &pkgs()),
            Some(("@acme/utils-core", None))
        );
    }

    #[test]
    fn workspace_pkg_non_scoped_sub_path_matches_up_to_first_slash() {
        assert_eq!(
            match_workspace_pkg("lodash/fp", &pkgs()),
            Some(("lodash", Some("fp")))
        );
    }

    #[test]
    fn workspace_pkg_at_alias_is_not_a_workspace_package() {
        assert_eq!(match_workspace_pkg("@/features/x", &pkgs()), None);
    }

    #[test]
    fn workspace_pkg_external_module_is_none() {
        assert_eq!(match_workspace_pkg("react", &pkgs()), None);
        assert_eq!(match_workspace_pkg("react/jsx-runtime", &pkgs()), None);
    }

    #[test]
    fn workspace_pkg_scoped_but_unregistered_is_none() {
        assert_eq!(match_workspace_pkg("@other/thing", &pkgs()), None);
        assert_eq!(match_workspace_pkg("@other/thing/sub", &pkgs()), None);
    }

    // --- resolve_file_with_workspace / build_dep_with_workspace ---

    fn no_tsconfigs() -> BTreeMap<String, TsconfigPaths> {
        BTreeMap::new()
    }

    fn ws_pkgs() -> HashMap<String, WorkspacePkg> {
        [
            (
                "@acme/utils-core",
                WorkspacePkg {
                    dir: "packages/utils-core".to_string(),
                    entry: Some("packages/utils-core/src/index.ts".to_string()),
                },
            ),
            (
                "@acme/no-entry",
                WorkspacePkg {
                    dir: "packages/no-entry".to_string(),
                    entry: None,
                },
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }

    #[test]
    fn resolve_file_with_workspace_matches_relative_and_alias_like_resolve_file() {
        let all = paths(&["features/x/bar.ts"]);
        assert_eq!(
            resolve_file_with_workspace(
                "./bar",
                "features/x/useFoo.ts",
                &all,
                &HashMap::new(),
                &no_tsconfigs()
            )
            .as_deref(),
            Some("features/x/bar.ts")
        );
    }

    #[test]
    fn resolve_file_with_workspace_bare_specifier_resolves_to_package_entry() {
        let all = paths(&["packages/utils-core/src/index.ts"]);
        assert_eq!(
            resolve_file_with_workspace(
                "@acme/utils-core",
                "a.ts",
                &all,
                &ws_pkgs(),
                &no_tsconfigs()
            )
            .as_deref(),
            Some("packages/utils-core/src/index.ts")
        );
    }

    #[test]
    fn resolve_file_with_workspace_sub_path_specifier_resolves_via_dir_and_try_ext() {
        let all = paths(&[
            "packages/utils-core/src/index.ts",
            "packages/utils-core/auth/hash.ts",
        ]);
        assert_eq!(
            resolve_file_with_workspace(
                "@acme/utils-core/auth/hash",
                "a.ts",
                &all,
                &ws_pkgs(),
                &no_tsconfigs()
            )
            .as_deref(),
            Some("packages/utils-core/auth/hash.ts")
        );
    }

    #[test]
    fn resolve_file_with_workspace_bare_specifier_none_when_package_has_no_entry() {
        let all = paths(&["packages/no-entry/index.ts"]);
        // `@acme/no-entry`'s entry is `None` (no resolvable candidate) — a bare import has nowhere to
        // go, though the package directory is still reachable via an explicit sub-path.
        assert_eq!(
            resolve_file_with_workspace(
                "@acme/no-entry",
                "a.ts",
                &all,
                &ws_pkgs(),
                &no_tsconfigs()
            ),
            None
        );
    }

    #[test]
    fn resolve_file_with_workspace_external_still_none() {
        let all = paths(&["a.ts"]);
        assert_eq!(
            resolve_file_with_workspace("react", "a.ts", &all, &ws_pkgs(), &no_tsconfigs()),
            None
        );
    }

    #[test]
    fn resolve_file_with_workspace_wins_over_same_named_npm_dependency() {
        let mut pkgs = ws_pkgs();
        pkgs.insert(
            "left-pad".to_string(),
            WorkspacePkg {
                dir: "packages/left-pad".to_string(),
                entry: Some("packages/left-pad/index.ts".to_string()),
            },
        );
        let all = paths(&["packages/left-pad/index.ts"]);
        assert_eq!(
            resolve_file_with_workspace("left-pad", "a.ts", &all, &pkgs, &no_tsconfigs())
                .as_deref(),
            Some("packages/left-pad/index.ts")
        );
    }

    #[test]
    fn build_dep_with_workspace_resolves_cross_package_edge() {
        let mut imports = ImportMap::new();
        imports.insert(
            "utils".to_string(),
            zzop_core::ImportBinding {
                specifier: "@acme/utils-core".into(),
                original: "*".into(),
                deferred: false,
                type_only: false,
            },
        );
        let all = paths(&["a.ts", "packages/utils-core/src/index.ts"]);
        let (dep, _type_only) = build_dep_with_workspace(
            &[("a.ts".to_string(), imports)],
            &[],
            &all,
            &ws_pkgs(),
            &no_tsconfigs(),
        );
        assert_eq!(
            dep["a.ts"],
            vec!["packages/utils-core/src/index.ts".to_string()]
        );
    }

    #[test]
    fn build_dep_with_workspace_matches_build_dep_when_no_workspace_pkgs() {
        let imports = parse_imports("a.ts", "import { x } from './b';\n");
        let all = paths(&["a.ts", "b.ts"]);
        let (dep, _type_only) = build_dep_with_workspace(
            &[("a.ts".to_string(), imports)],
            &[],
            &all,
            &HashMap::new(),
            &no_tsconfigs(),
        );
        assert_eq!(dep["a.ts"], vec!["b.ts".to_string()]);
    }

    #[test]
    fn build_dep_with_workspace_merges_re_exports_too() {
        // Same Defect A fix as `bare_named_re_export_creates_dep_edge`, through the workspace-aware
        // entry point the engine's incremental path actually calls.
        let re_exports = vec![(
            "barrel.ts".to_string(),
            vec![zzop_core::ReExport {
                specifier: "./b".to_string(),
                original: "x".to_string(),
                local_alias: "x".to_string(),
                type_only: false,
            }],
        )];
        let all = paths(&["barrel.ts", "b.ts"]);
        let (dep, _type_only) = build_dep_with_workspace(
            &[("barrel.ts".to_string(), ImportMap::new())],
            &re_exports,
            &all,
            &HashMap::new(),
            &no_tsconfigs(),
        );
        assert_eq!(dep["barrel.ts"], vec!["b.ts".to_string()]);
    }

    // --- TsconfigPaths: resolve_via_paths / resolve_via_base_url / governing_tsconfig ---

    fn tsconfigs(entries: &[(&str, TsconfigPaths)]) -> BTreeMap<String, TsconfigPaths> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    fn btree(entries: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    #[test]
    fn tsconfig_star_pattern_resolves_via_base_url() {
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "src".to_string(),
                paths: btree(&[("@/*", &["*"])]),
            },
        )]);
        let all = paths(&["src/features/x.ts"]);
        assert_eq!(
            resolve_file_with_workspace(
                "@/features/x",
                "anywhere/deep.ts",
                &all,
                &HashMap::new(),
                &cfgs
            )
            .as_deref(),
            Some("src/features/x.ts")
        );
    }

    #[test]
    fn tsconfig_exact_pattern_resolves() {
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "".to_string(),
                paths: btree(&[("shims", &["src/shims.ts"])]),
            },
        )]);
        let all = paths(&["src/shims.ts"]);
        assert_eq!(
            resolve_file_with_workspace("shims", "a.ts", &all, &HashMap::new(), &cfgs).as_deref(),
            Some("src/shims.ts")
        );
    }

    #[test]
    fn tsconfig_longest_prefix_wins_over_shorter_wildcard() {
        // Both `@/*` and `@/utils/*` match `@/utils/format` — the longer, more specific prefix must win.
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "".to_string(),
                paths: btree(&[
                    ("@/*", &["src/generic/*"]),
                    ("@/utils/*", &["src/special-utils/*"]),
                ]),
            },
        )]);
        let all = paths(&["src/generic/utils/format.ts", "src/special-utils/format.ts"]);
        assert_eq!(
            resolve_file_with_workspace("@/utils/format", "a.ts", &all, &HashMap::new(), &cfgs)
                .as_deref(),
            Some("src/special-utils/format.ts")
        );
    }

    #[test]
    fn tsconfig_bare_specifier_resolves_via_base_url_without_a_paths_entry() {
        // Resolves against `baseUrl` even when no `paths` pattern matches — the "absolute-from-src" convention.
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "src".to_string(),
                paths: BTreeMap::new(),
            },
        )]);
        let all = paths(&["src/utils/format.ts"]);
        assert_eq!(
            resolve_file_with_workspace("utils/format", "a.ts", &all, &HashMap::new(), &cfgs)
                .as_deref(),
            Some("src/utils/format.ts")
        );
    }

    #[test]
    fn tsconfig_nearest_ancestor_governs_nested_file() {
        // `packages/app`'s own tsconfig governs files under it, not the root tsconfig, even when both are registered.
        let cfgs = tsconfigs(&[
            (
                "",
                TsconfigPaths {
                    base_url: "".to_string(),
                    paths: btree(&[("@/*", &["root-src/*"])]),
                },
            ),
            (
                "packages/app",
                TsconfigPaths {
                    base_url: "packages/app/src".to_string(),
                    paths: BTreeMap::new(),
                },
            ),
        ]);
        let all = paths(&["packages/app/src/x.ts"]);
        assert_eq!(
            resolve_file_with_workspace(
                "x",
                "packages/app/deep/file.ts",
                &all,
                &HashMap::new(),
                &cfgs
            )
            .as_deref(),
            Some("packages/app/src/x.ts")
        );
    }

    #[test]
    fn tsconfig_relative_specifier_is_never_remapped_by_paths() {
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "".to_string(),
                paths: btree(&[("./bar", &["somewhere/else.ts"])]),
            },
        )]);
        let all = paths(&["a/bar.ts"]);
        assert_eq!(
            resolve_file_with_workspace("./bar", "a/x.ts", &all, &HashMap::new(), &cfgs).as_deref(),
            Some("a/bar.ts")
        );
    }

    #[test]
    fn tsconfig_non_matching_specifier_falls_through_to_workspace_then_external() {
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "".to_string(),
                paths: btree(&[("@/*", &["src/*"])]),
            },
        )]);
        let all = paths(&["packages/utils-core/src/index.ts"]);
        // Doesn't match `@/*`, but does name a workspace package — still resolves via `ws_pkgs`.
        assert_eq!(
            resolve_file_with_workspace("@acme/utils-core", "a.ts", &all, &ws_pkgs(), &cfgs)
                .as_deref(),
            Some("packages/utils-core/src/index.ts")
        );
        // Doesn't match `@/*` or any workspace package -> external.
        assert_eq!(
            resolve_file_with_workspace("react", "a.ts", &all, &ws_pkgs(), &cfgs),
            None
        );
    }

    #[test]
    fn tsconfig_paths_pattern_match_with_unresolvable_target_falls_through_to_base_url() {
        // `features/*` matches, but its target `nowhere/*` doesn't resolve — falls through to
        // baseUrl-relative resolution of the original specifier, unlike real `tsc`.
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "src".to_string(),
                paths: btree(&[("features/*", &["nowhere/*"])]),
            },
        )]);
        let all = paths(&["src/features/x.ts"]);
        assert_eq!(
            resolve_file_with_workspace("features/x", "a.ts", &all, &HashMap::new(), &cfgs)
                .as_deref(),
            Some("src/features/x.ts")
        );
    }
}
