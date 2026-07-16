//! Pure import-specifier -> candidate-file-path builder. No filesystem I/O and no `all_paths`
//! membership check here (unlike `zzop_parser_typescript::resolve`, which owns both the candidate
//! expansion AND the `all_paths` lookup in the same function) — the engine does the membership check
//! against its own known-paths set, since this crate has no visibility into the analysis tree. This
//! split mirrors the "resolve is pure, the engine wires it to a filesystem-derived set" boundary the
//! task brief calls for.
//!
//! ## Candidate semantics
//! - **Relative** (`./sib`, `../a/b` — the slash-relative form `lang::imports::parse_imports` emits):
//!   joined against `dirname(from_file)`, normalized (`.`/`..` segments resolved).
//! - **Absolute dotted** (`a.b.c`) and **bare single-segment** (`fastapi`): the dots become slashes,
//!   joined from the tree root (`from_file`'s own directory is irrelevant here — Python absolute imports
//!   always resolve from a top-level package, never relative to the importing file).
//! - **`original`** (the imported name in `from X import name` — `Some("c")` for `from a.b import c`,
//!   `None`/`Some("*")` for a star import or a plain `import a.b.c`): when present and DISTINCT from the
//!   resolved base path's own last segment, submodule-first candidates (`<base>/<original>.py`,
//!   `<base>/<original>/__init__.py`) are tried BEFORE the plain module candidates (`<base>.py`,
//!   `<base>/__init__.py`) — `from .sib import y` may name either a submodule `sib/y.py` or an attribute
//!   `y` defined inside `sib.py`/`sib/__init__.py`, and the submodule shape is tried first. The "distinct
//!   from the last segment" guard exists for `from . import x` (a bare-dot import with no module name):
//!   `parse_imports` already folds the imported name into the specifier itself (`specifier: "./x"`,
//!   `original: "x"`), so re-appending `original` there would produce a spurious `x/x.py` candidate no
//!   real import shape produces.
//!
//! Every candidate list is deduped (first occurrence kept) and returned in a deterministic, pinned
//! order — see this module's tests for the exact lists.

/// Ordered file-path candidates (tree-relative, POSIX slashes) for a Python import specifier — see
/// module doc for the full semantics. `original` is the imported name from `from X import name`
/// (`Some("c")` in `from a.b import c`); pass `None` for a plain `import a.b.c` or a star import
/// (`original: "*"` on the `ImportBinding` — the caller is expected to translate `"*"` to `None` before
/// calling, since `"*"` never names a real submodule).
pub fn python_import_candidates(
    specifier: &str,
    original: Option<&str>,
    from_file: &str,
) -> Vec<String> {
    let base = if specifier.starts_with('.') {
        normalize_join(dirname(from_file), specifier)
    } else {
        specifier.replace('.', "/")
    };

    let mut candidates: Vec<String> = Vec::new();
    if let Some(orig) = original {
        if !orig.is_empty() && orig != "*" && last_segment(&base) != orig {
            candidates.push(format!("{base}/{orig}.py"));
            candidates.push(format!("{base}/{orig}/__init__.py"));
        }
    }
    candidates.push(format!("{base}.py"));
    candidates.push(format!("{base}/__init__.py"));

    let mut seen = std::collections::HashSet::new();
    candidates.retain(|c| seen.insert(c.clone()));
    candidates
}

/// POSIX dirname: text before the last '/', or "" when there is no '/' (root-level file) — deliberately
/// `""` rather than `"."` so `normalize_join`'s join never introduces a spurious `./` prefix.
fn dirname(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[..i],
        None => "",
    }
}

/// The final `/`-delimited segment of `p` (the whole string when there is no `/`).
fn last_segment(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[i + 1..],
        None => p,
    }
}

/// POSIX join + `.`/`..`-segment normalize, mirroring `zzop_parser_typescript::resolve`'s private
/// `normalize`/dirname-join logic (reimplemented here — that helper is private to its own crate, and
/// this crate stays free of a `zzop-parser-typescript` dependency by design).
fn normalize_join(dir: &str, specifier: &str) -> String {
    let joined = if dir.is_empty() {
        specifier.to_string()
    } else {
        format!("{dir}/{specifier}")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_sibling_module_import_from_root_level_file() {
        // `from .helpers import x` in a root-level `main.py`.
        assert_eq!(
            python_import_candidates("./helpers", Some("x"), "main.py"),
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
            python_import_candidates("./routers", Some("items"), "app/main.py"),
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
            python_import_candidates("../shared", Some("utils"), "app/sub/routes.py"),
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
            python_import_candidates("./x", Some("x"), "app/main.py"),
            vec!["app/x.py".to_string(), "app/x/__init__.py".to_string()],
        );
    }

    #[test]
    fn no_original_yields_only_plain_module_candidates() {
        // A plain `import a.b.c` binding has `original: "*"` on the ImportMap side; the engine caller
        // translates that to `None` before calling this function.
        assert_eq!(
            python_import_candidates("a.b.c", None, "x.py"),
            vec!["a/b/c.py".to_string(), "a/b/c/__init__.py".to_string()],
        );
    }

    #[test]
    fn star_original_is_treated_the_same_as_none() {
        assert_eq!(
            python_import_candidates("./sib", Some("*"), "a.py"),
            vec!["sib.py".to_string(), "sib/__init__.py".to_string()],
        );
    }

    #[test]
    fn absolute_dotted_module_resolves_from_tree_root_regardless_of_from_file() {
        // `from a.b import c` — resolution ignores `from_file`'s own directory entirely.
        assert_eq!(
            python_import_candidates("a.b", Some("c"), "deep/nested/dir/file.py"),
            vec![
                "a/b/c.py".to_string(),
                "a/b/c/__init__.py".to_string(),
                "a/b.py".to_string(),
                "a/b/__init__.py".to_string(),
            ]
        );
    }

    #[test]
    fn bare_single_segment_external_package_still_expands_but_wont_match_in_tree() {
        // `import fastapi` — external package name, expanded the same way; the engine's membership
        // check against its known-paths set is what actually filters this out as unresolvable.
        assert_eq!(
            python_import_candidates("fastapi", None, "app.py"),
            vec!["fastapi.py".to_string(), "fastapi/__init__.py".to_string()],
        );
    }

    #[test]
    fn candidates_are_deduped() {
        // A pathological case where the submodule-first candidate happens to coincide with the plain
        // module candidate is impossible by construction (guarded by the `last_segment` check), but the
        // dedup pass is still exercised generically via the bare-dot test above.
        let out = python_import_candidates("./x", Some("x"), "main.py");
        let mut seen = std::collections::HashSet::new();
        assert!(out.iter().all(|c| seen.insert(c.clone())), "{out:?}");
    }
}
