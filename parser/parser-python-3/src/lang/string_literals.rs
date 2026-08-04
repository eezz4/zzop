//! Per-file BOUND-STRING-LITERAL projection for Python ([`zzop_core::BoundStringLiteral`]) — the
//! substrate `zzop_core::dsl::Matcher::LiteralScan` reads. The channel's own contract (hash + entropy,
//! NEVER the value; never-guess on the name) is `zzop_core::string_literals`'s to state; this doc owns
//! which Python binding shapes emit and which are deliberately silent.
//!
//! ## What is recognized
//! - **Assignment to a plain name** — `NAME = "value"`, at module, class or function level. A class
//!   body's `NAME = "value"` IS this shape (Python spells class attributes as assignments), so field
//!   initializers are covered without a separate arm. A chained `a = b = "value"` emits one entry per
//!   plain-name target.
//! - **Annotated assignment** — `NAME: str = "value"`.
//! - The VALUE is ruff's cooked string value, implicit concatenation included (`"ab" "cd"` hashes as
//!   `abcd`) — Python defines adjacent literals as ONE literal, so this is the exact value, not a
//!   guess.
//!
//! ## Deliberate silences
//! - **Attribute targets** (`self.key = "v"`, `cfg.key = "v"`) — an assignment to a member, not a
//!   declaration; the TS producer draws the identical line (`obj.key = "v"` silent) so a rule reading
//!   this channel sees the same population shape per language.
//! - **Tuple/star/subscript targets, dict literals, keyword arguments** — no single binding name
//!   (dict keys are values, not bindings, and unlike TS object literals they are not the idiomatic
//!   home of config constants — Python spells those as module-level assignments, which ARE covered).
//! - **f-strings** (interpolation — not a literal) and **bytes literals** (a different type; a rule
//!   judging text credentials has no cooked TEXT value to hash without choosing a decoding for it).
//!
//! Anchored on the TARGET name's line. Source order by construction (a preorder statement walk visits
//! assignments in source order; one statement emits its targets left to right).

use ruff_python_ast::visitor::{walk_stmt, Visitor};
use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::Ranged;
use zzop_core::{shannon_entropy_bits, value_hash_hex, BoundStringLiteral};

use crate::LineIndex;

/// Extract this file's bound string literals — see module doc. Empty on parse failure (never panics).
/// `_rel` is unused (ruff parsing needs no filename), kept to match the engine's uniform `(rel, text)`
/// projection call convention.
pub fn extract_string_literals(_rel: &str, text: &str) -> Vec<BoundStringLiteral> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let idx = LineIndex::new(text);
    let mut collector = LiteralCollector {
        idx: &idx,
        out: Vec::new(),
    };
    for stmt in &module.body {
        collector.visit_stmt(stmt);
    }
    collector.out
}

struct LiteralCollector<'a> {
    idx: &'a LineIndex,
    out: Vec<BoundStringLiteral>,
}

impl LiteralCollector<'_> {
    fn push(&mut self, name: &str, line: u32, value: &str) {
        self.out.push(BoundStringLiteral {
            name: name.to_string(),
            line,
            value_hash: value_hash_hex(value),
            entropy: shannon_entropy_bits(value),
        });
    }
}

/// The cooked value iff `expr` is a plain string literal (module doc's scope: implicit concatenation
/// included, f-strings and bytes excluded).
fn plain_str(expr: &Expr) -> Option<String> {
    match expr {
        Expr::StringLiteral(s) => Some(s.value.to_str().to_string()),
        _ => None,
    }
}

impl<'a> Visitor<'a> for LiteralCollector<'_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Assign(assign) => {
                if let Some(value) = plain_str(&assign.value) {
                    for target in &assign.targets {
                        if let Expr::Name(name) = target {
                            self.push(name.id.as_str(), self.idx.line_of(name.start()), &value);
                        }
                    }
                }
            }
            Stmt::AnnAssign(ann) => {
                if let (Expr::Name(name), Some(value)) =
                    (&*ann.target, ann.value.as_deref().and_then(plain_str))
                {
                    self.push(name.id.as_str(), self.idx.line_of(name.start()), &value);
                }
            }
            _ => {}
        }
        // Recurse into function/class bodies and every other compound statement.
        walk_stmt(self, stmt);
    }
}

#[cfg(test)]
mod tests;
