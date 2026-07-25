//! `cross-layer/retrying-write-no-idempotency` (critical) — a frontend WRITE call that runs under an
//! automatic retry (`IoConsume::retry_configured`, set by the parser-typescript egress `egress-retry-v1`
//! recognizer: an `axios-retry`-wired file or a `pRetry(...)`/`backOff(...)` wrapper) resolves to a REAL
//! provider route via the cross-layer join, AND that provider route carries no witnessed idempotency guard.
//! If the retry fires — a dropped response, a gateway timeout, a 5xx — the non-idempotent request is
//! replayed, and the provider applies it twice (double charge, duplicate order, doubled increment). A
//! single-repository linter cannot see this: the retry policy lives in the FE tree, the route it doubles
//! lives in the BE tree, and nothing but the HTTP contract ties them together.
//!
//! Anchored at the CONSUME site (where the retry is configured — the line the developer edits); the message
//! cites the provider `file:line` so the reader can check the handler directly.
//!
//! ## Scope of the claim (increment 2 — two-sided check)
//! Increment 1 fired at `warning` on the FE-trigger alone, honestly caveating that the provider guard was
//! never inspected. This increment adds the other side: [`IDEMPOTENCY_GUARDED_ATTR`] on the generic
//! entity-attribute channel (`zzop_core::AttributeStore::route_attr`) is a VETO — if the provider route (an
//! exact `IoKey`, or the longest covering `PathScope`) carries a truthy value for that key, the finding does
//! not fire at all: a guard was witnessed, so the retry is presumed safe. With BOTH sides now checked — a
//! real retry-policy trigger AND a real absence of a witnessed guard on the resolved provider — `critical`
//! is now justified: this is no longer "may double-apply if the provider happens to be unguarded", it is "a
//! replayed non-idempotent write, with no evidence anywhere in this analysis that it's guarded".
//!
//! ## The veto channel
//! [`IDEMPOTENCY_GUARDED_ATTR`] is open-vocab rule vocabulary read off `zzop_core::AttributeStore` (see that
//! module's own doc — "VOCAB-FREE ... consumed BY KEY"), the same shape as `mutating-route-no-auth`'s
//! `auth-guarded`. Two producers:
//! - **Native**: parser-typescript's `router_mounts` inline-handler recognizer sets a private, e2e-pinned
//!   `IDEMPOTENCY_GUARDED_ATTR_KEY` string on a handler it recognizes as reading the `Idempotency-Key`
//!   header — Express/TS only, today's only native source of this evidence.
//! - **Injected**: every other provider language/framework (Python, Java, Go, Rust, a non-recognized TS
//!   shape, ...) needs a Mode B overlay's `attributes` field to assert the same fact — injection-last, same
//!   composition rule as every other attribute channel in this codebase (native and injected evidence both
//!   feed the same store; whichever is present wins outright on an exact route match).
//!
//! ## Honest residual
//! Attribute ABSENCE means "no guard witnessed by either channel", not "proven unguarded" — a handler can be
//! naturally idempotent in a way neither channel expresses (a deterministic upsert keyed on a client-supplied
//! id, a database unique constraint that silently no-ops on conflict, ...) and this rule has no way to see
//! that. This rule honors NO inline marker (see this module's parent doc) — for a handler known-idempotent
//! by inspection the escape hatch is injecting the `idempotency-guarded` attribute, or a config `exclude`.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::{attr_is_truthy, disable_hint, AttributeStore, CrossLayerEdge, Finding, Severity};

use super::{is_write_method, split_key};

/// A retry-configured write consume site, keyed `(source, file, line)` — the join key against `edge.from`.
pub type RetrySite = (String, String, u32);

/// The attribute key this rule reads off the generic entity-attribute channel (`zzop_core::AttributeStore`)
/// to veto a finding whose provider route is known to guard against replayed writes. A producer/adapter
/// that recognizes a handler reads the `Idempotency-Key` header, dedupes, or is otherwise safe to retry
/// injects `{ target: <route IoKey | PathScope>, key: "idempotency-guarded", value: true }` on the PROVIDER
/// tree — see module doc "The veto channel" for the native producer (parser-typescript's `router_mounts`
/// inline-handler recognizer) vs. injection (every other provider language). This literal is RULE
/// vocabulary, never the kernel's — the store is queried by key, agnostic to what it means.
pub const IDEMPOTENCY_GUARDED_ATTR: &str = "idempotency-guarded";

/// The MARKUP-FREE claim this rule's finding must publish, pinned against `docs/rules/catalog.md` and
/// `site/rules.html` by `tests::the_retry_sightline_is_identical_in_the_finding_and_the_docs`.
///
/// Names the PRODUCER, not an extension list: `IoConsume::retry_configured` has exactly one producer
/// (`zzop_parser_typescript`'s egress collector), so there is no list to keep in step.
pub(crate) fn retry_sightline_claim() -> &'static str {
    "an automatic retry is witnessed only by the parser-typescript egress recognizer"
}

