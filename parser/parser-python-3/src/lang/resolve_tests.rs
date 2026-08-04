//! Tests for [`super::python_import_candidates`] — moved out of `resolve.rs` when the declared
//! `pythonPackageRoots` branch landed (U56, 2026-08-02) to keep the source file under the line ratchet.

use super::*;

#[test]
fn relative_sibling_module_import_from_root_level_file() {
    // `from .helpers import x` in a root-level `main.py`.
    assert_eq!(
        python_import_candidates("./helpers", Some("x"), "main.py", &[]),
        vec![
            "helpers/x.py".to_string(),
            "helpers/x/__init__.py".to_string(),
            "helpers.py".to_string(),
            "helpers/__init__.py".to_string(),
        ]
    );
}

#[test]
fn relative_sibling_module_import_from_nested_file() {
    // `from .routers import items` in `app/main.py`.
    assert_eq!(
        python_import_candidates("./routers", Some("items"), "app/main.py", &[]),
        vec![
            "app/routers/items.py".to_string(),
            "app/routers/items/__init__.py".to_string(),
            "app/routers.py".to_string(),
            "app/routers/__init__.py".to_string(),
        ]
    );
}

#[test]
fn relative_parent_walk_normalizes_dot_dot_segments() {
    // `from ..shared import utils` in `app/sub/routes.py`.
    assert_eq!(
        python_import_candidates("../shared", Some("utils"), "app/sub/routes.py", &[]),
        vec![
            "app/shared/utils.py".to_string(),
            "app/shared/utils/__init__.py".to_string(),
            "app/shared.py".to_string(),
            "app/shared/__init__.py".to_string(),
        ]
    );
}

#[test]
fn bare_dot_import_does_not_double_append_the_already_folded_name() {
    // `from . import x` in `app/main.py` -> parse_imports emits specifier "./x", original "x"
    // already folded in; re-appending `original` would spuriously try "app/x/x.py".
    assert_eq!(
        python_import_candidates("./x", Some("x"), "app/main.py", &[]),
        vec!["app/x.py".to_string(), "app/x/__init__.py".to_string()],
    );
}

#[test]
fn no_original_yields_only_plain_module_candidates() {
    // A plain `import a.b.c` binding has `original: "*"` on the ImportMap side; the engine caller
    // translates that to `None` before calling this function.
    assert_eq!(
        python_import_candidates("a.b.c", None, "x.py", &[]),
        vec![
            "a/b/c.py".to_string(),
            "a/b/c/__init__.py".to_string(),
            "src/a/b/c.py".to_string(),
            "src/a/b/c/__init__.py".to_string(),
        ],
    );
}

#[test]
fn star_original_is_treated_the_same_as_none() {
    assert_eq!(
        python_import_candidates("./sib", Some("*"), "a.py", &[]),
        vec!["sib.py".to_string(), "sib/__init__.py".to_string()],
    );
}

#[test]
fn absolute_dotted_module_resolves_from_tree_root_regardless_of_from_file() {
    // `from a.b import c` — resolution ignores `from_file`'s own directory entirely.
    assert_eq!(
        python_import_candidates("a.b", Some("c"), "deep/nested/dir/file.py", &[]),
        vec![
            "a/b/c.py".to_string(),
            "a/b/c/__init__.py".to_string(),
            "a/b.py".to_string(),
            "a/b/__init__.py".to_string(),
            "src/a/b/c.py".to_string(),
            "src/a/b/c/__init__.py".to_string(),
            "src/a/b.py".to_string(),
            "src/a/b/__init__.py".to_string(),
        ]
    );
}

#[test]
fn src_layout_root_is_offered_for_absolute_specifiers_only() {
    // The regression this branch exists for: a standard src-layout tree (`src/mypkg/...`, the shape
    // setuptools/poetry/hatch document) resolved ZERO absolute imports before 2026-07-30, because the
    // candidate list assumed the package name was a directory at the tree root. Relative imports
    // worked throughout, which is what disguised a LAYOUT gap as a Python one.
    let abs = python_import_candidates("mypkg.sub.helper", None, "src/mypkg/main.py", &[]);
    assert!(abs.contains(&"src/mypkg/sub/helper.py".to_string()));
    assert!(abs.contains(&"mypkg/sub/helper.py".to_string()));

    // Relative specifiers are already anchored to the importing file, so a `src/`-prefixed candidate
    // would name a path no import shape can mean. None is offered.
    let rel = python_import_candidates("./sib", None, "src/mypkg/main.py", &[]);
    assert!(rel.iter().all(|c| !c.starts_with("src/src/")));
    assert_eq!(
        rel,
        vec![
            "src/mypkg/sib.py".to_string(),
            "src/mypkg/sib/__init__.py".to_string(),
        ]
    );
}

#[test]
fn bare_single_segment_external_package_still_expands_but_wont_match_in_tree() {
    // `import fastapi` — external package name, expanded the same way; the engine's membership
    // check against its known-paths set is what actually filters this out as unresolvable.
    assert_eq!(
        python_import_candidates("fastapi", None, "app.py", &[]),
        vec![
            "fastapi.py".to_string(),
            "fastapi/__init__.py".to_string(),
            "src/fastapi.py".to_string(),
            "src/fastapi/__init__.py".to_string(),
        ],
    );
}

#[test]
fn candidates_are_deduped() {
    // A pathological case where the submodule-first candidate happens to coincide with the plain
    // module candidate is impossible by construction (guarded by the `last_segment` check), but the
    // dedup pass is still exercised generically via the bare-dot test above.
    let out = python_import_candidates("./x", Some("x"), "main.py", &[]);
    let mut seen = std::collections::HashSet::new();
    assert!(out.iter().all(|c| seen.insert(c.clone())), "{out:?}");
}

