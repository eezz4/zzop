// `inline_mod_use_census` PRINTS BY DESIGN — it is a recount command, not a gate, so its stdout line
// IS its entire product; suppressing the print would leave it recounting into the void. The workspace
// `print_stdout` lint exists because stdout carries a machine-readable contract on the SHIPPING lanes;
// a `cargo test` harness is not one of them. Written as a FILE-level inner attribute on purpose, even
// though only one function needs it: the exemption census
// (`policy_value_pins::a_stdout_exemption_sits_on_a_target_root_or_a_test_module_never_on_a_library`)
// finds sites by `line.starts_with("#![allow(")`, so a tighter `#[allow]` on the function would be
// narrower in scope and INVISIBLE to the audit that exists to enumerate these. Visible beats narrow
// here; that pin also names a `tests.rs` module as a permitted home, which is what this file is.
#![allow(clippy::print_stdout)]

use super::*;

fn binding<'a>(map: &'a ImportMap, local: &str) -> &'a ImportBinding {
    map.get(local)
        .unwrap_or_else(|| panic!("no binding for {local:?} in {map:?}"))
}

#[test]
fn plain_use_path_binds_the_last_segment() {
    let map = parse_imports("use a::b::c;\n");
    let b = binding(&map, "c");
    assert_eq!(b.specifier, "a::b::c");
    assert_eq!(b.original, "c");
    assert!(!b.deferred && !b.type_only);
}

#[test]
fn crate_prefixed_use_keeps_the_crate_head() {
    let map = parse_imports("use crate::routes::handler;\n");
    let b = binding(&map, "handler");
    assert_eq!(b.specifier, "crate::routes::handler");
}

#[test]
fn super_prefixed_use_keeps_the_super_head() {
    let map = parse_imports("use super::shared;\n");
    let b = binding(&map, "shared");
    assert_eq!(b.specifier, "super::shared");
}

#[test]
fn self_prefixed_use_keeps_the_self_head() {
    let map = parse_imports("use self::helper;\n");
    let b = binding(&map, "helper");
    assert_eq!(b.specifier, "self::helper");
}

#[test]
fn renamed_use_binds_the_alias_and_keeps_the_original() {
    let map = parse_imports("use a::b::c as d;\n");
    let bnd = binding(&map, "d");
    assert_eq!(bnd.specifier, "a::b::c");
    assert_eq!(bnd.original, "c");
    assert!(!map.contains_key("c"));
}

#[test]
fn grouped_use_tree_binds_every_member() {
    let map = parse_imports("use a::{b, c as d};\n");
    assert_eq!(binding(&map, "b").specifier, "a::b");
    assert_eq!(binding(&map, "d").specifier, "a::c");
    assert_eq!(binding(&map, "d").original, "c");
}

#[test]
fn nested_grouped_use_tree() {
    let map = parse_imports("use a::{b::{c, d}, e};\n");
    assert_eq!(binding(&map, "c").specifier, "a::b::c");
    assert_eq!(binding(&map, "d").specifier, "a::b::d");
    assert_eq!(binding(&map, "e").specifier, "a::e");
}

#[test]
fn glob_import_gets_a_synthetic_key() {
    let map = parse_imports("use a::b::*;\n");
    assert_eq!(map.len(), 1);
    let (_, b) = map.iter().next().unwrap();
    assert_eq!(b.specifier, "a::b");
    assert_eq!(b.original, "*");
}

#[test]
fn multiple_glob_imports_get_distinct_synthetic_keys() {
    let map = parse_imports("use a::*;\nuse b::*;\n");
    assert_eq!(map.len(), 2);
    let specifiers: Vec<&str> = map.values().map(|b| b.specifier.as_str()).collect();
    assert!(specifiers.contains(&"a"));
    assert!(specifiers.contains(&"b"));
}

#[test]
fn pub_use_is_recorded_as_an_ordinary_binding() {
    // No re-export flag in this crate's `ImportBinding` output — see module doc.
    let map = parse_imports("pub use crate::inner::Thing;\n");
    let b = binding(&map, "Thing");
    assert_eq!(b.specifier, "crate::inner::Thing");
}

#[test]
fn bodiless_mod_decl_is_an_import_edge_encoded_as_self() {
    let map = parse_imports("mod routes;\n");
    let b = binding(&map, "routes");
    assert_eq!(b.specifier, "self::routes");
    assert_eq!(b.original, "routes");
}

#[test]
fn mod_with_a_body_is_not_an_import_edge() {
    let map = parse_imports("mod inner {\n    fn f() {}\n}\n");
    assert!(!map.contains_key("inner"));
}

