//! Unit tests for the crate-shared helpers defined in `cross_layer/mod.rs` (the module-root
//! predicates every rule here imports) — split out for file size.

use super::*;

#[test]
fn is_all_slot_path_pins_the_gate_shape() {
    assert!(is_all_slot_path(&["{}"]));
    assert!(is_all_slot_path(&["{}", "{}"]));
    assert!(is_all_slot_path(&[]));
    assert!(!is_all_slot_path(&["users", "{}"]));
}

/// Pins the deliberate DISAGREEMENT with `zzop_core::key_carries_route_identity` at the root, the
/// one input where the two predicates must answer opposite (see both doc comments). Collapsing
/// them would either resurrect the `GET /` shadow false positive or reclassify every root consume
/// miss as `unresolvedConsumes`.
#[test]
fn the_root_path_is_contentless_here_but_route_identity_to_the_linker() {
    assert!(is_all_slot_path(&path_segments("/")));
    assert!(zzop_core::key_carries_route_identity("GET /"));
    // ... and they agree on the all-slot head-drop artifact.
    assert!(is_all_slot_path(&path_segments("/{}")));
    assert!(!zzop_core::key_carries_route_identity("GET /{}"));
}
