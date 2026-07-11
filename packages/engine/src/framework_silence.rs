//! Coverage self-report: four lexical, extractor-independent tripwires that flag when a tree LOOKS like
//! it carries a framework surface zzop cannot see, so cross-layer joins would otherwise go silently dark
//! with NO honesty channel firing at all (the gap dogfood round 9 found: a whole vue<->express pair went
//! ~totally blind and nothing in `warnings` said so).
//!
//! - S1 [`controller_silence_warning`]: DECORATOR-style controller idioms (Nest-, `@n8n/decorators`-, and
//!   Spring-style — the shapes `zzop_parser_typescript::adapters::controller_decorators` currently
//!   teaches) matched lexically, gated on NEAR-zero (not exact-zero) extracted `http` provides. Round 14's
//!   Angular-FE x Spring-BE pair lost 17/19 routes to a parser limit but still had 2 lexically-extracted
//!   provides tree-wide, which silenced an exact-zero gate; S1 now shares S2's `MIN_PROVIDES_FLOOR`
//!   near-zero floor rather than gating on exactly zero.
//! - S2 [`server_framework_import_warning`]: a server-framework PACKAGE IMPORT (express, koa, fastify,
//!   ...) present while extracted `http` provides stay near-zero. Closes the METHOD-CALL registration
//!   idiom S1's decorator regex structurally cannot see — round 9's be-express tree registered routes as
//!   `router.get('/x', ...)`, never a decorator, and still had 1 extracted provide, which would have
//!   short-circuited an exact-zero gate like S1's.
//! - S3 [`committed_spec_io_silence_warning`]: a committed OpenAPI/Swagger spec sits in the tree while
//!   this tree's io stays near-zero in BOTH directions (provides AND keyed consumes). Round 9's fe-vue
//!   tree talked to its backend through a client generated FROM `src/services/openapi.yml`, so the
//!   consume extractor (which reads call-site literals, not generated SDK internals) saw nothing.
//! - S4 [`client_library_import_warning`]: an http-CLIENT PACKAGE IMPORT (axios, `@angular/common/http`,
//!   ...) present while extracted `http` consumes stay near-zero — the consume-side dual of S2, closing
//!   the gap that round 14's Angular-FE tree exposed: ~15 real `HttpClient` call sites, 0 extracted
//!   consumes, and no consume-side honesty channel at all until now.
//!
//! All four are per-tree self-report `warnings: Vec<String>` strings (not `Finding`s — no rule id, no
//! catalog sync needed); over-disclosure is safe, silence is fatal (the coverage-disclosure decision doc's
//! governing principle) — each function is additive and may fire independently of the others.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

/// DELIBERATELY independent of the parsers' own controller/decorator vocabularies (a pinned
/// policy-value divergence, not an oversight): this tripwire exists to catch EXTRACTOR blindness, so
/// sharing the extractor's vocabulary would blind it to exactly the gaps it guards — an idiom the
/// extractor's vocab misses must still look "controller-shaped" to this independent regex for the
/// self-report to fire. Do not unify with `controller_decorators`/`provides` vocab.
fn controller_decorator_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"@\w*(?:Controller|Mapping|Get|Post|Put|Delete|Patch)\b").unwrap()
    })
}

const MIN_FILES: usize = 3;
const MAX_SAMPLES: usize = 3;

/// Near-zero (not exact-zero) floor shared by S1's `http_provides_count` gate, S2's `http_provides_count`
/// gate, S3's `io_provides`/`io_consumes_keyed` gates, and S4's `http_consumes_count` gate. Round 9's blind
/// be-express tree still had 1 extracted `http` provide — an exact-zero gate misses it entirely — and
/// round 14's blind Spring-BE tree (17/19 routes lost to a parser limit) still had 2 lexically-extracted
/// provides tree-wide, silencing S1's own original exact-zero gate the identical way. A near-zero floor
/// still fires on both while a real micro-BE with 1-2 genuinely-extracted routes (or a real micro-FE with
/// 1-2 genuinely-extracted consumes) gets a gracefully-worded disclosure it can read and ignore, rather
/// than silence.
const MIN_PROVIDES_FLOOR: usize = 3;

