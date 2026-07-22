//! Coverage for all seven framework-silence tripwires (S1-S7) and `provide_blind_sources`.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use super::builtin_fetch::builtin_fetch_lexical_warning;
use super::client_library_import::client_library_import_warning;
use super::committed_spec_io_silence::{committed_spec_io_silence_warning, IO_NEAR_ZERO_FLOOR};
use super::controller_silence::{controller_silence_warning, MIN_PROVIDES_FLOOR};
use super::fetch_wrapper::fetch_wrapper_call_site_warning;
use super::orm_schema_silence::orm_schema_silence_warning;
use super::server_framework_import::{provide_blind_sources, server_framework_import_warning};

/// Policy-value divergence pin: `coverage::CoverageCensus::join_contribution_zero` asserts on
/// EXACT zero JOINABLE io (an unconditional structural fact — files > 0 with 0 provides and 0
/// KEYED consumes; unresolved consumes don't count, they cannot join),
/// while the S1/S2/S4 tripwires here gate on this near-zero floor, because a heuristic
/// self-report must still fire at 1-2 extracted facts (round 9's be-express: 1 provide; round
/// 14's be-spring: 2) where the census assertion is already structurally false. Unifying the two
/// would either weaken the assertion into a heuristic or re-open the exact-zero silence hole —
/// change this relationship deliberately, never by drift (if the floor value itself changes,
/// update this pin AND the rationale in both module docs in the same commit).
#[test]
fn the_near_zero_floor_diverges_from_the_census_exact_zero_assertion_deliberately() {
    assert_eq!(
            MIN_PROVIDES_FLOOR, 3,
            "MIN_PROVIDES_FLOOR changed — re-justify the round-9/round-14 near-zero rationale and \
             the deliberate divergence from coverage::join_contribution_zero's exact-zero assertion"
        );
    assert_eq!(
        IO_NEAR_ZERO_FLOOR, MIN_PROVIDES_FLOOR,
        "IO_NEAR_ZERO_FLOOR is documented as an alias of MIN_PROVIDES_FLOOR — if they must \
             diverge, update both docs and this pin deliberately"
    );
}

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn three_or_more_matching_files_with_zero_http_provides_warns() {
    let dir = TempDir::new("zzop-coverage-warn");
    // `@FastController`/`@FastGet` — an invented decorator idiom matching the regex
    // (`@\w*(?:Controller|...)\b`): the suffix sits directly after `@` with only word chars between.
    dir.write(
        "a.ts",
        "@FastController('/a')\nclass A {\n  @FastGet('/x')\n  x() {}\n}\n",
    );
    dir.write(
        "b.ts",
        "@FastController('/b')\nclass B {\n  @FastGet('/y')\n  y() {}\n}\n",
    );
    dir.write(
        "c.ts",
        "@FastController('/c')\nclass C {\n  @FastGet('/z')\n  z() {}\n}\n",
    );
    let rels = vec!["a.ts".to_string(), "b.ts".to_string(), "c.ts".to_string()];
    let warning = controller_silence_warning(dir.path(), &rels, 0);
    assert!(
        warning.as_deref().is_some_and(|w| w.contains(
            "route decorators/annotations but only 0 http route(s) were extracted tree-wide"
        )),
        "got: {warning:?}"
    );
    // Funnel pin (D9): the disclosure must chain to CREATION — a reword that drops the
    // partial-envelope on-ramp or the embedded-contract pointer fails here.
    assert!(
        warning.as_deref().is_some_and(
            |w| w.contains("zzop contract envelope-guide") && w.contains("partial envelope")
        ),
        "got: {warning:?}"
    );
}

#[test]
fn three_or_more_matching_files_with_near_zero_provides_still_warns() {
    // Round 14's failure shape: a tree that DID extract some provides (here 2, still below
    // `MIN_PROVIDES_FLOOR`) must still warn — the whole point of the near-zero (not exact-zero) gate.
    let dir = TempDir::new("zzop-coverage-warn-near-zero");
    dir.write(
        "a.ts",
        "@FastController('/a')\nclass A {\n  @FastGet('/x')\n  x() {}\n}\n",
    );
    dir.write(
        "b.ts",
        "@FastController('/b')\nclass B {\n  @FastGet('/y')\n  y() {}\n}\n",
    );
    dir.write(
        "c.ts",
        "@FastController('/c')\nclass C {\n  @FastGet('/z')\n  z() {}\n}\n",
    );
    let rels = vec!["a.ts".to_string(), "b.ts".to_string(), "c.ts".to_string()];
    let warning = controller_silence_warning(dir.path(), &rels, 2);
    assert!(
        warning.as_deref().is_some_and(|w| w.contains(
            "route decorators/annotations but only 2 http route(s) were extracted tree-wide"
        )),
        "got: {warning:?}"
    );
}

#[test]
fn provides_at_the_floor_short_circuits_without_even_reading_files() {
    // Paths don't exist on disk; if this ever performed a real read it would silently skip
    // unreadable files rather than panic, so this just verifies the cheap short-circuit path
    // returns `None` once `http_provides_count` clears `MIN_PROVIDES_FLOOR`.
    let rels = vec![
        "does/not/exist/a.ts".to_string(),
        "does/not/exist/b.ts".to_string(),
        "does/not/exist/c.ts".to_string(),
    ];
    let warning = controller_silence_warning(Path::new("."), &rels, MIN_PROVIDES_FLOOR);
    assert!(warning.is_none());
}

