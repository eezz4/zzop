//! Per-file CALL-SITE projection for Go — the `console-write` and `env-read` families of
//! [`zzop_core::CallSite`], the substrate `zzop_core::dsl::Matcher::CallScan` reads.
//!
//! The channel's own contract (what a site carries, and why there is no `level`/`stream` field) is
//! `zzop_core::call_sites`'s to state. What that contract delegates to each PRODUCER is the boundary:
//! which spellings in THIS language are the family and which are deliberately not. That list is this
//! doc, and it is the whole of this module's judgment — the code below only matches the spellings
//! named here.
//!
//! ## What is recognized (wave 2)
//! - **`console-write`** — a call of `fmt.Print` / `fmt.Println` / `fmt.Printf`
//!   ([`FMT_PRINT_METHODS`]), and a call of `fmt.Fprint` / `fmt.Fprintln` / `fmt.Fprintf`
//!   ([`FMT_FPRINT_METHODS`]) **whose first argument is spelled `os.Stdout` or `os.Stderr`**
//!   ([`STD_STREAM_SPELLINGS`]). `callee` is the selector as written (`fmt.Println`, `fmt.Fprintf`) —
//!   the stream is an ARGUMENT and the channel carries no argument facts, so for the `Fprint*` trio
//!   the first-argument check is a producer-side FAMILY test (is this a console write at all), not a
//!   captured fact. `fmt.Fprintf(logFile, ...)` and `fmt.Fprintf(w, ...)` are not console writes and
//!   emit nothing — including when `w` is a local ALIAS of `os.Stdout`; the check is the spelling at
//!   the site, never a data-flow proof, so an aliased stream degrades to silence (recall), not to a
//!   guess.
//! - **`env-read`** — a call of `os.Getenv` or `os.LookupEnv` ([`ENV_READ_CALLEES`]), `callee` as
//!   written.
//! - **`process-exec`** (wave 3) — a call of `exec.Command` or `exec.CommandContext`
//!   ([`PROCESS_EXEC_CALLEES`]), `callee` as written. Those are `os/exec`'s two constructors; the
//!   `.Run()`/`.Output()`/`.Start()` that follow are methods on the returned `*exec.Cmd` and are NOT
//!   separate sites — one construction is one process, and emitting the chain's tail as well would
//!   double-count the same fact under a receiver this producer cannot resolve anyway. Deliberately
//!   NOT this family: `syscall.Exec` (a raw syscall wrapper no consuming rule asks about) and any
//!   third-party runner, for the same reason the sibling producers exclude wrappers — a rule's claim
//!   about argv/shell semantics is stated about the platform's API.
//! - **`hash-call`** (wave 4) — a `crypto/*` digest constructor: [`HASH_PACKAGES`] ×
//!   [`HASH_CONSTRUCTORS`] (`md5.New()`, `sha1.Sum(b)`, `sha256.New()`). `callee` is the selector as
//!   written; `algorithm` is the PACKAGE name, also as written (`"md5"`), because Go spells the
//!   digest in the package rather than in an argument — so this producer reads no argument at all and
//!   the never-guess rule has no dynamic case to refuse. An aliased import (`import h "crypto/md5"`)
//!   spells `h.New` and is silent, the same line every other family here draws. Third-party hash
//!   crates and `crypto/hmac` are excluded (the constants' own docs argue both).
//!
//! ## Deliberate silences — every one of these is a decision, not a gap
//! - **`log.Print*` is excluded**, and the design doc's own words are the reason (projection-contract
//!   §call-site channel): *"go `log.Print*` is a boundary case, excluded from v1, with the producer's
//!   module doc disclosing why."* The boundary: the stdlib `log` package DOES default to stderr, but it
//!   is a configurable logger — `log.SetOutput(file)` retargets every `log.Print*` in the process, and
//!   a rule banning console writes is not banning logging (the same FALSE-FOLD line
//!   `zzop_core::CALL_KIND_CONSOLE_WRITE` draws for slf4j/`ILogger`/`winston`). Folding it in would
//!   claim a sink the source never fixed.
//! - **Structured loggers** (`zap`, `logrus`, `slog`, or any `logger.Info(...)`) are NOT
//!   `console-write` — the same false-fold reason, and no such spelling appears below.
//! - **`fmt.Sprint*` / `fmt.Errorf`** build strings and write nothing; not the family.
//! - **`os.Environ()`** returns the WHOLE environment as a slice — not one of the two keyed-read
//!   idioms this producer names, the same line Python's producer draws for bare `os.environ` used as
//!   a mapping. Silent.
//! - **An aliased import** (`import f "fmt"`, `import goos "os"`) rebinds the package name, so the
//!   selector at the site is `f.Println` — not a spelling named here, silent. Same recall-direction
//!   degrade as every other silence in this module.
//! - **A dynamic key** (`os.Getenv(name)`) DOES emit — the read point is real and statically
//!   witnessed; only the key would be a guess, and the key is not a field of this channel. Same
//!   population line as TS's `process.env[k]` and Python's `os.environ[k]`.
//!
//! ## Known imprecision, accepted
//! The selector check is SYNTACTIC — a local variable named `fmt` or `os` shadowing the package
//! produces a site it should not, the same tradeoff the TypeScript producer documents for a local
//! named `console`. Rule-side that direction is the harmless one: shadowing `fmt` to mean something
//! else is itself vanishing.