#[test]
fn a_use_inside_an_inline_mod_is_a_decided_non_capability_not_a_gap() {
    // Pins the module doc's "why an inline `mod`'s `use` stays out" section. The name to watch is
    // NOT the shadowing pair — the top-level `File` simply survives untouched, and the collision the
    // key axis warns about was measured at 0 of 111 in the shipped corpus. The one that decides it is
    // `Serialize`: it collides with nothing, it is absolutely headed, and it is STILL not collected,
    // because collecting it would mean also deciding what to do with the relative-headed majority
    // (`super::`/`self::`), whose anchor depends on nesting depth that this file-level map cannot
    // carry and `resolve::rust_import_candidates` cannot receive.
    let map = parse_imports(
        "use std::fs::File;\n\nmod inner {\n    use std::io::Read as File;\n    use serde::Serialize;\n    use super::File as Outer;\n}\n",
    );
    assert_eq!(map.get("File").expect("File").specifier, "std::fs::File");
    assert!(!map.contains_key("Serialize"), "{map:?}");
    assert!(!map.contains_key("Outer"), "{map:?}");
    assert_eq!(map.len(), 1, "{map:?}");
}

/// The measuring stick for every number the module doc's non-capability argument rests on. Ignored by
/// default: it walks a real checkout, so it is a RECOUNT COMMAND, not a gate — nothing here asserts,
/// because the decision is what is pinned, not any particular corpus.
///
/// `ZZOP_CENSUS_ROOT=<dir> cargo test -p zzop-parser-rust --lib inline_mod_use_census -- --ignored --nocapture`
///
/// `clash_top` counts a nested leaf whose local name is ALREADY bound at file level — by a top-level
/// `use` leaf or by an item ident, since either one is an answer the map already gives correctly and
/// pouring would displace it. `relative` counts `super::`/`self::` heads. `pourable` is the remainder
/// that a naive implementation would add: neither displacing nor depth-anchored.
///
/// Reads, 2026-08-13 — `nested`/`clash_top`/`relative`/`pourable`:
/// `corpus/oss` 111/1/0/110 (5 files) · `crates` 152/40/119/30 · `parser` 90/18/39/48 ·
/// `corpus/x` 4299/786/1579/2183. Every one of `corpus/oss`'s 110 sits in `the-algorithm/navi`;
/// `be-axum` — the only corpus tree whose Rust emits `io.provides` — contributes 0, which is why no
/// finding and no join number could move. These drift as the tree changes and are NOT pinned: the
/// decision rests on the shape (relative heads dominate, yield is one vendored tree), not the digits.
/// ⚠ Count by PARSING, never by grepping `use`: a text scan over-counts roughly threefold here.
#[test]
#[ignore = "recount command for the module doc's census — walks a checkout, asserts nothing"]
fn inline_mod_use_census() {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !matches!(name.as_str(), "target" | ".git" | "node_modules") {
                    rs_files(&path, out);
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }

    /// Every `(local_name, specifier)` a `use` tree binds — the same leaves `walk_use_tree` inserts.
    fn use_leaves(tree: &UseTree, prefix: &[String], out: &mut Vec<(String, String)>) {
        match tree {
            UseTree::Path(p) => {
                let mut next = prefix.to_vec();
                next.push(p.ident.to_string());
                use_leaves(&p.tree, &next, out);
            }
            UseTree::Group(g) => g.items.iter().for_each(|s| use_leaves(s, prefix, out)),
            UseTree::Name(n) => {
                out.push((n.ident.to_string(), joined(prefix, &n.ident.to_string())))
            }
            UseTree::Rename(r) => {
                out.push((r.rename.to_string(), joined(prefix, &r.ident.to_string())))
            }
            UseTree::Glob(_) => out.push(("*".to_string(), prefix.join("::"))),
        }
    }

    /// Names an item list binds at its own level — `use` leaves plus item idents. A poured nested
    /// name that hits this set would displace an answer the map already gives correctly.
    fn bound_names(items: &[Item]) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        for item in items {
            match item {
                Item::Use(u) => {
                    let mut v = Vec::new();
                    use_leaves(&u.tree, &[], &mut v);
                    set.extend(v.into_iter().map(|(local, _)| local));
                }
                Item::Fn(f) => drop(set.insert(f.sig.ident.to_string())),
                Item::Struct(s) => drop(set.insert(s.ident.to_string())),
                Item::Enum(e) => drop(set.insert(e.ident.to_string())),
                Item::Trait(t) => drop(set.insert(t.ident.to_string())),
                Item::Type(t) => drop(set.insert(t.ident.to_string())),
                Item::Const(c) => drop(set.insert(c.ident.to_string())),
                Item::Static(s) => drop(set.insert(s.ident.to_string())),
                Item::Union(u) => drop(set.insert(u.ident.to_string())),
                Item::Mod(m) => drop(set.insert(m.ident.to_string())),
                _ => {}
            }
        }
        set
    }

    fn nested_leaves(items: &[Item], depth: usize, out: &mut Vec<(String, String)>) {
        for item in items {
            match item {
                Item::Use(u) if depth > 0 => use_leaves(&u.tree, &[], out),
                Item::Mod(m) => {
                    if let Some((_, inner)) = &m.content {
                        nested_leaves(inner, depth + 1, out);
                    }
                }
                _ => {}
            }
        }
    }

    // A test's cwd is its CRATE dir, not the repo root, so a relative `ZZOP_CENSUS_ROOT` is anchored
    // at the repo root here — otherwise the documented command walks nothing and reports a confident
    // row of zeros, which is the one failure mode a recount command must not have.
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root");
    let root = match std::env::var("ZZOP_CENSUS_ROOT") {
        Ok(v) if Path::new(&v).is_absolute() => PathBuf::from(v),
        Ok(v) => repo_root.join(v),
        Err(_) => repo_root.to_path_buf(),
    };
    assert!(
        root.is_dir(),
        "ZZOP_CENSUS_ROOT is not a directory: {root:?}"
    );
    let mut files = Vec::new();
    rs_files(&root, &mut files);
    assert!(!files.is_empty(), "no .rs files under {root:?}");

    let (mut nested, mut clash_top, mut relative, mut pourable) =
        (0_usize, 0_usize, 0_usize, 0_usize);
    let mut touched = BTreeSet::new();
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let Some(file) = crate::parse_file(&text) else {
            continue;
        };
        let top = bound_names(&file.items);
        let mut leaves = Vec::new();
        nested_leaves(&file.items, 0, &mut leaves);
        for (local, specifier) in leaves {
            nested += 1;
            let head = specifier.split("::").next().unwrap_or_default();
            let is_relative = head == "super" || head == "self";
            let is_clash = local != "*" && top.contains(&local);
            if is_relative {
                relative += 1;
            }
            if is_clash {
                clash_top += 1;
            }
            if !is_relative && !is_clash {
                pourable += 1;
                touched.insert(path.to_string_lossy().to_string());
            }
        }
    }
    println!(
        "root={} files={} nested={nested} clash_top={clash_top} relative={relative} \
         pourable={pourable} files_pourable={}",
        root.display(),
        files.len(),
        touched.len(),
    );
}

