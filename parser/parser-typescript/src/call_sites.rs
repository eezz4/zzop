//! Per-file CALL-SITE projection ([`zzop_core::CallSite`]) for the TypeScript/JavaScript frontend —
//! the `console-write` and `env-read` producers. Sibling of [`crate::extract_loop_spans`]: one parse,
//! one recursive walk, source-order emission, and an AST gate rather than a text regex (which is the
//! whole point of the channel — a regex fires inside string literals and comments, this does not).
//!
//! # What is emitted, and with which spelling
//! - **`console-write`** — a call of `console.<method>(...)` where the receiver is the bare identifier
//!   `console` and `<method>` is in [`crate::CONSOLE_WRITE_METHODS`]. `callee` is the spelling as
//!   written, receiver included: `console.log`, `console.error`. Optional forms (`console?.log(x)`,
//!   `console.log?.(x)`) count — the call is still a console write, only its guard differs.
//! - **`env-read`** — a member access on `process.env` at a KEY POSITION: `process.env.NAME`,
//!   `process.env["NAME"]`, or `process.env[k]` with a computed key. `callee` is `process.env` for all:
//!   this is an access,
//!   not a call, and the variable name is not carried (the channel has no field for it — see
//!   `zzop_core::call_sites`'s "what is deliberately NOT carried" and the design's `env-outside-config`
//!   note: no shipped rule reads which variable was read, so a name field would be a speculative field).
//!
//! # Deliberate silences — each one is a choice, not an oversight
//! - **Structured loggers** (`winston`/`pino`/`bunyan`, or any `logger.info(...)` object that is not
//!   `console`). Folding them into `console-write` would be a FALSE FOLD, and `zzop_core::
//!   CALL_KIND_CONSOLE_WRITE`'s doc owns the reason: configured output with levels and sinks is not a
//!   console write, and a rule banning console writes in a backend is not banning logging.
//! - **`import.meta.env.X`** (Vite/ESM build metadata). It is substituted at BUILD time and reads no
//!   process environment at run time — the same boundary `CALL_KIND_ENV_READ` draws for Rust's `env!()`.
//!   It falls out of the receiver check rather than needing a special case, and a test pins that.
//! - **Dynamic members** — `console[m](...)` emits NOTHING: the callee genuinely cannot be resolved, so
//!   there is no site. `process.env[k]` is the OPPOSITE case and DOES emit: the callee (`process.env`)
//!   is fully resolved and only the KEY is dynamic — and the key is not a field of this channel, so
//!   emitting guesses nothing. The line is therefore "is the callee resolvable", never "is the argument
//!   literal". The Python producer draws it at the same place (`os.environ[k]` emits there too); the two
//!   must not diverge, or a rule reading `env-read` would see different populations per language for no
//!   reason it could state. ⚠ This is WIDER than the line-scan rule it replaces
//!   (`\bprocess\.env\.[A-Za-z0-9_]+` cannot see a bracket form at all), so the migration's detection
//!   delta includes these — measured and disclosed, not silent.
//! - **Bare `process.env`** with no key access at all — `const e = process.env`, `{ PORT } = process.env`,
//!   `Object.keys(process.env)`. Same scope line as above: no key is named at the site. This matches
//!   what the shipped `env-outside-config` line-scan sees today (`\bprocess\.env\.[A-Za-z0-9_]+`), so the
//!   transfer does not lose a population here.
//! - **A non-bare receiver** — `globalThis.console.log(...)`, `window.console.log(...)`, or a `console`
//!   aliased through a local (`const c = console; c.log(x)`). v1 requires the identifier at the site.
//!
//! # The third family lives next door
//! **`process-exec`** (`child_process`'s `exec`/`spawn`/… family) is produced by [`process_exec`],
//! not here, and its own module doc owns every judgment about it. The split is not just line budget:
//! `console`/`process.env` are GLOBALS whose spelling at the site is the whole evidence, while an
//! `exec` is whatever the file BOUND it to — that family therefore resolves local names against this
//! file's own module bindings and stays silent when it cannot, which is a different kind of
//! producer and deserves to be read as one.
//!
//! # Known imprecision, accepted
//! The receiver check is SYNTACTIC — no scope resolution, no type proof, the same tradeoff every adapter
//! in this crate makes. A user-defined local named `console` or `process` therefore produces a site it
//! should not. Rule-side, that direction is the harmless one for both consuming rules (they judge a
//! spelling, and shadowing the platform's `console` to mean something else is itself vanishing).

use std::collections::HashMap;

