//! `cross-layer/body-field-drift` (warning) — the FE request-body literal a call site sends drifts from
//! the request-body DTO the matched BE handler declares (`body-shape-v1`: `ConsumeBodyShape` witnessed at
//! the consume site, `ProvideBodyShape` resolved at assemble time from a `@Body() dto: X` handler param —
//! see `packages/core/src/io.rs`'s module doc for both shapes).
//!
//! Two independent checks, both keyed off comparing the consume's witnessed body keys against the
//! provide's resolved DTO fields:
//! - **Missing required field**: the DTO declares a non-optional field the FE literal never sets, at a
//!   level the FE literal is otherwise EXHAUSTIVE about (`ConsumeBodyShape::complete_at`) — a real gap,
//!   not just an unseen spread/computed key.
//! - **Extra key**: the FE literal sets a key the DTO's field list does not declare, where the DTO's own
//!   field list is COMPLETE (`ProvideBodyShape::complete`) — an extends clause/index signature/constructor
//!   parameter properties would make this a false positive, so it's gated off whenever the DTO shape may be
//!   partial.
//! - **Missing sub-key wrapper**: when the handler's `@Body('user')` names a sub-key, but the FE literal's
//!   (exhaustively witnessed) root never sets that key at all — the DTO is nested one level deeper than
//!   what the FE actually sends.
//!
//! Anchored at the CONSUME site (the call the developer would actually edit); the message cites the
//! provide's `file:line` so the reader can cross-check the DTO declaration. Never guesses: an unresolved
//! `dto_ref` (the assemble-time class-shape merge could not find/disambiguate the DTO — see
//! `zzop_engine::analyze::compose::resolve_provide_body_refs`) or a DTO with zero fields AND an incomplete
//! shape (nothing comparable at all) is skipped outright.
//!
//! ## Caveat (embedded in every finding's message)
//! This is a witnessed-literal comparison only: a request interceptor, an Axios/fetch transform, or a
//! runtime-computed spread can add or strip fields the static literal never shows, and an "extra" key may
//! be silently ignored — or explicitly rejected — by the handler's actual validation pipeline (e.g.
//! `class-validator`'s `whitelist` option). Every message says so and points at manual verification.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::{
    disable_hint, ConsumeBodyShape, CrossLayerEdge, Finding, ProvideBodyShape, Severity,
};

pub fn body_field_drift_findings(
    edges: &[CrossLayerEdge],
    consume_bodies: &BTreeMap<(String, String, u32), ConsumeBodyShape>,
    provide_bodies: &BTreeMap<(String, String, u32), ProvideBodyShape>,
) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    for edge in edges.iter().filter(|e| e.kind == "http") {
        let consume_key = (
            edge.from.source.clone(),
            edge.from.file.clone(),
            edge.from.line,
        );
        let provide_key = (edge.to.source.clone(), edge.to.file.clone(), edge.to.line);
        let Some(consume) = consume_bodies.get(&consume_key) else {
            continue;
        };
        let Some(provide) = provide_bodies.get(&provide_key) else {
            continue;
        };
        // An unresolved `dto_ref` is a leak from assemble-time resolution (should not normally reach a
        // rule, but never guess if it does) — and a DTO with no fields AND a possibly-partial shape has
        // nothing comparable at all.
        if provide.dto_ref.is_some() {
            continue;
        }
        if provide.fields.is_empty() && !provide.complete {
            continue;
        }

        if let Some(finding) = build_drift_finding(edge, consume, provide) {
            out.push(finding);
        }
    }
    // Dedupe identical (file, line, message) — a fan-out consume (2+ edges from the same call site,
    // legal when a tree provides the same key twice) can produce byte-identical findings.
    out.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.message.cmp(&b.message))
    });
    out.dedup_by(|a, b| a.file == b.file && a.line == b.line && a.message == b.message);
    out
}

