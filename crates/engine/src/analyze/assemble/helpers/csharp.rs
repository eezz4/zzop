//! C# source-extension / import-resolution glue — split out from `helpers.rs` purely to keep that file
//! under the line-count ratchet; re-exported at `helpers`'s own top level (`pub(super) use csharp::{...}`
//! in `helpers.rs`) so every external call site (`super::helpers::is_csharp_source_ext`, etc., from
//! `dep_graph.rs`/`collect/census.rs`/`collect/candidates.rs`) keeps resolving unchanged — mirrors
//! `helpers::java`'s own split-out-module shape exactly.

use crate::pipeline::CSharpIndex;

/// True for the extension the dispatch table routes to `Language::CSharp` — same "duplicated rather than
/// threading the dispatch config" convention `is_java_source_ext`/`is_python_source_ext`/
/// `is_rust_source_ext`/`is_go_source_ext` document.
pub(in crate::analyze::assemble) fn is_csharp_source_ext(rel: &str) -> bool {
    rel.ends_with(".cs")
}

/// C# BCL/framework namespace family — the .NET base class library's own reverse-domain-free namespaces,
/// never a genuinely external (third-party) package. Excluded from the census the same way
/// `is_java_std_import` excludes `java`/`javax`: `System.*` is the .NET BCL itself, `Microsoft.*` is
/// Microsoft's own first-party framework surface (ASP.NET Core, EF Core, DI, logging, ...) — neither is a
/// third-party dependency a `cross-layer/sdk-import-no-visible-consume`-style census should ever flag.
const CSHARP_STD_IMPORT_HEADS: &[&str] = &["System", "Microsoft"];

/// True when `specifier`'s FIRST dotted segment is `System` or `Microsoft` — C#'s own BCL/framework
/// namespace rule (`System.Collections.Generic`, `Microsoft.AspNetCore.Mvc`). Never censused, never
/// staged for the F5 drain below — same "excluded before staging" treatment `is_java_std_import`/
/// `is_go_std_import`/`RUST_STD_CRATE_FAMILY` give their own std families.
pub(in crate::analyze::assemble) fn is_csharp_std_import(specifier: &str) -> bool {
    let head = specifier.split('.').next().unwrap_or(specifier);
    CSHARP_STD_IMPORT_HEADS.contains(&head)
}

/// C# import-specifier resolution glue — the C#-side counterpart of `resolve_java_import`, but WITHOUT
/// Java's rightmost-type-split retry: a C# `using` specifier is always namespace-shaped already (module
/// doc on [`crate::pipeline::CSharpIndex`] — C# has no per-type `using`, unlike Java's `import a.b.C;`),
/// so the specifier is looked up in `index.by_namespace` AS-IS, no trimming/splitting needed. Returns
/// every file declaring that namespace (0, 1, or many — namespace-fanout, mirroring `resolve_java_import`'s
/// glob-fanout doc: many files commonly share one namespace in C#, unlike Java's one-type-per-file
/// indexing). `using static X.Y;` / `using Alias = X.Y;` whose specifier names a TYPE rather than a
/// namespace simply misses here and resolves to empty — the caller's own job to census it, same
/// under-approximation `CSharpIndex`'s own module doc accepts. Called from BOTH
/// [`super::super::dep_graph::merge_csharp_dep_edges`] (dep-graph edges) and the census drain in
/// `super::super::collect::census` — same dual-call shape every other language's resolver here documents.
pub(in crate::analyze::assemble) fn resolve_csharp_import(
    specifier: &str,
    index: &CSharpIndex,
) -> Vec<String> {
    index
        .by_namespace
        .get(specifier)
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> CSharpIndex {
        let mut index = CSharpIndex::default();
        index.by_namespace.insert(
            "App.Services".to_string(),
            vec!["a/A.cs".to_string(), "b/B.cs".to_string()],
        );
        index
    }

    #[test]
    fn is_csharp_std_import_matches_system_and_microsoft_heads_only() {
        assert!(is_csharp_std_import("System"));
        assert!(is_csharp_std_import("System.Collections.Generic"));
        assert!(is_csharp_std_import("Microsoft.AspNetCore.Mvc"));
        assert!(!is_csharp_std_import("App.Services"));
        assert!(!is_csharp_std_import("Newtonsoft.Json"));
    }

    #[test]
    fn resolve_csharp_import_fans_out_to_every_file_declaring_the_namespace() {
        let idx = index();
        assert_eq!(
            resolve_csharp_import("App.Services", &idx),
            vec!["a/A.cs".to_string(), "b/B.cs".to_string()]
        );
    }

    #[test]
    fn resolve_csharp_import_returns_empty_for_unresolvable_specifier() {
        let idx = index();
        assert!(resolve_csharp_import("Some.ThirdParty.Lib", &idx).is_empty());
    }

    #[test]
    fn resolve_csharp_import_does_not_retry_a_type_shaped_specifier() {
        // A `using static Foo.Bar;` / aliased-type specifier that names a TYPE, not a namespace, must
        // simply miss — no rightmost-split retry the way Java's resolver has (module doc's documented
        // under-approximation).
        let idx = index();
        assert!(resolve_csharp_import("App.Services.SomeType", &idx).is_empty());
    }
}
