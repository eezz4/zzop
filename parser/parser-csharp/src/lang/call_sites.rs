//! Per-file CALL-SITE projection for C# — the `console-write` and `env-read` families of
//! [`zzop_core::CallSite`], the substrate `zzop_core::dsl::Matcher::CallScan` reads.
//!
//! The channel's own contract (what a site carries, and why there is no `level`/`stream` field) is
//! `zzop_core::call_sites`'s to state. What that contract delegates to each PRODUCER is the boundary:
//! which spellings in THIS language are the family and which are deliberately not. That list is this
//! doc, and it is the whole of this module's judgment.
//!
//! ## What is recognized (wave 2)
//! - **`console-write`** — an invocation whose callee, spelled as a plain dotted identifier chain, is
//!   `Console.Write` / `Console.WriteLine`, or the same pair through the `Console.Error` /
//!   `Console.Out` writer properties — each optionally prefixed `System.`. `callee` is the WHOLE
//!   spelling as written (`System.Console.WriteLine`, `Console.Error.WriteLine`): the `Error`/`Out`
//!   half rides in the callee verbatim rather than in a folded `stream` field, per the channel's
//!   false-fold rule.
//! - **`env-read`** — `Environment.GetEnvironmentVariable(...)`, optionally `System.`-prefixed.
//!   A dynamic name argument still emits (the read point is statically witnessed; the name is not a
//!   field of this channel), matching the TS/Python/Go producers' shared population line.
//!
//! ## What is recognized (wave 3)
//! - **`process-exec`** — `Process.Start(...)` (optionally `System.Diagnostics.`-qualified; every
//!   overload counts, since the channel carries no argument facts and each one launches a process),
//!   and `new ProcessStartInfo(...)` as a CONSTRUCTION. The constructor judgment is the Java
//!   producer's, verbatim and for the same reason: `zzop_core::CallSite` claims a statically
//!   witnessed USE of an API family at a line, and C# spells this act with `new` — making family
//!   membership depend on a language's syntax for the same act is what the cross-language channel
//!   exists to avoid. `callee` is the spelling as written (`Process.Start`,
//!   `new ProcessStartInfo`), so a rule can tell the two apart.
//!
//!   Deliberately NOT sites: `proc.Start()` / `process.Start()` on an INSTANCE — the receiver is a
//!   variable this producer cannot resolve, the same line every other arm here draws, and a
//!   `ProcessStartInfo` construction usually witnesses that launch anyway. Also not: `Process.GetProcessById`
//!   and the rest of the type's non-launching surface.
//!
//! ## What is recognized (wave 4)
//! - **`hash-call`** — two shapes, differing exactly where `CallSite::algorithm`'s never-guess rule
//!   bites. A per-algorithm type's factory ([`HASH_ALGORITHM_TYPES`] × [`HASH_FACTORY_METHOD`],
//!   `MD5.Create()`, `System.Security.Cryptography.SHA1.Create()`) names the algorithm in the TYPE, so
//!   `algorithm` is `Some("MD5")` with no argument read; the generic factories
//!   ([`HASH_GENERIC_FACTORY_TYPES`], `HashAlgorithm.Create("MD5")`) name it in an argument, so
//!   `algorithm` is `Some` only for a plain string literal and `None` for a variable. `callee` is the
//!   chain as written, namespace prefix included. Deliberately NOT this family: `HMACSHA1` and the
//!   rest of the HMAC surface, `Aes`/`TripleDES` (ciphers, not digests), the obsolete
//!   `new MD5CryptoServiceProvider()` construction (a `new`, not a factory — additive if a corpus
//!   shows it), and every third-party hashing package.
//!
//! ## Deliberate silences — every one of these is a decision, not a gap
//! - **`ILogger` and every structured logger** (`_logger.LogInformation(...)`, Serilog's
//!   `Log.Information(...)`, NLog) are NOT `console-write` — `zzop_core::CALL_KIND_CONSOLE_WRITE`'s
//!   doc owns the reason: configured output with levels and sinks is not a console write, and a rule
//!   banning console writes in a backend is not banning logging. No logger spelling appears below.
//! - **`Environment.GetEnvironmentVariables()`** (plural) returns the WHOLE environment — not the
//!   keyed-read idiom this producer names, the same line Python draws for bare `os.environ` and Go
//!   for `os.Environ()`. Silent.
//! - **`Environment.SetEnvironmentVariable(...)`** is a write, not a read. Silent.
//! - **An aliased writer** (`var w = Console.Error; w.WriteLine(x)`) — the site spells `w`, and the
//!   check is the spelling at the site, never a data-flow proof. Silent (recall direction).
//! - **A `using static System.Console;` bare `WriteLine(x)`** — its spelling at the site is
//!   `WriteLine`, not a chain naming `Console`, and the recognized set is exactly what the consuming
//!   rule pins; widening is a rule-side change, not a quiet producer-side one. Same line the Python
//!   producer draws for `from os import getenv` → bare `getenv(...)`.
//! - **`Console.ReadLine`/`Clear`/`Beep`** and the rest of the non-writing surface — not writes; only
//!   [`CONSOLE_WRITE_METHODS`] count.
//!
//! ## Known imprecision, accepted
//! The chain check is SYNTACTIC — a user-defined class named `Console` or `Environment` produces a
//! site it should not, the same tradeoff every sibling producer documents. Rule-side that direction
//! is the harmless one, and shadowing `System.Console` is itself vanishing.