/// Compares one matched (consume, provide) pair and builds a `Finding` when at least one drift check
/// fires; `None` when the shapes agree (or nothing comparable is exhaustively witnessed).
fn build_drift_finding(
    edge: &CrossLayerEdge,
    consume: &ConsumeBodyShape,
    provide: &ProvideBodyShape,
) -> Option<Finding> {
    let field_names: BTreeSet<&str> = provide.fields.iter().map(|f| f.name.as_str()).collect();
    let required_fields: Vec<&str> = provide
        .fields
        .iter()
        .filter(|f| !f.optional)
        .map(|f| f.name.as_str())
        .collect();

    // `children`: the consume keys one level under the DTO's anchor (the sub-key, or the body root when
    // there is none). `child_level_complete`: whether the consume literal exhaustively enumerated that
    // level, gating the "missing required" check (an incomplete level can't prove absence).
    let mut wrapper_missing = false;
    let (children, child_level_complete): (BTreeSet<String>, bool) =
        match provide.sub_key.as_deref() {
            Some(prefix) => {
                let root_complete = consume.complete_at.iter().any(|c| c.is_empty());
                let wrapper_present = consume.keys.iter().any(|k| k.as_str() == prefix);
                if root_complete && !wrapper_present && !required_fields.is_empty() {
                    wrapper_missing = true;
                }
                let prefix_dot = format!("{prefix}.");
                let children: BTreeSet<String> = consume
                    .keys
                    .iter()
                    .filter_map(|k| k.strip_prefix(prefix_dot.as_str()).map(String::from))
                    .collect();
                let complete = consume.complete_at.iter().any(|c| c.as_str() == prefix);
                (children, complete)
            }
            None => {
                let children: BTreeSet<String> = consume
                    .keys
                    .iter()
                    .filter(|k| !k.contains('.'))
                    .cloned()
                    .collect();
                let complete = consume.complete_at.iter().any(|c| c.is_empty());
                (children, complete)
            }
        };

    let mut extra_keys: Vec<String> = Vec::new();
    if provide.complete {
        extra_keys = children
            .iter()
            .filter(|c| !field_names.contains(c.as_str()))
            .cloned()
            .collect();
        extra_keys.sort();
    }

    let mut missing_required: Vec<String> = Vec::new();
    if child_level_complete {
        missing_required = required_fields
            .iter()
            .filter(|f| !children.contains(**f))
            .map(|f| f.to_string())
            .collect();
        missing_required.sort();
    }

    if !wrapper_missing && extra_keys.is_empty() && missing_required.is_empty() {
        return None;
    }

    let sub_key_path = match provide.sub_key.as_deref() {
        Some(prefix) => format!("body.{prefix}"),
        None => "body".to_string(),
    };

    let mut problems: Vec<String> = Vec::new();
    if wrapper_missing {
        // Only ever set inside the `Some(prefix)` arm above, so `sub_key` is guaranteed present here.
        let prefix = provide.sub_key.as_deref().unwrap_or_default();
        problems.push(format!(
            "FE body has no `{prefix}` key but the handler reads body.{prefix}"
        ));
    }
    if !missing_required.is_empty() {
        problems.push(format!(
            "missing required field(s): {}",
            missing_required.join(", ")
        ));
    }
    if !extra_keys.is_empty() {
        problems.push(format!(
            "key(s) not declared by the handler's DTO: {}",
            extra_keys.join(", ")
        ));
    }

    let message = format!(
        "FE body literal at this call site drifts from the request-body DTO the handler declares at \
         {}:{} (`{sub_key_path}`): {}. This check sees witnessed literals only: interceptors/transforms \
         can add or strip fields, and extra keys may be ignored or rejected server-side — verify against \
         the handler. {}",
        edge.to.file,
        edge.to.line,
        problems.join("; "),
        disable_hint("cross-layer/body-field-drift"),
    );

    let mut data = serde_json::json!({
        "provideFile": edge.to.file,
        "provideLine": edge.to.line,
        "subKey": provide.sub_key,
    });
    if wrapper_missing {
        data["wrapperMissing"] = serde_json::json!(true);
    }
    if !missing_required.is_empty() {
        data["missingRequired"] = serde_json::json!(missing_required);
    }
    if !extra_keys.is_empty() {
        data["extraKeys"] = serde_json::json!(extra_keys);
    }

    Some(Finding {
        rule_id: "cross-layer/body-field-drift".to_string(),
        severity: Severity::Warning,
        file: edge.from.file.clone(),
        line: edge.from.line,
        message,
        data: Some(data),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::io::{EdgeFrom, EdgeTo};
    use zzop_core::ProvideBodyField;

    fn edge(from_file: &str, from_line: u32, to_file: &str, to_line: u32) -> CrossLayerEdge {
        edge_kind("http", from_file, from_line, to_file, to_line)
    }

    fn edge_kind(
        kind: &str,
        from_file: &str,
        from_line: u32,
        to_file: &str,
        to_line: u32,
    ) -> CrossLayerEdge {
        CrossLayerEdge {
            kind: kind.to_string(),
            key: "POST /api/users".to_string(),
            from: EdgeFrom {
                source: "fe".to_string(),
                file: from_file.to_string(),
                line: from_line,
            },
            to: EdgeTo {
                source: "be".to_string(),
                file: to_file.to_string(),
                line: to_line,
                symbol: None,
            },
            cross_source: true,
            low_confidence_reason: None,
        }
    }

    fn consume(keys: &[&str], complete_at: &[&str]) -> ConsumeBodyShape {
        ConsumeBodyShape {
            keys: keys.iter().map(|s| s.to_string()).collect(),
            complete_at: complete_at.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn field(name: &str, optional: bool) -> ProvideBodyField {
        ProvideBodyField {
            name: name.to_string(),
            optional,
        }
    }

    fn provide(
        sub_key: Option<&str>,
        fields: Vec<ProvideBodyField>,
        complete: bool,
    ) -> ProvideBodyShape {
        ProvideBodyShape {
            sub_key: sub_key.map(str::to_string),
            dto_ref: None,
            fields,
            complete,
        }
    }

    fn bodies(
        entries: &[(&str, u32, &str, ConsumeBodyShape)],
    ) -> BTreeMap<(String, String, u32), ConsumeBodyShape> {
        entries
            .iter()
            .map(|(source, line, file, shape)| {
                ((source.to_string(), file.to_string(), *line), shape.clone())
            })
            .collect()
    }

    fn provides(
        entries: &[(&str, u32, &str, ProvideBodyShape)],
    ) -> BTreeMap<(String, String, u32), ProvideBodyShape> {
        entries
            .iter()
            .map(|(source, line, file, shape)| {
                ((source.to_string(), file.to_string(), *line), shape.clone())
            })
            .collect()
    }

    #[test]
    fn happy_drift_reports_both_missing_required_and_extra_key() {
        let e = edge("Api.tsx", 10, "Api.java", 20);
        let consumes = bodies(&[("fe", 10, "Api.tsx", consume(&["name", "extra"], &[""]))]);
        let provs = provides(&[(
            "be",
            20,
            "Api.java",
            provide(
                None,
                vec![field("name", false), field("email", false)],
                true,
            ),
        )]);
        let out = body_field_drift_findings(&[e], &consumes, &provs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/body-field-drift");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "Api.tsx");
        assert_eq!(out[0].line, 10);
        assert!(out[0].message.contains("missing required field(s): email"));
        assert!(out[0]
            .message
            .contains("key(s) not declared by the handler's DTO: extra"));
        assert!(out[0].message.contains("Api.java:20"));
        assert!(out[0].message.contains("witnessed literals only"));
        assert!(out[0].message.contains("cross-layer/body-field-drift"));
        assert!(out[0].message.contains("disabled_rules"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["missingRequired"], serde_json::json!(["email"]));
        assert_eq!(data["extraKeys"], serde_json::json!(["extra"]));
    }

    #[test]
    fn extra_key_is_suppressed_when_dto_shape_is_incomplete() {
        let e = edge("Api.tsx", 1, "Api.java", 2);
        let consumes = bodies(&[("fe", 1, "Api.tsx", consume(&["name", "extra"], &[""]))]);
        let provs = provides(&[(
            "be",
            2,
            "Api.java",
            provide(None, vec![field("name", false)], false), // complete: false
        )]);
        assert!(body_field_drift_findings(&[e], &consumes, &provs).is_empty());
    }

    #[test]
    fn missing_required_is_suppressed_when_consume_level_is_incomplete() {
        let e = edge("Api.tsx", 1, "Api.java", 2);
        // No "" in complete_at -> root level not exhaustively witnessed (e.g. a spread present).
        let consumes = bodies(&[("fe", 1, "Api.tsx", consume(&["name"], &[]))]);
        let provs = provides(&[(
            "be",
            2,
            "Api.java",
            provide(
                None,
                vec![field("name", false), field("email", false)],
                true,
            ),
        )]);
        assert!(body_field_drift_findings(&[e], &consumes, &provs).is_empty());
    }

    #[test]
    fn sub_key_wrapper_missing_with_required_fields_fires() {
        let e = edge("Api.tsx", 5, "Api.java", 9);
        // Root is exhaustively witnessed and does NOT contain a "user" key at all.
        let consumes = bodies(&[("fe", 5, "Api.tsx", consume(&["other"], &[""]))]);
        let provs = provides(&[(
            "be",
            9,
            "Api.java",
            provide(Some("user"), vec![field("email", false)], true),
        )]);
        let out = body_field_drift_findings(&[e], &consumes, &provs);
        assert_eq!(out.len(), 1);
        assert!(out[0]
            .message
            .contains("FE body has no `user` key but the handler reads body.user"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["wrapperMissing"], serde_json::json!(true));
    }

    #[test]
    fn sub_key_wrapper_missing_but_all_fields_optional_is_silent() {
        let e = edge("Api.tsx", 5, "Api.java", 9);
        let consumes = bodies(&[("fe", 5, "Api.tsx", consume(&["other"], &[""]))]);
        let provs = provides(&[(
            "be",
            9,
            "Api.java",
            provide(Some("user"), vec![field("email", true)], true),
        )]);
        assert!(body_field_drift_findings(&[e], &consumes, &provs).is_empty());
    }

    #[test]
    fn non_http_edge_is_skipped() {
        let e = edge_kind("trpc", "Api.tsx", 1, "Api.java", 2);
        let consumes = bodies(&[("fe", 1, "Api.tsx", consume(&["name"], &[""]))]);
        let provs = provides(&[(
            "be",
            2,
            "Api.java",
            provide(None, vec![field("name", false)], true),
        )]);
        assert!(body_field_drift_findings(&[e], &consumes, &provs).is_empty());
    }

    #[test]
    fn consume_only_body_is_silent() {
        let e = edge("Api.tsx", 1, "Api.java", 2);
        let consumes = bodies(&[("fe", 1, "Api.tsx", consume(&["name"], &[""]))]);
        let provs: BTreeMap<(String, String, u32), ProvideBodyShape> = BTreeMap::new();
        assert!(body_field_drift_findings(&[e], &consumes, &provs).is_empty());
    }

    #[test]
    fn provide_only_body_is_silent() {
        let e = edge("Api.tsx", 1, "Api.java", 2);
        let consumes: BTreeMap<(String, String, u32), ConsumeBodyShape> = BTreeMap::new();
        let provs = provides(&[(
            "be",
            2,
            "Api.java",
            provide(None, vec![field("name", false)], true),
        )]);
        assert!(body_field_drift_findings(&[e], &consumes, &provs).is_empty());
    }

    #[test]
    fn unresolved_dto_ref_leak_is_silent() {
        let e = edge("Api.tsx", 1, "Api.java", 2);
        let consumes = bodies(&[("fe", 1, "Api.tsx", consume(&["extra"], &[""]))]);
        let mut shape = provide(None, vec![], true);
        shape.dto_ref = Some("CreateUserDto".to_string());
        let provs = provides(&[("be", 2, "Api.java", shape)]);
        assert!(body_field_drift_findings(&[e], &consumes, &provs).is_empty());
    }

    #[test]
    fn empty_fields_and_incomplete_shape_has_nothing_comparable() {
        let e = edge("Api.tsx", 1, "Api.java", 2);
        let consumes = bodies(&[("fe", 1, "Api.tsx", consume(&["extra"], &[""]))]);
        let provs = provides(&[("be", 2, "Api.java", provide(None, vec![], false))]);
        assert!(body_field_drift_findings(&[e], &consumes, &provs).is_empty());
    }

    #[test]
    fn fan_out_consume_joining_the_same_provide_site_twice_is_deduped() {
        // Two edges from the SAME consume call site to the SAME provide site (a legal multi-provider-key
        // join can produce a duplicate edge pair) must yield exactly one finding, not two.
        let edges = vec![
            edge("Api.tsx", 1, "Api.java", 2),
            edge("Api.tsx", 1, "Api.java", 2),
        ];
        let consumes = bodies(&[("fe", 1, "Api.tsx", consume(&["extra"], &[""]))]);
        let provs = provides(&[(
            "be",
            2,
            "Api.java",
            provide(None, vec![field("name", false)], true),
        )]);
        let out = body_field_drift_findings(&edges, &consumes, &provs);
        assert_eq!(out.len(), 1, "identical (file, line, message) must dedupe");
    }

    #[test]
    fn determinism_sorted_by_file_then_line() {
        let edges = vec![
            edge("z.tsx", 1, "Api.java", 1),
            edge("a.tsx", 9, "Api.java", 2),
            edge("a.tsx", 2, "Api.java", 3),
        ];
        let consumes = bodies(&[
            ("fe", 1, "z.tsx", consume(&["extra"], &[""])),
            ("fe", 9, "a.tsx", consume(&["extra"], &[""])),
            ("fe", 2, "a.tsx", consume(&["extra"], &[""])),
        ]);
        let provs = provides(&[
            (
                "be",
                1,
                "Api.java",
                provide(None, vec![field("name", false)], true),
            ),
            (
                "be",
                2,
                "Api.java",
                provide(None, vec![field("name", false)], true),
            ),
            (
                "be",
                3,
                "Api.java",
                provide(None, vec![field("name", false)], true),
            ),
        ]);
        let out = body_field_drift_findings(&edges, &consumes, &provs);
        let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
        assert_eq!(sites, vec![("a.tsx", 2), ("a.tsx", 9), ("z.tsx", 1)]);
    }
}
