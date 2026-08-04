//! Per-file BOUND-STRING-LITERAL projection for Rust ([`zzop_core::BoundStringLiteral`]) — the
//! substrate `zzop_core::dsl::Matcher::LiteralScan` reads. The channel's contract (hash + entropy,
//! NEVER the value; never-guess on the name) is `zzop_core::string_literals`'s to state; this doc owns
//! which Rust shapes emit and which are deliberately silent.
//!
//! ## What is recognized
//! - **`let` bindings** — `let key = "…";`, typed (`let key: &str = "…";`) or not, `mut` or not.
//! - **`const` items** — `const KEY: &str = "…";`, at module level or inside an `impl` block.
//! - **`static` items** — `static KEY: &str = "…";`.
//!
//! The VALUE is syn's cooked literal value (escapes decoded, raw strings resolved) — the string the
//! program actually ships, same convention as the swc/ruff producers. Anchored on the NAME's line.
//!
//! ## Test regions are NOT skipped here, deliberately
//! The Rust io adapters skip `#[cfg(test)]` subtrees at extraction so the channels they feed stay
//! clean for every consumer. This channel takes the OPPOSITE side, on purpose: its consuming rule
//! class is credential-at-rest (`scan_test_regions: true` — the COMMIT is the leak, a fixture secret
//! still has to be rotated), so extraction must see test code and the per-rule `scan_test_regions`
//! flag decides, per rule, whether `SourceFile::test_spans` subtracts. Skipping here would overrule
//! every rule's declared choice at a layer that cannot see it.
//!
//! ## Deliberate silences
//! - **Assignments** (`x = "v";`, `self.key = "v";`) and **struct literal fields**
//!   (`Config { key: "v" }`) — the declaration-only line every sibling producer draws (only TS
//!   includes object properties, by explicit A17 judgment).
//! - **`let` with destructuring patterns**, **concatenations/`format!`/any macro output** — no single
//!   name, or no literal. A literal inside a MACRO INVOCATION's argument tokens is invisible (syn
//!   parses macro arguments as an opaque `TokenStream` — crate root doc's shared macro scope note);
//!   degrade direction is the channel's declared RECALL.
//! - **`.to_string()` / `String::from("…")` initializers** — the literal is an argument there, not
//!   the initializer expression; v1 keeps the one-literal-node rule. Additive later if measured worth.
//! - **Byte strings** (`b"…"`) — a different type with no text value.

use syn::visit::{self, Visit};
use syn::{Expr, ImplItemConst, ItemConst, ItemStatic, Lit, Local, Pat};

use zzop_core::{shannon_entropy_bits, value_hash_hex, BoundStringLiteral};

/// Extract this file's bound string literals — see module doc. Empty for an unparseable file (the
/// same degrade-to-nothing contract every `extract_*` in this crate upholds). `_rel` is unused (syn
/// parsing needs no filename) — kept to match the engine's uniform `(rel, text)` call convention.
pub fn extract_string_literals(_rel: &str, text: &str) -> Vec<BoundStringLiteral> {
    let Some(file) = crate::parse_file(text) else {
        return Vec::new();
    };
    let mut collector = LiteralCollector { out: Vec::new() };
    collector.visit_file(&file);
    collector.out
}

/// Preorder walk — entries come out in source order, the channel's determinism contract.
struct LiteralCollector {
    out: Vec<BoundStringLiteral>,
}

impl LiteralCollector {
    fn push(&mut self, name: &syn::Ident, value: &str) {
        self.out.push(BoundStringLiteral {
            name: name.to_string(),
            // The crate-shared conversion (`crate::line_of`), never a local reimplementation.
            line: crate::line_of(name),
            value_hash: value_hash_hex(value),
            entropy: shannon_entropy_bits(value),
        });
    }
}

/// The cooked value iff the expression IS a plain string literal (module doc's scope line).
fn plain_str(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(l) => match &l.lit {
            Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        _ => None,
    }
}

/// The single binding identifier of a `let` pattern: a plain ident, or a plain ident behind one type
/// ascription (`let key: &str = …` parses the ident inside a `Pat::Type`). Anything else — tuples,
/// structs, references, or-patterns — has no single name and resolves to nothing.
fn pat_ident(pat: &Pat) -> Option<&syn::Ident> {
    match pat {
        Pat::Ident(p) => Some(&p.ident),
        Pat::Type(t) => match &*t.pat {
            Pat::Ident(p) => Some(&p.ident),
            _ => None,
        },
        _ => None,
    }
}

impl<'ast> Visit<'ast> for LiteralCollector {
    fn visit_local(&mut self, n: &'ast Local) {
        if let (Some(ident), Some(init)) = (pat_ident(&n.pat), &n.init) {
            if let Some(value) = plain_str(&init.expr) {
                self.push(ident, &value);
            }
        }
        visit::visit_local(self, n);
    }

    fn visit_item_const(&mut self, n: &'ast ItemConst) {
        if let Some(value) = plain_str(&n.expr) {
            self.push(&n.ident, &value);
        }
        visit::visit_item_const(self, n);
    }

    fn visit_impl_item_const(&mut self, n: &'ast ImplItemConst) {
        if let Some(value) = plain_str(&n.expr) {
            self.push(&n.ident, &value);
        }
        visit::visit_impl_item_const(self, n);
    }

    fn visit_item_static(&mut self, n: &'ast ItemStatic) {
        if let Some(value) = plain_str(&n.expr) {
            self.push(&n.ident, &value);
        }
        visit::visit_item_static(self, n);
    }
}

#[cfg(test)]
mod tests;