/// Returns a ready-to-push `warnings` entry if `candidate_rels` show a controller-decorator-looking line
/// in at least `MIN_FILES` distinct files while `http_provides_count` sits below `MIN_PROVIDES_FLOOR` —
/// NEAR-zero, not exact-zero. Round 14's Angular-FE x Spring-BE pair lost 17/19 routes to a parser limit
/// but still had 2 lexically-extracted provides tree-wide; an exact-zero gate silently passes right over
/// that tree — the identical failure shape S2's `MIN_PROVIDES_FLOOR` near-zero floor was built to catch,
/// so S1 now shares it rather than gating on exactly zero. Cheap on the success path: skips the disk
/// re-read entirely once `http_provides_count >= MIN_PROVIDES_FLOOR`.
///
/// Mild FP surface (accepted): `MIN_FILES`+ files matching the decorator regex while provides stay below
/// the floor can over-fire on, e.g., a tree with a MapStruct `@Mapping` mapper cluster (the regex's
/// `Mapping` alternative matches `@Mapping` too) alongside a genuinely tiny (1-2 route) controller.
/// Accepted under the coverage self-report's governing principle: over-disclosure is safe, silence is
/// fatal.
///
/// Determinism: relies on `candidate_rels` already being sorted/deduped by the caller
/// (`analyze::assemble`) — this function performs no re-sort, so an unsorted input would yield a
/// non-deterministic sample.
pub fn controller_silence_warning(
    root: &Path,
    candidate_rels: &[String],
    http_provides_count: usize,
) -> Option<String> {
    if http_provides_count >= MIN_PROVIDES_FLOOR {
        return None;
    }
    let re = controller_decorator_re();
    let mut matched: Vec<&str> = Vec::new();
    for rel in candidate_rels {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        if text.lines().any(|line| re.is_match(line)) {
            matched.push(rel.as_str());
        }
    }
    if matched.len() < MIN_FILES {
        return None;
    }
    let sample: Vec<&str> = matched.iter().take(MAX_SAMPLES).copied().collect();
    let mut sample_str = sample.join(", ");
    if matched.len() > MAX_SAMPLES {
        sample_str.push_str(&format!(", +{} more", matched.len() - MAX_SAMPLES));
    }
    Some(format!(
        "{} file(s) carry controller-style route decorators/annotations but only {http_provides_count} \
http route(s) were extracted tree-wide — the framework's registration idiom may be unsupported; \
cross-layer joins will be silent for this tree (e.g. {sample_str}) — project this tree's routes with a \
Mode B overlay adapter (see the adapter examples) to restore cross-layer visibility.",
        matched.len()
    ))
}

// --- S2: server-framework import tripwire (provide side) --------------------------------------------

/// Server-framework package specifiers whose route-registration idiom is typically a runtime METHOD CALL
/// (`app.get(...)`, `router.post(...)`) rather than a decorator — invisible to `controller_decorator_re`
/// above. Deliberately server frameworks ONLY: an HTTP CLIENT library (axios, got, ...) says nothing about
/// whether THIS tree serves routes, so including one here would false-positive on an ordinary FE tree.
const SERVER_FRAMEWORK_SPECIFIERS: &[&str] = &[
    "express",
    "koa",
    "fastify",
    "@hapi/hapi",
    "restify",
    "polka",
    "@nestjs/core",
    "@nestjs/common",
    "hono",
    "@trpc/server",
];

/// Whether `specifier` names one of `SERVER_FRAMEWORK_SPECIFIERS`, exact-segment matched: the specifier
/// itself equals the vocab entry, or is a subpath import of it (`"express/lib/router"` still counts as
/// `express`). Deliberately NOT a substring match — every vocab entry here is already a whole, exact npm
/// package identity (unlike `sdk_import_no_visible_consume`'s fragment vocab, e.g. `"sdk"`/`"openapi"`,
/// which needs a real anchored regex to bound a free-form name), so a plain equals-or-prefix check is the
/// exact-segment-boundary equivalent without the regex overhead.
fn is_server_framework_specifier(specifier: &str) -> bool {
    SERVER_FRAMEWORK_SPECIFIERS
        .iter()
        .any(|vocab| specifier == *vocab || specifier.starts_with(&format!("{vocab}/")))
}

/// Returns a ready-to-push `warnings` entry when at least one server-framework package (see
/// `SERVER_FRAMEWORK_SPECIFIERS`) is imported anywhere in the tree while `http_provides_count` sits below
/// `MIN_PROVIDES_FLOOR`. Pure map lookup — no disk IO, so this is cheap on every tree regardless of
/// outcome.
///
/// Determinism: `package_import_files` is a `BTreeMap<specifier, BTreeSet<importing file>>` (both levels
/// already sorted), so iteration order and the first-example-file pick are both deterministic without any
/// extra sort here.
pub fn server_framework_import_warning(
    package_import_files: &BTreeMap<String, BTreeSet<String>>,
    http_provides_count: usize,
) -> Option<String> {
    if http_provides_count >= MIN_PROVIDES_FLOOR {
        return None;
    }
    let mut matched: Vec<(&str, usize, &str)> = Vec::new();
    for (specifier, files) in package_import_files {
        if !is_server_framework_specifier(specifier) {
            continue;
        }
        let Some(example) = files.iter().next() else {
            continue;
        };
        matched.push((specifier.as_str(), files.len(), example.as_str()));
    }
    if matched.is_empty() {
        return None;
    }
    let spec_list = matched
        .iter()
        .map(|(specifier, count, example)| format!("{specifier} ({count} file(s), e.g. {example})"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "server-framework package(s) imported but only {http_provides_count} http route(s) were extracted \
tree-wide: {spec_list} — the registration idiom may be a runtime method call (e.g. `router.get(...)`, \
`app.post(...)`) rather than a decorator, which this extraction pass does not yet recognize; cross-layer \
joins will be near-silent for this tree — project this tree's routes with a Mode B overlay adapter (see \
the adapter examples) to restore cross-layer visibility."
    ))
}

