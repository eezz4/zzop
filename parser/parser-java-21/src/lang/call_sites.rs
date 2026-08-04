//! Per-file CALL-SITE projection for Java — the `console-write` and `env-read` families of
//! [`zzop_core::CallSite`], the substrate `zzop_core::dsl::Matcher::CallScan` reads.
//!
//! The channel's own contract (what a site carries, and why there is no `level`/`stream` field) is
//! `zzop_core::call_sites`'s to state. What that contract delegates to each PRODUCER is the boundary:
//! which spellings in THIS language are the family and which are deliberately not. That list is this
//! doc, and it is the whole of this module's judgment.
//!
//! ## What is recognized (wave 2)
//! - **`console-write`** — a `method_invocation` whose receiver is spelled `System.out` or
//!   `System.err` and whose method is one of [`PRINT_STREAM_WRITE_METHODS`]. `callee` is the whole
//!   spelling as written: `System.out.println`, `System.err.printf`. The `out`/`err` half rides in
//!   the callee VERBATIM — it is not folded into a `stream` field, per the channel's false-fold rule
//!   (a rule that cares matches the spelling).
//! - **`env-read`** — a `method_invocation` spelled `System.getenv(...)`. Both the keyed form
//!   (`System.getenv("PORT")`) and the whole-map form (`System.getenv()`) emit: each is a real,
//!   statically witnessed read of the process environment, and the channel carries no argument facts,
//!   so the producer does not ask which — the contract's "the callee resolves, the argument is not
//!   interrogated" line.
//!
//! ## What is recognized (wave 3)
//! - **`process-exec`** — two shapes, and each is a JUDGMENT this doc owes a reason for.
//!   - `Runtime.getRuntime().exec(...)` — a call whose receiver is itself a call. W2's chained-call
//!     bullet says an outer call on a method's RESULT is silent, and that stays true in general;
//!     this is a NAMED EXCEPTION for one FIXED platform chain, because `Runtime.getRuntime()` is the
//!     singleton accessor spelled at the site — the receiver is resolvable by spelling alone, which
//!     is the actual line the channel draws ("the callee resolves, or there is no site"), not
//!     "receivers are never calls". `callee` is `Runtime.getRuntime().exec`, the spelling as
//!     written. An exec through a VARIABLE (`Runtime rt = Runtime.getRuntime(); rt.exec(cmd)`) is
//!     NOT that fixed spelling and stays silent — recall direction, disclosed in the consuming
//!     rule's own message because it is exactly what the retired bare-word regex used to catch.
//!   - `new ProcessBuilder(...)` — an OBJECT CREATION, not a method invocation. Is a constructor a
//!     "call site"? For this channel, yes: `zzop_core::CallSite`'s contract is a statically witnessed
//!     USE of an API family at a line, and Java spells process construction with `new`. Excluding it
//!     would make the family's membership depend on a language's syntax for the same act — the
//!     `console-write` family already spans a method call (`System.out.println`), a builtin
//!     (`print`), and a package function (`fmt.Println`) for exactly this reason. `callee` is
//!     `new ProcessBuilder`, spelled so a rule can tell the two shapes apart. The `.start()` that
//!     follows is NOT a second site: one construction is one process, and `pb.start()`'s receiver is
//!     a variable this producer cannot resolve anyway.
//!
//! ## What is recognized (wave 4)
//! - **`hash-call`** — `MessageDigest.getInstance(...)` on the bare `hash_call::MESSAGE_DIGEST_TYPE`
//!   identifier, the JDK's own digest factory. `callee` is `MessageDigest.getInstance`; `algorithm`
//!   is the first argument ONLY when it is a plain string literal, carried verbatim
//!   (`"MD5"`, `"SHA-1"` — case and hyphen are the author's, so a consuming rule owns its own
//!   normalization). `MessageDigest.getInstance(algoVar)` and a constant reference are `None`:
//!   never-guess, and a rule filtering on algorithm goes silent there rather than approximating.
//!   Deliberately NOT this family: commons-codec's `DigestUtils.md5(...)` (a third-party wrapper —
//!   the same boundary every sibling producer draws), `Cipher.getInstance(...)` (encryption, not a
//!   digest; a cipher family would be its own kind with its own rule), `Mac.getInstance(...)` (an
//!   HMAC's strength is its inner hash's). The QUALIFIED spelling
//!   `java.security.MessageDigest.getInstance(...)` IS recognized — see `hash_call::names_message_digest` for
//!   why this family admits a dotted receiver where the console/exec families do not.
//!
//! ## Deliberate silences — every one of these is a decision, not a gap
//! - **Structured loggers** (slf4j's `log.info(...)`, log4j, `java.util.logging`) are NOT
//!   `console-write` — `zzop_core::CALL_KIND_CONSOLE_WRITE`'s doc owns the reason: configured output
//!   with levels and sinks is not a console write, and a rule banning console writes in a backend is
//!   not banning logging. Folding them in would be a FALSE FOLD, so no logger spelling appears below.
//! - **An aliased stream** (`PrintStream ps = System.out; ps.println(x)`) — the site spells `ps`,
//!   and the check is the spelling at the site, never a data-flow proof. Silent, in the channel's
//!   declared RECALL direction.
//! - **`System.console().printf(...)`** — a different device (the interactive console, null when
//!   detached), reached through a call, not the `System.out`/`System.err` field spelling. Out of v1.
//! - **A fully qualified spelling** (`java.lang.System.out.println`) — v1 requires the bare
//!   `System` identifier at the site, the same "bare receiver" line the TS producer draws for
//!   `window.console.log`. Nobody writes the qualified form in practice; if a corpus proves
//!   otherwise, widening is additive.
//! - **A static-imported stream** (`import static java.lang.System.out;` then `out.println(x)`) —
//!   the site spells `out.println`, which does not name `System`, and the producer claims spellings,
//!   not bindings. The C# producer draws the identical line for `using static System.Console;` bare
//!   `WriteLine`; both are pinned by a negative test. Widening either is a producer+rule change,
//!   never a quiet one.
//! - **A call chained onto a write's RESULT** (`System.out.printf("a").println("b")` — `printf`
//!   returns the `PrintStream`, so the outer `.println` writes too). The INNER call emits; the outer
//!   one's receiver is a method invocation, not the `System.<stream>` field spelling, so it is
//!   silent — the callee-resolvability line, not an oversight. One site per chain, on the spelling
//!   that names the stream.
//! - **`PRINT_STREAM_WRITE_METHODS` scope**: the four that WRITE a message (`print`, `println`,
//!   `printf`, `format` — `printf` literally delegates to `format`). `append`/`write`/`flush` are
//!   byte/char-level plumbing no rule in this build asks about; adding one is additive and costs
//!   nothing but a test.
//!
//! ## Known imprecision, accepted
//! The receiver check is SYNTACTIC — a user-defined class named `System` with an `out` field would
//! produce a site it should not, the same tradeoff the TS producer documents for a local named
//! `console`. That direction is the harmless one for the consuming rules, and shadowing
//! `java.lang.System` is itself vanishing.