use swc_core::common::{SourceMap, Span};
use swc_core::ecma::ast::{
    CallExpr, Callee, Expr, ExprOrSpread, MemberExpr, MemberProp, OptCall, OptChainBase,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{
    CallSite, CALL_KIND_CONSOLE_WRITE, CALL_KIND_ENV_READ, CALL_KIND_HASH_CALL,
    CALL_KIND_PROCESS_EXEC,
};

use crate::{line_of, parse_with_cm};

mod hash_call;
mod process_exec;

/// POLICY VOCABULARY — the `console` methods that count as a CONSOLE WRITE, consumed by
/// [`extract_call_sites`] to project [`CALL_KIND_CONSOLE_WRITE`] sites. These are the platform's own
/// names (WHATWG console), not names a project picks, so they are built in and not declarable.
///
/// **Scope, stated so the blanks read as choices**: the six that WRITE a message. `console.table` /
/// `dir` / `group` / `time` / `assert` / `count` are deliberately out — they are diagnostic or
/// formatting affordances, and no rule in this build asks about them; adding one is additive and costs
/// nothing but a test.
///
/// **Do not narrow this list alone.** The consuming rule names its own SUBSET again in its
/// `CallScan::callee_pattern` (a JSON pack cannot reference a Rust constant — the same two-spellings
/// trap [`crate::PROMISE_CONTINUATION_METHODS`] documents). Widening here is safe (the rule's pattern
/// still selects); narrowing silently DELETES findings, because the site the rule's pattern waits for
/// is never projected and nothing turns red.
pub const CONSOLE_WRITE_METHODS: &[&str] = &["log", "error", "warn", "info", "debug", "trace"];

/// Projects this file's call sites in SOURCE ORDER — see `zzop_core::dsl::SourceFile::call_sites` for
/// the contract this mirrors and this module's doc for the recognized idioms and the deliberate
/// silences. Order is part of the contract (the cache round trip pins it), and it is SORTED by source
/// offset rather than trusted from the walk — the Python producer's precedent, needed here for the
/// same reason ruff needed it: swc's visitor walks a node's FIELDS in AST-struct order, which is not
/// source order everywhere. Measured case (pinned in `call_sites_tests`): a `ClassProp`'s
/// `decorators` field sits after its key/value, so `@Dec(console.log(1))` above `x = console.log(2)`
/// walked value-first and emitted line 3 before line 2. Stable sort; same-offset sites keep walk
/// order.
///
/// A file swc cannot parse yields no sites at all, exactly as its span siblings do — the engine gates
/// this projection on `!degraded` anyway, and the double guard costs nothing.
pub fn extract_call_sites(file: &str, source: &str) -> Vec<CallSite> {
    let Some((cm, module)) = parse_with_cm(file, source) else {
        return Vec::new();
    };
    let mut collector = CallSiteCollector {
        cm: &cm,
        exec_bindings: process_exec::child_process_bindings(&module),
        hash_bindings: process_exec::module_bindings(
            &module,
            hash_call::CRYPTO_SPECIFIERS,
            hash_call::CRYPTO_HASH_METHODS,
        ),
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out.sort_by_key(|(lo, _)| *lo);
    collector.out.into_iter().map(|(_, site)| site).collect()
}

struct CallSiteCollector<'a> {
    cm: &'a SourceMap,
    /// This file's own module-level `child_process` bindings — see [`process_exec`] for why this
    /// family needs a resolver where `console-write`/`env-read` (globals) do not.
    exec_bindings: HashMap<String, process_exec::Binding>,
    /// The same, for Node `crypto` — see [`hash_call`].
    hash_bindings: HashMap<String, process_exec::Binding>,
    /// `(site span lo, site)` — the offset exists only to restore source order (fn doc).
    out: Vec<(u32, CallSite)>,
}

impl CallSiteCollector<'_> {
    fn push(&mut self, kind: &str, span: Span, callee: String) {
        self.push_with_algorithm(kind, span, callee, None);
    }

    /// The one push that fills `CallSite::algorithm` — only [`hash_call`] uses it, and only with a
    /// spelling the source actually wrote (that module owns the never-guess list).
    fn push_with_algorithm(
        &mut self,
        kind: &str,
        span: Span,
        callee: String,
        algorithm: Option<String>,
    ) {
        self.out.push((
            span.lo.0,
            CallSite {
                kind: kind.to_string(),
                line: line_of(self.cm, span.lo),
                callee,
                algorithm,
            },
        ));
    }

    /// One call's callee expression: emits a `console-write` site iff it is `console.<method>` spelled
    /// with the bare receiver identifier and a recognized method name. `span` is the CALL's span, so a
    /// call broken across lines is attributed to the line its receiver is on.
    fn record_console(&mut self, callee: &Expr, span: Span) {
        let Some(m) = callee_member(callee) else {
            return;
        };
        let (Expr::Ident(obj), MemberProp::Ident(method)) = (&*m.obj, &m.prop) else {
            return;
        };
        if obj.sym.as_str() != "console" || !CONSOLE_WRITE_METHODS.contains(&method.sym.as_str()) {
            return;
        }
        // Reconstructed rather than sliced out of the source text, and identical to it: both halves are
        // identifiers this arm just proved, so there is no whitespace or comment a `console . log`
        // spelling could hide — `format!` here cannot drift from what was written.
        let callee = format!("console.{}", method.sym);
        self.push(CALL_KIND_CONSOLE_WRITE, span, callee);
    }

    /// One call's callee expression: emits a `process-exec` site iff the callee RESOLVES to one of
    /// this file's `child_process` bindings — see [`process_exec`] for the recognized binding shapes,
    /// the spelling contract, and every deliberate silence.
    fn record_process_exec(&mut self, callee: &Expr, span: Span) {
        if let Some(spelling) = process_exec::exec_callee(callee, &self.exec_bindings) {
            self.push(CALL_KIND_PROCESS_EXEC, span, spelling);
        }
    }

    /// One call: emits a `hash-call` site iff the callee RESOLVES to a Node `crypto` digest binding,
    /// carrying the algorithm ONLY when the first argument spells one — see [`hash_call`].
    fn record_hash_call(&mut self, callee: &Expr, args: &[ExprOrSpread], span: Span) {
        if let Some(spelling) = hash_call::hash_callee(callee, &self.hash_bindings) {
            let algorithm = hash_call::spelled_algorithm(args);
            self.push_with_algorithm(CALL_KIND_HASH_CALL, span, spelling, algorithm);
        }
    }

    /// One member access: emits an `env-read` site iff it reads a NAMED key off `process.env`.
    fn record_env_read(&mut self, m: &MemberExpr) {
        if !is_process_env(&m.obj) {
            return;
        }
        // A key POSITION, resolvable or not. The key itself is never carried, so a computed key is not
        // a guess — it is a read of the process environment at this line either way. Pinned identically
        // in the Python producer (`os.environ[k]`); the two must not diverge, because a rule reading
        // `env-read` would then see different populations per language for no reason it could state.
        let names_a_key = match &m.prop {
            MemberProp::Ident(_) | MemberProp::Computed(_) => true,
            MemberProp::PrivateName(_) => false,
        };
        if names_a_key {
            self.push(CALL_KIND_ENV_READ, m.span, "process.env".to_string());
        }
    }
}

