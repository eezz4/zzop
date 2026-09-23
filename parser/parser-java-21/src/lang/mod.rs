//! CST -> Common-IR LANGUAGE projection — see crate root doc's "Layout" section.

pub mod call_sites;
pub mod calls;
pub mod imports;
pub mod loop_spans;
pub mod string_literals;
pub mod symbols;
pub mod used_names;

// The R1 counterfactual measurement (review ledger V148). Test-only: it holds every parsed tree
// instead of dropping it, so a poller outside the process can read the other half of the pair.
#[cfg(test)]
mod ast_hold_census;
