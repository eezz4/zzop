//! The tail of the CONSUME channel: the last three passes that mutate `io_consumes` before it is
//! frozen, in the one order that is correct.
//!
//! Split out of `super`'s `compose` on 2026-07-29 (line cap) along a seam that was already there. The
//! provide side of that function has many producers whose ordering constraints interleave; this is the
//! whole of the consume side after late const resolution, and its constraints are internal to itself
//! plus one boundary shared with the caller — everything here must run AFTER the per-file collection
//! loop and `late_resolve_cross_file_consumes`, and BEFORE `io_consumes` is frozen into `MinimalIr::io`
//! or read by any whole-tree rule (`unprovided-consume`) or the cross-layer linker.

use std::collections::{BTreeMap, HashMap, HashSet};

use zzop_core::IoConsume;

use crate::analyze::compose::{
    apply_client_base_prefixes, apply_config_client_base, resolve_wrapper_consumes,
};

/// Runs the wrapper-consume join, then the two base-path applies, in that order. `client_base` is
/// `EngineConfig::client_base` (the config-declared `trees[].topology.clientBase`).
#[allow(clippy::too_many_arguments)]
pub(super) fn finish_consumes(
    io_consumes: &mut Vec<IoConsume>,
    wrapper_def_pairs: Vec<(String, Vec<zzop_core::WrapperDefFragment>)>,
    wrapper_call_pairs: Vec<(String, Vec<zzop_core::WrapperCallFragment>)>,
    ts_paths: &HashSet<String>,
    workspace_pkgs: &HashMap<String, zzop_parser_typescript::WorkspacePkg>,
    tsconfigs: &BTreeMap<String, zzop_parser_typescript::TsconfigPaths>,
    client_base: Option<&str>,
    warnings: &mut Vec<String>,
) {
    // Wrapper-consume join — re-anchors HTTP consumes from wrapper internals to real FE call sites
    // (`resolve_wrapper_consumes`'s own doc). Needs the workspace resolver, which is why this whole
    // block sits after the caller has built `pkg_scan`/`tsconfigs`.
    if !wrapper_call_pairs.is_empty() && !wrapper_def_pairs.is_empty() {
        resolve_wrapper_consumes(
            wrapper_def_pairs,
            wrapper_call_pairs,
            |specifier, from_file| {
                zzop_parser_typescript::resolve_file_with_workspace(
                    specifier,
                    from_file,
                    ts_paths,
                    workspace_pkgs,
                    tsconfigs,
                )
            },
            io_consumes,
        );
    }

    // Axios `baseURL` path-prefix apply + strip (`axios-defaults-base-v1`) — the CONSUME-side
    // counterpart of `apply_and_strip_global_prefix`. MUST run after `late_resolve_cross_file_consumes`,
    // which fills `key` IN PLACE and preserves the `client` tag — that tag is the load-bearing reason
    // for the ordering (a late-resolved axios consume still gets the prefix). Sitting after the
    // wrapper-consume join above is only "after the last consume-mutating pass" hygiene:
    // wrapper-emitted consumes carry `client: None` and are DELIBERATELY never prefixed (custom
    // wrappers stay uninterpreted — overlay territory).
    // See `compose::apply_client_base_prefixes`'s own doc for the full placement rationale.
    let code_extracted_bases = apply_client_base_prefixes(io_consumes, warnings);

    // The CONFIG-declared client base (`trees[].topology.clientBase`) — the calling side's mirror of
    // `apply_config_mounts`, and like it the LAST prefix transform on its channel so a declaration is
    // the outermost layer. Runs right after the code-extracted pass so it can name, in its no-effect
    // warning, the clients that already received one — see `compose::apply_config_client_base`'s doc
    // for why a declaration is idempotent where the code-extracted pass is not.
    apply_config_client_base(io_consumes, client_base, &code_extracted_bases, warnings);
}
