//! `cross-layer/unprovided-mutation-call` (warning) — one finding per `CrossLayerResult::unprovided_consumes`
//! entry of kind `"http"` with a resolved key whose method is a write verb: a state-changing call whose
//! target no analyzed source provides — a silent failure worse than a read returning nothing useful.
//! Anchored at the consume — that's where a fix has to start. This can co-fire with
//! `cross-layer/method-mismatch`, `cross-layer/version-skew`, and `cross-layer/path-near-miss` for the same
//! consume site: those name the EXACT shape of drift when a close-enough candidate provide exists. Their
//! absence means no near candidate exists at all — the target may not exist anywhere in this run.

use zpz_core::io::TaggedConsume;
use zpz_core::{Finding, Severity};

use super::{is_write_method, split_key};

pub fn unprovided_mutation_call_findings(unprovided_consumes: &[TaggedConsume]) -> Vec<Finding> {
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
                 close candidate exists at all: the target route may genuinely not exist yet, its provider \
                 repo may simply be missing from this `analyzeTrees` run, or the route exists but registers \
                 under a non-literal base path (an enum/constant `@Controller(...)` argument, or a \
                 file-routing/dispatch-table framework) this extractor could not resolve — check the provider \
                 source directly before concluding the route is missing. Disable via rule config \
                 `disabled_rules: [\"cross-layer/unprovided-mutation-call\"]` if the provider is known to live \
                 outside this analysis (a repo not included in this run, a third-party service, ...).",
                c.source
            );
            Some(Finding {
                rule_id: "cross-layer/unprovided-mutation-call".to_string(),
                severity: Severity::Warning,
                file: c.consume.file.clone(),
                line: c.consume.line,
                message,
                data: Some(serde_json::json!({
                    "consumeKey": key,
                    "consumeSource": c.source,
                    "method": method,
                    "path": path,
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
    use zpz_core::io::IoConsume;

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
                kind: kind.to_string(),
                key: key.map(str::to_string),
                file: file.to_string(),
                line,
                raw: None,
                method: None,
            },
        }
    }

    #[test]
    fn dangling_write_consume_is_flagged_anchored_at_the_consume() {
        let out = unprovided_mutation_call_findings(&[consume(
            "http",
            Some("POST /api/orders"),
            "fe",
            "Ctx.tsx",
            10,
        )]);
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
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["method"], "POST");
        assert_eq!(data["path"], "/api/orders");
    }

    #[test]
    fn read_method_dangling_consume_is_not_this_rules_turf() {
        let out = unprovided_mutation_call_findings(&[consume(
            "http",
            Some("GET /api/orders"),
            "fe",
            "Ctx.tsx",
            10,
        )]);
        assert!(out.is_empty());
    }

    #[test]
    fn unresolved_key_none_is_handled_defensively_never_panics() {
        let out = unprovided_mutation_call_findings(&[consume("http", None, "fe", "Dyn.tsx", 5)]);
        assert!(out.is_empty());
    }

    #[test]
    fn non_http_dangling_consume_is_ignored() {
        let out = unprovided_mutation_call_findings(&[consume(
            "queue",
            Some("POST /api/orders"),
            "fe",
            "Ctx.tsx",
            10,
        )]);
        assert!(out.is_empty());
    }

    #[test]
    fn determinism_multiple_findings_sorted_by_file_then_line() {
        let out = unprovided_mutation_call_findings(&[
            consume("http", Some("POST /b"), "fe", "z.tsx", 1),
            consume("http", Some("PUT /a"), "fe", "a.tsx", 9),
            consume("http", Some("DELETE /c"), "fe", "a.tsx", 2),
        ]);
        let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
        assert_eq!(sites, vec![("a.tsx", 2), ("a.tsx", 9), ("z.tsx", 1)]);
    }
}
