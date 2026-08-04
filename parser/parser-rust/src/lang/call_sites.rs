//! Per-file CALL-SITE projection for Rust — the `env-read` family ONLY of [`zzop_core::CallSite`],
//! the substrate `zzop_core::dsl::Matcher::CallScan` reads.
//!
//! The channel's own contract (what a site carries, and why there is no `level`/`stream` field) is
//! `zzop_core::call_sites`'s to state. What that contract delegates to each PRODUCER is the boundary:
//! which spellings in THIS language are the family and which are deliberately not. For Rust that
//! boundary excludes an entire FAMILY, and stating why is most of this doc.
//!
//! ## What is recognized (wave 2)
//! - **`env-read`** — a call of `std::env::var` / `std::env::var_os` (also spelled `env::var` /
//!   `env::var_os` after `use std::env;`) — [`ENV_READ_CALLEES`]. `callee` is the path exactly as
//!   written at the site, so the two spellings of one function stay two spellings (the channel's
//!   original-spelling contract). A dynamic key (`env::var(name)`) still emits — the read point is
//!   real and statically witnessed, and the key is not a field of this channel.
//!
//! ## What is recognized (wave 3)
//! - **`process-exec`** — a call of `std::process::Command::new` / `process::Command::new` /
//!   `Command::new` ([`PROCESS_EXEC_CALLEES`]), `callee` as written, so the three spellings of one
//!   constructor stay three callees (the original-spelling contract, the same call `env-read`
//!   makes). The builder methods that follow (`.arg`, `.spawn`, `.output`, `.status`) are NOT
//!   separate sites — one construction is one process, and their receiver is a value this producer
//!   does not resolve. Deliberately NOT this family: `tokio::process::Command` and every other
//!   third-party runner (not the platform's API, and the consuming rule's argv claim is stated about
//!   `std`'s), and `std::process::exit`/`abort`, which end THIS process rather than launching one.
//!
//! ## What is recognized (wave 4) — and why this family breaks the platform-API rule on purpose
//! - **`hash-call`** — the digest constructors of the crates Rust actually hashes with:
//!   `md5::compute` / `md5::Md5::new` (the `md5` crate), and the RustCrypto `Digest::new` shape
//!   `Md5::new()` / `Sha1::new()` / `Sha256::new()` / `Sha512::new()`, in their bare and
//!   crate-qualified spellings ([`HASH_CALLEES`]). `callee` is the path as written; `algorithm` is the
//!   TYPE or crate segment that names the digest, also as written (`"Md5"`, `"md5"`, `"Sha1"`) — no
//!   argument is read, so the never-guess rule has no dynamic case to refuse here.
//!
//!   Every sibling producer excludes third-party APIs, and this one deliberately does not, because in
//!   Rust that rule would produce an EMPTY family: the standard library ships no cryptographic digest
//!   at all. Applying the boundary literally would make a cross-language rule structurally silent for
//!   one whole language while reading as covered — the "substrate stands, silence remains" failure the
//!   projection contract's same-wave covenant exists to prevent. The exception is therefore what keeps the
//!   cross-language claim TRUE rather than convenient, and it is bounded the same way every other
//!   family is: a fixed spelling list, argued at the constant, widened only with evidence.
//!
//!   Deliberately NOT this family even so: `ring` and `openssl` (whose digest is selected by an
//!   ALGORITHM CONSTANT passed as a value — a different shape, needing argument resolution this
//!   channel does not do), `hmac`/`pbkdf2`/`argon2` (an HMAC's strength is its inner hash's; a KDF's
//!   is its parameters'), and `bcrypt` (an adaptive hash, i.e. the recommended answer rather than the
//!   defect).
//!
//! ## `console-write` is DELIBERATELY NOT PRODUCED — the `println!` judgment
//! In the fact layer `println!`/`eprintln!`/`print!` ARE console writes. They are still not emitted,
//! because the design doc (projection-contract §call-site channel) rules the CONSUMING side out
//! permanently: a console write is a CLI's NORMAL OUTPUT in Rust, so `console-in-be`/`console-in-loop`
//! never admit `.rs` in their `file_pattern` — the same permanent-blank the doc places beside
//! `loop_spans`' Rust `.map` silence. Emitting the fact anyway would carry sites no shipped rule can
//! ever read — exactly the speculative-fact shape the "a channel widens only in the same wave as a
//! rule that reads it" covenant (2026-08-02) exists to forbid. If that judgment is ever reversed,
//! the producer arm is additive and starts here.
//!
//! ## Deliberate silences — every one of these is a decision, not a gap
//! - **`env!()` / `option_env!()`** are NOT `env-read` — they are resolved at COMPILE time and read
//!   no process environment at run time. This is `zzop_core::CALL_KIND_ENV_READ`'s own named
//!   boundary (the same line the TS producer draws for `import.meta.env`), and it needs no special
//!   case in the code: a macro invocation is not a call expression, so it falls out of the walk.
//! - **`log::error!` / `tracing::info!`** are structured loggers AND macros — doubly out.
//! - **A bare `var("X")`** after `use std::env::var;` — its spelling is `var`, a name far too common
//!   to claim (`serde_json::Value::var`, any local helper). The recognized set is exactly what the
//!   consuming rule pins; widening is a rule-side change. Same line Python draws for bare `getenv`.
//! - **`std::env::vars()` / `env::vars_os()`** iterate the WHOLE environment — not the keyed-read
//!   idiom, the same line every sibling producer draws for bulk reads (`os.environ` as a mapping,
//!   `os.Environ()`, `GetEnvironmentVariables()`).
//! - **A re-exported or aliased path** (`use std::env as e; e::var(..)`) spells `e::var` — not a
//!   recognized spelling, silent (recall direction, never a guess).
//! - **A leading-colon path** (`::std::env::var("X")`) — the crate-root-anchored spelling. It is the
//!   same function, but the recognized set is exact spellings and `::std::env::var` is a fifth one;
//!   admitting it means normalizing paths, and normalization is exactly what the original-spelling
//!   contract forbids the producer to start doing. Silent (recall direction), pinned by test.
//! - **Macro-argument positions**: a call written inside a macro invocation's tokens
//!   (`println!("{}", std::env::var("X").unwrap())`) is invisible — syn parses macro arguments as an
//!   opaque `TokenStream` (crate root doc's shared macro scope note), the same silence
//!   `lang::loop_spans` documents for loops inside `tokio::select!`. Degrade direction: recall.
//!
//! ## Known imprecision, accepted
//! The path check is SYNTACTIC — no name resolution, no proof that `env` means `std::env` at the
//! site. A file that defines its own `mod env { pub fn var(..) }` (or imports some other crate's
//! `env` module) therefore produces an `env-read` site it should not — measured, not hypothetical.
//! Same tradeoff every sibling producer documents (TS's local named `console`, Go's local named
//! `fmt`/`os`, Java's user class named `System`): rule-side that direction is the harmless one for
//! the consuming rule, and shadowing `std::env` with a module that reads no environment is itself
//! vanishing.