#[test]
fn below_threshold_file_count_does_not_warn() {
    let dir = TempDir::new("zzop-coverage-below-threshold");
    dir.write(
        "a.ts",
        "@FastController('/a')\nclass A {\n  @FastGet('/x')\n  x() {}\n}\n",
    );
    dir.write(
        "b.ts",
        "@FastController('/b')\nclass B {\n  @FastGet('/y')\n  y() {}\n}\n",
    );
    let rels = vec!["a.ts".to_string(), "b.ts".to_string()];
    let warning = controller_silence_warning(dir.path(), &rels, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn decorator_mentions_in_comments_or_string_literals_do_not_trip_the_silence_report() {
    // Dogfooding zzop on its own source surfaced this: a SAST tool DOCUMENTS (`/// … @GetMapping`,
    // ` * @Controller`) and FIXTURES (`dir.write("a.ts", "@FastController('/a')")`) the very decorators
    // it detects. Neither is a real route — a real decorator LEADS its line — so a tree whose only
    // decorator-shaped tokens are in comments or string literals must stay silent.
    let dir = TempDir::new("zzop-coverage-mentions");
    dir.write(
        "a.rs",
        "/// A Spring `@GetMapping` annotation draws from this set.\npub const X: u8 = 1;\n",
    );
    dir.write(
        "b.rs",
        "/**\n * The bare `@PostMapping` decorator, joined onto the class prefix.\n */\nfn b() {}\n",
    );
    // The zzop-fixture shape: the decorator is embedded in a Rust string literal, mid-line.
    dir.write(
        "c.rs",
        "fn c() {\n    dir.write(\"x.ts\", \"@FastController('/a')\\n@Get('/y')\");\n}\n",
    );
    let rels = vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()];
    let warning = controller_silence_warning(dir.path(), &rels, 0);
    assert!(
        warning.is_none(),
        "comment/string-literal mentions must not fire: {warning:?}"
    );
}

#[test]
fn a_real_code_line_decorator_still_fires_even_amid_comment_mentions() {
    // The refinement must not over-correct: a decorator on a real CODE line still counts, even in a
    // file that also has commented mentions.
    let dir = TempDir::new("zzop-coverage-code-line");
    for (f, body) in [
        ("a.ts", "// mentions @GetMapping in a comment\n@Controller('/a')\nclass A {\n  @Get('/x')\n  x() {}\n}\n"),
        ("b.ts", "@Controller('/b')\nclass B {\n  @Post('/y')\n  y() {}\n}\n"),
        ("c.ts", "@Controller('/c')\nclass C {\n  @Put('/z')\n  z() {}\n}\n"),
    ] {
        dir.write(f, body);
    }
    let rels = vec!["a.ts".to_string(), "b.ts".to_string(), "c.ts".to_string()];
    let warning = controller_silence_warning(dir.path(), &rels, 0);
    assert!(
        warning
            .as_deref()
            .is_some_and(|w| w.contains("route decorators/annotations")),
        "got: {warning:?}"
    );
}

#[test]
fn angular_style_decorators_never_match_the_controller_regex() {
    // None of Angular's own decorator vocabulary lexically matches
    // Controller/Mapping/Get/Post/Put/Delete/Patch.
    let dir = TempDir::new("zzop-coverage-angular");
    dir.write(
            "a.ts",
            "@Component({ selector: 'app-a' })\nclass A {\n  @Input() x: string;\n  @Output() y = new EventEmitter();\n  @HostListener('click')\n  onClick() {}\n}\n",
        );
    dir.write(
        "b.ts",
        "@Component({ selector: 'app-b' })\nclass B {\n  @Inject(TOKEN) dep: any;\n}\n",
    );
    dir.write(
        "c.ts",
        "@Component({ selector: 'app-c' })\nclass C {\n  @Input() z: number;\n}\n",
    );
    let rels = vec!["a.ts".to_string(), "b.ts".to_string(), "c.ts".to_string()];
    let warning = controller_silence_warning(dir.path(), &rels, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

// --- S2 -----------------------------------------------------------------------------------------

fn package_import_files(entries: &[(&str, &[&str])]) -> BTreeMap<String, BTreeSet<String>> {
    entries
        .iter()
        .map(|(specifier, files)| {
            (
                specifier.to_string(),
                files.iter().map(|f| f.to_string()).collect(),
            )
        })
        .collect()
}

#[test]
fn express_import_with_near_zero_provides_warns() {
    let map = package_import_files(&[("express", &["src/app.ts"])]);
    let warning = server_framework_import_warning(&map, 1);
    assert!(
        warning
            .as_deref()
            .is_some_and(|w| w.contains("express") && w.contains("src/app.ts")),
        "got: {warning:?}"
    );
}

#[test]
fn healthy_provides_count_short_circuits_even_with_a_server_import() {
    let map = package_import_files(&[("express", &["src/app.ts"])]);
    let warning = server_framework_import_warning(&map, 3);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn no_server_framework_import_never_warns() {
    let map = package_import_files(&[("react", &["src/App.tsx"]), ("lodash", &["src/x.ts"])]);
    let warning = server_framework_import_warning(&map, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn http_client_libraries_are_not_server_frameworks() {
    // axios/got/etc. say nothing about whether THIS tree serves routes — deliberately excluded from
    // `SERVER_FRAMEWORK_SPECIFIERS`.
    let map = package_import_files(&[("axios", &["src/api.ts"]), ("got", &["src/api2.ts"])]);
    let warning = server_framework_import_warning(&map, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn server_framework_and_client_library_vocab_stay_disjoint() {
    // S2's server-only list and S4's client-only list must never cross-fire: express never trips S4,
    // and axios never trips S2 (the reverse direction is already covered by
    // `http_client_libraries_are_not_server_frameworks` above).
    let map = package_import_files(&[("express", &["src/app.ts"])]);
    let warning = client_library_import_warning(&map, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn go_server_framework_imports_trip_s2() {
    // Go server-framework census entries are the FULL import path (see
    // `SERVER_FRAMEWORK_SPECIFIERS`'s own doc for why) — a version-suffixed real-world import path
    // (`.../fiber/v2`) still matches its bare vocab entry via the pre-existing slash-subpath arm.
    for specifier in [
        "github.com/gin-gonic/gin",
        "github.com/labstack/echo",
        "github.com/go-chi/chi",
        "github.com/gofiber/fiber/v2",
    ] {
        let map = package_import_files(&[(specifier, &["main.go"])]);
        let warning = server_framework_import_warning(&map, 0);
        assert!(warning.is_some(), "{specifier} did not trip S2");
    }
}

#[test]
fn go_http_client_import_trips_s4() {
    let map = package_import_files(&[("github.com/go-resty/resty/v2", &["client.go"])]);
    let warning = client_library_import_warning(&map, 0);
    assert!(warning.is_some(), "go-resty/resty did not trip S4");
}

#[test]
fn go_standard_library_net_http_trips_neither_side() {
    // `net/http` is natively supported on BOTH the server-provide and client-consume sides (real
    // extraction, not a blindness heuristic) — deliberately absent from both vocab lists, since
    // adding it would break this exact disjoint-pin expectation.
    let map = package_import_files(&[("net/http", &["main.go"])]);
    assert!(server_framework_import_warning(&map, 0).is_none());
    assert!(client_library_import_warning(&map, 0).is_none());
}

#[test]
fn go_server_and_client_vocab_stay_disjoint() {
    let server_map = package_import_files(&[("github.com/gin-gonic/gin", &["main.go"])]);
    assert!(client_library_import_warning(&server_map, 0).is_none());
    let client_map = package_import_files(&[("github.com/go-resty/resty", &["client.go"])]);
    assert!(server_framework_import_warning(&client_map, 0).is_none());
}

#[test]
fn java_server_framework_import_trips_s2() {
    let map = package_import_files(&[("org.springframework", &["Controller.java"])]);
    assert!(server_framework_import_warning(&map, 0).is_some());
}

#[test]
fn java_realistic_census_grain_keeps_server_and_client_disjoint() {
    // `collect::census::drain_java_candidates` censuses EVERY unresolved `org.springframework.*` import
    // at the SAME two-segment grain (`java_census_key`'s doc) — a real Java-sourced census entry is
    // therefore ALWAYS exactly `"org.springframework"`, never a longer specifier like
    // `"org.springframework.web.client"`. At that realistic grain the disjoint invariant holds cleanly,
    // same as every other language's own pin: the server vocab's exact-match entry fires S2, and does
    // NOT reach far enough (the client vocab entry requires the `org.springframework.web.client` prefix)
    // to also fire S4.
    let map = package_import_files(&[("org.springframework", &["RestClient.java"])]);
    assert!(server_framework_import_warning(&map, 0).is_some());
    assert!(client_library_import_warning(&map, 0).is_none());
}

#[test]
fn java_client_vocab_literal_prefix_overlaps_server_vocab_documented_not_a_bug() {
    // Unlike Go's gin/resty (separate projects, naturally disjoint import paths), Spring's client HTTP
    // tooling (`org.springframework.web.client`) lives INSIDE the framework's own root namespace
    // (`org.springframework`) alongside its server-side MVC surface. Tested here directly against the
    // matcher functions with the CLIENT vocab entry's own literal string as the map key — a shape the
    // real Java F5 drain never actually produces (`java_realistic_census_grain_keeps_server_and_client_
    // disjoint` above pins the REAL grain-collapsed behavior, which stays disjoint) — this test instead
    // documents what would happen if a future, finer-grained Java census (or any other caller) ever DID
    // produce this longer specifier as a literal key: `is_server_framework_specifier`'s "." prefix arm
    // (`specifier.starts_with("org.springframework.")`) matches `"org.springframework.web.client"` too,
    // so BOTH S2 and S4 would fire together. This is deliberate over-disclosure, not a disjoint-vocab
    // violation: the module doc's governing principle is "over-disclosure is safe, silence is fatal" —
    // both channels firing for a Spring-client-only import is a false-positive-SHAPED S2, but a SAFE one
    // (it still points the reader at the right escape hatch), and narrowing the matcher to avoid it would
    // require a bespoke exception unlike every other vocab entry in this module. Deliberately NOT
    // asserted disjoint, unlike the Go pin just above — see this test's own name.
    let map = package_import_files(&[(
        "org.springframework.web.client",
        &["client/RestClient.java"],
    )]);
    assert!(server_framework_import_warning(&map, 0).is_some());
    assert!(client_library_import_warning(&map, 0).is_some());
}

#[test]
fn a_lookalike_specifier_does_not_match_via_substring() {
    // "expressive" must not match the "express" vocab entry (not a whole-segment match).
    let map = package_import_files(&[("expressive", &["src/x.ts"])]);
    let warning = server_framework_import_warning(&map, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn a_subpath_import_of_a_server_framework_still_matches() {
    let map = package_import_files(&[("express/lib/router", &["src/x.ts"])]);
    let warning = server_framework_import_warning(&map, 0);
    assert!(warning.is_some(), "got: {warning:?}");
}

// --- Python server-framework vocab: fastapi/flask/django ------------------------------------------

#[test]
fn fastapi_import_with_near_zero_provides_warns() {
    let map = package_import_files(&[("fastapi", &["app/main.py"])]);
    let warning = server_framework_import_warning(&map, 1);
    assert!(
        warning
            .as_deref()
            .is_some_and(|w| w.contains("fastapi") && w.contains("app/main.py")),
        "got: {warning:?}"
    );
}

#[test]
fn flask_and_django_bare_imports_match() {
    for vocab in ["flask", "django"] {
        let map = package_import_files(&[(vocab, &["app.py"])]);
        let warning = server_framework_import_warning(&map, 0);
        assert!(warning.is_some(), "{vocab} got: {warning:?}");
    }
}

#[test]
fn fastapi_dotted_subpath_specifier_still_matches() {
    // `from fastapi.routing import APIRoute` -> specifier `fastapi.routing` (Python's absolute-dotted
    // convention, distinct from npm's slash-subpath form) — must still count as `fastapi`.
    let map = package_import_files(&[("fastapi.routing", &["app/main.py"])]);
    let warning = server_framework_import_warning(&map, 0);
    assert!(warning.is_some(), "got: {warning:?}");
}

#[test]
fn a_python_lookalike_specifier_does_not_match_via_substring_or_prefix() {
    // Neither "fastapi2" (no delimiter after the vocab entry) nor "myfastapi" (vocab entry is not a
    // leading segment at all) may match — same exact-segment-boundary discipline the npm vocab
    // already gets from `a_lookalike_specifier_does_not_match_via_substring` above.
    let map = package_import_files(&[("fastapi2", &["app.py"]), ("myfastapi", &["app2.py"])]);
    let warning = server_framework_import_warning(&map, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

// --- provide_blind_sources (unprovided-mutation-call's severity gate, symmetric to
// majority_unresolved_http_sources) -------------------------------------------------------------

fn package_import_sites(
    entries: &[(&str, &str)],
) -> Vec<zzop_rules_cross_layer::PackageImportSite> {
    entries
        .iter()
        .map(
            |(source, specifier)| zzop_rules_cross_layer::PackageImportSite {
                source: source.to_string(),
                specifier: specifier.to_string(),
                file_count: 1,
                example_file: "src/app.ts".to_string(),
            },
        )
        .collect()
}

fn provide_counts(entries: &[(&str, usize)]) -> Vec<(String, usize)> {
    entries
        .iter()
        .map(|(source, count)| (source.to_string(), *count))
        .collect()
}

#[test]
fn framework_importer_with_two_provides_is_blind() {
    let imports = package_import_sites(&[("be", "express")]);
    let counts = provide_counts(&[("be", 2)]);
    let blind = provide_blind_sources(&imports, &counts);
    assert_eq!(blind, BTreeSet::from(["be".to_string()]));
}

#[test]
fn framework_importer_with_three_provides_is_not_blind() {
    // MIN_PROVIDES_FLOOR is 3 — the floor itself already clears the gate (strict less-than).
    let imports = package_import_sites(&[("be", "express")]);
    let counts = provide_counts(&[("be", 3)]);
    let blind = provide_blind_sources(&imports, &counts);
    assert!(blind.is_empty(), "got: {blind:?}");
}

#[test]
fn non_framework_importer_with_zero_provides_is_not_blind() {
    // Importing react/lodash says nothing about whether this tree serves routes — same S2 rationale.
    let imports = package_import_sites(&[("fe", "react")]);
    let counts = provide_counts(&[("fe", 0)]);
    let blind = provide_blind_sources(&imports, &counts);
    assert!(blind.is_empty(), "got: {blind:?}");
}

#[test]
fn framework_importer_missing_from_provide_counts_defaults_to_zero_and_is_blind() {
    // A source with 0 http provides tree-wide may legitimately have no entry in http_provide_counts;
    // the helper must treat a missing entry as 0, not silently skip the source.
    let imports = package_import_sites(&[("be", "@nestjs/common")]);
    let blind = provide_blind_sources(&imports, &[]);
    assert_eq!(blind, BTreeSet::from(["be".to_string()]));
}

#[test]
fn no_package_imports_at_all_yields_no_blind_sources() {
    let blind = provide_blind_sources(&[], &provide_counts(&[("be", 0)]));
    assert!(blind.is_empty(), "got: {blind:?}");
}

// --- S3 -----------------------------------------------------------------------------------------

#[test]
fn committed_openapi_spec_with_zero_io_both_directions_warns() {
    let dir = TempDir::new("zzop-coverage-openapi-spec");
    dir.write(
            "src/services/openapi.yml",
            "openapi: 3.0.0\ninfo:\n  title: Example\npaths:\n  /users:\n    get:\n      summary: list\n",
        );
    let rels = vec!["src/services/openapi.yml".to_string()];
    let warning = committed_spec_io_silence_warning(dir.path(), &rels, 0, 0);
    assert!(
        warning
            .as_deref()
            .is_some_and(|w| w.contains("src/services/openapi.yml")),
        "got: {warning:?}"
    );
    // Funnel pin (D9): same chain-to-creation tail as every sibling silence warning.
    assert!(
        warning
            .as_deref()
            .is_some_and(|w| w.contains("zzop contract envelope-guide") && w.contains("partial")),
        "got: {warning:?}"
    );
}

#[test]
fn healthy_provides_short_circuits_without_reading_the_spec_file() {
    // The spec path doesn't exist on disk; if this ever performed a real read on the healthy-provides
    // path it would silently skip an unreadable file rather than panic, so this just verifies the
    // cheap short-circuit (gate before disk IO) returns `None` — same style as
    // `provides_at_the_floor_short_circuits_without_even_reading_files` above.
    let rels = vec!["does/not/exist/openapi.yml".to_string()];
    let warning = committed_spec_io_silence_warning(Path::new("."), &rels, 3, 0);
    assert!(warning.is_none());
}

#[test]
fn healthy_keyed_consumes_short_circuits_without_reading_the_spec_file() {
    let rels = vec!["does/not/exist/openapi.yml".to_string()];
    let warning = committed_spec_io_silence_warning(Path::new("."), &rels, 0, 3);
    assert!(warning.is_none());
}

#[test]
fn basename_matches_but_no_paths_marker_stays_silent() {
    let dir = TempDir::new("zzop-coverage-openapi-no-paths");
    dir.write(
        "src/openapi-theme.json",
        "{\"title\": \"just a theme file\", \"colors\": [\"red\", \"blue\"]}\n",
    );
    let rels = vec!["src/openapi-theme.json".to_string()];
    let warning = committed_spec_io_silence_warning(dir.path(), &rels, 0, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn non_spec_shaped_filenames_are_never_probed() {
    let dir = TempDir::new("zzop-coverage-not-a-spec");
    dir.write("src/config.yml", "paths:\n  data: /var/data\n");
    let rels = vec!["src/config.yml".to_string()];
    let warning = committed_spec_io_silence_warning(dir.path(), &rels, 0, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

// --- S4 -----------------------------------------------------------------------------------------

#[test]
fn angular_http_client_import_with_zero_consumes_warns() {
    let map = package_import_files(&[("@angular/common/http", &["src/api.service.ts"])]);
    let warning = client_library_import_warning(&map, 0);
    assert!(
            warning.as_deref().is_some_and(
                |w| w.contains("@angular/common/http") && w.contains("src/api.service.ts")
            ),
            "got: {warning:?}"
        );
}

#[test]
fn angular_http_client_import_with_near_zero_consumes_still_warns() {
    // Round 14's actual shape: some consumes extracted (2), still below `MIN_PROVIDES_FLOOR`.
    let map = package_import_files(&[("@angular/common/http", &["src/api.service.ts"])]);
    let warning = client_library_import_warning(&map, 2);
    assert!(
        warning
            .as_deref()
            .is_some_and(|w| w.contains("@angular/common/http")),
        "got: {warning:?}"
    );
}

#[test]
fn axios_import_with_near_zero_consumes_warns() {
    let map = package_import_files(&[("axios", &["src/api.ts"])]);
    let warning = client_library_import_warning(&map, 1);
    assert!(
        warning.as_deref().is_some_and(|w| w.contains("axios")
                && w.contains("src/api.ts")
                // Same D9 funnel tail as S5's: chain the disclosure to the minimal on-ramp.
                && w.contains("a partial envelope covering just the consume channel is enough")
                && w.contains("zzop contract envelope-guide")
                && w.contains("docs/NORMALIZED_AST.md")),
        "got: {warning:?}"
    );
}

#[test]
fn healthy_consumes_count_short_circuits_even_with_a_client_import() {
    let map = package_import_files(&[("axios", &["src/api.ts"])]);
    let warning = client_library_import_warning(&map, 3);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn no_http_client_import_never_warns() {
    let map = package_import_files(&[("react", &["src/App.tsx"]), ("lodash", &["src/x.ts"])]);
    let warning = client_library_import_warning(&map, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn a_client_lookalike_specifier_does_not_match_via_substring() {
    // "axios-mock-adapter" must not match the "axios" vocab entry (not a whole-segment match).
    let map = package_import_files(&[("axios-mock-adapter", &["src/x.test.ts"])]);
    let warning = client_library_import_warning(&map, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn a_subpath_import_of_a_http_client_still_matches() {
    // "@angular/common/http/testing" is a subpath of the "@angular/common/http" vocab entry and
    // matches by the same prefix rule `is_server_framework_specifier` uses — a testing-only import
    // still implies the client is present in the tree, which is the intended (accepted) behavior.
    let map = package_import_files(&[("@angular/common/http/testing", &["src/api.spec.ts"])]);
    let warning = client_library_import_warning(&map, 0);
    assert!(warning.is_some(), "got: {warning:?}");
}

// --- S5 -----------------------------------------------------------------------------------------

use super::builtin_fetch::FETCH_CALL_SITES_MIN;

/// One js source line holding a single dynamic-but-INTERNAL-intent `fetch(` call site: a computed
/// template literal (`fetch(`${base}q<i>`)`) with no absolute-URL scheme — the computed-internal dark
/// shape the extractor leaves unresolved, which the internal-intent census filter counts. (A bare-var
/// `fetch(BASE + p)` shape carries no string literal and no longer counts under the intent filter.)
fn dynamic_fetch_lines(n: usize) -> String {
    (0..n)
        .map(|i| format!("export const c{i} = (base: string) => fetch(`${{base}}q{i}`);\n"))
        .collect()
}

#[test]
fn wrapper_style_fetch_call_sites_with_near_zero_keyed_consumes_warn() {
    // The live shape this tripwire was built from: many lexical fetch call sites, near-none keyed —
    // window./globalThis. prefixed calls count too (`\b` sits between `.` and `fetch`). Each fetch
    // carries an internal-relative (non-absolute) literal URL so the intent filter counts it.
    let dir = TempDir::new("zzop-coverage-builtin-fetch");
    dir.write("src/api.ts", &dynamic_fetch_lines(3));
    dir.write(
        "src/wrap.ts",
        "export const w = (b) => window.fetch(`${b}/w`);\nexport const g = (b) => globalThis.fetch(`${b}/g`);\n",
    );
    let rels = vec!["src/api.ts".to_string(), "src/wrap.ts".to_string()];
    let warning = builtin_fetch_lexical_warning(dir.path(), &rels, 1);
    assert!(
        warning.as_deref().is_some_and(|w| w.contains(
            "5 builtin `fetch(` call site(s) appear lexically across 2 js/ts file(s) but only 1 keyed http consume(s)"
        ) && w.contains("Mode B overlay adapter")
            // D9 funnel: the disclosure chains to CREATION — minimal partial-envelope on-ramp plus
            // the embedded-contract print command (MCP hosts) / docs path (repo users).
            && w.contains("a partial envelope covering just the consume channel is enough")
            && w.contains("zzop contract envelope-guide")
            && w.contains("docs/NORMALIZED_AST.md")),
        "got: {warning:?}"
    );
}

#[test]
fn healthy_keyed_consumes_short_circuit_without_even_reading_files() {
    // Paths don't exist on disk — verifies the cheap short-circuit path returns `None` once the
    // keyed consume count clears the floor, same style as S1/S3's short-circuit tests.
    let rels = vec!["does/not/exist/api.ts".to_string()];
    let warning = builtin_fetch_lexical_warning(Path::new("."), &rels, MIN_PROVIDES_FLOOR);
    assert!(warning.is_none());
}

#[test]
fn call_site_count_at_the_floor_fires_and_one_below_stays_silent() {
    // The threshold boundary, pinned against the const itself: FETCH_CALL_SITES_MIN occurrences
    // fire, FETCH_CALL_SITES_MIN - 1 stay silent (a couple of stray dynamic fetch calls are a
    // resolution gap, not wrapper blindness).
    let at = TempDir::new("zzop-coverage-fetch-at-floor");
    at.write("a.ts", &dynamic_fetch_lines(FETCH_CALL_SITES_MIN));
    let rels = vec!["a.ts".to_string()];
    assert!(
        builtin_fetch_lexical_warning(at.path(), &rels, 0).is_some(),
        "exactly FETCH_CALL_SITES_MIN sites must fire"
    );

    let below = TempDir::new("zzop-coverage-fetch-below-floor");
    below.write("a.ts", &dynamic_fetch_lines(FETCH_CALL_SITES_MIN - 1));
    let warning = builtin_fetch_lexical_warning(below.path(), &rels, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn non_js_ts_files_are_never_censused() {
    // `fetch(` tokens in a non-js/ts file (here .java) say nothing about builtin fetch — the census
    // filters by the js/ts-family extension set before any read.
    let dir = TempDir::new("zzop-coverage-fetch-java");
    dir.write(
        "A.java",
        &"Object r = client.fetch(url);\n".repeat(FETCH_CALL_SITES_MIN + 1),
    );
    let rels = vec!["A.java".to_string()];
    let warning = builtin_fetch_lexical_warning(dir.path(), &rels, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn lookalike_identifiers_do_not_count_as_fetch_calls() {
    // `refetch(`, `useFetch(`, `prefetch(` — no word boundary before `fetch`, so none count; the
    // property forms (`window.fetch(`) DO count and are covered by the wrapper test above.
    let dir = TempDir::new("zzop-coverage-fetch-lookalike");
    dir.write(
        "a.ts",
        &"refetch(1); useFetch(2); prefetch(3);\n".repeat(FETCH_CALL_SITES_MIN),
    );
    let rels = vec!["a.ts".to_string()];
    let warning = builtin_fetch_lexical_warning(dir.path(), &rels, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn absolute_url_and_bare_const_fetch_sites_do_not_count() {
    // The internal-intent filter's whole job: a tree whose `fetch(` tokens are ALL either absolute-URL
    // (`fetch("https://cdn…")` — a CDN/third-party service, nothing internal to join) or bare-const
    // (`fetch(ENDPOINT_URL)` — the external-corpus `fetch(URL)` idiom, no string literal at all) stays
    // SILENT even at FETCH_CALL_SITES_MIN+ tokens, because none show internal-relative intent.
    let dir = TempDir::new("zzop-coverage-fetch-external-intent");
    let mut src = String::new();
    for i in 0..FETCH_CALL_SITES_MIN {
        src.push_str(&format!(
            "export const a{i} = () => fetch('https://cdn.example.com/x{i}.json');\n"
        ));
    }
    for i in 0..FETCH_CALL_SITES_MIN {
        src.push_str(&format!("export const b{i} = () => fetch(ENDPOINT_URL);\n"));
    }
    dir.write("src/ext.ts", &src);
    let rels = vec!["src/ext.ts".to_string()];
    let warning = builtin_fetch_lexical_warning(dir.path(), &rels, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn computed_relative_template_fetch_sites_do_count() {
    // The census's real target: `fetch(`${base}q<i>`)` — a template literal with no absolute scheme
    // (the computed-internal dark shape the extractor leaves unresolved) DOES count as internal-intent.
    let dir = TempDir::new("zzop-coverage-fetch-computed-intent");
    dir.write("src/api.ts", &dynamic_fetch_lines(FETCH_CALL_SITES_MIN));
    let rels = vec!["src/api.ts".to_string()];
    let warning = builtin_fetch_lexical_warning(dir.path(), &rels, 0);
    assert!(
        warning.as_deref().is_some_and(|w| w
            .contains("5 builtin `fetch(` call site(s) appear lexically across 1 js/ts file(s)")),
        "got: {warning:?}"
    );
}

// --- S6 -----------------------------------------------------------------------------------------

#[test]
fn typeorm_marker_with_zero_db_table_facts_warns_naming_typeorm() {
    let map = package_import_files(&[("typeorm", &["src/user.entity.ts"])]);
    let warning = orm_schema_silence_warning(&map, 0);
    assert!(
        warning.as_deref().is_some_and(|w| w.contains("TypeORM")
            && w.contains("src/user.entity.ts")
            && w.contains("zero db-table io facts")
            && w.contains("a partial envelope covering just the db-table channel is enough")
            && w.contains("zzop contract envelope-guide")
            && w.contains("docs/NORMALIZED_AST.md")),
        "got: {warning:?}"
    );
}

#[test]
fn jpa_marker_at_java_census_grain_warns_naming_jpa() {
    // Java's own F5 census drains to the first-two-dotted-segments grain (`java_census_key`'s doc) —
    // a real `jakarta.persistence.Entity` import censuses as exactly this key.
    let map = package_import_files(&[("jakarta.persistence", &["Order.java"])]);
    let warning = orm_schema_silence_warning(&map, 0);
    assert!(
        warning
            .as_deref()
            .is_some_and(|w| w.contains("Jakarta Persistence (JPA)") && w.contains("Order.java")),
        "got: {warning:?}"
    );
}

#[test]
fn sqlalchemy_marker_with_zero_db_table_facts_warns() {
    let map = package_import_files(&[("sqlalchemy", &["models.py"])]);
    let warning = orm_schema_silence_warning(&map, 0);
    assert!(
        warning.as_deref().is_some_and(|w| w.contains("SQLAlchemy")),
        "got: {warning:?}"
    );
}

#[test]
fn gorm_marker_with_zero_db_table_facts_warns() {
    let map = package_import_files(&[("gorm.io/gorm", &["model.go"])]);
    let warning = orm_schema_silence_warning(&map, 0);
    assert!(
        warning.as_deref().is_some_and(|w| w.contains("GORM")),
        "got: {warning:?}"
    );
}

#[test]
fn nonzero_db_table_facts_short_circuit_even_with_an_orm_marker() {
    // A Prisma repo whose native path DID extract db-table facts (or any tree with adapter-overlaid
    // db-table facts) must stay silent even if an unrelated ORM marker is also present.
    let map = package_import_files(&[("typeorm", &["src/user.entity.ts"])]);
    let warning = orm_schema_silence_warning(&map, 3);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn prisma_import_with_zero_db_table_facts_warns_naming_prisma() {
    // Round-10 dogfood reversal: the native Prisma path only recognizes the `getPrisma()` accessor
    // idiom — a bare-singleton `prisma.<model>.<method>` repo (be-express) extracts ZERO db-table
    // facts, and excluding prisma from the vocab masked exactly that gap. The exact-zero gate keeps
    // the entry self-correcting (see the vocab doc + the nonzero short-circuit test below).
    let map = package_import_files(&[("@prisma/client", &["src/db.ts"])]);
    let warning = orm_schema_silence_warning(&map, 0);
    assert!(
        warning
            .as_deref()
            .is_some_and(|w| w.contains("Prisma") && w.contains("src/db.ts")),
        "got: {warning:?}"
    );
}

#[test]
fn prisma_import_with_extracted_db_table_facts_stays_silent() {
    // The getPrisma()-idiom repo where the native path DID extract facts: nonzero count short-circuits.
    let map = package_import_files(&[("@prisma/client", &["src/db.ts"])]);
    let warning = orm_schema_silence_warning(&map, 2);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn no_orm_marker_never_warns() {
    let map = package_import_files(&[("react", &["src/App.tsx"]), ("lodash", &["src/x.ts"])]);
    let warning = orm_schema_silence_warning(&map, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn an_orm_lookalike_specifier_does_not_match_via_substring() {
    let map = package_import_files(&[("typeorm-extension", &["src/x.ts"])]);
    let warning = orm_schema_silence_warning(&map, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn a_subpath_import_of_an_orm_still_matches() {
    let map = package_import_files(&[("typeorm/decorator/Entity", &["src/user.entity.ts"])]);
    let warning = orm_schema_silence_warning(&map, 0);
    assert!(warning.is_some(), "got: {warning:?}");
}

// --- S7 -----------------------------------------------------------------------------------------

/// `src/lib/api.js`-shaped wrapper module: a private `send` helper wraps the one internal `fetch(`
/// call, and `get`/`post`/`put`/`del` are the only names re-exported — mirrors blind-field test R10's
/// fe-svelte class verbatim (`send` itself is deliberately NOT exported, and must not count).
fn wrapper_module_src() -> &'static str {
    "const base = 'https://api.example.com';\n\
async function send(method, path) { return fetch(base + '/' + path); }\n\
export function get(path) { return send('GET', path); }\n\
export function post(path, data) { return send('POST', path, data); }\n\
export function put(path, data) { return send('PUT', path, data); }\n\
export function del(path) { return send('DELETE', path); }\n"
}

#[test]
fn wrapper_module_with_enough_cross_file_call_sites_fires() {
    let dir = TempDir::new("zzop-coverage-fetch-wrapper-fires");
    dir.write("src/lib/api.js", wrapper_module_src());
    // `$lib/api` is SvelteKit's own alias for `src/lib/api.js` — the loose suffix match resolves it via
    // the shared trailing segment `api`, with no bundler-alias table involved.
    dir.write(
        "src/routes/a.js",
        "import * as api from '$lib/api';\nexport async function load() {\n  await api.get('a');\n  await api.post('b', {});\n}\n",
    );
    dir.write(
        "src/routes/b.js",
        "import * as api from '$lib/api';\nexport async function load() {\n  await api.put('c', {});\n  await api.del('d');\n  await api.get('e');\n}\n",
    );
    let rels = vec![
        "src/lib/api.js".to_string(),
        "src/routes/a.js".to_string(),
        "src/routes/b.js".to_string(),
    ];
    let warning = fetch_wrapper_call_site_warning(dir.path(), &rels, 0);
    assert!(
        warning
            .as_deref()
            .is_some_and(|w| w.contains("src/lib/api.js")
            && w.contains("5 cross-file call site(s)")
            && w.contains("src/routes/a.js")
            && w.contains("src/routes/b.js")
            && w.contains("Mode B overlay adapter")
            // D9 funnel tail — same chain-to-creation convention as every sibling tripwire.
            && w.contains("a partial envelope covering just the consume channel is enough")
            && w.contains("zzop contract envelope-guide")
            && w.contains("docs/NORMALIZED_AST.md")),
        "got: {warning:?}"
    );
}

#[test]
fn healthy_keyed_consumes_short_circuit_without_even_reading_files_s7() {
    // Paths don't exist on disk — verifies the cheap short-circuit path returns `None` once the keyed
    // consume count clears the shared S5/S7 floor, same style as S5's own short-circuit test.
    let rels = vec!["does/not/exist/api.js".to_string()];
    let warning = fetch_wrapper_call_site_warning(Path::new("."), &rels, MIN_PROVIDES_FLOOR);
    assert!(warning.is_none());
}

#[test]
fn wrapper_call_sites_below_the_floor_stay_silent() {
    let dir = TempDir::new("zzop-coverage-fetch-wrapper-below-floor");
    dir.write("src/lib/api.js", wrapper_module_src());
    dir.write(
        "src/routes/a.js",
        "import * as api from '$lib/api';\nexport async function load() {\n  await api.get('a');\n  await api.post('b', {});\n}\n",
    );
    dir.write(
        "src/routes/b.js",
        "import * as api from '$lib/api';\nexport async function load() {\n  await api.put('c', {});\n  await api.del('d');\n}\n",
    );
    let rels = vec![
        "src/lib/api.js".to_string(),
        "src/routes/a.js".to_string(),
        "src/routes/b.js".to_string(),
    ];
    // 4 total cross-file call sites — one short of FETCH_CALL_SITES_MIN (5).
    let warning = fetch_wrapper_call_site_warning(dir.path(), &rels, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn no_wrapper_export_names_stays_silent() {
    // `helper` is not in WRAPPER_EXPORT_NAMES — a plain re-exported fetch helper (unlike an http-verby
    // sender) is a resolution gap S5 already covers, not this tripwire's target.
    let dir = TempDir::new("zzop-coverage-fetch-wrapper-no-export");
    dir.write(
        "src/lib/util.js",
        "export function helper(path) { return fetch(path); }\n",
    );
    dir.write(
        "src/routes/a.js",
        "import { helper } from '../lib/util';\nhelper('a'); helper('b'); helper('c'); helper('d'); helper('e');\n",
    );
    let rels = vec!["src/lib/util.js".to_string(), "src/routes/a.js".to_string()];
    let warning = fetch_wrapper_call_site_warning(dir.path(), &rels, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn class_methods_named_like_the_vocab_are_not_exported_bindings() {
    // A class whose METHODS happen to be named `get`/`post` (Angular's typed-HttpClient idiom) must not
    // be mistaken for a wrapper-defining file even when the file also calls builtin `fetch(` elsewhere —
    // PASS 1 requires an actual top-level EXPORTED binding (`export function`/`export const`/
    // `export {}`), never a class method, however the method happens to be named.
    let dir = TempDir::new("zzop-coverage-fetch-wrapper-class-methods");
    dir.write(
        "src/app/articles.service.ts",
        "export class ArticlesService {\n  ping() { return fetch('/health'); }\n  get(id) { return this.raw(id); }\n  post(x) { return this.raw(x); }\n}\n",
    );
    let rels = vec!["src/app/articles.service.ts".to_string()];
    let warning = fetch_wrapper_call_site_warning(dir.path(), &rels, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}

#[test]
fn a_non_importing_file_with_the_same_names_does_not_count() {
    // A file that never imports the wrapper (no matching `from '...'` specifier) must not contribute
    // call sites even though it happens to call functions with the same vocab names — PASS 2's
    // importer gate, not just the name match, is what attributes a call site to the wrapper.
    let dir = TempDir::new("zzop-coverage-fetch-wrapper-non-importer");
    dir.write("src/lib/api.js", wrapper_module_src());
    dir.write(
        "src/unrelated/map-utils.js",
        "export function get(m, k) { return m.get ? m.get(k) : m[k]; }\n\
get({}, 'a'); get({}, 'b'); get({}, 'c'); get({}, 'd'); get({}, 'e');\n",
    );
    let rels = vec![
        "src/lib/api.js".to_string(),
        "src/unrelated/map-utils.js".to_string(),
    ];
    let warning = fetch_wrapper_call_site_warning(dir.path(), &rels, 0);
    assert!(warning.is_none(), "got: {warning:?}");
}