use tree_sitter::{Node, TreeCursor};
use zzop_core::{
    CallSite, CALL_KIND_CONSOLE_WRITE, CALL_KIND_ENV_READ, CALL_KIND_HASH_CALL,
    CALL_KIND_PROCESS_EXEC,
};

use crate::util::{line_of, node_text, string_literal_text, valid_named_children};

/// The `Console` methods that count as a CONSOLE WRITE — the platform's own names
/// (`System.Console` / `System.IO.TextWriter`), not names a project picks, so built in and not
/// declarable. Scope argued in the module doc (the two that WRITE a message).
pub const CONSOLE_WRITE_METHODS: &[&str] = &["Write", "WriteLine"];

/// The `Console` writer properties a write may go through — `Console.Error.WriteLine`,
/// `Console.Out.Write`. The property name rides in the callee verbatim (module doc).
const CONSOLE_WRITER_PROPERTIES: &[&str] = &["Error", "Out"];

/// `System.Diagnostics.Process`'s static launcher, matched as the chain's tail so the optional
/// namespace prefix is not a different fact (module doc).
const PROCESS_START: &str = "Process.Start";

/// The type whose construction configures a process launch — recognized in its `new` form only
/// (module doc's constructor judgment, shared with the Java producer).
const PROCESS_START_INFO_TYPE: &str = "ProcessStartInfo";

/// The static factory method every `System.Security.Cryptography` digest type exposes.
const HASH_FACTORY_METHOD: &str = "Create";

/// The digest TYPES whose own name IS the algorithm (`MD5.Create()` → `"MD5"`) — .NET's own class
/// names. Scope stated so the blanks read as choices: the digests a shipped rule asks about, weak and
/// strong alike, because a family carrying only the weak ones would put the rule's judgment in the
/// producer. `HMACSHA1` and friends are absent — an HMAC's strength is its inner hash's, which this
/// site does not separately spell. `Aes`/`TripleDES` are absent because they are ciphers, not
/// digests; a cipher family would be its own kind with its own consuming rule.
const HASH_ALGORITHM_TYPES: &[&str] = &["MD5", "SHA1", "SHA256", "SHA384", "SHA512"];

/// The GENERIC factories whose algorithm is a string argument rather than the type name — the one C#
/// spelling where the never-guess rule has a dynamic case to refuse.
const HASH_GENERIC_FACTORY_TYPES: &[&str] = &["HashAlgorithm", "CryptoConfig"];

/// Extract this file's call sites — see module doc for the recognized spellings and every deliberate
/// silence. Empty on parse failure (never panics); a partial in-file error skips just that subtree.
/// Source order comes free from the preorder walk (an emitting node's line is its own leftmost
/// token's line). `_rel` is unused (tree-sitter parsing needs no filename), kept to match the
/// engine's uniform `(rel, text)` call convention.
pub fn extract_call_sites(_rel: &str, text: &str) -> Vec<CallSite> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    walk(&mut cursor, text, &mut out);
    out
}

