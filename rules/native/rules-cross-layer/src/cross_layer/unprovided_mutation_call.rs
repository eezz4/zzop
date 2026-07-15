//! `cross-layer/unprovided-mutation-call` (warning, downgraded to info when the run has a provide-blind
//! source) — one finding per `CrossLayerResult::unprovided_consumes` entry of kind `"http"` with a resolved
//! key whose method is a write verb: a state-changing call whose target no analyzed source provides — a
//! silent failure worse than a read returning nothing useful. Anchored at the consume — that's where a fix
//! has to start. This can co-fire with `cross-layer/method-mismatch`, `cross-layer/version-skew`, and
//! `cross-layer/path-near-miss` for the same consume site: those name the EXACT shape of drift when a
//! close-enough candidate provide exists. Their absence means no near candidate exists at all — the target
//! may not exist anywhere in this run.
//!
//! ## Confidence downgrade when the run is provide-blind
//! Class-extrapolation defect (opus-reviewer, symmetric sibling of `unconsumed-mutation-endpoint`'s
//! consume-blind downgrade): this rule fired Warning unconditionally, even on a run where a
//! framework-bearing tree extracted almost no `http` routes (the S2 `server_framework_import_warning`
//! tripwire condition, `zzop_engine::framework_silence`) — a confident "no matching provide anywhere"
//! verdict is only trustworthy when the provide side was actually resolved. When `provide_blind_sources`
//! (`zzop_engine::framework_silence::provide_blind_sources`, the provide-side analog of
//! `super::majority_unresolved_http_sources`) is non-empty for this run, "unprovided" cannot be trusted as a
//! confident zero — this rule de-escalates to `Severity::Info` and names the blind source(s) in the message
//! instead of silently keeping Warning. With zero blind sources, severity and message keep today's
//! Warning framing unchanged. This is a de-escalation to match confidence, NOT suppression — the finding
//! still fires either way (`output-philosophy.md` §0: total by default). The sealed class: a confident
//! cross-layer zero (unconsumed / unprovided) must never fire at warning severity without gating on the
//! resolution completeness of the OTHER side.

use std::collections::BTreeSet;

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, Finding, Severity};

use super::{is_write_method, split_key};