mod hash_call;

use hash_call::{first_string_argument, names_message_digest};
use tree_sitter::{Node, TreeCursor};
use zzop_core::{
    CallSite, CALL_KIND_CONSOLE_WRITE, CALL_KIND_ENV_READ, CALL_KIND_HASH_CALL,
    CALL_KIND_PROCESS_EXEC,
};

use crate::util::{line_of, node_text};

/// The `PrintStream` methods that count as a CONSOLE WRITE — the platform's own names
/// (`java.io.PrintStream`), not names a project picks, so built in and not declarable. Scope argued
/// in the module doc.
pub const PRINT_STREAM_WRITE_METHODS: &[&str] = &["print", "println", "printf", "format"];

/// The two `System` stream fields whose writes are console writes. `System.console()` is a call, not
/// a field, and is deliberately not here (module doc).
const SYSTEM_STREAM_FIELDS: &[&str] = &["out", "err"];

/// The `java.lang` type whose construction launches a process — recognized in its `new` form only
/// (module doc's constructor judgment).
const PROCESS_BUILDER_TYPE: &str = "ProcessBuilder";

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
                "method_invocation" => record(node, src, out),
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

/// One `method_invocation`: emits a site iff it is one of the module-doc spellings. The line is the
/// invocation node's own start line (its leftmost token — `System`), so a call broken across lines is
/// attributed where its receiver is.
fn record(call: Node, src: &str, out: &mut Vec<CallSite>) {
    let Some(name) = call.child_by_field_name("name") else {
        return;
    };
    let method = node_text(name, src);
    let object = call.child_by_field_name("object");

    // `System.getenv(...)` — receiver is the bare identifier `System`.
    if method == "getenv"
        && object.is_some_and(|o| o.kind() == "identifier" && node_text(o, src) == "System")
    {
        out.push(CallSite {
            kind: CALL_KIND_ENV_READ.to_string(),
            line: line_of(call),
            callee: "System.getenv".to_string(),
            algorithm: None,
        });
        return;
    }

    // `Runtime.getRuntime().exec(...)` — the ONE fixed platform chain this producer resolves through
    // a call receiver (module doc's named exception). `callee` keeps the whole spelling.
    if method == "exec" && object.is_some_and(|o| is_get_runtime(o, src)) {
        out.push(CallSite {
            kind: CALL_KIND_PROCESS_EXEC.to_string(),
            line: line_of(call),
            callee: "Runtime.getRuntime().exec".to_string(),
            algorithm: None,
        });
        return;
    }

    // `MessageDigest.getInstance("MD5")` — the JCA factory, whose algorithm is a string ARGUMENT, so
    // this is the one Java spelling where the never-guess rule has a dynamic case to refuse.
    if method == "getInstance" && object.is_some_and(|o| names_message_digest(o, src)) {
        out.push(CallSite {
            kind: CALL_KIND_HASH_CALL.to_string(),
            line: line_of(call),
            // The receiver's own spelling, prefix included — `MessageDigest.getInstance` or
            // `java.security.MessageDigest.getInstance`, exactly as written.
            callee: format!(
                "{}.getInstance",
                node_text(object.expect("receiver checked above"), src)
            ),
            algorithm: first_string_argument(call, src),
        });
        return;
    }

    // `System.out.println(...)` / `System.err.printf(...)` — receiver is the field access
    // `System.<out|err>` with the bare `System` identifier underneath.
    if !PRINT_STREAM_WRITE_METHODS.contains(&method) {
        return;
    }
    let Some(stream) = object.filter(|o| o.kind() == "field_access") else {
        return;
    };
    let (Some(obj), Some(field)) = (
        stream.child_by_field_name("object"),
        stream.child_by_field_name("field"),
    ) else {
        return;
    };
    if obj.kind() != "identifier" || node_text(obj, src) != "System" {
        return;
    }
    let field = node_text(field, src);
    if !SYSTEM_STREAM_FIELDS.contains(&field) {
        return;
    }
    // Reconstructed rather than sliced out of the source, and identical to it: every piece is an
    // identifier this function just proved, so no whitespace/comment interleaving can drift it.
    out.push(CallSite {
        kind: CALL_KIND_CONSOLE_WRITE.to_string(),
        line: line_of(call),
        callee: format!("System.{field}.{method}"),
        algorithm: None,
    });
}

