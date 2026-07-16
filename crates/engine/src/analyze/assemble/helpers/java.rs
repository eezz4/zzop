//! Java source-extension / import-resolution glue — split out from `helpers.rs` purely to keep that file
//! under the line-count ratchet; re-exported at `helpers`'s own top level (`pub(super) use java::{...}`
//! in `helpers.rs`) so every external call site (`super::helpers::is_java_source_ext`, etc., from
//! `dep_graph.rs`/`collect/census.rs`/`collect/candidates.rs`) keeps resolving unchanged.

use crate::pipeline::JavaIndex;

/// True for the extension the dispatch table routes to `Language::Java21` — same "duplicated rather than
/// threading the dispatch config" convention `is_python_source_ext`/`is_rust_source_ext`/
/// `is_go_source_ext` document.
pub(in crate::analyze::assemble) fn is_java_source_ext(rel: &str) -> bool {
    rel.ends_with(".java")
}

/// Java standard-library import family — the JDK's own reverse-domain namespaces, never a genuinely
/// external (third-party) package. Excluded from the F5 census the same way `RUST_STD_CRATE_FAMILY`/
/// `is_go_std_import` exclude their own languages' std families. `jakarta.*` is DELIBERATELY absent from
/// this list even though it looks JDK-shaped: it identifies an external framework family (Jakarta EE,
/// the post-`javax` rename lineage — Spring/Hibernate/etc. depend on it as a real third-party artifact),
/// unlike `java`/`javax` which name the JDK's OWN bundled namespaces — see `is_java_std_import`'s doc.
const JAVA_STD_IMPORT_HEADS: &[&str] = &["java", "javax"];

/// True when `specifier`'s FIRST dotted segment is `java` or `javax` — Java's own JDK-namespace rule
/// (`java.util.List`, `javax.xml.parsers.DocumentBuilderFactory`, ...). Never censused, never staged for
/// the F5 drain below — same "excluded before staging" treatment `RUST_STD_CRATE_FAMILY`/
/// `is_go_std_import` give their own std families.
pub(in crate::analyze::assemble) fn is_java_std_import(specifier: &str) -> bool {
    let head = specifier.split('.').next().unwrap_or(specifier);
    JAVA_STD_IMPORT_HEADS.contains(&head)
}

/// The census GRAIN for an unresolved Java import specifier: the first TWO dotted segments
/// (`org.springframework.web.bind.annotation.GetMapping` -> `org.springframework`), stripped of any
/// trailing `.*` glob marker first. Deliberately NOT Go's full-path grain (`drain_go_candidates`'s doc):
/// Java's reverse-domain convention makes the FIRST segment alone (`org`/`com`/`io`) carry no package
/// identity at all — every reverse-domain-named artifact on Earth starts with one of a handful of TLD
/// segments, so a one-segment grain would collapse `org.springframework` and `org.apache.commons` into
/// the same meaningless `org` census entry. Two segments is the shallowest grain that actually names a
/// real organization/package identity (a deliberate T3-style granularity difference from Go's own
/// convention, not an oversight — Go import paths are already domain-qualified at the FIRST `/`-segment,
/// e.g. `github.com/gin-gonic/gin`, so Go's full-path grain never has this collapse problem to begin
/// with).
pub(in crate::analyze::assemble) fn java_census_key(specifier: &str) -> String {
    let trimmed = specifier.strip_suffix(".*").unwrap_or(specifier);
    trimmed.split('.').take(2).collect::<Vec<_>>().join(".")
}

/// Java import-specifier resolution glue — the Java-side counterpart of `resolve_rust_import`/
/// `resolve_go_import_package_dir`, but returning every resolved TARGET FILE (0, 1, or many) rather than
/// a single `Option`: a glob import (`a.b.*`) can fan out to every file in a package (the Go
/// package-fanout precedent, `pipeline::java_index`'s own doc), while a plain/static import resolves to
/// at most one file. Called from BOTH [`super::super::dep_graph::merge_java_dep_edges`] (dep-graph edges)
/// and the census F5 drain in `super::super::collect::census` — same dual-call shape every other
/// language's resolver here documents.
///
/// Resolution order (task-pinned, trim gate added per opus review F4): a glob specifier (`a.b.*`)
/// fans out to `index.by_package["a.b"]` (every file declaring that package, possibly empty when the
/// package has no in-tree file). A non-glob specifier is first tried AS-IS (rightmost dot splits
/// `package` from `type` — the plain-import shape, `a.b.C` -> package `a.b`, type `C`); when that
/// misses, trailing segments are trimmed one at a time and each remainder retried — one trim recovers
/// the static-member-import shape (`a.b.C.m` -> `a.b.C`), further trims recover deeper nested-type
/// imports (`a.b.Outer.Inner.Deep` -> `a.b.Outer`, whose FILE declares the whole nesting). Each
/// trimmed attempt is gated on its candidate TYPE segment being uppercase-initial (Java's type-naming
/// convention): without the gate, a miss like `com.example.a.C` (`C` unindexed) would trim to package
/// `com.example` + "type" `a` and spuriously match a top-level class literally named `a` — a
/// false-positive resolution that silently swallows the specifier's census key (F5-drain suppression).
/// The lowercase gate also terminates the trim walk (segments below it are package-shaped). The AS-IS
/// first attempt stays convention-free — an exact index hit needs no guard. Returns empty when no
/// attempt resolves (an external/unresolvable import — the caller's own job to census it).
pub(in crate::analyze::assemble) fn resolve_java_import(
    specifier: &str,
    index: &JavaIndex,
) -> Vec<String> {
    if let Some(package) = specifier.strip_suffix(".*") {
        return index.by_package.get(package).cloned().unwrap_or_default();
    }
    if let Some(file) = try_split_java_type(specifier, index) {
        return vec![file.clone()];
    }
    let mut candidate = specifier;
    while let Some((trimmed, _last)) = candidate.rsplit_once('.') {
        candidate = trimmed;
        let type_segment = candidate
            .rsplit_once('.')
            .map(|(_, ty)| ty)
            .unwrap_or(candidate);
        if !type_segment.starts_with(|c: char| c.is_ascii_uppercase()) {
            break; // package-shaped (lowercase) segment reached — nothing shallower is a type
        }
        if let Some(file) = try_split_java_type(candidate, index) {
            return vec![file.clone()];
        }
    }
    Vec::new()
}

