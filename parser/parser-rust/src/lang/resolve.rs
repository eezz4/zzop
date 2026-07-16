//! Pure import-specifier -> candidate-file-path builder — the Rust-side counterpart of
//! `zzop_parser_python_3::lang::resolve::python_import_candidates`. No filesystem I/O and no `all_paths`
//! membership check here; the engine does that check against its own known-paths set (same split
//! `python_import_candidates`'s own doc describes).
//!
//! ## File-layout anchoring
//! Rust's module-to-file mapping has two file shapes for "a directory that is itself a module": the
//! legacy `dir/mod.rs`, and the 2018+ `dir.rs` (a file NEXT TO its own `dir/` children directory, not
//! inside it) — collectively "root-shaped" basenames (`lib.rs`/`main.rs`/`mod.rs` — a crate/binary root
//! or an old-style directory module) versus a non-root `foo.rs` file (a 2018+-style directory module
//! whose children live in a SIBLING `foo/` directory, not inside `dirname(F)` itself). Every anchor
//! below is built from ONE shared primitive, `child_anchor_dir`, that answers "where do the child
//! modules of the module THIS FILE represents live?":
//! - root-shaped basename (`lib.rs`/`main.rs`/`mod.rs`) -> children live in `dirname(F)` itself.
//! - non-root `foo.rs` -> children live in `dirname(F)/foo/` (the mandatory Rust-2018 subtlety: a
//!   non-root module file's children are NOT siblings of the file, they are nested under a same-named
//!   directory).
//!
//! `crate::`, `super::`, and `self::`/mod-decl specifiers each anchor differently, but all reduce to
//! `child_anchor_dir` (or its dirname, for `super::`, the parent's own child-anchor-dir):
//! - `self::a` (and a bodiless `mod a;` declaration, which `lang::imports` encodes as `self::a` — see
//!   that module's doc) -> anchor = `child_anchor_dir(F)` directly: "the child-module dir of the module
//!   THIS FILE represents".
//! - `super::a` -> anchor = `dirname(child_anchor_dir(F))`: the PARENT module's own child-anchor-dir is
//!   exactly one path segment above this file's own child-anchor-dir, regardless of whether F itself is
//!   root- or non-root-shaped (both cases fold to the same one-dirname-up computation — see this
//!   module's tests for the algebra).
//! - `crate::a::b` -> anchor = the crate's `src/` root: the prefix of `F` up to and including its
//!   RIGHTMOST `/src/` path segment (a workspace can nest a `src/` inside another crate's tree; the
//!   rightmost occurrence is always the innermost, correct one). When `F` has no `src/` segment at all
//!   (a non-standard layout), the anchor falls back to the tree root `""` — documented limitation, not a
//!   panic.
//! - Any other head (`serde`, `tokio`, a bare crate-relative path with no `crate::`/`super::`/`self::`
//!   keyword) -> treated as EXTERNAL, empty candidate list. A bare `use foo::bar;` (2018+ edition,
//!   ambiguous between "an external crate named `foo`" and "a crate-relative path", a distinction that
//!   requires knowing the whole crate's `extern crate`/dependency graph) is deliberately NOT resolved —
//!   never guessed; only the three unambiguous keyword-prefixed forms are.
//!
//! ## Last-segment ambiguity: item vs. module
//! Mirrors `python_import_candidates`'s own "the imported name may be either a submodule or an attribute
//! defined inside the parent module's own file" ambiguity (that function's doc, "`original`" section).
//! For `crate::a::b`, `b` might be its own module file (`a/b.rs` or `a/b/mod.rs`) OR a plain item
//! declared directly inside `a`'s own file (`a.rs` or `a/mod.rs`). Both interpretations are always
//! emitted, module-shaped candidates first (most specific), in this order:
//! 1. `<anchor>/<rest.joined>.rs`
//! 2. `<anchor>/<rest.joined>/mod.rs`
//! 3. `<anchor>/<rest[..last]>.rs` (or the crate-root's `lib.rs`/`main.rs` pair — see below)
//! 4. `<anchor>/<rest[..last]>/mod.rs` (omitted in the crate-root case)
//!
//! When candidates 3/4's parent path is EXACTLY the crate's `src/` root (i.e. `rest` has a single
//! segment and `anchor == crate_src_root(F)` — true for `crate::ITEM`, for `self::ITEM` written directly
//! inside the crate root file, and for a `super::ITEM` chain that walks back up to the crate root), the
//! crate root is never named `src.rs`/`src/mod.rs` — a crate root is ALWAYS `lib.rs` or `main.rs`
//! specifically. Candidates 3/4 become `<root>/lib.rs` and `<root>/main.rs` in that case instead.

