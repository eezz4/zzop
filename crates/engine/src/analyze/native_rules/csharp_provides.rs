//! The whole-corpus C# ASP.NET Core HTTP-provides pass — see `run_csharp_provides_project_pass`'s doc.

use std::collections::HashSet;

use zzop_core::IoProvide;

/// Whole-corpus C# ASP.NET Core HTTP-provides pass — wires
/// `zzop_parser_csharp::extract_csharp_http_provides_project` (see that module's doc for the per-file-invisible
/// fact it resolves: non-literal route CONSTANTS — a `[HttpGet(Routes.List)]` method path / `[Route(ApiRoutes.
/// Base)]` class prefix whose `const string` lives in another file) into `assemble`, mirroring
/// `run_java_provides_project_pass` exactly. Runs once per `analyze_tree` call over EVERY non-degraded
/// csharp-dispatched file (`csharp_rels`), reading each file's text fresh off disk.
///
/// **Why uncached, recomputed in full every call** (identical reasoning to the Java pass): folding a
/// whole-corpus-dependent result into the per-file cache would let an edit to ONE file (e.g. a
/// route-constants-only `static class Routes`, which declares no routes of its own) leave every OTHER
/// already-cached `.cs` file's provides silently stale. So this never consults `zzop_cache::AnalysisCache`.
///
/// **Cache interplay — no staleness path, no `CACHE_SCHEMA_VERSION` bump needed.** Unlike the Java per-file
/// pass, C# per-file `http` provides ARE cached (the fused pass caches each `.cs` file's `IoFacts`). This pass
/// REPLACES them wholesale: it retains-OUT every `http` provide on a `csharp_rels` file, then extends with its
/// own output computed from a fresh disk read of every such file. So even a `.cs` per-file entry cached under
/// the OLD parser fingerprint (which DROPPED a non-literal route) cannot leave the final C# `http` provides
/// stale — the project pass re-reads that file from disk and re-resolves it regardless of cache state. The
/// only cached per-file provides that survive are those on files NOT in `csharp_rels` (degraded files, which
/// the project pass cannot re-parse anyway), and a degraded file's content hash change re-projects it fresh —
/// so no schema bump is required (the `zzop_parser_csharp::PARSER_FINGERPRINT` bump only forces a harmless
/// one-time re-projection with byte-identical per-file output).
///
/// **Merge semantics** (mostly identical to the Java pass): the project pass finds a SUPERSET of the per-file
/// C# `http` provides (it re-runs the per-file minimal-API producer verbatim AND additionally resolves the
/// attribute-controller constants the per-file pass drops). C#-specific twist: a `partial class` split across
/// files is MERGED into one controller (`zzop_parser_csharp::project`'s "Partial classes"), so its routes are
/// NOT lost. The one accepted exception is narrower than Java's: only GENUINELY-DISTINCT (non-partial) classes
/// that happen to share a simple name are dropped by the ambiguous-class guard even when their prefix is
/// literal, deleting their per-file provides without a project-side substitute. Accepted: two distinct
/// non-partial classes with the same simple name are rare, and this keeps ONE source of truth rather than a
/// latent two-pass disagreement.
pub(in crate::analyze) fn run_csharp_provides_project_pass(
    root: &std::path::Path,
    csharp_rels: &[String],
    io_provides: &mut Vec<IoProvide>,
) {
    let csharp_set: HashSet<&str> = csharp_rels.iter().map(String::as_str).collect();
    let mut files: Vec<(String, String)> = Vec::with_capacity(csharp_rels.len());
    for rel in csharp_rels {
        // Unreadable (deleted/permission race since the fused pass's own read) — treat as absent rather than
        // fail the whole analysis, same convention the Java pass and `dead_export_findings` document.
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            files.push((rel.clone(), String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    if files.is_empty() {
        return;
    }
    let report = zzop_parser_csharp::extract_csharp_http_provides_project(&files);
    io_provides.retain(|p| !(p.kind == "http" && csharp_set.contains(p.file.as_str())));
    io_provides.extend(report.provides);
}
