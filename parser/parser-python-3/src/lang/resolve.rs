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
//!   joined from the tree root AND from `src/` (`from_file`'s own directory is irrelevant here — Python
//!   absolute imports always resolve from a top-level package, never relative to the importing file).
//!   The `src/` root is the src-layout that setuptools, poetry and hatch all document as the recommended
//!   project shape; see the comment at the branch for why an extra candidate is free and cannot invent
//!   an edge. On top of those two ecosystem-fact roots, `package_roots` carries the run's DECLARED
//!   `vocabulary.pythonPackageRoots` entries (see [`declared_base`] for the two entry forms) — a project
//!   whose packages resolve from neither the tree root nor `src/` (an interposed `backend/`, or an
//!   editable-install package name symlinked to the tree root) states so instead of losing every
//!   absolute import edge. Relative specifiers get the tree root only — they are already anchored to
//!   `from_file`, so neither `src/` nor a declared root can apply.
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
//! order — built-in roots first, declared roots after, in declared order — see this module's tests for
//! the exact lists.

/// Ordered file-path candidates (tree-relative, POSIX slashes) for a Python import specifier — see
/// module doc for the full semantics. `original` is the imported name from `from X import name`
/// (`Some("c")` in `from a.b import c`); pass `None` for a plain `import a.b.c` or a star import
/// (`original: "*"` on the `ImportBinding` — the caller is expected to translate `"*"` to `None` before
/// calling, since `"*"` never names a real submodule). `package_roots` is the run's declared
/// `vocabulary.pythonPackageRoots` (empty when undeclared — the two built-in roots always apply).
pub fn python_import_candidates(
    specifier: &str,
    original: Option<&str>,
    from_file: &str,
    package_roots: &[&str],
) -> Vec<String> {
    let base = if specifier.starts_with('.') {
        normalize_join(dirname(from_file), specifier)
    } else {
        specifier.replace('.', "/")
    };

    // Absolute dotted specifiers are tried from the tree root AND from `src/` — the src-layout that
    // setuptools, poetry and hatch all document as the recommended project shape, where `mypkg` lives at
    // `src/mypkg/` and is importable as `mypkg` only because the packaging metadata says so. Measured
    // 2026-07-30: without this, a standard src-layout tree resolves ZERO absolute imports (relative ones
    // worked, which is what made the gap look like a Python problem rather than a layout one) — the
    // resolver silently assumed "package name == a directory at the tree root", true for flat layout by
    // coincidence and false the moment one directory is interposed.
    //
    // Reading `pyproject.toml` for the real `where`/`package-dir` value would be the literal-minded fix
    // and is NOT what this needs: candidates here are filtered by the engine against the set of paths
    // that actually exist (`resolve_python_import`'s `all_paths.contains`), so an extra spelling costs
    // one failed set lookup and can never invent an edge. A candidate is a question, not a claim.
    // The DECLARED roots below extend the same safety argument to layouts no heuristic can know
    // (U56, 2026-08-02): a wrong declaration is one more candidate that fails the membership check.
    // Relative specifiers are excluded because they are already anchored to the importing file.
    let mut bases: Vec<String> = vec![base.clone()];
    if !specifier.starts_with('.') {
        bases.push(format!("src/{base}"));
        for entry in package_roots {
            if let Some(declared) = declared_base(entry, specifier) {
                bases.push(declared);
            }
        }
    }

    let mut candidates: Vec<String> = Vec::new();
    for base in &bases {
        if let Some(orig) = original {
            if !orig.is_empty() && orig != "*" && last_segment(base) != orig {
                let sub = join(base, orig);
                candidates.push(format!("{sub}.py"));
                candidates.push(format!("{sub}/__init__.py"));
            }
        }
        if base.is_empty() {
            // A declared package that IS the tree root (`"tml="` with `import tml`): the only file that
            // can mark it is the root `__init__.py` — there is no `<nothing>.py` spelling to try.
            candidates.push("__init__.py".to_string());
        } else {
            candidates.push(format!("{base}.py"));
            candidates.push(format!("{base}/__init__.py"));
        }
    }

    let mut seen = std::collections::HashSet::new();
    candidates.retain(|c| seen.insert(c.clone()));
    candidates
}

/// The extra candidate base one declared `pythonPackageRoots` entry contributes for `specifier`, or
/// `None` when the entry does not apply. Two entry forms, one per layout family the declaration exists
/// for (U56):
///
/// - **`"<dir>"`** (no `=`) — an additional package ROOT: every absolute specifier is also tried under
///   `<dir>/` (`"backend"` makes `app.api.main` try `backend/app/api/main.py`), exactly the shape the
///   built-in `""`/`"src/"` roots already have.
/// - **`"<package>=<dir>"`** — a package-NAME mapping, the editable-install idiom where the import name
///   points at a tree directory that does not carry the name (`ln -s $(pwd) site-packages/tml`):
///   `"tml="` (or `"tml=."`) maps `tml.x.y` to `x/y.py` at the tree root, `"tml=lib"` to `lib/x/y.py`.
///   Applies only to specifiers equal to the package name or dotted under it.
///
/// `<dir>` is tree-relative, POSIX slashes; a leading `./` and a trailing `/` are normalized away and
/// `"."` means the tree root. No validation beyond that, deliberately: a malformed entry produces
/// candidates the engine's membership check never finds, which is the same "a candidate is a question,
/// not a claim" safety every other candidate here rides.
fn declared_base(entry: &str, specifier: &str) -> Option<String> {
    let (package, dir) = match entry.split_once('=') {
        Some((package, dir)) => (package.trim(), dir.trim()),
        None => ("", entry.trim()),
    };
    let rest = if package.is_empty() {
        specifier
    } else if specifier == package {
        ""
    } else {
        specifier.strip_prefix(package)?.strip_prefix('.')?
    };
    let dir = dir.trim_start_matches("./").trim_end_matches('/');
    let dir = if dir == "." { "" } else { dir };
    let path = rest.replace('.', "/");
    Some(if dir.is_empty() {
        path
    } else {
        join(dir, &path)
    })
}

/// POSIX join that tolerates an empty side — `join("", "x") == "x"`, `join("a", "") == "a"` — so the
/// empty-base case a declared root can produce never grows a leading/trailing `/`.
fn join(base: &str, name: &str) -> String {
    if base.is_empty() {
        name.to_string()
    } else if name.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{name}")
    }
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
#[path = "resolve_tests.rs"]
mod tests;
