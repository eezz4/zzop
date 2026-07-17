use std::collections::{HashMap, HashSet};

use zzop_core::{normalize_http_path, IoProvide};

/// Compose whole-tree `http` PROVIDEs from per-file router-mount fragments
/// (`zzop_parser_typescript::router_mounts` — Hono-style chained builders and cross-file
/// `.route(prefix, subRouter)` mounts). The provide-side twin of [`compose_trpc_provides`]: same
/// root-exclusion conservatism, same import-resolver closure, same dedup/sort discipline; composition
/// joins URL path prefixes instead of dotted procedure paths.
///
/// **Roots**: a fragment is a DFS root only when nothing mounts it — neither by resolved edge nor by
/// NAME (some `Mount.ident` anywhere equals its name, even unresolved): a mounted-but-unresolvable
/// sub-router must not surface its entries with a truncated (missing prefix) URL — under-reporting is
/// honest, mis-keying is not.
///
/// **Mount resolution**: same-file fragment named `ident` first; else `resolve(specifier, from_file,
/// ident)` → target file, preferring the fragment named `ident` and falling back to the file's SOLE
/// fragment (covers `export default route` re-imported under an arbitrary local alias — the common
/// one-router-per-file layout). Ambiguous (multi-fragment, no name match) or unresolvable mounts
/// skip that subtree. `ident` is threaded through to `resolve` (not just used by this function's own
/// post-resolution fallback above) for languages whose specifier resolves to a whole DIRECTORY of
/// candidate files rather than one file — Go's import-path-to-package-directory resolution is the
/// first such case (`crate::analyze::assemble::provides`'s Go resolver branch); every other
/// language's resolver ignores the parameter and resolves to a single file exactly as before.
///
/// Provide anchors: `file`/`line` of the VERB registration (the leaf file, not the mount site),
/// `symbol` = handler name — the place a reader would edit the route.
///
/// Also composes producer-judged attributes riding the same fragments (`RouterMountEntry::Verb::attr_keys`,
/// `RouterMountEntry::Mount::attr_keys`, `RouterMountEntry::ScopedAttr` — see `zzop_core::fragments`'s
/// module doc) into `zzop_core::Attribute`s, on the SAME composed prefix/key the provides above use, so a
/// route-level or router-level middleware guard judged by the producer lands on the exact `IoKey`/`PathScope`
/// a consumer (e.g. `zzop_rules_http::mutating_route_no_auth`'s `AttributeStore` lookup) queries. Only
/// root-reachable subtrees ever emit — attribute emission happens inside the same DFS as provide emission,
/// so a mounted-but-unreachable fragment's `ScopedAttr`s are silent exactly like its `Verb`s are.
pub(crate) fn compose_router_mount_provides(
    fragments: Vec<(String, Vec<zzop_core::RouterMountFragment>)>,
    resolve: impl Fn(&str, &str, &str) -> Option<String>,
) -> (Vec<IoProvide>, Vec<zzop_core::Attribute>) {
    use zzop_core::{RouterMountEntry, RouterMountFragment};

    let mut fragments = fragments;
    fragments.sort_by(|a, b| a.0.cmp(&b.0));

    // (file, fragment) node list + per-file index; nodes keep per-file source order.
    let mut nodes: Vec<(&str, &RouterMountFragment)> = Vec::new();
    let mut by_file: HashMap<&str, Vec<usize>> = HashMap::new();
    for (file, frags) in &fragments {
        for frag in frags {
            by_file.entry(file.as_str()).or_default().push(nodes.len());
            nodes.push((file.as_str(), frag));
        }
    }

    let find_child = |from_file: &str, ident: &str, specifier: Option<&str>| -> Option<usize> {
        let candidates_in = |file: &str| -> Option<usize> {
            let idxs = by_file.get(file)?;
            if let Some(&idx) = idxs.iter().find(|&&i| nodes[i].1.name == ident) {
                return Some(idx);
            }
            if idxs.len() == 1 {
                return Some(idxs[0]);
            }
            None
        };
        match specifier {
            None => candidates_in(from_file),
            Some(spec) => {
                let target = resolve(spec, from_file, ident)?;
                candidates_in(&target)
            }
        }
    };

    // Root exclusion: mounted by name anywhere (unresolved-conservative) OR by resolved edge.
    let mut mounted_names: HashSet<&str> = HashSet::new();
    let mut mounted_nodes: HashSet<usize> = HashSet::new();
    for (file, frag) in &nodes {
        for entry in &frag.entries {
            if let RouterMountEntry::Mount {
                ident, specifier, ..
            } = entry
            {
                mounted_names.insert(ident.as_str());
                if let Some(child) = find_child(file, ident, specifier.as_deref()) {
                    mounted_nodes.insert(child);
                }
            }
        }
    }

    fn join_prefix(prefix: &str, seg: &str) -> String {
        if seg == "/" || seg.is_empty() {
            return prefix.to_string();
        }
        let base = prefix.trim_end_matches('/');
        if seg.starts_with('/') {
            format!("{base}{seg}")
        } else {
            format!("{base}/{seg}")
        }
    }

    /// `(from_file, ident, specifier)` → node index of the mounted child fragment, if resolvable.
    type FindChild<'a> = dyn Fn(&str, &str, Option<&str>) -> Option<usize> + 'a;

    #[allow(clippy::too_many_arguments)]
    fn walk(
        idx: usize,
        prefix: &str,
        nodes: &[(&str, &zzop_core::RouterMountFragment)],
        find_child: &FindChild,
        ancestry: &mut Vec<usize>,
        out: &mut Vec<IoProvide>,
        attrs: &mut Vec<zzop_core::Attribute>,
    ) {
        if ancestry.contains(&idx) {
            return; // cycle guard — mirrors compose_trpc_provides' ancestry stack
        }
        ancestry.push(idx);
        let (file, frag) = nodes[idx];
        for entry in &frag.entries {
            match entry {
                zzop_core::RouterMountEntry::Verb {
                    method,
                    path,
                    handler,
                    line,
                    attr_keys,
                } => {
                    let full = join_prefix(prefix, path);
                    let key = zzop_core::http_interface_key(method, &full);
                    for attr_key in attr_keys {
                        attrs.push(zzop_core::Attribute {
                            target: zzop_core::EntityRef::IoKey {
                                kind: "http".to_string(),
                                key: key.clone(),
                            },
                            key: attr_key.clone(),
                            value: serde_json::Value::Bool(true),
                        });
                    }
                    out.push(IoProvide {
                        body: None,
                        kind: "http".to_string(),
                        key,
                        file: file.to_string(),
                        line: *line,
                        symbol: handler.clone(),
                    });
                }
                zzop_core::RouterMountEntry::Mount {
                    prefix: mount_prefix,
                    ident,
                    specifier,
                    attr_keys,
                } => {
                    match find_child(file, ident, specifier.as_deref()) {
                        Some(child) => {
                            walk(
                                child,
                                &join_prefix(prefix, mount_prefix),
                                nodes,
                                find_child,
                                ancestry,
                                out,
                                attrs,
                            );
                        }
                        None => {
                            // Unresolvable/ambiguous mount — the ident could not be disambiguated
                            // between a sub-router and a middleware guard. Producer-judged attr keys
                            // resolve here as a PathScope, since no child fragment exists to recurse
                            // into. Normalized (`:param`/`{param}` -> `{}`) via the same
                            // `http_interface_key`-shared helper the Verb arm's `key` above uses, so
                            // a `:param`-carrying mount chain's PathScope prefix covers the
                            // normalized route keys it's meant to scope, not their raw pre-normalized
                            // spelling (which a route key never carries).
                            let scoped_prefix =
                                normalize_http_path(&join_prefix(prefix, mount_prefix));
                            for attr_key in attr_keys {
                                attrs.push(zzop_core::Attribute {
                                    target: zzop_core::EntityRef::PathScope {
                                        prefix: scoped_prefix.clone(),
                                    },
                                    key: attr_key.clone(),
                                    value: serde_json::Value::Bool(true),
                                });
                            }
                        }
                    }
                }
                zzop_core::RouterMountEntry::ScopedAttr {
                    prefix: attr_prefix,
                    key,
                    line: _,
                } => {
                    // Normalized for the same reason as the unresolved-Mount arm above: a
                    // `:param`-carrying `.use` prefix chain must scope the NORMALIZED route path,
                    // not its raw `:param` spelling.
                    attrs.push(zzop_core::Attribute {
                        target: zzop_core::EntityRef::PathScope {
                            prefix: normalize_http_path(&join_prefix(prefix, attr_prefix)),
                        },
                        key: key.clone(),
                        value: serde_json::Value::Bool(true),
                    });
                }
            }
        }
        ancestry.pop();
    }

    let mut out: Vec<IoProvide> = Vec::new();
    let mut attrs: Vec<zzop_core::Attribute> = Vec::new();
    for idx in 0..nodes.len() {
        if mounted_nodes.contains(&idx) || mounted_names.contains(nodes[idx].1.name.as_str()) {
            continue;
        }
        let mut ancestry = Vec::new();
        walk(
            idx,
            "",
            &nodes,
            &find_child,
            &mut ancestry,
            &mut out,
            &mut attrs,
        );
    }

    out.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out.dedup_by(|a, b| a.key == b.key && a.file == b.file && a.line == b.line);

    // Deterministic dedup/sort: sort by (serialized target, key) — a stable, order-independent key
    // over `EntityRef`'s variant+fields — then drop exact (target, key, value) duplicates. Two
    // identical `Attribute`s can arise legitimately (e.g. the same `auth-guarded` PathScope emitted
    // from two independent mount chains that both terminate at the same unresolved ident).
    fn attr_sort_key(a: &zzop_core::Attribute) -> String {
        format!(
            "{}\u{0}{}",
            serde_json::to_string(&a.target).unwrap_or_default(),
            a.key
        )
    }
    attrs.sort_by_key(attr_sort_key);
    attrs.dedup_by(|a, b| a == b);

    (out, attrs)
}

#[cfg(test)]
mod tests;