// --- S3: committed-spec io-silence tripwire (consume side) ------------------------------------------

/// Both-direction io-near-zero floor for the committed-spec tripwire. Its own constant (rather than
/// reusing `MIN_PROVIDES_FLOOR` by name) since S2 and S3 gate on different substrates — `http`-only
/// extracted provides vs. total io provides + keyed consumes — and may need to diverge independently
/// later, even though both currently carry the same round-9-derived near-zero rationale and value.
/// `pub(crate)` so `analyze::assemble` can precheck it before building the sorted walked-rel list this
/// function's candidate scan needs — the same "cheap on the success path" convention `controller_silence_warning`'s
/// own doc describes, extended past disk IO to the (much cheaper, but non-zero on a huge tree) rel-list
/// sort itself.
pub(crate) const IO_NEAR_ZERO_FLOOR: usize = MIN_PROVIDES_FLOOR;

/// Cap on how many spec-shaped candidate files get a real disk read (the content probe) — bounds
/// worst-case IO even on a tree with several oddly-named `openapi`/`swagger` json/yaml files, without
/// requiring the caller to pre-filter beyond the walked-file list it already has.
const MAX_SPEC_PROBES: usize = 5;

/// Whether `rel`'s basename looks like a committed OpenAPI/Swagger spec: extension json/yaml/yml AND the
/// basename contains "openapi" or "swagger" (case-insensitive). Cheap, no disk IO — the caller filters the
/// full walked-rel list with this before any probe read happens.
fn is_spec_candidate_rel(rel: &str) -> bool {
    let path = Path::new(rel);
    let ext_ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        e.eq_ignore_ascii_case("json")
            || e.eq_ignore_ascii_case("yaml")
            || e.eq_ignore_ascii_case("yml")
    });
    if !ext_ok {
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower.contains("openapi") || lower.contains("swagger")
}

/// Returns a ready-to-push `warnings` entry when a committed OpenAPI/Swagger spec file exists in the tree
/// while `io_provides_count`/`io_consumes_keyed_count` are BOTH below `IO_NEAR_ZERO_FLOOR` — the signature
/// of a tree that talks through a GENERATED client (SDK class/methods) built from that spec, which the
/// literal-call-site consume extractor cannot see into.
///
/// Gated before any disk IO: returns `None` immediately when either io count already clears the floor (a
/// server tree with real provides, or an FE with healthy keyed consumes, pays zero probe cost). Only then
/// does it filter `all_walked_rels` for spec-shaped candidates and read up to `MAX_SPEC_PROBES` of them,
/// requiring a `"paths"` (json) or `paths:` (yaml) marker before firing — belt-and-braces against a
/// coincidentally named file (e.g. `swagger-ui.css`, already excluded by extension, or a `swagger-theme.json`
/// asset that never mentions `paths`).
///
/// Determinism: `all_walked_rels` must already be sorted by the caller (`analyze::assemble`, the same
/// convention `controller_silence_warning`'s `candidate_rels` relies on) — the first matching candidate
/// probed/reported is therefore deterministic without any extra sort here.
pub fn committed_spec_io_silence_warning(
    root: &Path,
    all_walked_rels: &[String],
    io_provides_count: usize,
    io_consumes_keyed_count: usize,
) -> Option<String> {
    if io_provides_count >= IO_NEAR_ZERO_FLOOR || io_consumes_keyed_count >= IO_NEAR_ZERO_FLOOR {
        return None;
    }
    for rel in all_walked_rels
        .iter()
        .filter(|rel| is_spec_candidate_rel(rel))
        .take(MAX_SPEC_PROBES)
    {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        if !(text.contains("\"paths\"") || text.contains("paths:")) {
            continue;
        }
        return Some(format!(
            "a committed OpenAPI/Swagger spec exists at {rel} but this tree contributed almost no \
joinable io ({io_provides_count} provide(s) / {io_consumes_keyed_count} keyed consume(s)) — if the app \
talks through a GENERATED client (SDK class/methods) rather than direct calls, native extraction cannot \
see those calls; project the generated client with the Mode B openapi-sdk-adapter (see the adapter \
examples for its generated class-method client support) to restore cross-layer visibility."
        ));
    }
    None
}