/// Splits `s` at its rightmost dot into `(package, type)` and looks it up in `index.by_type` — the one
/// resolution primitive [`resolve_java_import`]'s attempts (as-is, then each gated trim) all call.
fn try_split_java_type<'a>(s: &str, index: &'a JavaIndex) -> Option<&'a String> {
    let (package, type_name) = s.rsplit_once('.')?;
    index
        .by_type
        .get(&(package.to_string(), type_name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> JavaIndex {
        let mut index = JavaIndex::default();
        index.by_type.insert(
            ("com.example.a".to_string(), "C".to_string()),
            "a/C.java".to_string(),
        );
        index.by_package.insert(
            "com.example.a".to_string(),
            vec!["a/C.java".to_string(), "a/D.java".to_string()],
        );
        index
    }

    #[test]
    fn is_java_std_import_matches_java_and_javax_heads_only() {
        assert!(is_java_std_import("java.util.List"));
        assert!(is_java_std_import(
            "javax.xml.parsers.DocumentBuilderFactory"
        ));
        assert!(!is_java_std_import(
            "jakarta.servlet.http.HttpServletRequest"
        ));
        assert!(!is_java_std_import(
            "org.springframework.web.bind.annotation.GetMapping"
        ));
    }

    #[test]
    fn java_census_key_takes_first_two_dotted_segments() {
        assert_eq!(
            java_census_key("org.springframework.web.bind.annotation.GetMapping"),
            "org.springframework"
        );
        assert_eq!(
            java_census_key("com.fasterxml.jackson.databind.ObjectMapper"),
            "com.fasterxml"
        );
    }

    #[test]
    fn java_census_key_strips_trailing_glob_before_grain() {
        assert_eq!(
            java_census_key("org.springframework.web.bind.annotation.*"),
            "org.springframework"
        );
    }

    #[test]
    fn resolve_java_import_resolves_plain_type_import() {
        let idx = index();
        assert_eq!(
            resolve_java_import("com.example.a.C", &idx),
            vec!["a/C.java".to_string()]
        );
    }

    #[test]
    fn resolve_java_import_resolves_static_member_import_via_trim() {
        let idx = index();
        assert_eq!(
            resolve_java_import("com.example.a.C.someMethod", &idx),
            vec!["a/C.java".to_string()]
        );
    }

    #[test]
    fn resolve_java_import_fans_out_glob_to_every_package_file() {
        let idx = index();
        assert_eq!(
            resolve_java_import("com.example.a.*", &idx),
            vec!["a/C.java".to_string(), "a/D.java".to_string()]
        );
    }

    #[test]
    fn resolve_java_import_returns_empty_for_unresolvable_specifier() {
        let idx = index();
        assert!(
            resolve_java_import("org.springframework.web.bind.annotation.GetMapping", &idx)
                .is_empty()
        );
    }

    /// Opus review F4 regression: a miss whose trim lands on a LOWERCASE (package-shaped) segment
    /// must NOT resolve to a shallower namesake type. Here `com.example.a.C2` misses as-is (only
    /// `C` is indexed); the old ungated trim would retry `com.example.a` = package `com.example` +
    /// "type" `a` — and a top-level class literally named `a` would spuriously match, swallowing
    /// the census key.
    #[test]
    fn trim_never_resolves_to_a_lowercase_namesake() {
        let mut idx = index();
        idx.by_type.insert(
            ("com.example".to_string(), "a".to_string()),
            "weird/a.java".to_string(),
        );
        assert!(resolve_java_import("com.example.a.C2", &idx).is_empty());
    }

    /// Opus review F4 companion: deeper-nested type imports resolve by iterative trimming — the
    /// declaring FILE of `Outer` carries the whole nesting, so `a.b.Outer.Inner.Deep` walks back to
    /// the indexed `("a.b", "Outer")` entry (each trimmed candidate segment is uppercase, so the
    /// gate lets the walk continue).
    #[test]
    fn deeply_nested_type_import_resolves_to_the_outer_declaring_file() {
        let mut idx = index();
        idx.by_type.insert(
            ("a.b".to_string(), "Outer".to_string()),
            "a/b/Outer.java".to_string(),
        );
        assert_eq!(
            resolve_java_import("a.b.Outer.Inner.Deep", &idx),
            vec!["a/b/Outer.java".to_string()]
        );
    }
}
