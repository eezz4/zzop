//! `cross-layer/sdk-import-no-visible-consume` (info) — a tree that imports an SDK-shaped package
//! (`@scope/sdk`, `*-sdk`, `openapi*`, `*api-client*`) from several files, OR an opaque HTTP client
//! library (`superagent`, `got`, `node-fetch`, ...) that the egress extractor cannot trace at all, while
//! having almost no statically visible `http` consumes. A tree consuming its API exclusively through such
//! a client produces zero (or near-zero) fetch-shaped consumes for the join to see, so not even
//! `cross-layer/unresolved-consume-ratio` (which needs >= 5 visible consumes) can report the blind spot.
//! This rule is that report: consumption exists (the import proves it) but flows through a client the
//! egress extractor cannot see, so join-based findings are structurally weak for this tree.
//!
//! Two disjoint detection classes feed the same report:
//! - **SDK-shaped**: a generated/vendor SDK package, credible only once imported from several files
//!   (`MIN_SDK_IMPORTING_FILES`) — a single dangling import proves nothing about tree-wide consumption.
//! - **Opaque HTTP client**: a hand-rolled API client built on a library `match_http_call`
//!   (parser-typescript's egress extractor) does not recognize at all (unlike axios/ky/fetch, which ARE
//!   recognized and must stay excluded from this pattern to avoid FPs on normal trees). Generated-SDK
//!   clients such as oazapfts belong here too: the engine deliberately does not recognize them (decision:
//!   generated SDKs are injection adapters, not engine vocab — see `examples/oazapfts-adapter`), so an
//!   unadapted oazapfts import is exactly the same join-blind shape as a hand-rolled opaque client. These
//!   are credible from a single importing file (`MIN_OPAQUE_CLIENT_IMPORTING_FILES`) since the common idiom
//!   is exactly one central client module wrapping the library for the whole tree.
//!
//! The rule id stays `sdk-import-no-visible-consume` even though the scope is now broader than "SDK" —
//! kept for compatibility with existing `disabled_rules` configs and dashboards.
//!
//! Fires only below `unresolved_consume_ratio`'s `MIN_TOTAL_CONSUMES` floor — the two rules partition the
//! blind-spot space and never co-fire on the same tree.

use std::collections::BTreeMap;

use zzop_core::{disable_hint, Finding, Severity};

use super::{PackageImportSite, MIN_TOTAL_CONSUMES};

/// An SDK package must be imported from at least this many distinct files before the tree-level
/// "consumption flows through an SDK" claim is credible — a single dangling import proves nothing.
const MIN_SDK_IMPORTING_FILES: usize = 3;

/// An opaque HTTP client library is credible from a single importing file — the common idiom is one
/// central hand-rolled client module wrapping the library for the whole tree, unlike a generated SDK
/// which tends to be imported broadly.
const MIN_OPAQUE_CLIENT_IMPORTING_FILES: usize = 1;

/// SDK-shaped package specifier: a whole `sdk`/`openapi` name segment, an `api-client` compound, or a
/// GraphQL client library (`@apollo/client`, `urql`, `graphql-request`) — the same join-blindness as a
/// generated REST SDK. Excludes the bare `graphql` package, since a GraphQL server imports that too and
/// would be misframed as SDK-driven. Segment-anchored so e.g. `sdkim` never matches.
const SDK_SPECIFIER_PATTERN: &str =
    r"(?i)(^|[/@-])(sdk|openapi|api-client|apollo|urql|graphql-request)([/-]|$)";

/// Opaque HTTP client library specifier: packages `match_http_call` (parser-typescript's egress
/// extractor) does not recognize at all, so any HTTP calls made through them are invisible to the
/// cross-layer join. Deliberately excludes axios/ky/fetch — those ARE recognized by the extractor, so
/// including them here would false-positive on ordinary trees. `oazapfts` IS included: the engine no
/// longer recognizes the oazapfts-generated-SDK call family natively (decision: generated SDKs are
/// injection adapters, not engine vocab), so an unadapted oazapfts import is exactly this pattern's
/// opaque-client shape. Segment-anchored so e.g. `requestly` or `gotham` never match — only a whole
/// package name (or the trailing segment of a scoped name) counts.
const OPAQUE_HTTP_CLIENT_PATTERN: &str = r"(?i)(^|[/@-])(superagent-promise|superagent|request-promise|request|got|node-fetch|needle|undici|phin|bent|oazapfts)([/-]|$)";