/// The sightline sentence the finding appends.
///
/// Why this rule needs one at all, more than most: its TRIGGER is a fact only one producer emits, so the
/// rule is structurally silent everywhere else — and unlike the veto side (whose language limits the
/// message already spells out at length), nothing in the finding says so, because a finding only exists
/// where the trigger fired. The silence is doubly narrow: three SIBLING TypeScript consume producers
/// (hono-client, tRPC, the engine's intra-file fetch-wrapper synthesis) also leave `retry_configured`
/// unset, so even a TypeScript caller can be dark — see `zzop_core::io::IoConsume::retry_configured`'s
/// own doc, which calls that the "increment-2 gap".
fn retry_sightline() -> String {
    format!(
        "LANGUAGE SIGHTLINE: {claim} (an `axios-retry`-wired file, or a `pRetry(...)`/`backOff(...)` \
         wrapper) — no Python (`tenacity`, `urllib3` Retry), Java (Spring Retry, Resilience4j), Go, \
         Rust or C# retry policy is recognized, and within TypeScript the hono-client, tRPC and \
         fetch-wrapper consume paths leave the tag unset too. Since that tag is this rule's only \
         trigger, ZERO findings of this rule means the retry side was NOT ANALYZED there, never \"no \
         replayed write\".",
        claim = retry_sightline_claim()
    )
}

/// Flags every `http` edge whose consumer side is a retry-configured write (`retry_sites` membership) AND
/// whose provider side carries no witnessed [`IDEMPOTENCY_GUARDED_ATTR`] veto. The verb is re-derived from
/// the edge key defensively; `retry_configured` is only ever set on writes, so a non-write here would be a
/// producer bug and is skipped rather than reported. `provider_attrs` is keyed by provider tree source id
/// (`edge.to.source`) — the caller hands one `AttributeStore` per analyzed tree.
pub fn retrying_write_no_idempotency_findings(
    edges: &[CrossLayerEdge],
    retry_sites: &BTreeSet<RetrySite>,
    provider_attrs: &BTreeMap<String, &AttributeStore>,
) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    for edge in edges.iter().filter(|e| e.kind == "http") {
        let site = (
            edge.from.source.clone(),
            edge.from.file.clone(),
            edge.from.line,
        );
        if !retry_sites.contains(&site) {
            continue;
        }
        // Defensive: the tag is write-only by construction, but never fabricate a claim if a future
        // producer change leaks a read here.
        let Some((method, path)) = split_key(&edge.key) else {
            continue;
        };
        if !is_write_method(method) {
            continue;
        }

        // The veto: a witnessed idempotency guard on the resolved provider route clears this edge
        // entirely — see module doc "The veto channel". Absent store, absent attribute, or an explicit
        // falsy value all fall through to the finding below.
        let guarded = provider_attrs
            .get(edge.to.source.as_str())
            .and_then(|store| store.route_attr("http", &edge.key, IDEMPOTENCY_GUARDED_ATTR))
            .is_some_and(attr_is_truthy);
        if guarded {
            continue;
        }

        let cross = if edge.cross_source {
            " across repositories"
        } else {
            ""
        };
        let injection_stub = serde_json::to_string(&serde_json::json!({
            "target": {"ioKey": {"kind": "http", "key": format!("{method} {path}")}},
            "key": IDEMPOTENCY_GUARDED_ATTR,
            "value": true,
        }))
        .expect("json! value of only primitives/strings never fails to serialize");

        let message = format!(
            "This write call is retry-configured and resolves to `{method} {path}`, provided at {}:{}{cross}. \
             No idempotency guard was witnessed on that provider route: no truthy `idempotency-guarded` \
             attribute is set (neither natively recognized — e.g. an inline handler reading the \
             `Idempotency-Key` header — nor injected via a Mode B overlay's `attributes`). If the retry fires \
             (timeout, dropped response, 5xx) the request is replayed, and a non-idempotent handler applies \
             the write twice (double charge, duplicate order). Make the handler idempotent, or if it already \
             is, inject the attribute on the provider tree (paste-ready stub in this finding's \
             `data.injectionStub`; contract: MCP resource `zzop://contract/envelope-guide` on MCP hosts, \n             `zzop contract envelope-guide` with the CLI binary). {} {}",
            edge.to.file,
            edge.to.line,
            disable_hint("cross-layer/retrying-write-no-idempotency"),
            retry_sightline(),
        );

        out.push(Finding {
            rule_id: "cross-layer/retrying-write-no-idempotency".to_string(),
            severity: Severity::Critical,
            file: edge.from.file.clone(),
            line: edge.from.line,
            message,
            data: Some(serde_json::json!({
                "method": method,
                "path": path,
                "provideFile": edge.to.file,
                "provideLine": edge.to.line,
                "crossSource": edge.cross_source,
                "injectionStub": injection_stub,
            })),
        });
    }
    // A fan-out consume (one call site legally matching the same key in 2+ provider trees) can yield
    // byte-identical findings — dedupe on (file, line, message), same convention as `body_field_drift`.
    out.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.message.cmp(&b.message))
    });
    out.dedup_by(|a, b| a.file == b.file && a.line == b.line && a.message == b.message);
    out
}

#[cfg(test)]
mod tests;