// --- S4: http-client import tripwire (consume side) --------------------------------------------------

/// HTTP-CLIENT package specifiers whose presence says this tree CALLS OUT over http, so near-zero
/// extracted consumes signals consume-extractor blindness (wrapper indirection, an unrecognized DI idiom,
/// or a generated SDK with no committed spec to anchor `committed_spec_io_silence_warning` on).
/// Deliberately client libraries ONLY — the dual of S2's server-only list: a server-framework import says
/// nothing about whether THIS tree calls OUT over http, so including one here would false-positive on an
/// ordinary BE tree. `fetch`-global and `undici` usage are deliberately absent: `fetch` is a global, not a
/// module specifier, so there is no import to anchor on, and a bare `undici` presence is ambiguous with its
/// common role as a runtime polyfill rather than a deliberate direct dependency.
const HTTP_CLIENT_SPECIFIERS: &[&str] = &[
    "axios",
    "ky",
    "got",
    "superagent",
    "ofetch",
    "redaxios",
    "wretch",
    "node-fetch",
    "@angular/common/http",
];

/// Whether `specifier` names one of `HTTP_CLIENT_SPECIFIERS`, exact-segment matched the same way
/// `is_server_framework_specifier` matches `SERVER_FRAMEWORK_SPECIFIERS`: the specifier itself equals the
/// vocab entry, or is a subpath import of it (`"@angular/common/http/testing"` still counts as
/// `@angular/common/http` — a testing-only import still implies the client is present in the tree).
/// Deliberately NOT a substring match, so a lookalike specifier (e.g. `"axios-mock-adapter"`) never
/// matches.
fn is_http_client_specifier(specifier: &str) -> bool {
    HTTP_CLIENT_SPECIFIERS
        .iter()
        .any(|vocab| specifier == *vocab || specifier.starts_with(&format!("{vocab}/")))
}

/// Returns a ready-to-push `warnings` entry when at least one http-client package (see
/// `HTTP_CLIENT_SPECIFIERS`) is imported anywhere in the tree while `http_consumes_count` sits below
/// `MIN_PROVIDES_FLOOR`. Pure map lookup — no disk IO, so this is cheap on every tree regardless of
/// outcome.
///
/// Gate substrate: `http_consumes_count` must be ALL extracted `http`-kind consume records — keyed AND
/// unresolved both, not just the keyed subset. An unresolved record still proves the extractor SAW the
/// call site (it just could not resolve the target key); blindness is when the extractor saw
/// (near-)nothing at all. Counting only keyed consumes here would conflate "saw it, could not join it" (a
/// resolution gap, already its own disclosure class) with "never saw it" (this tripwire's actual target).
///
/// Determinism: `package_import_files` is a `BTreeMap<specifier, BTreeSet<importing file>>` (both levels
/// already sorted), so iteration order and the first-example-file pick are both deterministic without any
/// extra sort here — same convention as `server_framework_import_warning`.
pub fn client_library_import_warning(
    package_import_files: &BTreeMap<String, BTreeSet<String>>,
    http_consumes_count: usize,
) -> Option<String> {
    if http_consumes_count >= MIN_PROVIDES_FLOOR {
        return None;
    }
    let mut matched: Vec<(&str, usize, &str)> = Vec::new();
    for (specifier, files) in package_import_files {
        if !is_http_client_specifier(specifier) {
            continue;
        }
        let Some(example) = files.iter().next() else {
            continue;
        };
        matched.push((specifier.as_str(), files.len(), example.as_str()));
    }
    if matched.is_empty() {
        return None;
    }
    let spec_list = matched
        .iter()
        .map(|(specifier, count, example)| format!("{specifier} ({count} file(s), e.g. {example})"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "http-client package(s) imported but only {http_consumes_count} http consume site(s) were \
extracted tree-wide: {spec_list} — the call idiom may be a wrapper or DI pattern this extraction pass does \
not recognize; cross-layer joins will be near-silent from this tree's consume side — project this tree's \
consumes with a Mode B overlay adapter (see the adapter examples) to restore cross-layer visibility."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Policy-value divergence pin: `coverage::CoverageCensus::join_contribution_zero` asserts on
    /// EXACT zero (an unconditional structural fact — files > 0 with literally no io either way),
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
            let dir =
                std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
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
            warning
                .as_deref()
                .is_some_and(|w| w.contains("axios") && w.contains("src/api.ts")),
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
}
