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
//! **A basename cannot answer this on its own, and assuming it could was a measured defect.** Cargo lets
//! a manifest name ANY file as a target root (`[[test]] path = "dsl/db/db.rs"`, `[[bin]]`, `[[bench]]`,
//! `[lib]`), and such a file is a CRATE ROOT — its children are siblings, exactly like `lib.rs`. Judged
//! by basename alone, `db.rs` reads as a 2018-style directory module, so `mod queries;` anchored at
//! `dsl/db/db/` where nothing exists, and the item-vs-module fallback below then landed on the declaring
//! file ITSELF. Measured on this repo before the fix: 12 self-edges (one per declared target) and 77
//! files with no incoming edge at all. So `target_roots` — the set of manifest-declared target files,
//! tree-relative — is a REQUIRED input, not an optimization; the engine owns reading the manifests
//! (`pipeline::scan_rust_workspace`) and this module stays pure.
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
//! - `crate::a::b` -> anchor = the crate's root DIRECTORY, taken as the longest of two readings: the
//!   prefix of `F` up to and including its RIGHTMOST `/src/` segment (a workspace can nest a `src/`
//!   inside another crate's tree; the rightmost occurrence is the innermost, correct one), and the
//!   directory of the innermost declared target root `F` sits under. Longest wins because the two nest
//!   in both directions — a `[[bin]] path = "src/bin/tool.rs"` is INSIDE `src/`, while a pack target at
//!   `dsl/db/db.rs` has no `src/` above it at all. When neither reading applies, the anchor falls back to
//!   the tree root `""` — documented limitation, not a panic.
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
//! segment and `anchor == crate_root_dir(F)` — true for `crate::ITEM`, for `self::ITEM` written directly
//! inside the crate root file, and for a `super::ITEM` chain that walks back up to the crate root), the
//! crate root is never named `src.rs`/`src/mod.rs` — a crate root is ALWAYS `lib.rs` or `main.rs`
//! specifically. Candidates 3/4 become `<root>/lib.rs` and `<root>/main.rs` in that case instead.

use std::collections::BTreeSet;

/// Specifier head that marks a `#[path = "..."]` module declaration, whose payload is a literal FILE
/// PATH rather than a `::`-separated module path. `#` cannot begin a Rust identifier, so this head can
/// never collide with a real specifier — which is the whole reason it is spelled with one.
///
/// Exported because two engine sites must recognize it and neither may spell it a second time: the
/// dep-graph resolver (`analyze::assemble::helpers::rust`, which must not fall through to the
/// same-workspace CRATE lookup on it) and the package-import census (`assemble::collect::candidates`,
/// which would otherwise stage `#path` as though it were an external crate name and feed a
/// framework-silence tripwire a crate that does not exist).
pub const PATH_ATTR_HEAD: &str = "#path";

/// `PATH_ATTR_HEAD` plus its separator — the form actually stripped, defined once so the head and the
/// separator cannot drift apart.
const PATH_ATTR_PREFIX: &str = "#path::";

/// Candidates for a `#[path = "<literal>"] mod x;` declaration — see `lang::imports`' own doc for why
/// the literal cannot ride the `self::` path.
///
/// **The literal is relative to the directory containing `from_file`**, and `.`/`..` segments are
/// resolved lexically (no filesystem, same purity contract as the rest of this module). A `..` that
/// would climb ABOVE the tree root yields an empty list rather than a clamped path: the target is
/// genuinely outside the analyzed tree, and inventing an in-tree path for it is exactly the guess this
/// crate refuses. An absolute literal is likewise refused — it cannot be tree-relative.
///
/// **Exactly one candidate, always: the literal itself.** rustc reads a `#[path]` value as a FILENAME
/// verbatim — no extension inference, no `mod.rs` fallback — so an extensionless literal names an
/// extensionless file and nothing else. This returned `<lit>.rs` and `<lit>/mod.rs` for that case until
/// 2026-08-06, which could only ever invent an edge to a file the declaration did not name; the
/// convention path's module-shaped ordering does not apply here, because there is no convention left to
/// apply once an author has spelled the path out.
fn path_attr_candidates(literal: &str, from_file: &str) -> Vec<String> {
    if literal.is_empty() || literal.starts_with('/') || literal.contains(':') {
        return Vec::new();
    }
    let mut stack: Vec<&str> = dirname(from_file)
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    // Bound before the loop: `stack` borrows from it, so it must outlive the walk.
    let normalized = literal.replace('\\', "/");
    for seg in normalized.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Vec::new(); // climbed out of the tree — refuse rather than clamp.
                }
            }
            s => stack.push(s),
        }
    }
    let joined = stack.join("/");
    if joined.is_empty() {
        return Vec::new();
    }
    // rustc reads a `#[path]` literal AS A FILENAME, verbatim — no extension inference and no `mod.rs`
    // fallback. Verified against rustc 1.97.1: `#[path = "foo"]` compiles when an extensionless file
    // `foo` exists, and fails with `couldn't read foo` when only `foo.rs` or only `foo/mod.rs` does. So
    // an extensionless literal has exactly ONE candidate — itself. This branch used to offer
    // `<lit>.rs`/`<lit>/mod.rs`, which cannot be right and can only invent an edge to a file the
    // declaration never named.
    vec![joined]
}