use tree_sitter::{Node, TreeCursor};
use zzop_core::{
    CallSite, CALL_KIND_CONSOLE_WRITE, CALL_KIND_ENV_READ, CALL_KIND_HASH_CALL,
    CALL_KIND_PROCESS_EXEC,
};

use crate::util::{line_of, valid_named_children};

/// The `fmt` functions that write to STDOUT unconditionally — the platform's own names (Go stdlib),
/// not names a project picks, so they are built in and not declarable. `Sprint*`/`Errorf` build
/// strings and are deliberately out (module doc).
pub const FMT_PRINT_METHODS: &[&str] = &["Print", "Println", "Printf"];

/// The `fmt` functions that write to an EXPLICIT writer — a console write only when that writer is
/// spelled as a standard stream at the site ([`STD_STREAM_SPELLINGS`], module doc).
pub const FMT_FPRINT_METHODS: &[&str] = &["Fprint", "Fprintln", "Fprintf"];

/// The two first-argument spellings that make an `Fprint*` call a console write. A writer reached any
/// other way (a local alias, a field, a function result) is silent — spelling, never data flow.
pub const STD_STREAM_SPELLINGS: &[&str] = &["os.Stdout", "os.Stderr"];

/// The CALLED env-read idioms, spelled as the callee is spelled. `os.Environ` (whole-environment
/// slice) is deliberately absent — module doc.
pub const ENV_READ_CALLEES: &[&str] = &["os.Getenv", "os.LookupEnv"];

/// `os/exec`'s two process constructors, spelled as the callee is spelled — the platform's own names.
/// The `*exec.Cmd` methods that run the built command are deliberately absent (module doc: one
/// construction is one process).
pub const PROCESS_EXEC_CALLEES: &[&str] = &["exec.Command", "exec.CommandContext"];

/// The `crypto/*` stdlib digest packages this producer claims — the package name IS the algorithm
/// (module doc), so this list is both the family gate and the `algorithm` vocabulary. Scope stated so
/// the blanks read as choices: the digests a shipped rule asks about, weak and strong alike, because a
/// family that only carried the weak ones would make "is any digest built here" unanswerable and would
/// put the rule's judgment in the producer. `crypto/hmac` is absent — an HMAC's strength is its inner
/// hash's, which the site does not spell.
pub const HASH_PACKAGES: &[&str] = &["md5", "sha1", "sha256", "sha512"];

/// The `crypto/*` entry points that CONSTRUCT or COMPUTE a digest. `New`/`Sum` are the two the stdlib
/// exposes per package; `NewWithPrefix` and the internal helpers are not.
pub const HASH_CONSTRUCTORS: &[&str] = &["New", "Sum", "Sum224", "Sum256", "Sum384", "Sum512"];

