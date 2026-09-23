//! The two provide passes that see the WHOLE PROJECT rather than one file, and the stack they need.
//!
//! ## Why they are one module
//!
//! Java Spring and C# ASP.NET Core both put a route's path somewhere the per-file pass cannot reach: a
//! constant in another file (`[HttpGet(Routes.List)]`), or a mapping that only resolves once the whole
//! type index exists. Each therefore runs after the per-file pass and REPLACES that language's `http`
//! provides wholesale. They are the only two passes in `provides` with that shape, they run adjacently,
//! and they share a hazard the per-file lane does not — see below. Splitting them out kept
//! `provides.rs` under the 300-line cap without inventing a seam: this one was already there.
//!
//! ## The hazard: these run on the CALLING thread
//!
//! A whole-project pass parses, and every parser here is recursive-descent, so a deeply nested file
//! turns source depth into stack depth. The per-file pass got 64 MiB of stack in
//! [`crate::pipeline::with_parse_stack`], and the call-graph pass got it too — this one was the THIRD
//! site and the last to be found. It is why C# kept aborting at nesting depth 1,000, an order of
//! magnitude below every other language, after the other two already had room (2026-09-08, external
//! review round 12, review ledger V99). `zzop coverage` reaches this pass as well, which is how it was
//! isolated from the other two.
//!
//! Java's pass is wrapped for the same reason even though no Java abort was traced to it specifically:
//! it parses the same way, and leaving one of two adjacent passes unwrapped is how the third site went
//! unnoticed in the first place.

use zzop_core::IoProvide;

use crate::analyze::native_rules::{
    run_csharp_provides_project_pass, run_java_provides_project_pass,
};

/// Runs whichever whole-project provide passes this tree has files for, each on a parsing-sized stack.
///
/// Both arms are a no-op when their language contributed no files, so a single-language tree pays one
/// predicate and nothing else.
pub(super) fn run(
    root: &std::path::Path,
    java_rels: &[String],
    csharp_rels: &[String],
    // C# only: `vocabulary.csharpRootRouteBuilderVariableNames` decides which bare `MapGet` receivers are
    // the prefix-free root. Java's pass reads no declared vocabulary — Spring names its routes in
    // annotations, which no project renames.
    vocab: &crate::vocabulary::ResolvedVocabulary<'_>,
    io_provides: &mut Vec<IoProvide>,
    warnings: &mut Vec<String>,
) {
    if !java_rels.is_empty() {
        crate::pipeline::with_parse_stack(|| {
            run_java_provides_project_pass(root, java_rels, io_provides);
        });
    }
    if !csharp_rels.is_empty() {
        crate::pipeline::with_parse_stack(|| {
            run_csharp_provides_project_pass(root, csharp_rels, vocab, io_provides, warnings);
        });
    }
}
