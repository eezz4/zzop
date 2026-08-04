//! Shared path predicates — repo-relative path shape checks reused across rule packs and parsers for
//! "not deployed / test surface" reasoning (e.g. skipping a test file's DB access when it isn't real
//! deployed coupling).

use std::sync::OnceLock;

use regex::Regex;

/// True when `path` looks like a test/spec file or sits under a test-only directory — the shared
/// "not deployed" path predicate. Also used to skip route registrations / DB-table access / query call
/// sites that only exist in test/fixture code, not real deployed surface.
pub fn is_test_file(path: &str) -> bool {
    test_patterns().iter().any(|re| re.is_match(path))
}

fn test_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"\.(test|spec)\.(t|j)sx?$",
            r"_test\.go$",
            r"(^|/)test_[^/]*\.py$",
            r"_test\.py$",
            r"Tests?\.java$",
            r"(^|/)Test[A-Z][^/]*\.java$",
            // C# conventions: `FooTests.cs`/`FooTest.cs` files (case-sensitive on purpose — the
            // convention is PascalCase, and a lowercase tail like `contests.cs` must not match) and
            // `MyApp.Tests/` project directories.
            r"Tests?\.cs$",
            r"(?i)\.tests?/",
            // `(?i)` (added with the C# arm, 2026-08-03) also admits spellings the old case-sensitive
            // arm rejected — `Tests/`, `TESTS/`, `Spec/` — which .NET solutions actually use for the
            // directory itself. The runner-directory arm below stays case-sensitive: those are JS
            // ecosystem tool names, always lowercase.
            r"(?i)(^|/)(__tests__|__test__|tests?|spec)/",
            // Directories named for a test runner (or literally `testing`) are test surface by the same
            // "not deployed" reasoning as `__tests__`.
            r"(^|/)(e2e|cypress|playwright|testing)/",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e2e_infra_directories_are_test_paths() {
        assert!(is_test_file(
            "packages/testing/playwright/scripts/import-data.mjs"
        ));
        assert!(is_test_file("app/e2e/flows/login.ts"));
        assert!(is_test_file("cypress/scripts/setup.js"));
        // Whole-segment match only — names merely containing "testing" are not test paths.
        assert!(!is_test_file("src/app-testing-utils/service.ts"));
    }

    #[test]
    fn csharp_test_conventions() {
        // File arm: `*Tests.cs` / `*Test.cs`.
        assert!(is_test_file("Api.Tests/UserServiceTests.cs"));
        assert!(is_test_file("src/UserServiceTest.cs"));
        // Directory arm: a `.Tests`/`.Test` project segment classifies everything under it.
        assert!(is_test_file("MyApp.Tests/Fixtures/SeedData.cs"));
        assert!(is_test_file("myapp.tests/Helper.cs"));
        // Case-sensitivity of the file arm: PascalCase only — a word merely ending in "tests" is
        // not the convention.
        assert!(!is_test_file("src/Contests.cs"));
        assert!(!is_test_file("src/UserService.cs"));
        // A `Test` PREFIX is the Java convention, deliberately not extended to `.cs`.
        assert!(!is_test_file("src/TestData.cs"));
    }

    #[test]
    fn case_insensitive_dir_arm_widening_is_pinned() {
        // NEW matches admitted by the `(?i)` added for C# (previously non-matching):
        assert!(is_test_file("Tests/Fixture.cs"));
        assert!(is_test_file("src/TESTS/helper.ts"));
        assert!(is_test_file("Spec/models/user.rb"));
        // Unchanged: the runner-directory arm stays case-sensitive.
        assert!(!is_test_file("Testing/service.cs"));
        assert!(!is_test_file("E2E/flows/login.ts"));
        // Unchanged: whole-segment discipline survives the flag.
        assert!(!is_test_file("src/latest/service.ts"));
        assert!(!is_test_file("src/app-testing-utils/service.ts"));
    }

    #[test]
    fn spec_and_test_extensions() {
        assert!(is_test_file("src/foo.test.ts"));
        assert!(is_test_file("src/foo.spec.tsx"));
        assert!(is_test_file("pkg/foo_test.go"));
        assert!(is_test_file("app/test_foo.py"));
        assert!(is_test_file("app/foo_test.py"));
        assert!(is_test_file("src/FooTest.java"));
        assert!(is_test_file("src/TestFoo.java"));
        assert!(!is_test_file("src/foo.ts"));
    }
}
