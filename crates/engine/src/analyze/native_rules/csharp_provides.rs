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
    // The run's declared C# route vocabulary — the SAME resolved value the per-file pass read. This pass
    // REPLACES that pass's `http` provides wholesale, so handing it a different vocabulary would make the
    // replacement silently lose (or invent) root-level minimal-API routes.
    vocab: &crate::vocabulary::ResolvedVocabulary<'_>,
    io_provides: &mut Vec<IoProvide>,
    // Where the pass's own SKIPS go. The report has counted them since it shipped and nothing read them:
    // nothing in the workspace read them at all (the one example binary that prints counters reads the
    // JAVA report, whose own counters are still unread — see `provides.rs`'s Java call). A count that
    // reaches no output is not a disclosure.
    warnings: &mut Vec<String>,
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
    let report =
        zzop_parser_csharp::extract_csharp_http_provides_project(&files, &vocab.csharp_routes());
    io_provides.retain(|p| !(p.kind == "http" && csharp_set.contains(p.file.as_str())));
    if let Some(w) = skip_warning(&report) {
        warnings.push(w);
    }
    io_provides.extend(report.provides);
}

/// The pass's own skip census as one `warnings` line, or `None` when it skipped nothing.
///
/// Each counter is a route this build DECIDED not to key rather than one it failed to see, and the
/// difference matters to the reader: the routes exist, the attributes were read, and the value that would
/// have keyed them lives somewhere this pass could not follow. Silence there reads as "this controller has
/// no routes", which is the reading every disclosure in this crate exists to prevent.
fn skip_warning(report: &zzop_parser_csharp::CSharpProjectProvidesReport) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if report.skipped_unresolved_prefix > 0 {
        parts.push(format!(
            "{} controller(s) whose class-level `[Route(CONST)]` prefix named a constant this \
             corpus could not resolve (out-of-corpus, computed, or declared in two places) — ALL \
             of that controller's routes are skipped, since keying them at an empty prefix would \
             put them at the wrong paths",
            report.skipped_unresolved_prefix
        ));
    }
    if report.skipped_unresolved_method_path > 0 {
        parts.push(format!(
            "{} route(s) whose own `[HttpGet(CONST)]`/`[Route(CONST)]` path named a constant this \
             corpus could not resolve — the method-level twin of the same skip",
            report.skipped_unresolved_method_path
        ));
    }

    // The ambiguous-class-name census is a SEPARATE sentence, because it is not route work. The
    // counter increments for every duplicate simple class name in the corpus, controller or not — two
    // `Program` or `Startup` classes in a multi-project solution are the normal case and involve no
    // route at all. Folding it into the route clause read as "89 routes were skipped", which is a
    // number this build never measured.
    let ambiguous = (report.skipped_ambiguous_class_name > 0).then(|| {
        format!(
            " Separately, and NOT a route count: {} class declaration(s) in this corpus share a simple \
             name with another non-partial class, so a qualified reference to that name cannot be told \
             apart and each such declaration is dropped from the corpus index. Most are ordinary \
             same-named types across projects (`Program`, `Startup`, `Constants`) and cost nothing; a \
             CONTROLLER among them would lose its routes, and this build does not tell the two apart.",
            report.skipped_ambiguous_class_name
        )
    });
    if parts.is_empty() {
        return ambiguous.map(|a| a.trim_start().to_string());
    }
    Some(
        format!(
            "C# route extraction skipped work it could see: {}. These routes are NOT reported as \
         absent — they were found and deliberately not keyed, because a route keyed at a guessed \
         path is worse than one this run does not carry. To close it, put the referenced constant \
         in a `const string` this run's scope includes, or supply the routes through an adapter \
         overlay (Mode B). One skip this line does NOT cover: a minimal-API endpoint registered on \
         a GROUP VARIABLE is dropped without being counted at all, so the number above is a floor \
         rather than a total.",
            parts.join("; ")
        ) + ambiguous.as_deref().unwrap_or(""),
    )
}

#[cfg(test)]
mod tests {
    use super::skip_warning;
    use zzop_parser_csharp::CSharpProjectProvidesReport;

    /// A zero census must produce NO line — an unconditional one would train readers to skip the
    /// channel, which is the failure mode every disclosure in this crate is built to avoid.
    #[test]
    fn a_clean_run_says_nothing() {
        assert!(skip_warning(&CSharpProjectProvidesReport::default()).is_none());
    }

    /// Every counter reaches the reader, and the line says what the number MEANS: not "these routes are
    /// absent" but "these routes were found and deliberately not keyed". The floor caveat rides along
    /// because the minimal-API group-variable skip is not counted anywhere yet, so the number is a
    /// lower bound and a reader treating it as a total would under-read the gap.
    #[test]
    fn every_counter_reaches_the_reader_and_the_number_is_declared_a_floor() {
        let report = CSharpProjectProvidesReport {
            skipped_unresolved_prefix: 3,
            skipped_ambiguous_class_name: 89,
            skipped_unresolved_method_path: 7,
            ..Default::default()
        };
        let w = skip_warning(&report).expect("a non-zero census must be disclosed");
        assert!(w.contains("3 controller(s)"), "{w}");
        assert!(w.contains("7 route(s)"), "{w}");
        assert!(w.contains("89 class declaration(s)"), "{w}");
        assert!(w.contains("NOT reported as absent"), "{w}");
        assert!(w.contains("floor rather than a total"), "{w}");
    }
}