/// Same error/missing-skipping recursive-descent shape as `lang::loop_spans::walk` — preorder.
fn walk(cursor: &mut TreeCursor, src: &str, out: &mut Vec<CallSite>) {
    loop {
        let node = cursor.node();
        if !node.is_error() && !node.is_missing() {
            match node.kind() {
                "invocation_expression" => record(node, src, out),
                "object_creation_expression" => record_new(node, src, out),
                _ => {}
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

/// One invocation: emits a site iff its callee chain is one of the module-doc spellings. The line is
/// the invocation node's own start line (its leftmost token — the chain's first identifier).
fn record(call: Node, src: &str, out: &mut Vec<CallSite>) {
    let Some(func) = call.child_by_field_name("function") else {
        return;
    };
    let Some(spelling) = dotted_chain(func, src) else {
        return; // not a plain identifier chain (`x?.y()`, `a().b()`, ...) — never guessed at.
    };
    // Family membership ignores an optional leading `System.`; the emitted callee keeps the spelling
    // as written, prefix included.
    let bare = spelling.strip_prefix("System.").unwrap_or(&spelling);
    // `System.Diagnostics.Process.Start` strips to `Diagnostics.Process.Start`, so the exec test
    // reads the chain's TAIL rather than an exact bare spelling — the namespace prefix is optional in
    // C# and its presence is not a different fact.
    let mut algorithm = None;
    let kind = if bare == "Environment.GetEnvironmentVariable" {
        CALL_KIND_ENV_READ
    } else if bare == PROCESS_START || bare.ends_with(&format!(".{PROCESS_START}")) {
        CALL_KIND_PROCESS_EXEC
    } else if let Some(algo) = hash_algorithm(bare, call, src) {
        algorithm = algo;
        CALL_KIND_HASH_CALL
    } else if is_console_write(bare) {
        CALL_KIND_CONSOLE_WRITE
    } else {
        return;
    };
    out.push(CallSite {
        kind: kind.to_string(),
        line: line_of(call),
        callee: spelling,
        algorithm,
    });
}

/// The `hash-call` family test for one invocation's `System.`-stripped chain: `Some(algorithm)` when
/// it IS this family, `None` when it is not. The inner `Option` is the never-guess axis (module doc):
/// `Some(Some("MD5"))` for `MD5.Create()` (the TYPE names the algorithm — no argument read) and for
/// `HashAlgorithm.Create("MD5")` with a literal, `Some(None)` for `HashAlgorithm.Create(algoVar)`,
/// where the construction is real but the algorithm is not spelled at the site.
fn hash_algorithm(bare: &str, call: Node, src: &str) -> Option<Option<String>> {
    // `MD5.Create()` / `System.Security.Cryptography.SHA1.Create()` — the chain's LAST TWO segments
    // are `<Algorithm>.Create`, so the tail test accepts the namespace-qualified spelling exactly the
    // way `PROCESS_START`'s does.
    let (head, method) = bare.rsplit_once('.')?;
    if method == HASH_FACTORY_METHOD {
        let type_name = head.rsplit('.').next().unwrap_or(head);
        if HASH_ALGORITHM_TYPES.contains(&type_name) {
            return Some(Some(type_name.to_string()));
        }
        // `HashAlgorithm.Create("MD5")` / `CryptoConfig.CreateFromName("MD5")` — the generic factory,
        // whose algorithm is a string ARGUMENT.
        if HASH_GENERIC_FACTORY_TYPES.contains(&type_name) {
            return Some(first_string_argument(call, src));
        }
    }
    None
}

/// This call's first argument when it is a plain `"…"` string literal — the ONE argument-derived fact
/// this producer reads, and only for the generic hash factory. `None` for a variable, an interpolated
/// or verbatim string (a different grammar node kind), or no argument: never-guess.
fn first_string_argument(call: Node, src: &str) -> Option<String> {
    let args = call.child_by_field_name("arguments")?;
    let first = valid_named_children(args).into_iter().next()?;
    // An `argument` node wraps the expression; unwrap one level before reading the literal.
    let expr = if first.kind() == "argument" {
        valid_named_children(first).into_iter().next()?
    } else {
        first
    };
    string_literal_text(expr, src)
}

/// Is `bare` (the `System.`-stripped chain) one of the recognized console-write spellings —
/// `Console.<method>` or `Console.<Error|Out>.<method>`?
fn is_console_write(bare: &str) -> bool {
    let Some(rest) = bare.strip_prefix("Console.") else {
        return false;
    };
    if CONSOLE_WRITE_METHODS.contains(&rest) {
        return true;
    }
    CONSOLE_WRITER_PROPERTIES.iter().any(|p| {
        rest.strip_prefix(p)
            .and_then(|r| r.strip_prefix('.'))
            .is_some_and(|m| CONSOLE_WRITE_METHODS.contains(&m))
    })
}

/// The dotted spelling of a plain `identifier(.identifier)*` chain, or `None` for any other shape
/// (conditional access, a call in the chain, generics) — never-guess. Reassembly cannot drift from
/// the source: every piece is an identifier this function just proved.
fn dotted_chain(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(node_text(node, src).to_string()),
        "member_access_expression" => {
            let obj = node.child_by_field_name("expression")?;
            let name = node.child_by_field_name("name")?;
            if name.kind() != "identifier" {
                return None;
            }
            Some(format!(
                "{}.{}",
                dotted_chain(obj, src)?,
                node_text(name, src)
            ))
        }
        _ => None,
    }
}

/// One `new X(...)`: emits a `process-exec` site iff `X`'s spelling ends in
/// [`PROCESS_START_INFO_TYPE`] — module doc's constructor judgment. The tail test accepts the
/// namespace-qualified spelling for the same reason [`PROCESS_START`]'s does.
fn record_new(node: Node, src: &str, out: &mut Vec<CallSite>) {
    let Some(ty) = node.child_by_field_name("type") else {
        return;
    };
    let spelling = node_text(ty, src);
    if spelling != PROCESS_START_INFO_TYPE
        && !spelling.ends_with(&format!(".{PROCESS_START_INFO_TYPE}"))
    {
        return;
    }
    out.push(CallSite {
        kind: CALL_KIND_PROCESS_EXEC.to_string(),
        line: line_of(node),
        callee: format!("new {spelling}"),
        algorithm: None,
    });
}

#[cfg(test)]
mod tests;
