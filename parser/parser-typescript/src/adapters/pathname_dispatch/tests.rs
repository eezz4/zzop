//! Coverage for `extract_pathname_dispatch_provides`: the canonical corpus shapes (A-D), the
//! switch fallthrough-grouping shape, verb-mention edge cases, every never-guess FP guard, the
//! Durable Object veto, and the pre-gate.
use super::*;

mod guards;
mod regex;
mod shapes;

fn keys(out: &[IoProvide]) -> Vec<String> {
    out.iter().map(|p| p.key.clone()).collect()
}