/// Is this node the exact spelling `Runtime.getRuntime()` — a no-argument invocation of `getRuntime`
/// on the bare `Runtime` identifier? Spelling only; a `Runtime` shadowed by a user class is the same
/// accepted syntactic imprecision the module doc states for `System`.
fn is_get_runtime(node: Node, src: &str) -> bool {
    if node.kind() != "method_invocation" {
        return false;
    }
    let (Some(name), Some(obj)) = (
        node.child_by_field_name("name"),
        node.child_by_field_name("object"),
    ) else {
        return false;
    };
    node_text(name, src) == "getRuntime"
        && obj.kind() == "identifier"
        && node_text(obj, src) == "Runtime"
}

/// One `new X(...)`: emits a `process-exec` site iff `X` is [`PROCESS_BUILDER_TYPE`] — module doc's
/// constructor judgment. The type must be spelled bare (`new ProcessBuilder`); a qualified
/// `new java.lang.ProcessBuilder` is the same "bare receiver" line every other arm here draws.
fn record_new(node: Node, src: &str, out: &mut Vec<CallSite>) {
    let Some(ty) = node.child_by_field_name("type") else {
        return;
    };
    if ty.kind() != "type_identifier" || node_text(ty, src) != PROCESS_BUILDER_TYPE {
        return;
    }
    out.push(CallSite {
        kind: CALL_KIND_PROCESS_EXEC.to_string(),
        line: line_of(node),
        callee: format!("new {PROCESS_BUILDER_TYPE}"),
        algorithm: None,
    });
}

#[cfg(test)]
mod tests;
