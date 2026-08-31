//! Deployment-manifest boundary directories — the DISCLOSURE substrate `duplicate-route` reads, and
//! nothing else in this engine consumes.
//!
//! Disclosure only, never a filter: `.claude` §24 rules that a finding raised on a heuristic is one a
//! reader dismisses in seconds, while a finding DELETED by a heuristic is a real shadow that leaves no
//! trace, so the erasing direction needs a DECLARATION (`trees[]`) and a manifest declares packaging
//! rather than a process. See `zzop_rules_http::duplicate_route::boundary`, which owns both the
//! resolution rule (nearest ancestor wins) and the vocabulary (`is_deployment_manifest`).
//!
//! ## Why this probes the disk instead of reusing an existing index
//! The set used to be derived from `PackageJsonScan`/`GoModuleMap` — two indexes that exist for npm and
//! Go IMPORT RESOLUTION. Deriving a deployment axis from them made it blind to every ecosystem that
//! resolves imports some other way: a Maven monorepo of four Spring Boot applications, each with its own
//! `pom.xml` and its own port, measured ZERO boundaries, and all of its `duplicate-route` findings told
//! the reader to merge handlers that live in different JVMs. The question "is there a manifest here" is
//! not the question either index was built to answer, so it is asked directly.
//!
//! The probe is scoped to the ancestor directories of the sites that could actually straddle a boundary
//! (http provide files), which is the same shape `pipeline::go_module::scan_go_modules` uses for `go.mod`
//! — walk each subject's ancestors, read the manifest position, degrade to nothing on any I/O failure.

use std::collections::BTreeSet;
use std::path::Path;

/// Every tree-relative directory above an http provide site that carries a deployable-unit manifest
/// (`""` = the tree root). A directory that cannot be read contributes nothing rather than failing the
/// run: an unmeasured boundary is a disclosure this rule simply does not make.
pub(super) fn scan(root: &Path, io_provides: &[zzop_core::IoProvide]) -> BTreeSet<String> {
    let mut candidates: BTreeSet<String> = BTreeSet::new();
    for provide in io_provides.iter().filter(|p| p.kind == "http") {
        let mut dir: &str = match provide.file.rfind('/') {
            Some(i) => &provide.file[..i],
            None => "",
        };
        loop {
            candidates.insert(dir.to_string());
            if dir.is_empty() {
                break;
            }
            dir = match dir.rfind('/') {
                Some(i) => &dir[..i],
                None => "",
            };
        }
    }
    candidates
        .into_iter()
        .filter(|dir| {
            let abs = if dir.is_empty() {
                root.to_path_buf()
            } else {
                root.join(dir)
            };
            let Ok(entries) = std::fs::read_dir(abs) else {
                return false;
            };
            entries.filter_map(Result::ok).any(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(zzop_rules_http::is_deployment_manifest)
            })
        })
        .collect()
}