#[test]
fn a_bodiless_mod_nested_inside_an_inline_mod_is_not_an_edge_either() {
    // Same anchor argument, `mod` form: `mod y;` inside `mod x { .. }` lives at `x/y.rs`, so the
    // file-level `self::y` encoding would name the sibling `y.rs` — one segment off, the exact
    // disagreement the `#[path]` section above describes for a different cause.
    let map = parse_imports("mod x {\n    mod y;\n}\n");
    assert!(map.is_empty(), "{map:?}");
}

#[test]
fn external_crate_head_is_recorded_verbatim() {
    let map = parse_imports("use serde::Deserialize;\n");
    let b = binding(&map, "Deserialize");
    assert_eq!(b.specifier, "serde::Deserialize");
}

#[test]
fn use_nested_inside_a_function_body_is_out_of_v1_scope() {
    let map = parse_imports("fn f() {\n    use std::collections::HashMap;\n}\n");
    assert!(map.is_empty(), "{map:?}");
}

#[test]
fn parse_failure_yields_empty_map() {
    assert!(parse_imports("use (:\n").is_empty());
}

#[test]
fn empty_file_yields_empty_map() {
    assert!(parse_imports("").is_empty());
}

// --- `#[path = "..."]` module declarations (see module doc) ---

#[test]
fn path_attr_mod_carries_the_literal_not_the_module_name() {
    // The module is named `tests`; its file is not `tests.rs`. Encoding this as `self::tests` would
    // send the resolver after a file that does not exist — which is what used to happen.
    let map = parse_imports("#[cfg(test)]\n#[path = \"resolve_tests.rs\"]\nmod tests;\n");
    let b = map.get("tests").expect("tests binding");
    assert_eq!(b.specifier, "#path::resolve_tests.rs");
    assert_eq!(b.original, "tests");
}

#[test]
fn a_sibling_cfg_attribute_is_not_mistaken_for_the_path_attribute() {
    // `#[cfg(test)] #[path = ...]` is the dominant pairing in practice, and the attribute list order
    // is the author's choice — neither order may change what is read.
    let reversed = parse_imports("#[path = \"a/b.rs\"]\n#[cfg(test)]\nmod m;\n");
    assert_eq!(reversed.get("m").expect("m").specifier, "#path::a/b.rs");
    let cfg_only = parse_imports("#[cfg(test)]\nmod m;\n");
    assert_eq!(cfg_only.get("m").expect("m").specifier, "self::m");
}

#[test]
fn a_mod_with_no_path_attribute_keeps_the_convention_specifier() {
    let map = parse_imports("mod plain;\n");
    assert_eq!(map.get("plain").expect("plain").specifier, "self::plain");
}

#[test]
fn a_path_attribute_shape_this_parser_does_not_understand_falls_back_to_convention() {
    // Never-guess: an unrecognized attribute form leaves the declaration on the convention path
    // rather than inventing a target from it.
    let map = parse_imports("#[path]\nmod m;\n");
    assert_eq!(map.get("m").expect("m").specifier, "self::m");
}

#[test]
fn a_mod_with_a_body_is_still_not_an_edge_even_with_a_path_attribute() {
    // An inline body means the contents are in THIS file; there is nothing to resolve. The attribute
    // does not change that, and this pins that the `content.is_none()` guard still runs first.
    assert!(parse_imports("#[path = \"x.rs\"]\nmod m { }\n").is_empty());
}
