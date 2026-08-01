//! The mount-tree walk: descending a router fragment graph into composed `IoProvide`s.
//!
//! Split from the parent by size, but the seam is real — everything here is about DESCENT (how a
//! prefix accumulates down a chain and what happens when a child cannot be found), while the parent
//! owns node indexing, root selection and output ordering.

use std::collections::{BTreeMap, HashMap};

use zzop_core::{normalize_http_path, IoProvide};
pub(super) fn join_prefix(prefix: &str, seg: &str) -> String {
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
pub(super) type FindChild<'a> = dyn Fn(&str, &str, Option<&str>) -> Option<usize> + 'a;

/// One mount's descent, shared by the literal [`RouterMountEntry::Mount`] arm and the resolved
/// half of [`RouterMountEntry::MountRef`]. Factored out so the two can never drift: a reference
/// that resolves must behave EXACTLY like the literal it resolved to, or the const map would
/// quietly become a second routing semantics.
#[allow(clippy::too_many_arguments)]
fn descend_mount(
    file: &str,
    prefix: &str,
    mount_prefix: &str,
    ident: &str,
    specifier: Option<&str>,
    attr_keys: &[String],
    nodes: &[(&str, &zzop_core::RouterMountFragment)],
    find_child: &FindChild,
    ancestry: &mut Vec<usize>,
    out: &mut Vec<IoProvide>,
    attrs: &mut Vec<zzop_core::Attribute>,
    consts: &HashMap<String, String>,
    unresolved: &mut BTreeMap<(String, String), u32>,
) {
    match find_child(file, ident, specifier) {
        Some(child) => {
            walk(
                child,
                &join_prefix(prefix, mount_prefix),
                nodes,
                find_child,
                ancestry,
                out,
                attrs,
                consts,
                unresolved,
            );
        }
        None => {
            // Unresolvable/ambiguous mount — the ident could not be disambiguated between a
            // sub-router and a middleware guard. Producer-judged attr keys resolve here as a
            // PathScope, since no child fragment exists to recurse into. Normalized
            // (`:param`/`{param}` -> `{}`) via the same `http_interface_key`-shared helper the
            // Verb arm's `key` uses, so a `:param`-carrying mount chain's PathScope prefix covers
            // the normalized route keys it's meant to scope, not their raw pre-normalized spelling
            // (which a route key never carries).
            let scoped_prefix = normalize_http_path(&join_prefix(prefix, mount_prefix));
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

#[allow(clippy::too_many_arguments)]
pub(super) fn walk(
    idx: usize,
    prefix: &str,
    nodes: &[(&str, &zzop_core::RouterMountFragment)],
    find_child: &FindChild,
    ancestry: &mut Vec<usize>,
    out: &mut Vec<IoProvide>,
    attrs: &mut Vec<zzop_core::Attribute>,
    consts: &HashMap<String, String>,
    unresolved: &mut BTreeMap<(String, String), u32>,
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
            } => descend_mount(
                file,
                prefix,
                mount_prefix,
                ident,
                specifier.as_deref(),
                attr_keys,
                nodes,
                find_child,
                ancestry,
                out,
                attrs,
                consts,
                unresolved,
            ),
            zzop_core::RouterMountEntry::MountRef {
                prefix_ref,
                ident,
                specifier,
                attr_keys,
            } => match consts.get(prefix_ref) {
                Some(resolved) => descend_mount(
                    file,
                    prefix,
                    resolved,
                    ident,
                    specifier.as_deref(),
                    attr_keys,
                    nodes,
                    find_child,
                    ancestry,
                    out,
                    attrs,
                    consts,
                    unresolved,
                ),
                // NEVER fall back to `/`. A root mount would emit every route under this router at
                // a path the deployment does not serve, and a confident wrong key is indistinguishable
                // from a correct one — the exact failure this variant exists to prevent. Drop the
                // subtree and disclose it.
                None => {
                    *unresolved
                        .entry((file.to_string(), prefix_ref.clone()))
                        .or_insert(0) += 1;
                }
            },
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