/// The member access a call is calling THROUGH, if it is one. Two spellings reach here for the same
/// call: `console.log(x)` puts the member directly in the callee position, while `console?.log(x)`
/// wraps it in an optional-chain node (`console.log?.(x)` is the third and needs no unwrapping — its
/// optionality is on the CALL, which is already an `OptCall`). An optional-chain CALL in callee position
/// is a different thing entirely (`a?.b()(x)`) and is not unwrapped.
fn callee_member(expr: &Expr) -> Option<&MemberExpr> {
    match expr {
        Expr::Member(m) => Some(m),
        Expr::OptChain(o) => match &*o.base {
            OptChainBase::Member(m) => Some(m),
            OptChainBase::Call(_) => None,
        },
        _ => None,
    }
}

/// Is this expression the `process.env` receiver itself? Bare identifiers only on both halves, which is
/// what excludes `import.meta.env` (its object is a meta-property, not an identifier) without naming it.
fn is_process_env(expr: &Expr) -> bool {
    let Expr::Member(m) = expr else { return false };
    matches!(&*m.obj, Expr::Ident(o) if o.sym.as_str() == "process")
        && matches!(&m.prop, MemberProp::Ident(p) if p.sym.as_str() == "env")
}

impl Visit for CallSiteCollector<'_> {
    fn visit_call_expr(&mut self, n: &CallExpr) {
        if let Callee::Expr(callee) = &n.callee {
            self.record_console(callee, n.span);
            self.record_process_exec(callee, n.span);
            self.record_hash_call(callee, &n.args, n.span);
        }
        n.visit_children_with(self); // recurse: nested calls, and arguments.
    }

    /// The optional-call form (`console?.log(x)`, `console.log?.(x)`) is a distinct AST node from
    /// [`CallExpr`], so it needs its own arm — without it the two spellings would disagree about the
    /// same call for no reason a reader of the source could see.
    fn visit_opt_call(&mut self, n: &OptCall) {
        self.record_console(&n.callee, n.span);
        self.record_process_exec(&n.callee, n.span);
        self.record_hash_call(&n.callee, &n.args, n.span);
        n.visit_children_with(self);
    }

    fn visit_member_expr(&mut self, n: &MemberExpr) {
        self.record_env_read(n);
        // Recursing cannot double-count: `process.env.A.b`'s outer access has `process.env.A` (not
        // `process.env`) as its object, so only the inner one matches.
        n.visit_children_with(self);
    }
}
