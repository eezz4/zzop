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