pub fn unprovided_mutation_call_findings(
    unprovided_consumes: &[TaggedConsume],
    provide_blind_sources: &BTreeSet<String>,
) -> Vec<Finding> {
    // Run-level, not per-consume: "is this run's provide side blind at all" is the question, since a blind
    // source ANYWHERE in the run is a plausible unseen provider of ANY write call regardless of which tree
    // consumes it (see this rule's module doc's "Confidence downgrade" section).
    let severity = if provide_blind_sources.is_empty() {
        Severity::Warning
    } else {
        Severity::Info
    };
    let downgrade_note = if provide_blind_sources.is_empty() {
        String::new()
    } else {
        let named: Vec<String> = provide_blind_sources
            .iter()
            .take(3)
            .map(|s| format!("`{s}`"))
            .collect();
        let more = provide_blind_sources.len() - named.len();
        let more_note = if more > 0 {
            format!(", and {more} more")
        } else {
            String::new()
        };
        format!(
            " This run has a provider-side blind spot too: source(s) {}{more_note} import a server \
             framework but extracted almost no http routes tree-wide — so severity here is reduced to \
             info: \"no matching provide anywhere\" cannot be trusted as a confident zero, and the provider \
             may well exist in one of those sources, unseen by this extraction. Check the source directly \
             before treating this call as unprovided.",
            named.join(", ")
        )
    };

    let mut out: Vec<Finding> = unprovided_consumes
        .iter()
        .filter(|c| c.consume.kind == "http")
        .filter_map(|c| {
            let key = c.consume.key.as_deref()?;
            let (method, path) = split_key(key)?;
            if !is_write_method(method) {
                return None;
            }
            let message = format!(
                "write call `{key}` (source `{}`) has no matching provide anywhere in this analysis. On a \
                 state-changing call a silent 404 (or an unintended catch-all match) is worse than on a read \
                 — a write that appears to succeed but changes nothing, or drifts, is easy to miss. If \
                 `cross-layer/method-mismatch`, `cross-layer/version-skew`, or `cross-layer/path-near-miss` \
                 also fired for this same consume, one of them likely names the exact drift (a method typo, \
                 a version-segment skew, or a near-miss path) — check those first. If none of them fired, no \
                 near-miss candidate was reported: the target route may genuinely not exist yet, its provider \
                 repo may simply be missing from this `analyzeTrees` run, or the route exists but registers \
                 under a non-literal base path (an enum/constant `@Controller(...)` argument, or a \
                 file-routing/dispatch-table framework) this extractor could not resolve — check the provider \
                 source directly before concluding the route is missing.{downgrade_note} {} if the provider \
                 is known to live outside this analysis (a repo not included in this run, a third-party \
                 service, ...).",
                c.source,
                disable_hint("cross-layer/unprovided-mutation-call")
            );
            Some(Finding {
                rule_id: "cross-layer/unprovided-mutation-call".to_string(),
                severity,
                file: c.consume.file.clone(),
                line: c.consume.line,
                message,
                data: Some(serde_json::json!({
                    "consumeKey": key,
                    "consumeSource": c.source,
                    "method": method,
                    "path": path,
                    "provideBlindSourceCount": provide_blind_sources.len(),
                })),
            })
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::io::IoConsume;

    fn consume(
        kind: &str,
        key: Option<&str>,
        source: &str,
        file: &str,
        line: u32,
    ) -> TaggedConsume {
        TaggedConsume {
            source: source.to_string(),
            consume: IoConsume {
                client: None,
                body: None,
                kind: kind.to_string(),
                key: key.map(str::to_string),
                file: file.to_string(),
                line,
                raw: None,
                method: None,
            },
        }
    }

    fn no_blind() -> BTreeSet<String> {
        BTreeSet::new()
    }

    fn blind(sources: &[&str]) -> BTreeSet<String> {
        sources.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn dangling_write_consume_is_flagged_anchored_at_the_consume() {
        let out = unprovided_mutation_call_findings(
            &[consume(
                "http",
                Some("POST /api/orders"),
                "fe",
                "Ctx.tsx",
                10,
            )],
            &no_blind(),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/unprovided-mutation-call");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "Ctx.tsx");
        assert_eq!(out[0].line, 10);
        assert!(out[0].message.contains("POST /api/orders"));
        assert!(out[0].message.contains("cross-layer/method-mismatch"));
        assert!(out[0].message.contains("cross-layer/version-skew"));
        assert!(out[0].message.contains("cross-layer/path-near-miss"));
        assert!(out[0].message.contains("disabled_rules"));
        assert!(!out[0].message.contains("provider-side blind spot"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["method"], "POST");
        assert_eq!(data["path"], "/api/orders");
        assert_eq!(data["provideBlindSourceCount"], 0);
    }

    #[test]
    fn read_method_dangling_consume_is_not_this_rules_turf() {
        let out = unprovided_mutation_call_findings(
            &[consume(
                "http",
                Some("GET /api/orders"),
                "fe",
                "Ctx.tsx",
                10,
            )],
            &no_blind(),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn unresolved_key_none_is_handled_defensively_never_panics() {
        let out = unprovided_mutation_call_findings(
            &[consume("http", None, "fe", "Dyn.tsx", 5)],
            &no_blind(),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn non_http_dangling_consume_is_ignored() {
        let out = unprovided_mutation_call_findings(
            &[consume(
                "queue",
                Some("POST /api/orders"),
                "fe",
                "Ctx.tsx",
                10,
            )],
            &no_blind(),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn determinism_multiple_findings_sorted_by_file_then_line() {
        let out = unprovided_mutation_call_findings(
            &[
                consume("http", Some("POST /b"), "fe", "z.tsx", 1),
                consume("http", Some("PUT /a"), "fe", "a.tsx", 9),
                consume("http", Some("DELETE /c"), "fe", "a.tsx", 2),
            ],
            &no_blind(),
        );
        let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
        assert_eq!(sites, vec![("a.tsx", 2), ("a.tsx", 9), ("z.tsx", 1)]);
    }

    // --- Severity calibration (symmetric to unconsumed_mutation_endpoint's consume-blind downgrade) ---

    #[test]
    fn a_provide_blind_source_downgrades_severity_to_info_and_names_the_source() {
        let out = unprovided_mutation_call_findings(
            &[consume(
                "http",
                Some("POST /api/orders"),
                "fe",
                "Ctx.tsx",
                10,
            )],
            &blind(&["be"]),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Info);
        assert!(out[0].message.contains("`be`"), "{}", out[0].message);
        assert!(
            out[0].message.contains("provider-side blind spot"),
            "{}",
            out[0].message
        );
        // Still names the write call and points at the near-miss cross-references — the downgrade
        // lowers confidence, not the underlying claim.
        assert!(out[0].message.contains("POST /api/orders"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["provideBlindSourceCount"], 1);
    }

    #[test]
    fn no_provide_blind_source_keeps_warning_and_todays_framing() {
        let out = unprovided_mutation_call_findings(
            &[consume(
                "http",
                Some("POST /api/orders"),
                "fe",
                "Ctx.tsx",
                10,
            )],
            &no_blind(),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Warning);
        assert!(!out[0].message.contains("provider-side blind spot"));
    }

    #[test]
    fn provide_blind_source_list_is_capped_at_three_with_a_remainder_count() {
        let out = unprovided_mutation_call_findings(
            &[consume(
                "http",
                Some("POST /api/orders"),
                "fe",
                "Ctx.tsx",
                10,
            )],
            &blind(&["a", "b", "c", "d", "e"]),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Info);
        assert!(out[0].message.contains("`a`"));
        assert!(out[0].message.contains("`b`"));
        assert!(out[0].message.contains("`c`"));
        assert!(!out[0].message.contains("`d`"));
        assert!(out[0].message.contains("and 2 more"), "{}", out[0].message);
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["provideBlindSourceCount"], 5);
    }
}
