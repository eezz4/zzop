//! Shared path predicates — repo-relative path shape checks reused across rule packs and parsers for
//! "not deployed / test surface" reasoning (e.g. skipping a test file's DB access when it isn't real
//! deployed coupling).

/// True when `path` looks like a test/spec file or sits under a test-only directory — the shared
/// "not deployed" path predicate. Also used to skip route registrations / DB-table access / query call
/// sites that only exist in test/fixture code, not real deployed surface.
///
/// ## One owner, and why it is the DSL fragment rather than a table here (2026-08-10)
/// This function used to carry its OWN arm table, and the DSL packs' `${test-paths}` fragment carried a
/// second one. Neither dominated the other and both were incomplete in the direction the other covered:
/// this table knew `_test.go`, `test_*.py`, `*Tests.cs` and `FooTest.java`; the fragment knew
/// `fixtures/`, `foo-spec.ts` and the runner config files. 132 of the 144 bundled rules (measured 2026-08-10) consult the
/// FRAGMENT, so every non-TypeScript test convention was invisible to the rule layer — measured on a
/// tree holding nothing but `services/handler_test.go`, `services/test_login.py` and
/// `Api.Tests/UserTests.cs`: 14 findings, every one of them a false positive, and 1 for the same bytes
/// moved under `tests/`.
///
/// The repair is not a third table. `crates/core/src/dsl/shared_fragments.json` now holds the UNION and
/// is the only place a test-path arm may be written; this predicate reads it. That direction (fragment
/// owns, Rust consumes) rather than the reverse, because the fragment is the side with those consumers, a
/// published name (`${test-paths}`, which external packs reference), and three guards already reading it
/// (`dsl::tests_fragments::{superset, name_census}`, `scripts/check-rule-desc-tokens.sh`). The pins that
/// used to guard the Rust table stay below and now guard the fragment — which is what wires "the repo
/// already knew the answer" to the layer that was getting it wrong.
///
/// ## The two conflicts the merge had to settle
/// * `e2e/`, `cypress/`, `playwright/`, `testing/` — this table matched them case-SENSITIVELY ("JS
///   ecosystem tool names, always lowercase"), the fragment case-insensitively. The fragment's spelling
///   wins, so `E2E/` and `Testing/` are now test paths (see the pins below, which changed polarity for
///   exactly these two). The failure directions are not symmetric: over-excluding a directory literally
///   named `Testing/` costs an allowed under-report, while under-excluding it produces a wrong claim
///   about code that never ships.
/// * `Tests?.cs` / `Tests?.java` / `Test[A-Z]*.java` — these stay case-SENSITIVE, spelled `(?-i:…)`
///   inside the otherwise case-insensitive fragment. Not the same call as above: `Contests.cs` is a word
///   that happens to end in "tests", not a file named for testing, and the PascalCase requirement is the
///   only thing telling them apart.
pub fn is_test_file(path: &str) -> bool {
    crate::dsl::test_path_re().is_match(path)
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
        // CHANGED POLARITY 2026-08-10, when this predicate and the DSL's `${test-paths}` fragment were
        // merged into one owner: the runner-directory arm used to be case-sensitive HERE and
        // case-insensitive THERE, and the fragment's spelling won. See `is_test_file`'s doc for why the
        // wider reading is the honest one for a subtractive predicate.
        assert!(is_test_file("Testing/service.cs"));
        assert!(is_test_file("E2E/flows/login.ts"));
        // Unchanged: whole-segment discipline survives the flag.
        assert!(!is_test_file("src/latest/service.ts"));
        assert!(!is_test_file("src/app-testing-utils/service.ts"));
    }

    /// The arms that were the FRAGMENT's alone before the 2026-08-10 merge — pinned here so the union
    /// cannot silently shrink back to either side's old table. Their absence from this predicate was
    /// never the reported defect (the DSL had them); their absence from the DSL for Go/Python/C# was.
    #[test]
    fn arms_the_dsl_fragment_contributed_to_the_merge() {
        assert!(is_test_file("src/fixtures/user.json"));
        assert!(is_test_file("src/fixture/user.json"));
        assert!(is_test_file("src/user-spec.ts"));
        assert!(is_test_file("vitest.config.ts"));
        assert!(is_test_file("app/playwright.config.ts"));
        // The dot-infix arm is extension-agnostic in the fragment, so it reaches languages the old
        // `\.(test|spec)\.(t|j)sx?$` arm here could not.
        assert!(is_test_file("api/user.test.py"));
        assert!(!is_test_file("src/spectrum/service.ts"));
    }

    /// The whole point of the merge, stated as the three conventions that produced the measured false
    /// positives — asserted through the DSL's own fragment, which is what the rule layer consults.
    #[test]
    fn the_dsl_fragment_matches_every_language_convention_this_predicate_knows() {
        let re = crate::dsl::test_path_re();
        for path in [
            "services/handler_test.go",
            "services/test_login.py",
            "services/login_test.py",
            "Api.Tests/UserTests.cs",
            "src/UserServiceTest.cs",
            "src/FooTest.java",
            "src/TestFoo.java",
        ] {
            assert!(
                re.is_match(path),
                "{path} is an idiomatic test path the rule layer must decline"
            );
        }
        for path in ["src/Contests.cs", "src/service.go", "app/login.py"] {
            assert!(!re.is_match(path), "{path} is production code");
        }
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
