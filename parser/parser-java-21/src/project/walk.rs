//! Corpus indexing, `extends`-chain walking, and route emission for the whole-project provides pass —
//! see the parent module doc (`project/mod.rs`) for the two whole-corpus facts this resolves and the
//! "AST-native design change" that lets `emit_class_routes` below just replay each row's PRECOMPUTED
//! `methods` instead of re-deriving them.

use std::collections::{HashMap, HashSet};

use zzop_core::{http_interface_key, IoProvide};

use super::collect::collect_from_root;
use super::resolve::{resolve_class_prefix, resolve_method_path};
use super::{ClassRow, MethodPath, PrefixState, ProjectProvidesReport};

/// Extracts Spring MVC HTTP route `IoProvide`s across an entire Java corpus — see module doc for what
/// this resolves beyond `provides::extract_http_provides`'s per-file pass. Never panics: a file that
/// fails to parse is silently skipped (degrades to "no rows from this file", same as every other
/// never-guess gate in this crate).
pub fn extract_http_provides_project(files: &[(String, String)]) -> ProjectProvidesReport {
    let mut rows_by_name: HashMap<String, Vec<ClassRow>> = HashMap::new();
    for (rel, text) in files {
        let Some(tree) = crate::parse_tree(text) else {
            continue;
        };
        collect_from_root(rel, tree.root_node(), text, &mut rows_by_name);
    }

    // Unique-name resolution: a simple class name declared in exactly one file/position is safe to use
    // as an `extends`/qualifier resolution target; 2+ declarations (a common name reused across the
    // corpus) is ambiguous.
    let mut skipped_ambiguous_class_name = 0u32;
    let mut classes: HashMap<String, ClassRow> = HashMap::new();
    for (name, mut rows) in rows_by_name {
        if rows.len() == 1 {
            classes.insert(name, rows.pop().unwrap());
        } else {
            skipped_ambiguous_class_name += rows.len() as u32;
        }
    }

    let mut provides = Vec::new();
    let mut seen: HashSet<(String, String, u32, Option<String>)> = HashSet::new();
    let mut skipped_unresolved_prefix = 0u32;
    let mut skipped_unresolved_method_path = 0u32;
    let mut memo: HashMap<(String, String), Option<String>> = HashMap::new();

    // Every row carrying its own @RestController/@Controller is a gating root — walk its `extends`
    // chain, emitting routes for every class along the way. Root names are collected up front and
    // sorted so iteration order is deterministic regardless of `HashMap` iteration order.
    let mut root_names: Vec<&String> = classes
        .iter()
        .filter(|(_, row)| row.is_controller)
        .map(|(name, _)| name)
        .collect();
    root_names.sort();

    for root_name in root_names {
        walk_chain(
            root_name,
            &classes,
            &mut memo,
            &mut provides,
            &mut seen,
            &mut skipped_unresolved_prefix,
            &mut skipped_unresolved_method_path,
        );
    }

    ProjectProvidesReport {
        provides,
        skipped_unresolved_prefix,
        skipped_ambiguous_class_name,
        skipped_unresolved_method_path,
    }
}

/// Walks `root_name`'s `extends` chain (root first, then its superclass, ...; capped and cycle-safe) and
/// emits every class's own routes along the way: the first class in root-to-leaf order that declares its
/// own `@RequestMapping` (resolved or not) fixes the prefix — or the unresolved-skip — for every class
/// from that point on.
#[allow(clippy::too_many_arguments)]
fn walk_chain(
    root_name: &str,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
    provides: &mut Vec<IoProvide>,
    seen: &mut HashSet<(String, String, u32, Option<String>)>,
    skipped_unresolved_prefix: &mut u32,
    skipped_unresolved_method_path: &mut u32,
) {
    let mut chain_names: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut cur = Some(root_name.to_string());
    let mut depth = 0;
    while let Some(name) = cur {
        if depth > 40 || !visited.insert(name.clone()) {
            break; // cycle or pathologically deep chain — defensive, not expected in real inheritance.
        }
        depth += 1;
        let next = classes.get(&name).and_then(|c| c.extends.clone());
        chain_names.push(name);
        cur = next;
    }

    let mut effective: Option<String> = None;
    let mut blocked = false;
    for name in &chain_names {
        let Some(row) = classes.get(name) else {
            continue;
        };
        if effective.is_none() && !blocked {
            match resolve_class_prefix(name, &row.request_mapping_arg, classes, memo) {
                PrefixState::NoMapping => {}
                PrefixState::Resolved(p) => effective = Some(p),
                PrefixState::Unresolved => blocked = true,
            }
        }
        if blocked {
            *skipped_unresolved_prefix += 1;
            continue;
        }
        let prefix = effective.clone().unwrap_or_default();
        emit_class_routes(
            name,
            row,
            &prefix,
            classes,
            memo,
            provides,
            seen,
            skipped_unresolved_method_path,
        );
    }
}

/// Emits every `IoProvide` for `row`'s own PRECOMPUTED `methods` under `prefix`, deduped against `seen`
/// so two different gating roots reaching the same base class with the same resolved prefix (the
/// `FooController extends FooControllerCE` shape) do not double-emit. A method whose own path is a
/// `MethodPath::Unresolved` constant reference is resolved against the corpus here (scoped to `class_name`,
/// the declaring class — `resolve::resolve_method_path`); an unresolvable one is skipped and counted, never
/// keyed at the empty base.
#[allow(clippy::too_many_arguments)]
fn emit_class_routes(
    class_name: &str,
    row: &ClassRow,
    prefix: &str,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
    provides: &mut Vec<IoProvide>,
    seen: &mut HashSet<(String, String, u32, Option<String>)>,
    skipped_unresolved_method_path: &mut u32,
) {
    for method in &row.methods {
        let method_path = match &method.path {
            MethodPath::Literal(p) => p.clone(),
            MethodPath::Unresolved(args) => {
                match resolve_method_path(class_name, args, classes, memo) {
                    Some(p) => p,
                    None => {
                        *skipped_unresolved_method_path += 1;
                        continue;
                    }
                }
            }
        };
        let full_path = format!("{prefix}/{method_path}");
        let key = http_interface_key(&method.verb, &full_path);
        let dedupe_key = (
            key.clone(),
            row.file.clone(),
            method.line,
            Some(method.name.clone()),
        );
        if !seen.insert(dedupe_key) {
            continue;
        }
        provides.push(IoProvide {
            body: None,
            kind: "http".to_string(),
            key,
            file: row.file.clone(),
            line: method.line,
            symbol: Some(method.name.clone()),
        });
    }
}
