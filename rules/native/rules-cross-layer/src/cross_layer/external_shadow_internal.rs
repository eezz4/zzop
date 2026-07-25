//! `cross-layer/external-shadow-internal` (warning) — an `external` (host-carrying) http consume whose
//! normalized `METHOD path` matches a route some analyzed tree actually PROVIDES. The caller reaches an
//! INTERNAL route through a hardcoded absolute URL (an environment host baked directly into the call site)
//! instead of the relative/proxy path the route is normally reached through — a classic source of
//! environment-specific breakage (the hardcoded host is wrong in another environment) and a bypass of
//! whatever the proxy/gateway layer was meant to enforce (auth, rewriting, rate limiting). Anchored at the
//! consume — the fix (drop the hardcoded host, go through the relative/proxied path) lands there.
//!
//! Consume sites in test-path files (`zzop_core::is_test_file`) are skipped — a test mocking a
//! vendor/own API is not deployed egress.
//!
//! ## Contentless-path gate (why a third-party root URL is not a shadow)
//! The match is made on the HOST-STRIPPED path, so the rule's evidence is only as strong as the path.
//! When the path carries no literal segment ([`super::is_all_slot_path`] — the bare root `/`, or an
//! all-slot `/{}` head-drop artifact) the comparison decides NOTHING: every third-party root URL
//! projects onto `GET /`, so `https://v2ex.com/?tab=hot` "matches" any worker/root handler in the
//! analysis exactly as well as a genuinely misrouted internal URL would. Such consumes are skipped.
//!
//! Measured: mono-hub `community-hub-fe/src/lib/sources/V2EX_HOT.ts:14` (`GET https://www.v2ex.com/?tab=hot`,
//! query dropped by [`split_external_key`]) was reported as shadowing `@base/utils-all`'s `GET /`
//! worker root handler (2026-07-25) — nothing about `v2ex.com` is internal. The corpus reproduces it
//! only once same-file URL-binding resolution puts that call in the `external_consumes` bucket at all,
//! which is why the pin below carries the corpus key verbatim rather than relying on a corpus run.
//!
//! **Documented residual (a real shadow this gate silences)**: a hardcoded INTERNAL host whose path is
//! the root — `fetch('https://app.internal.example.com/')` against an internal `GET /`. That call is a
//! genuine environment-host bake-in, and it now stays silent. Accepted because the matcher cannot tell
//! it from the third-party case: both produce byte-identical evidence (`GET /` == `GET /`), so firing
//! on it was correct only by coincidence. Pinned as an explicit false negative below so widening the
//! rule back requires new evidence — a host axis (which hosts are OURS) this analysis does not have —
//! rather than deleting a line. The other two path-comparing external-egress rules each already carried
//! a narrower hand-written version of this gate — `external_base_url_drift` (2+ path segments) and
//! `external_version_inconsistent` ("a root call pins no version") — and now share the predicate; this
//! rule was the one family member that had none.

use std::collections::BTreeMap;

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, http_interface_key, Finding, Severity};

use super::{is_all_slot_path, path_segments, split_external_key, HttpProvideSite};

pub fn external_shadow_internal_findings(
    external_consumes: &[TaggedConsume],
    all_provides: &[HttpProvideSite],
) -> Vec<Finding> {
    let mut by_key: BTreeMap<&str, Vec<&HttpProvideSite>> = BTreeMap::new();
    for p in all_provides {
        by_key.entry(p.key.as_str()).or_default().push(p);
    }
    for sites in by_key.values_mut() {
        sites.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then(a.file.cmp(&b.file))
                .then(a.line.cmp(&b.line))
        });
    }

    let mut out = Vec::new();
    for c in external_consumes
        .iter()
        .filter(|c| c.consume.kind == "http" && !zzop_core::is_test_file(&c.consume.file))
    {
        let Some(key) = c.consume.key.as_deref() else {
            continue;
        };
        let Some(url) = split_external_key(key) else {
            continue;
        };
        // The host is this key's identity and the match throws it away — so a path carrying no
        // literal segment leaves nothing to match ON (see the module doc's contentless-path gate).
        if is_all_slot_path(&path_segments(url.path)) {
            continue;
        }
        let normalized = http_interface_key(url.method, url.path);
        let Some(sites) = by_key.get(normalized.as_str()) else {
            continue;
        };
        let Some(first) = sites.first() else {
            continue;
        };
        let other_provide_count = sites.len() - 1;
        let other_note = if other_provide_count > 0 {
            format!(" (and {other_provide_count} other provide site(s) also serve this route)")
        } else {
            String::new()
        };

        let message = format!(
            "consume `{key}` (source `{}`) reaches host `{}` with an absolute URL, but the same route \
             `{normalized}` is provided INTERNALLY by this analysis — at {}:{} (source `{}`){other_note}. This \
             looks like a hardcoded environment host baked into the call site instead of the relative or \
             proxied path the route is normally reached through, which breaks in any other environment and \
             may bypass whatever a gateway/proxy layer enforces (auth, rewriting, rate limiting). Verify \
             whether this call is meant to hit the internal route directly, and if so replace the hardcoded \
             host with the relative/proxy path. {} if hitting this host directly (bypassing the proxy) is \
             intentional, e.g. a health check or an internal tool calling a fixed deployment URL on purpose.",
            c.source, url.host, first.file, first.line, first.source,
            disable_hint("cross-layer/external-shadow-internal"),
        );
        out.push(Finding {
            rule_id: "cross-layer/external-shadow-internal".to_string(),
            severity: Severity::Warning,
            file: c.consume.file.clone(),
            line: c.consume.line,
            message,
            data: Some(serde_json::json!({
                "consumeKey": key,
                "host": url.host,
                "normalizedKey": normalized,
                "matchedProvide": {"source": first.source, "file": first.file, "line": first.line},
                "otherProvideCount": other_provide_count,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests;
