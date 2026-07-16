//! Cross-file / fragment composition — the "fragment now, compose later" passes over data the fused
//! per-file pass already collected (no second parse): late cross-file constant re-resolution for `http`
//! CONSUMEs, tRPC router-fragment composition into `trpc` PROVIDEs, code-registered router-mount
//! composition into `http` PROVIDEs, wrapper-consume joins, controller-prefix route-fragment
//! resolution into `http` PROVIDEs, the NestJS global-prefix apply/strip, and the axios `baseURL`
//! path-prefix apply/strip (the CONSUME-side counterpart of the global-prefix seam).

mod body_refs;
mod client_base;
mod config_mounts;
mod const_map;
mod controller_prefix;
mod global_prefix;
mod router_mounts;
mod trpc;
mod wrapper_consumes;

pub(crate) use body_refs::resolve_provide_body_refs;
pub(crate) use client_base::apply_client_base_prefixes;
pub(crate) use config_mounts::apply_config_mounts;
pub(crate) use const_map::{late_resolve_cross_file_consumes, merge_const_map_fragments};
pub(crate) use controller_prefix::compose_controller_prefix_provides;
pub(super) use global_prefix::apply_and_strip_global_prefix;
pub(crate) use router_mounts::compose_router_mount_provides;
pub(crate) use trpc::compose_trpc_provides;
pub(crate) use wrapper_consumes::resolve_wrapper_consumes;