use syn::visit::{self, Visit};
use syn::{Expr, ExprCall};
use zzop_core::{CallSite, CALL_KIND_ENV_READ, CALL_KIND_HASH_CALL, CALL_KIND_PROCESS_EXEC};

/// The CALLED env-read idioms, spelled as the callee path is spelled. Both the fully qualified and
/// the `use std::env;`-qualified spellings are listed, because both are common and each site keeps
/// its own (module doc). `vars`/`vars_os` (whole-environment iteration) are deliberately absent.
pub const ENV_READ_CALLEES: &[&str] = &[
    "std::env::var",
    "std::env::var_os",
    "env::var",
    "env::var_os",
];

/// The PROCESS-EXEC constructor, in each of the three spellings a file may use for it — the
/// platform's own path (`std::process::Command`), kept per spelling for the same original-spelling
/// reason [`ENV_READ_CALLEES`] is. Builder methods and third-party runners are deliberately absent
/// (module doc).
pub const PROCESS_EXEC_CALLEES: &[&str] = &[
    "std::process::Command::new",
    "process::Command::new",
    "Command::new",
];

/// The digest constructors this producer claims, paired with the algorithm each one NAMES —
/// `(callee spelling, algorithm)`. Both halves are kept verbatim: the spelling is what a rule's
/// `callee_pattern` sees, the algorithm what its `algorithm_pattern` sees, and neither is normalized
/// (`Md5` and `md5` are different spellings of the same digest and stay different here — a consuming
/// rule owns its own case-insensitivity). The module doc argues why a third-party surface is claimed
/// at all in this one language, and which crates are still excluded.
pub const HASH_CALLEES: &[(&str, &str)] = &[
    ("md5::compute", "md5"),
    ("md5::Md5::new", "Md5"),
    ("Md5::new", "Md5"),
    ("md5::Context::new", "md5"),
    ("sha1::Sha1::new", "Sha1"),
    ("Sha1::new", "Sha1"),
    ("sha2::Sha256::new", "Sha256"),
    ("Sha256::new", "Sha256"),
    ("sha2::Sha512::new", "Sha512"),
    ("Sha512::new", "Sha512"),
];

/// Extract this file's call sites — see module doc for the one recognized family, the `println!`
/// judgment, and every deliberate silence. Empty for an unparseable file (the same
/// degrade-to-nothing contract every `extract_*` in this crate upholds). Source order comes free
/// from syn's preorder visit (an emitting call's line is its own leftmost token's). `_rel` is unused
/// (syn parsing needs no filename), kept to match the engine's uniform `(rel, text)` call convention.
pub fn extract_call_sites(_rel: &str, text: &str) -> Vec<CallSite> {
    let Some(file) = crate::parse_file(text) else {
        return Vec::new();
    };
    let mut collector = CallSiteCollector { out: Vec::new() };
    collector.visit_file(&file);
    collector.out
}

struct CallSiteCollector {
    out: Vec<CallSite>,
}

impl<'ast> Visit<'ast> for CallSiteCollector {
    fn visit_expr_call(&mut self, n: &'ast ExprCall) {
        if let Expr::Path(p) = &*n.func {
            // The path's own spelling: plain identifier segments joined with `::`. A path with
            // generic arguments in any segment (`Vec::<u8>::var` cannot be an env read) or a
            // leading `::` is not reassembled — never guessed; the plain join below is exact for
            // every spelling the recognized set can contain.
            let spelling = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            let plain = p.path.leading_colon.is_none()
                && p.path.segments.iter().all(|s| s.arguments.is_none());
            let hash = HASH_CALLEES.iter().find(|(c, _)| *c == spelling);
            let (kind, algorithm) = if !plain {
                (None, None)
            } else if ENV_READ_CALLEES.contains(&spelling.as_str()) {
                (Some(CALL_KIND_ENV_READ), None)
            } else if PROCESS_EXEC_CALLEES.contains(&spelling.as_str()) {
                (Some(CALL_KIND_PROCESS_EXEC), None)
            } else if let Some((_, algo)) = hash {
                (Some(CALL_KIND_HASH_CALL), Some((*algo).to_string()))
            } else {
                (None, None)
            };
            if let Some(kind) = kind {
                self.out.push(CallSite {
                    kind: kind.to_string(),
                    line: crate::line_of(n),
                    callee: spelling,
                    algorithm,
                });
            }
        }
        visit::visit_expr_call(self, n);
    }
}

#[cfg(test)]
mod tests;