// ── Declared `pythonPackageRoots` entries (U56) ─────────────────────────────────────────────────────

#[test]
fn a_plain_dir_entry_adds_one_more_root_after_the_built_ins() {
    // `"backend"` — the interposed-directory layout: `app.api.main` also tried under `backend/`.
    assert_eq!(
        python_import_candidates("app.api.main", None, "backend/app/cli.py", &["backend"]),
        vec![
            "app/api/main.py".to_string(),
            "app/api/main/__init__.py".to_string(),
            "src/app/api/main.py".to_string(),
            "src/app/api/main/__init__.py".to_string(),
            "backend/app/api/main.py".to_string(),
            "backend/app/api/main/__init__.py".to_string(),
        ],
    );
    // Trailing-slash and `./`-prefixed spellings normalize to the same candidates.
    assert_eq!(
        python_import_candidates("app.api.main", None, "x.py", &["backend/"]),
        python_import_candidates("app.api.main", None, "x.py", &["./backend"]),
    );
}

#[test]
fn a_package_mapping_entry_strips_the_package_name_before_joining() {
    // `"tml="` — the editable-install idiom (`ln -s $(pwd) site-packages/tml`): the import name
    // `tml` points at the tree root itself, so `tml.projects.home` means `projects/home`.
    let out = python_import_candidates(
        "tml.projects.home",
        Some("model"),
        "core/train.py",
        &["tml="],
    );
    assert!(
        out.contains(&"projects/home/model.py".to_string()),
        "{out:?}"
    );
    assert!(out.contains(&"projects/home.py".to_string()), "{out:?}");
    // The unmapped tree-root/src candidates are still offered first (built-ins are facts, not knobs).
    assert_eq!(out[0], "tml/projects/home/model.py".to_string());

    // `"tml=."` spells the same mapping; a non-root dir joins under it.
    assert_eq!(
        python_import_candidates("tml.projects.home", None, "x.py", &["tml="]),
        python_import_candidates("tml.projects.home", None, "x.py", &["tml=."]),
    );
    let lib = python_import_candidates("tml.a", None, "x.py", &["tml=lib"]);
    assert!(lib.contains(&"lib/a.py".to_string()), "{lib:?}");
}

#[test]
fn a_package_mapping_applies_only_to_its_own_dotted_subtree() {
    // `tmlx` shares the prefix bytes but is a different package: the mapping must not apply.
    let out = python_import_candidates("tmlx.a", None, "x.py", &["tml="]);
    assert!(
        !out.contains(&"x/a.py".to_string()) && !out.contains(&"a.py".to_string()),
        "{out:?}"
    );

    // `import tml` itself: the only marker the mapped-to tree root can carry is `__init__.py`.
    let root = python_import_candidates("tml", None, "x.py", &["tml="]);
    assert!(root.contains(&"__init__.py".to_string()), "{root:?}");

    // `from tml import x` — submodule-first candidates join from the mapped (empty) base cleanly.
    let from = python_import_candidates("tml", Some("x"), "x.py", &["tml="]);
    assert!(from.contains(&"x.py".to_string()) && from.contains(&"x/__init__.py".to_string()));
}

#[test]
fn declared_roots_never_apply_to_relative_specifiers() {
    // A relative import is already anchored to the importing file; a declared root would name a path
    // no import shape can mean — exactly the rule the built-in `src/` root already follows.
    assert_eq!(
        python_import_candidates("./sib", None, "app/main.py", &["backend", "tml="]),
        python_import_candidates("./sib", None, "app/main.py", &[]),
    );
}

#[test]
fn no_declaration_reproduces_the_pre_u56_absolute_dotted_candidates_exactly() {
    // The safety floor, scoped honestly: for an ABSOLUTE DOTTED specifier, an empty declaration is
    // byte-for-byte the pre-U56 behavior — same candidates, same order — so an undeclared run cannot
    // move on the shape that carries virtually every real absolute import. It is deliberately NOT a
    // claim about every input: the empty-base relative edge DID change (see the next test), and that
    // change is a repair.
    assert_eq!(
        python_import_candidates("a.b", Some("c"), "d/f.py", &[]),
        vec![
            "a/b/c.py".to_string(),
            "a/b/c/__init__.py".to_string(),
            "a/b.py".to_string(),
            "a/b/__init__.py".to_string(),
            "src/a/b/c.py".to_string(),
            "src/a/b/c/__init__.py".to_string(),
            "src/a/b.py".to_string(),
            "src/a/b/__init__.py".to_string(),
        ]
    );
}

#[test]
fn an_empty_relative_base_yields_the_root_marker_candidates() {
    // A relative specifier that resolves to the TREE ROOT itself (`from . import config` in a
    // root-level file — specifier ".", original "config") flows an empty base into the candidate
    // builder. Pinned as NEW behavior, on purpose: before 2026-08-02 an empty base produced
    // `.py`/`/__init__.py`-shaped candidates that could never match any tree path, so this edge
    // surfacing real candidates (`config.py`, `config/__init__.py`, `__init__.py`) is an intended
    // repair, not drift — a candidate is a question, not a claim, and these are the only files that
    // can answer it.
    assert_eq!(
        python_import_candidates(".", Some("config"), "main.py", &[]),
        vec![
            "config.py".to_string(),
            "config/__init__.py".to_string(),
            "__init__.py".to_string(),
        ]
    );
}
