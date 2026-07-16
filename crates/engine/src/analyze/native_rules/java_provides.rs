//! The whole-corpus Java Spring HTTP-provides pass — see `run_java_provides_project_pass`'s doc.

use std::collections::HashSet;

use zzop_core::IoProvide;

/// Whole-corpus Java Spring HTTP-provides pass — wires `zzop_parser_java_21::extract_http_provides_project`
/// (see that module's doc for the two per-file-invisible facts it resolves: CE-split `extends`-chain
/// gating, and constant/constant-concatenation class-level `@RequestMapping` prefixes) into `assemble`.
/// Runs once per `analyze_tree` call, over EVERY non-degraded java-dispatched file (`java_rels`),
/// reading each file's text fresh off disk — the fused per-file pass drops each file's text after
/// projecting its own slice, and folding a whole-corpus-dependent result into the per-file cache would
/// let an edit to one file (e.g. a prefix-constants-only file with no routes of its own) leave every
/// OTHER already-cached java file's provides silently stale. Recomputed in full on every call — never
/// consults `zzop_cache::AnalysisCache`.
///
/// **Merge semantics**: `io_provides` already carries the fused per-file pass's own java `http` provides
/// — same-file controllers with a literal (or absent) class-level `@RequestMapping`. The project pass
/// finds a superset of that, with one known exception: a controller whose simple class name is
/// duplicated across the corpus is skipped by the project pass's ambiguous-class guard even when its
/// prefix is literal, so its per-file provides are deleted by this replacement without a project-side
/// substitute (route loss). Accepted: duplicate controller class names are rare, and key-based dedupe
/// instead would leave a latent trap where the two passes silently disagree on one fact and both
/// entries survive. So this REPLACES the per-file java `http` provides wholesale with the project
/// pass's own output, for every file in `java_rels`: one source of truth.
pub(in crate::analyze) fn run_java_provides_project_pass(
    root: &std::path::Path,
    java_rels: &[String],
    io_provides: &mut Vec<IoProvide>,
) {
    let java_set: HashSet<&str> = java_rels.iter().map(String::as_str).collect();
    let mut files: Vec<(String, String)> = Vec::with_capacity(java_rels.len());
    for rel in java_rels {
        // Unreadable (deleted/permission race since the fused pass's own read) — same "treat as absent
        // rather than fail the whole analysis" convention `dead_export_findings` documents for its own
        // disk re-read.
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            files.push((rel.clone(), String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    if files.is_empty() {
        return;
    }
    let report = zzop_parser_java_21::extract_http_provides_project(&files);
    io_provides.retain(|p| !(p.kind == "http" && java_set.contains(p.file.as_str())));
    io_provides.extend(report.provides);
}