/// Ordered file-path candidates (tree-relative, POSIX slashes) for a Rust `use`/`mod` specifier as
/// `lang::imports::parse_imports` emits it — see module doc for the full semantics. Returns an empty vec
/// for any specifier not headed by `crate`/`super`/`self` (external crates, and any bare unprefixed
/// 2018+ path this crate does not attempt to disambiguate — module doc).
pub fn rust_import_candidates(specifier: &str, from_file: &str) -> Vec<String> {
    let segs: Vec<&str> = specifier.split("::").filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 {
        return Vec::new();
    }
    let (head, rest) = (segs[0], &segs[1..]);
    let anchor = match head {
        "crate" => crate_src_root(from_file),
        "super" => parent_anchor_dir(from_file),
        "self" => child_anchor_dir(from_file),
        _ => return Vec::new(), // external crate head, or an unresolvable bare 2018+ path.
    };

    let full = join_all(&anchor, rest);
    let parent = join_all(&anchor, &rest[..rest.len() - 1]);

    let mut candidates = vec![format!("{full}.rs"), format!("{full}/mod.rs")];
    if rest.len() == 1 && anchor == crate_src_root(from_file) {
        candidates.push(join(&parent, "lib.rs"));
        candidates.push(join(&parent, "main.rs"));
    } else if !parent.is_empty() {
        candidates.push(format!("{parent}.rs"));
        candidates.push(format!("{parent}/mod.rs"));
    }
    dedupe(candidates)
}

/// The directory where the CHILD modules of the module file `f` itself represents live — see module
/// doc's "File-layout anchoring" section. Shared by `self::`/mod-decl resolution directly, and by
/// `super::` resolution (one `dirname` further up).
fn child_anchor_dir(f: &str) -> String {
    let dir = dirname(f);
    let base = basename(f);
    if is_root_basename(base) {
        dir.to_string()
    } else {
        join(dir, file_stem(base))
    }
}

/// The child-anchor-dir of `f`'s own PARENT module — module doc's `super::` bullet.
fn parent_anchor_dir(f: &str) -> String {
    dirname(&child_anchor_dir(f)).to_string()
}

/// The prefix of `from_file` up to and including its rightmost `/src/`-named path segment; `""` when no
/// segment named `src` is present at all (module doc's fallback).
fn crate_src_root(from_file: &str) -> String {
    let segs: Vec<&str> = from_file.split('/').collect();
    match segs.iter().rposition(|&s| s == "src") {
        Some(idx) => segs[..=idx].join("/"),
        None => String::new(),
    }
}

fn dirname(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[..i],
        None => "",
    }
}

fn basename(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[i + 1..],
        None => p,
    }
}

fn file_stem(base: &str) -> &str {
    base.strip_suffix(".rs").unwrap_or(base)
}

fn is_root_basename(base: &str) -> bool {
    matches!(base, "lib.rs" | "main.rs" | "mod.rs")
}

/// POSIX join: `""` for `dir` means "at the tree root" (no spurious leading slash) — same convention
/// `python_import_candidates`'s own dirname/join helpers use.
fn join(dir: &str, seg: &str) -> String {
    if dir.is_empty() {
        seg.to_string()
    } else {
        format!("{dir}/{seg}")
    }
}

fn join_all(base: &str, segs: &[&str]) -> String {
    let mut d = base.to_string();
    for s in segs {
        d = join(&d, s);
    }
    d
}

fn dedupe(candidates: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|c| seen.insert(c.clone()))
        .collect()
}

#[cfg(test)]
mod tests;
