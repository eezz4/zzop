//! One parse, three walks — the fact bundle the `unimported-export` analysis reads straight off disk.
//!
//! `crates/engine`'s `dead_exports` pass runs OUTSIDE `zzop_cache::AnalysisCache`: `FileArtifact`
//! carries none of these three facts, so even a 100%-cache-hit run re-reads and re-parses every
//! dispatched TypeScript file. It used to call [`crate::parse_re_exports`],
//! [`crate::parse_dynamic_imports`] and a third standalone export-alias entrypoint on the same
//! `&str`, which is THREE independent swc parses of one text. This entrypoint parses once and runs
//! the three existing walks over that single `Module`.
//!
//! For two of the three facts it is an ADDITION, not a replacement: `parse_re_exports` and
//! `parse_dynamic_imports` keep their own callers (`project.rs`'s Common-IR build and
//! `pipeline::fresh`'s projector table) and are untouched. The third had no caller left once this
//! bundle existed, so its standalone shell was deleted rather than kept as a public entrypoint
//! nobody calls — `export_aliases` now exposes only the walk composed below. Either way each fact
//! still owns its extraction rules in its own module and this module only composes the walks, so
//! there is exactly one definition of each fact and no second answer for it to drift from.
//!
//! No swc type crosses the crate boundary here: all three facts are already `zzop-core`/std types,
//! so `parse_module` stays `pub(crate)`.

use zzop_core::ReExport;

use crate::export_aliases::local_export_aliases_from_module;
use crate::parse_module;
use crate::re_exports::{dynamic_imports_from_module, re_exports_from_module};

/// The three per-file facts `unimported-export` needs beyond `FileArtifact`. Field-for-field identical
/// to what the three individual entrypoints return, in the same order they produce them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeadExportFacts {
    /// `export ... from "./y"` — see [`crate::parse_re_exports`].
    pub re_exports: Vec<ReExport>,
    /// `import("./x")` specifiers — see [`crate::parse_dynamic_imports`].
    pub dynamic_imports: Vec<String>,
    /// From-less `export { X as Y }` renames — see the `export_aliases` module's doc.
    pub export_aliases: Vec<(String, String)>,
}

/// Parses `source` ONCE and returns all three `unimported-export` facts. An unparseable file yields the
/// empty bundle — the same graceful degrade each individual entrypoint performs on its own, so the
/// bundled answer is indistinguishable from three separate calls in that case too.
pub fn parse_dead_export_facts(file: &str, source: &str) -> DeadExportFacts {
    let Some(module) = parse_module(file, source) else {
        return DeadExportFacts::default();
    };
    DeadExportFacts {
        re_exports: re_exports_from_module(&module),
        dynamic_imports: dynamic_imports_from_module(&module),
        export_aliases: local_export_aliases_from_module(&module),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_dynamic_imports, parse_re_exports};

    /// The identity this module exists to preserve: bundling must be a pure perf change, so on any
    /// input the bundle must equal the standalone calls field for field. The sources carry all three
    /// facts at once (plus shapes each walk deliberately drops) so a divergence fails here rather
    /// than in a corpus re-measurement.
    ///
    /// `export_aliases` has no standalone entrypoint left to compare against (see the module doc),
    /// and comparing it to an inline `parse_module` + walk would only restate this module's own body.
    /// Its value is asserted directly by `the_pin_source_is_not_vacuous` below, by
    /// `export_aliases`' own unit tests over the same walk, and end to end by
    /// `crates/engine/tests/analyze_dead_exports.rs`' alias cases.
    #[test]
    fn bundle_equals_the_standalone_entrypoints() {
        let sources = [
            // All three facts present in one file, plus the shapes each walk rejects.
            "export { A as B } from \"./a\";\nexport * from \"./b\";\nexport type * as ns from \"./c\";\n\
             const x = 1;\nexport { x as publicX };\nexport { x };\n\
             const m = lazy(() => import(\"./lazy\"));\nasync function f() { await import(\"./deep\"); }\n",
            // Unparseable — every fact must degrade to empty on both paths.
            "function f( {\n",
            // Nothing to find.
            "export const only = 1;\n",
        ];
        for (i, src) in sources.iter().enumerate() {
            let file = "x.ts";
            let bundled = parse_dead_export_facts(file, src);
            assert_eq!(
                bundled.re_exports,
                parse_re_exports(file, src),
                "source #{i}: re_exports diverged"
            );
            assert_eq!(
                bundled.dynamic_imports,
                parse_dynamic_imports(file, src),
                "source #{i}: dynamic_imports diverged"
            );
        }
    }

    /// The graceful degrade `export_aliases`' deleted standalone entrypoint used to own, now the
    /// bundle's: an unparseable file yields the empty bundle rather than panicking.
    #[test]
    fn an_unparseable_file_degrades_to_the_empty_bundle() {
        assert_eq!(
            parse_dead_export_facts("b.ts", "function f( {\n"),
            DeadExportFacts::default()
        );
    }

    /// The first source above must actually carry all three facts — otherwise the pin above would
    /// pass on three empty vectors and prove nothing.
    #[test]
    fn the_pin_source_is_not_vacuous() {
        let facts = parse_dead_export_facts(
            "x.ts",
            "export { A as B } from \"./a\";\nconst x = 1;\nexport { x as publicX };\n\
             const m = import(\"./lazy\");\n",
        );
        assert_eq!(facts.re_exports.len(), 1);
        assert_eq!(facts.dynamic_imports, vec!["./lazy".to_string()]);
        assert_eq!(
            facts.export_aliases,
            vec![("x".to_string(), "publicX".to_string())]
        );
    }
}