/// Which detection class flagged a package — exposed in `data` so downstream tooling can distinguish
/// "consumption likely flows through a generated SDK" from "consumption flows through an untraceable
/// hand-rolled client".
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PackageKind {
    Sdk,
    OpaqueClient,
}

impl PackageKind {
    fn as_str(self) -> &'static str {
        match self {
            PackageKind::Sdk => "sdk",
            PackageKind::OpaqueClient => "opaqueClient",
        }
    }
}

pub fn sdk_import_no_visible_consume_findings(
    package_imports: &[PackageImportSite],
    http_consume_totals: &[(String, usize)],
) -> Vec<Finding> {
    let sdk_re = regex::Regex::new(SDK_SPECIFIER_PATTERN).unwrap();
    let opaque_re = regex::Regex::new(OPAQUE_HTTP_CLIENT_PATTERN).unwrap();
    let totals: BTreeMap<&str, usize> = http_consume_totals
        .iter()
        .map(|(s, n)| (s.as_str(), *n))
        .collect();

    // source -> (package, kind) qualifying for a claim: SDK-shaped imported widely enough, or an
    // opaque HTTP client imported at all.
    let mut flagged_by_source: BTreeMap<&str, Vec<(&PackageImportSite, PackageKind)>> =
        BTreeMap::new();
    for p in package_imports {
        let kind = if p.file_count >= MIN_SDK_IMPORTING_FILES && sdk_re.is_match(&p.specifier) {
            Some(PackageKind::Sdk)
        } else if p.file_count >= MIN_OPAQUE_CLIENT_IMPORTING_FILES
            && opaque_re.is_match(&p.specifier)
        {
            Some(PackageKind::OpaqueClient)
        } else {
            None
        };
        if let Some(kind) = kind {
            flagged_by_source
                .entry(p.source.as_str())
                .or_default()
                .push((p, kind));
        }
    }

    let mut out = Vec::new();
    for (source, mut packages) in flagged_by_source {
        let visible = totals.get(source).copied().unwrap_or(0);
        // At or above the ratio rule's floor, `unresolved-consume-ratio` owns the blind-spot report.
        if visible >= MIN_TOTAL_CONSUMES {
            continue;
        }
        packages.sort_by(|(a, _), (b, _)| a.specifier.cmp(&b.specifier));
        let names: Vec<&str> = packages.iter().map(|(p, _)| p.specifier.as_str()).collect();
        let first = packages[0].0;
        let file_count_total = packages.iter().map(|(p, _)| p.file_count).sum::<usize>();
        let message = format!(
            "source `{source}` imports the client/SDK package{} {} from {file_count_total} file{} \
             with only {visible} statically visible http consume{} — API calls flow through a client \
             the egress extractor cannot see, so the cross-layer join is blind for this source and \
             join-based findings (`cross-layer/unconsumed-endpoint`, `cross-layer/unprovided-mutation-call`, \
             ...) are structurally weak here. Prefer literal paths at recognized call sites where \
             practical, or feed this source through a Normalized AST adapter (Mode B) that projects the \
             calls as `IoConsume` facts. {} if the source is intentionally client/SDK-driven and the join \
             blindness is accepted.",
            if names.len() == 1 { "" } else { "s" },
            names
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", "),
            if file_count_total == 1 { "" } else { "s" },
            if visible == 1 { "" } else { "s" },
            disable_hint("cross-layer/sdk-import-no-visible-consume"),
        );
        out.push(Finding {
            rule_id: "cross-layer/sdk-import-no-visible-consume".to_string(),
            severity: Severity::Info,
            file: first.example_file.clone(),
            line: 1,
            message,
            data: Some(serde_json::json!({
                "source": source,
                "sdkPackages": packages
                    .iter()
                    .map(|(p, kind)| serde_json::json!({
                        "specifier": p.specifier,
                        "fileCount": p.file_count,
                        "exampleFile": p.example_file,
                        "kind": kind.as_str(),
                    }))
                    .collect::<Vec<_>>(),
                "visibleHttpConsumes": visible,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests;