/// Ordered file-path candidates (tree-relative, POSIX slashes) for a Rust `use`/`mod` specifier as
/// `lang::imports::parse_imports` emits it — see module doc for the full semantics. Returns an empty vec
/// for any specifier not headed by `crate`/`super`/`self` (external crates, and any bare unprefixed
/// 2018+ path this crate does not attempt to disambiguate — module doc).
pub fn rust_import_candidates(
    specifier: &str,
    from_file: &str,
    target_roots: &BTreeSet<String>,
) -> Vec<String> {
    if let Some(literal) = specifier.strip_prefix(PATH_ATTR_PREFIX) {
        return path_attr_candidates(literal, from_file);
    }
    let segs: Vec<&str> = specifier.split("::").filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 {
        return Vec::new();
    }
    let (head, rest) = (segs[0], &segs[1..]);
    let anchor = match head {
        "crate" => crate_root_dir(from_file, target_roots),
        "super" => parent_anchor_dir(from_file, target_roots),
        "self" => child_anchor_dir(from_file, target_roots),
        _ => return Vec::new(), // external crate head, or an unresolvable bare 2018+ path.
    };

    let full = join_all(&anchor, rest);
    let parent = join_all(&anchor, &rest[..rest.len() - 1]);

    let mut candidates = vec![format!("{full}.rs"), format!("{full}/mod.rs")];
    if rest.len() == 1 && anchor == crate_root_dir(from_file, target_roots) {
        // "A crate root is always `lib.rs` or `main.rs`" is FALSE for a manifest-declared target, whose
        // root file is whatever the manifest named — so the declared ones are emitted too. IN ADDITION,
        // never instead: a declared target legally COEXISTS with a real `lib.rs`/`main.rs` in the same
        // directory (`[[bin]] path = "src/tool.rs"` beside the lib), and an exclusive-else here removed
        // the convention pair from the candidate list, minting `use crate::Thing` edges onto the
        // declared bin instead of the `lib.rs` that declares the item. Convention first (the common
        // case), declared appended; a candidate that does not exist costs nothing — the consumer
        // (`zzop-engine`'s `resolve_rust_import`) takes the first candidate present in the tree.
        candidates.push(join(&parent, "lib.rs"));
        candidates.push(join(&parent, "main.rs"));
        candidates.extend(
            target_roots
                .iter()
                .filter(|root| dirname(root) == parent)
                .cloned(),
        );
    } else if !parent.is_empty() {
        candidates.push(format!("{parent}.rs"));
        candidates.push(format!("{parent}/mod.rs"));
    }
    dedupe(candidates)
}

/// The directory where the CHILD modules of the module file `f` itself represents live — see module
/// doc's "File-layout anchoring" section. Shared by `self::`/mod-decl resolution directly, and by
/// `super::` resolution (one `dirname` further up).
fn child_anchor_dir(f: &str, target_roots: &BTreeSet<String>) -> String {
    let dir = dirname(f);
    // A manifest-declared target file is a CRATE ROOT whatever it is called, so its children are
    // siblings — the `lib.rs` shape, reached by declaration instead of by name.
    if is_root_basename(basename(f)) || target_roots.contains(f) {
        dir.to_string()
    } else {
        join(dir, file_stem(basename(f)))
    }
}

/// The child-anchor-dir of `f`'s own PARENT module — module doc's `super::` bullet.
fn parent_anchor_dir(f: &str, target_roots: &BTreeSet<String>) -> String {
    dirname(&child_anchor_dir(f, target_roots)).to_string()
}

/// The directory `crate::` anchors at for `from_file` — module doc's `crate::` bullet. The LONGER of two
/// readings, because the two nest in both directions: a declared target can sit inside `src/`
/// (`[[bin]] path = "src/bin/tool.rs"`) or entirely outside it (a DSL pack target with no `src/` above).
fn crate_root_dir(from_file: &str, target_roots: &BTreeSet<String>) -> String {
    let segs: Vec<&str> = from_file.split('/').collect();
    let by_src = match segs.iter().rposition(|&s| s == "src") {
        Some(idx) => segs[..=idx].join("/"),
        None => String::new(),
    };
    let by_declared = target_roots
        .iter()
        .map(|root| dirname(root))
        .filter(|dir| !dir.is_empty() && from_file.starts_with(&format!("{dir}/")))
        .max_by_key(|dir| dir.len())
        .unwrap_or("");
    if by_declared.len() > by_src.len() {
        by_declared.to_string()
    } else {
        by_src
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
