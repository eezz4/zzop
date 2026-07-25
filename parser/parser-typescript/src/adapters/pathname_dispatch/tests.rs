//! Coverage for `extract_pathname_dispatch_provides`: the canonical corpus shapes (A-D), the
//! switch fallthrough-grouping shape, verb-mention edge cases, every never-guess FP guard, the
//! Durable Object veto, and the pre-gate.
use super::*;

mod branch_symbol;
mod guards;
mod regex;
mod shapes;

fn keys(out: &[IoProvide]) -> Vec<String> {
    out.iter().map(|p| p.key.clone()).collect()
}

/// `(key, symbol)` pairs — the pairing is the point for the per-branch `symbol` rule: two sibling
/// routes of one dispatcher must not carry the same symbol.
fn keyed_symbols(out: &[IoProvide]) -> Vec<(String, Option<String>)> {
    out.iter()
        .map(|p| (p.key.clone(), p.symbol.clone()))
        .collect()
}