/// Extract this file's call sites — see module doc for the recognized spellings and every deliberate
/// silence. Empty on parse failure (never panics); a partial in-file error skips just that subtree,
/// the same "extract from the valid regions only" discipline every walk in this crate follows.
/// Source-order emission comes free from the preorder walk (an emitting call's line is its own
/// leftmost token's line, so an outer node can never emit after something nested inside it) — the
/// same argument `zzop_parser_typescript::extract_call_sites` makes. `_rel` is unused (tree-sitter
/// parsing needs no filename), kept to match the engine's uniform `(rel, text)` call convention.
pub fn extract_call_sites(_rel: &str, text: &str) -> Vec<CallSite> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    walk(&mut cursor, text, &mut out);
    out
}

/// Same error/missing-skipping recursive-descent shape as `lang::loop_spans::walk` — preorder, so a
/// call nested in another call's arguments emits after its enclosing call.
fn walk(cursor: &mut TreeCursor, src: &str, out: &mut Vec<CallSite>) {
    loop {
        let node = cursor.node();
        if !node.is_error() && !node.is_missing() {
            if node.kind() == "call_expression" {
                record(node, src, out);
            }
            if cursor.goto_first_child() {
                walk(cursor, src, out);
                cursor.goto_parent();
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// One call: emits a site iff its callee is one of the module-doc spellings. The line is the CALL
/// node's own start line (its leftmost token — the package identifier), so a call broken across
/// lines is attributed where its receiver is.
fn record(call: Node, src: &str, out: &mut Vec<CallSite>) {
    let Some(func) = call.child_by_field_name("function") else {
        return;
    };
    let Some((pkg, method)) = package_selector(func, src) else {
        return; // not `pkg.Method` — a bare call, a chained/indexed callee: never guessed at.
    };
    let callee = format!("{pkg}.{method}");
    // The ONLY family that carries an algorithm here, and it is not an argument: Go's stdlib names the
    // digest with the PACKAGE (`md5.New` / `sha1.Sum`), so the spelling at the site IS the algorithm —
    // no argument is read, and the never-guess rule needs no `None` branch for a dynamic one.
    let mut algorithm = None;
    let kind = if ENV_READ_CALLEES.contains(&callee.as_str()) {
        CALL_KIND_ENV_READ
    } else if PROCESS_EXEC_CALLEES.contains(&callee.as_str()) {
        CALL_KIND_PROCESS_EXEC
    } else if HASH_PACKAGES.contains(&pkg.as_str()) && HASH_CONSTRUCTORS.contains(&method.as_str())
    {
        algorithm = Some(pkg.clone());
        CALL_KIND_HASH_CALL
    } else if pkg == "fmt"
        && (FMT_PRINT_METHODS.contains(&method.as_str())
            || (FMT_FPRINT_METHODS.contains(&method.as_str())
                && first_arg_is_std_stream(call, src)))
    {
        CALL_KIND_CONSOLE_WRITE
    } else {
        return;
    };
    out.push(CallSite {
        kind: kind.to_string(),
        line: line_of(call),
        callee,
        algorithm,
    });
}

/// `expr` as a two-segment `identifier.field` selector (`fmt.Println`, `os.Stdout`), or `None` for
/// any other shape — never-guess. Reassembly cannot drift from the source: both halves are single
/// identifiers this match just proved (the same argument the TS producer's `format!` makes).
fn package_selector(expr: Node, src: &str) -> Option<(String, String)> {
    if expr.kind() != "selector_expression" {
        return None;
    }
    let operand = expr.child_by_field_name("operand")?;
    let field = expr.child_by_field_name("field")?;
    if operand.kind() != "identifier" || field.kind() != "field_identifier" {
        return None;
    }
    Some((
        crate::util::node_text(operand, src).to_string(),
        crate::util::node_text(field, src).to_string(),
    ))
}

/// Is the call's FIRST argument spelled `os.Stdout`/`os.Stderr`? Family test for `Fprint*` — module
/// doc owns why this reads a spelling and not a data flow.
fn first_arg_is_std_stream(call: Node, src: &str) -> bool {
    let Some(args) = call.child_by_field_name("arguments") else {
        return false;
    };
    let Some(first) = valid_named_children(args).into_iter().next() else {
        return false;
    };
    package_selector(first, src).is_some_and(|(pkg, field)| {
        STD_STREAM_SPELLINGS.contains(&format!("{pkg}.{field}").as_str())
    })
}

#[cfg(test)]
mod tests;
