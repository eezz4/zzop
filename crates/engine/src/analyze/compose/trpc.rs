use std::collections::{HashMap, HashSet};

use zzop_core::IoProvide;

/// Composes every file's tRPC router fragment into whole-tree `IoProvide`s — the assembly-time
/// counterpart of `late_resolve_cross_file_consumes` for the `trpc` kind, except here the cross-file
/// join produces brand-new PROVIDEs directly rather than re-keying an already-emitted CONSUME: a leaf's
/// full dotted route path is often only knowable once every file's fragment is assembled together.
///
/// `resolve` is `(specifier, from_file) -> Option<target_rel>` — the caller passes a closure over
/// `zzop_parser_typescript::resolve_file_with_workspace` (the same resolver `assemble` uses for TS
/// dep-graph edges) so this function itself stays a pure, filesystem-free composition — easy to unit
/// test with a hand-built resolver map.
///
/// ## Resolution
/// Fragments are indexed by `(rel, name)`. Each `ProcedureRouterEntry::Ref` is resolved to a target fragment
/// key: `specifier: Some(s)` -> `resolve(s, rel)` gives the target file, then `(target_rel, ident)`;
/// `specifier: None` -> same-file, `(rel, ident)`. A `Ref` whose specifier does not resolve, or whose
/// resolved key names no known fragment, is skipped — honest absence, never fabricated.
///
/// ## Roots and composition
/// A fragment is a ROOT when no resolved `Ref` anywhere in the corpus targets it — composition starts
/// from every root (BTree-ordered for determinism) and walks each fragment's entries depth-first:
/// `Nested` appends its `key` to the current dotted path; `Ref` with a non-empty `key` appends `key`
/// then recurses into the target fragment; `Ref` with an empty `key` (a `mergeRouters(...)` argument)
/// splices the target's entries in at the current path, adding no segment; `Leaf` emits one `IoProvide`
/// (`file`/`line` from the leaf's own originating fragment, which after a `Ref` hop is the target
/// fragment's file, not the file containing the `Ref`). An `ancestry` stack guards against a cyclic
/// `Ref` chain — a fragment already on the stack is skipped rather than recursed into again.
///
/// Deduped on `(kind, key, file, line)` and sorted to match the ordering `assemble` applies to every
/// other `IoProvide` before freezing `MinimalIr::io`.
pub(crate) fn compose_trpc_provides(
    fragments: Vec<(String, Vec<zzop_core::ProcedureRouterFragment>)>,
    resolve: impl Fn(&str, &str) -> Option<String>,
) -> Vec<IoProvide> {
    use zzop_core::{ProcedureRouterEntry, ProcedureRouterFragment};

    let mut by_key: HashMap<(String, String), &ProcedureRouterFragment> = HashMap::new();
    for (rel, frags) in &fragments {
        for frag in frags {
            by_key.insert((rel.clone(), frag.name.clone()), frag);
        }
    }

    // Resolves one `Ref`'s target fragment key, honoring the `specifier: Some -> resolve` / `specifier:
    // None -> same-file` split — shared by both the "which fragments are targeted" pass below and the
    // actual composition walk.
    let ref_target = |origin_rel: &str, ident: &str, specifier: &Option<String>| {
        let target_rel = match specifier {
            Some(s) => resolve(s, origin_rel)?,
            None => origin_rel.to_string(),
        };
        let key = (target_rel, ident.to_string());
        by_key.contains_key(&key).then_some(key)
    };

    // Every fragment key targeted by at least one resolved `Ref`, anywhere — plus, conservatively, every
    // ident ANY `Ref` names (resolved or not): a fragment whose name some mount references is intended
    // as a sub-router somewhere, so promoting it to a root when the mount's specifier failed to resolve
    // would emit its leaves under a truncated path (e.g. bare `createLicense` instead of
    // `viewer.admin.createLicense`) — a mis-keyed provide. Skipping it entirely is the honest-absence
    // choice; the cost is missing a genuinely independent root that merely shares its name with some ref
    // ident — rare, and an under- rather than over-report.
    let mut targeted: HashSet<(String, String)> = HashSet::new();
    let mut ref_named_idents: HashSet<String> = HashSet::new();
    fn collect_targeted(
        entries: &[ProcedureRouterEntry],
        origin_rel: &str,
        ref_target: &impl Fn(&str, &str, &Option<String>) -> Option<(String, String)>,
        targeted: &mut HashSet<(String, String)>,
        ref_named_idents: &mut HashSet<String>,
    ) {
        for entry in entries {
            match entry {
                ProcedureRouterEntry::Ref {
                    ident, specifier, ..
                } => {
                    ref_named_idents.insert(ident.clone());
                    if let Some(key) = ref_target(origin_rel, ident, specifier) {
                        targeted.insert(key);
                    }
                }
                ProcedureRouterEntry::Nested { entries, .. } => {
                    collect_targeted(entries, origin_rel, ref_target, targeted, ref_named_idents);
                }
                ProcedureRouterEntry::Leaf { .. } => {}
            }
        }
    }
    for (rel, frags) in &fragments {
        for frag in frags {
            collect_targeted(
                &frag.entries,
                rel,
                &ref_target,
                &mut targeted,
                &mut ref_named_idents,
            );
        }
    }

    let mut roots: Vec<(String, String)> = by_key
        .keys()
        .filter(|k| !targeted.contains(*k) && !ref_named_idents.contains(&k.1))
        .cloned()
        .collect();
    roots.sort();

    #[allow(clippy::too_many_arguments)]
    fn compose_entries(
        entries: &[ProcedureRouterEntry],
        origin_rel: &str,
        path: &[String],
        by_key: &HashMap<(String, String), &ProcedureRouterFragment>,
        ref_target: &impl Fn(&str, &str, &Option<String>) -> Option<(String, String)>,
        ancestry: &mut Vec<(String, String)>,
        out: &mut Vec<IoProvide>,
    ) {
        for entry in entries {
            match entry {
                ProcedureRouterEntry::Leaf { key, verb, line } => {
                    let full_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{key}", path.join("."))
                    };
                    out.push(IoProvide {
                        body: None,
                        kind: "trpc".to_string(),
                        key: format!("{verb} {full_path}"),
                        file: origin_rel.to_string(),
                        line: *line,
                        symbol: None,
                    });
                }
                ProcedureRouterEntry::Nested {
                    key,
                    entries: inner,
                } => {
                    let mut new_path = path.to_vec();
                    new_path.push(key.clone());
                    compose_entries(
                        inner, origin_rel, &new_path, by_key, ref_target, ancestry, out,
                    );
                }
                ProcedureRouterEntry::Ref {
                    key,
                    ident,
                    specifier,
                } => {
                    let Some(target_key) = ref_target(origin_rel, ident, specifier) else {
                        continue; // unresolvable specifier, or no fragment named `ident` there — skip, never guess
                    };
                    if ancestry.contains(&target_key) {
                        continue; // cycle guard
                    }
                    let target_frag = by_key[&target_key];
                    let new_path = if key.is_empty() {
                        path.to_vec() // mergeRouters splice-in-place — no path segment added
                    } else {
                        let mut p = path.to_vec();
                        p.push(key.clone());
                        p
                    };
                    ancestry.push(target_key.clone());
                    compose_entries(
                        &target_frag.entries,
                        &target_key.0,
                        &new_path,
                        by_key,
                        ref_target,
                        ancestry,
                        out,
                    );
                    ancestry.pop();
                }
            }
        }
    }

    let mut out = Vec::new();
    for root_key in roots {
        let frag = by_key[&root_key];
        let mut ancestry = vec![root_key.clone()];
        compose_entries(
            &frag.entries,
            &root_key.0,
            &[],
            &by_key,
            &ref_target,
            &mut ancestry,
            &mut out,
        );
    }

    let mut seen: HashSet<(String, String, String, u32)> = HashSet::new();
    out.retain(|p| seen.insert((p.kind.clone(), p.key.clone(), p.file.clone(), p.line)));
    out.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out
}

#[cfg(test)]
mod tests;
