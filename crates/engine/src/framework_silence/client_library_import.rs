//! S4: http-client import tripwire (consume side).

use std::collections::{BTreeMap, BTreeSet};

use super::controller_silence::MIN_PROVIDES_FLOOR;

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
    "reqwest",
    // Go http-client library — same full-import-path-as-census-key reasoning
    // `server_framework_import`'s own `SERVER_FRAMEWORK_SPECIFIERS` doc gives for its own Go entries; the
    // slash-subpath arm below already covers `"github.com/go-resty/resty/v2"` for free.
    "github.com/go-resty/resty",
    // Spring's RestTemplate/WebClient (Java) — NOT extracted in v1 (this crate's Spring adapter is
    // provide-side only, no http-egress/consume extractor exists for Java yet), so this entry is a pure
    // disclosure vocab word, unlike most of this list's natively-recognized dual: a tree importing
    // `org.springframework.web.client.*` and showing near-zero http consumes is ALWAYS blind here, never
    // a false positive. Census-grain note: Java's F5 drain censuses at the first-TWO-dotted-segments grain
    // (`org.springframework`, not `org.springframework.web.client`) — see this entry's own disjoint-pin
    // caveat below (`java_client_vocab_nested_under_server_vocab_is_deliberate_overlap_not_a_bug` in
    // `tests.rs`): a real Java import specifier censused at that coarser grain never reaches THIS vocab
    // entry's own exact/subpath match at all (it only ever produces the `"org.springframework"` key,
    // which matches S2's entry, not this one) — kept for defensive symmetry with the rest of this list
    // and to keep the entry self-documenting, should the census grain ever change.
    "org.springframework.web.client",
];

/// Whether `specifier` names one of `HTTP_CLIENT_SPECIFIERS`, exact-segment matched the same way
/// `is_server_framework_specifier` matches `SERVER_FRAMEWORK_SPECIFIERS`: the specifier itself equals the
/// vocab entry, or is a subpath import of it (`"@angular/common/http/testing"` still counts as
/// `@angular/common/http` — a testing-only import still implies the client is present in the tree), or the
/// Rust `::`-subpath form (`"reqwest::blocking"` still counts as `reqwest` — same defensive-symmetry note
/// as `is_server_framework_specifier`'s own doc: `collect::collect` censuses only the bare crate head
/// today, so this arm is dormant against a real census entry, kept for a future full-specifier change).
/// Deliberately NOT a substring match, so a lookalike specifier (e.g. `"axios-mock-adapter"`) never
/// matches.
fn is_http_client_specifier(specifier: &str) -> bool {
    HTTP_CLIENT_SPECIFIERS.iter().any(|vocab| {
        specifier == *vocab
            || specifier.starts_with(&format!("{vocab}/"))
            || specifier.starts_with(&format!("{vocab}::"))
    })
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
consumes with a Mode B overlay adapter (see the adapter examples) to restore cross-layer visibility: a \
partial envelope covering just the consume channel is enough; contract: `zzop-mcp contract envelope-guide` \
on MCP hosts, docs/NORMALIZED_AST.md in the repo."
    ))
}
