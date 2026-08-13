//! The two CLASSIFICATION enums of the IR — what a projected declaration is
//! ([`SourceSymbolKind`]) and what makes a store write non-idempotent ([`NonIdempotentKind`]).
//!
//! Split out of `ir.rs` on 2026-08-12 for the repo's per-file line cap, when writing down
//! `SourceSymbolKind`'s per-language collapse pushed that file past it. The seam is a real one rather
//! than a slice at the 300th line: both types are small, closed vocabularies that classify something
//! ELSE in the IR, and neither is referenced by the span/leaf contract that is `ir.rs`'s own subject.

use serde::{Deserialize, Serialize};

/// What a projected declaration IS, in five buckets — and the vocabulary is TypeScript's, which is a
/// fact about this enum's history rather than about the languages it now describes.
///
/// Every other front end COLLAPSES its own vocabulary into these names, and nothing said so until
/// 2026-08-12. That silence is what makes it worth writing down: a `symbol-scan` rule filters on
/// `kind`, so an author who reads `Class` as "a class" writes a rule that also judges Rust structs and
/// Prisma models, and one who reads `Interface` as "an interface" writes one that judges Rust traits.
/// Neither is a bug — the collapse is deliberate, and a per-language kind space would make a portable
/// rule unwritable — but a rule author has to know which question `kind` actually answers.
///
/// Measured from the front ends themselves (`SourceSymbolKind::` sites under `parser/*/src`):
///
/// | | `Class` | `Interface` | `Type` | `Const` | `Function` |
/// |---|---|---|---|---|---|
/// | TypeScript | `class` | `interface` | `type` | `const` | `function` |
/// | Java | `class` · `enum` · `record` | `interface` · `@interface` | — | field | method · ctor |
/// | Rust | `struct` · `enum` · `union` | `trait` | `type` alias | `const` · `static` | `fn` |
/// | Go | `struct` | `interface` | defined type · alias | `const` | `func` |
/// | C# | `class` | `interface` | `type` | `const` | method |
/// | Python | `class` | — | — | module-level binding | `def` |
/// | Prisma | `model` | — | — | — | — |
///
/// The dashes are the other half of the same point: Python projects no `Interface` and no `Type` at
/// all, so a rule gated on either is SILENT there rather than empty-handed — the difference between
/// "no match" and "this question is unaskable here", which the `kind` field alone cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceSymbolKind {
    Function,
    Class,
    Const,
    Type,
    Interface,
}

/// Classifies a store-write call as non-idempotent for `zzop_rules_http::http_scan`'s
/// `non-idempotent-write` rule: a retry of any of these effects is not a no-op. `as_str` gives the
/// wire/label form used both in `Finding::data.kind` and (via serde) in the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NonIdempotentKind {
    /// `create`/`createMany`/`insert` — a retry inserts a duplicate row.
    Create,
    /// An `update`/`updateMany`/`upsert` whose data carries an atomic accumulation op
    /// (`increment`/`decrement`/`push`/`multiply`) — a retry applies the delta again.
    AtomicAccumulate,
    /// A counter-store bump (`incr`/`incrby`/`decr`/`decrby`) — a retry bumps it again.
    Counter,
}

impl NonIdempotentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::AtomicAccumulate => "atomic-accumulate",
            Self::Counter => "counter",
        }
    }
}
