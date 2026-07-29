//! Mode A's topology apply + IO freeze — the last thing that touches `io_provides`/`io_consumes`
//! before they become `MinimalIr::io`.
//!
//! Split out of `super::ingest` on 2026-07-29 (line cap) along the seam the two blocks already shared:
//! both are config-declared topology applied to an ORIGIN-AGNOSTIC channel, and both are constrained to
//! sit between "every producer above has finished" and "the vectors are sorted and frozen". Keeping the
//! freeze in the same function as the applies is deliberate — the ordering constraint is between them,
//! and a seam that separated them would let a future pass slip in after the sort.

use zzop_core::{IoConsume, IoFacts, IoProvide};

use crate::EngineConfig;

/// Applies the config-declared topology to both channels, then sorts and freezes them. Returns `None`
/// when the envelope carried no IO at all (the `MinimalIr::io` absent case).
pub(super) fn apply_topology_and_freeze(
    mut io_provides: Vec<IoProvide>,
    mut io_consumes: Vec<IoConsume>,
    config: &EngineConfig,
    warnings: &mut Vec<String>,
) -> Option<IoFacts> {
    // Deployment-topology mount apply (`EngineConfig::mounts`, config-declared) — the Mode A counterpart
    // of `analyze::assemble`'s own call (`analyze/mod.rs`'s placement doc, which this mirrors): must run
    // AFTER every provide-composing step in `super::ingest` (tRPC/router-mount fragment composition,
    // `compose_trpc_provides`/`compose_router_mount_provides`) so a config mount covers every http provide
    // this mode ever produces, and BEFORE `io_provides` is sorted/frozen into `MinimalIr::io` below —
    // deployment topology is origin-agnostic (same rationale Mode B's overlay provides receive mounts
    // under, in `analyze::assemble`), so a tree analyzed via Mode A must not silently freeze un-mounted
    // keys while the native path mounts the same config. See `compose::apply_config_mounts`'s own doc for
    // the winner-selection/validation/zero-effect-tripwire rules.
    crate::analyze::apply_config_mounts(&mut io_provides, &config.mounts, warnings);

    // The CONSUME-side counterpart (`EngineConfig::client_base`), here for the identical reason and under
    // the identical constraint. Mode A runs no code-extracted base pass — there is no sentinel channel in
    // an envelope — so the declared base is the only one a Mode A tree can carry, and `already_prefixed`
    // is empty.
    crate::analyze::apply_config_client_base(
        &mut io_consumes,
        config.client_base.as_deref(),
        &[],
        warnings,
    );

    io_provides.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    io_consumes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    if io_provides.is_empty() && io_consumes.is_empty() {
        return None;
    }
    Some(IoFacts {
        provides: io_provides,
        consumes: io_consumes,
    })
}
