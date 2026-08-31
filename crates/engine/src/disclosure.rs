//! Known silent-failure-class registry — zzop's honest, pinned list of the ways its own output can be
//! silently misread (the taxonomy behind the coverage-disclosure feature, Stage 2). An AI consumer must
//! learn not just what zzop found, but which CLASSES of blindness zzop does and does NOT yet actively
//! detect — "meta honesty": zzop never pretends to be silently complete, so even an unknown-unknown
//! leaves the holes in zzop's OWN disclosure visible. Pinned by a meta test (see the `tests` module) so
//! extending the taxonomy without registering the new class fails the gate.
//!
//! DELIVERY, since 2026-07-29 (partial reversal of coverage-disclosure decision 1c): the run reply
//! carries this registry's SHAPE — [`disclosure_counts`], so "gaps exist, and there are this many" is
//! still unmissable without asking — while the PROSE moved to the `disclosure-classes` contract
//! document ([`disclosure_contract_text`], served by `zzop contract` and MCP `resources/read`). The
//! text was ~10.6KB of byte-identical bytes on every call, ~65% of a small tree's whole reply, i.e. a
//! fixed tax on the reader it was written for. Both halves derive from `BLINDNESS_REGISTRY` here, which
//! is what keeps the fold honest; the run-VARYING disclosure channels (`coverage.joinContributionZero`,
//! the `warnings` tripwires) never rode this registry and are untouched.
//!
//! Vocabulary-free by construction: every `summary` describes a MECHANISM (a census fact, a self-report,
//! a low-confidence marker), never a rule-pack id — the registry is meta about detection, not a rule list.

mod types;

pub use types::{BlindnessClass, DisclosureStatus};

/// The pinned registry, in stable order (group extraction -> analysis -> input -> trust, taxonomy order
/// within a group). Statuses reflect what is SHIPPED (the per-tree coverage census + the
/// `joinContributionZero` assertion, the self-report warnings, near-miss matching, low-confidence edge
/// markers). The rows live one file down (`disclosure/registry.rs`) — split on the file-line cap, and
/// re-exported here so every existing `zzop_engine::disclosure::BLINDNESS_REGISTRY` path is unchanged.
mod registry;

/// The pinned silent-failure-class registry, and the ONE way to read the rows. They are assembled from
/// four per-group modules (see `registry`), so there is no single const to hand out — and a function
/// keeps the same call-shape as other engine surfaces (`register_all_native`, etc.) anyway.
pub use registry::blindness_registry;

/// The registry's two DERIVED views — the per-status tallies a run reply carries, and the full text
/// the contract lane serves. Both live one file down (`disclosure/document.rs`), beside the rows
/// themselves, so this file stays the registry's CONTRACT and nothing else.
mod document;

pub use document::{disclosure_contract_text, disclosure_counts};

#[cfg(test)]
mod tests;
