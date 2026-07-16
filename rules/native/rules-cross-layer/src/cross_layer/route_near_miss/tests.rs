use super::*;

fn consume(kind: &str, key: Option<&str>, source: &str, file: &str, line: u32) -> TaggedConsume {
    TaggedConsume {
        source: source.to_string(),
        consume: zzop_core::IoConsume {
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

fn provide(key: &str, source: &str, file: &str, line: u32) -> HttpProvideSite {
    HttpProvideSite {
        source: source.to_string(),
        key: key.to_string(),
        file: file.to_string(),
        line,
    }
}

#[test]
fn case_dimension_fires() {
    let unprovided_consumes = vec![consume(
        "http",
        Some("GET /api/Articles"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /api/articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    let out = route_near_miss_findings(&unprovided_consumes, &provides);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].rule_id, "cross-layer/route-near-miss");
    assert_eq!(out[0].severity, Severity::Info);
    assert_eq!(out[0].file, "Api.tsx");
    assert_eq!(out[0].line, 10);
    assert_eq!(out[0].data.as_ref().unwrap()["dimension"], "case");
    assert!(out[0].message.contains("casing"));
    assert!(out[0].message.contains("disabled_rules"));
    assert!(out[0].message.contains("articles.controller.ts:22"));
}

#[test]
fn prefix_dimension_fires_when_consume_is_missing_the_prefix() {
    // The canonical NestJS setGlobalPrefix('api') drift — a 1-segment shared suffix. MUST fire.
    let unprovided_consumes = vec![consume(
        "http",
        Some("GET /articles"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /api/articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    let out = route_near_miss_findings(&unprovided_consumes, &provides);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].data.as_ref().unwrap()["dimension"], "prefix");
    assert!(out[0].message.contains("missing path prefix"));
    assert!(out[0].message.contains("/api"));
}

#[test]
fn prefix_dimension_fires_when_consume_carries_the_extra_prefix() {
    let unprovided_consumes = vec![consume(
        "http",
        Some("GET /api/articles"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    let out = route_near_miss_findings(&unprovided_consumes, &provides);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].data.as_ref().unwrap()["dimension"], "prefix");
    assert!(out[0].message.contains("extra path prefix"));
    assert!(out[0].message.contains("/api"));
}

#[test]
fn prefix_dimension_single_short_segment_suffix_is_accepted_low_confidence() {
    // `/me` vs `/api/me` — a 1-segment shared suffix. Intentionally accepted at Info: low-confidence
    // but the base-prefix shape is real. Pinned here so a future tightening is a visible change.
    let unprovided_consumes = vec![consume("http", Some("GET /me"), "fe-react", "Api.tsx", 10)];
    let provides = vec![provide("GET /api/me", "be-nest", "me.controller.ts", 22)];
    let out = route_near_miss_findings(&unprovided_consumes, &provides);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].data.as_ref().unwrap()["dimension"], "prefix");
}

#[test]
fn prefix_dimension_param_leading_run_is_excluded() {
    // `/articles` vs `/{}/articles` — the added leading run is a `{}` parameter, not a base path. Must
    // NOT fire (FIX 3): a parameter segment is never a prefix.
    let unprovided_consumes = vec![consume(
        "http",
        Some("GET /articles"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /{}/articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
}

#[test]
fn prefix_dimension_does_not_fire_when_added_prefix_is_too_long() {
    // `/x` vs `/a/b/c/x` — a 3-segment added prefix exceeds the 1-2 base-path bound. Must NOT fire.
    let unprovided_consumes = vec![consume("http", Some("GET /x"), "fe-react", "Api.tsx", 10)];
    let provides = vec![provide("GET /a/b/c/x", "be-nest", "x.controller.ts", 22)];
    assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
}

#[test]
fn all_slot_consume_is_gated_out_of_prefix_dimension() {
    // Field-measured case: a head-drop artifact key `GET /{}` must not vacuously satisfy the
    // prefix dimension against a same-shaped provide (`GET /api/{}` — suffix `{}`==`{}`, leading
    // run `api` is all-literal and length 1).
    let unprovided_consumes = vec![consume("http", Some("GET /{}"), "fe-react", "Api.tsx", 10)];
    let provides = vec![provide(
        "GET /api/{}",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
}

#[test]
fn prefix_dimension_does_not_fire_when_suffix_does_not_match() {
    let unprovided_consumes = vec![consume(
        "http",
        Some("GET /widgets"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /api/articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
}

#[test]
fn arity_shaped_pair_no_longer_fires() {
    // Literal segments equal, differing `{}` count (`/api/articles/{}/{}` vs `/api/articles/{}`) — the
    // former `arity` dimension. Dropped as mostly-FP (REST collection/item shapes), so nothing fires.
    let unprovided_consumes = vec![consume(
        "http",
        Some("GET /api/articles/{}/{}"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /api/articles/{}",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
}

#[test]
fn pure_param_generalization_pair_is_disjoint_from_path_near_miss() {
    // Same segment count, every segment equal-or-`{}` — this is path_near_miss's pair shape, not
    // route-near-miss's; route-near-miss must stay silent on it.
    let unprovided_consumes = vec![consume(
        "http",
        Some("GET /users/{}/profile"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /users/active/profile",
        "be-nest",
        "users.controller.ts",
        22,
    )];
    assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
}

#[test]
fn differing_method_is_not_a_near_miss() {
    let unprovided_consumes = vec![consume(
        "http",
        Some("POST /api/Articles"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /api/articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
}

#[test]
fn genuinely_unrelated_routes_do_not_fire() {
    let unprovided_consumes = vec![consume(
        "http",
        Some("GET /widgets/{}"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /api/articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
}

#[test]
fn non_http_dangling_consume_is_ignored() {
    let unprovided_consumes = vec![consume(
        "queue",
        Some("GET /api/Articles"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /api/articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
}

#[test]
fn priority_prefers_case_over_prefix() {
    // One provide matches by case, another by prefix — case wins (higher confidence).
    let unprovided_consumes = vec![consume(
        "http",
        Some("GET /api/Articles"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![
        // Prefix match: consume carries an extra `/api` the provide lacks.
        provide("GET /Articles", "be-nest", "prefix.ts", 5),
        // Case match: same segment count, differs only in casing.
        provide("GET /api/articles", "be-nest", "case.ts", 9),
    ];
    let out = route_near_miss_findings(&unprovided_consumes, &provides);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].data.as_ref().unwrap()["dimension"], "case");
    assert!(out[0].message.contains("case.ts:9"));
}

#[test]
fn determinism_stable_sorted_output() {
    let unprovided_consumes = vec![
        consume("http", Some("GET /api/Articles"), "fe-react", "B.tsx", 2),
        consume("http", Some("GET /api/Comments"), "fe-react", "A.tsx", 1),
    ];
    let provides = vec![
        provide("GET /api/articles", "be-nest", "articles.controller.ts", 22),
        provide("GET /api/comments", "be-nest", "comments.controller.ts", 8),
    ];
    let out = route_near_miss_findings(&unprovided_consumes, &provides);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].file, "A.tsx");
    assert_eq!(out[1].file, "B.tsx");

    // Re-running yields byte-identical output (candidate sort is deterministic too).
    let out2 = route_near_miss_findings(&unprovided_consumes, &provides);
    assert_eq!(
        out.iter().map(|f| &f.message).collect::<Vec<_>>(),
        out2.iter().map(|f| &f.message).collect::<Vec<_>>()
    );
}

#[test]
fn results_targets_map_records_the_chosen_provide_with_first_consume_ref() {
    let unprovided_consumes = vec![consume(
        "http",
        Some("GET /articles"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /api/articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    let result = route_near_miss_results(&unprovided_consumes, &provides);
    assert_eq!(result.findings.len(), 1);
    let target = result
        .targets
        .get(&(
            "be-nest".to_string(),
            "articles.controller.ts".to_string(),
            22,
        ))
        .expect("chosen provide site must be recorded in targets");
    assert_eq!(target.consume_file, "Api.tsx");
    assert_eq!(target.consume_line, 10);
    assert_eq!(target.count, 1);
}

#[test]
fn results_targets_map_counts_multiple_consumes_naming_the_same_provide_and_keeps_the_first() {
    // Two consumes both near-miss the same provide — count must be 2, and the recorded consume ref must
    // stay the FIRST one (input order), even though the second one has an earlier file/line lexically.
    let unprovided_consumes = vec![
        consume("http", Some("GET /articles"), "fe-react", "Z.tsx", 10),
        consume("http", Some("GET /articles"), "fe-react", "A.tsx", 1),
    ];
    let provides = vec![provide(
        "GET /api/articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    let result = route_near_miss_results(&unprovided_consumes, &provides);
    assert_eq!(result.findings.len(), 2);
    let target = result
        .targets
        .get(&(
            "be-nest".to_string(),
            "articles.controller.ts".to_string(),
            22,
        ))
        .expect("chosen provide site must be recorded in targets");
    assert_eq!(target.consume_file, "Z.tsx");
    assert_eq!(target.consume_line, 10);
    assert_eq!(target.count, 2);
}

#[test]
fn prefix_records_capture_the_prefix_dimension_details() {
    // The canonical `GET /articles` vs `GET /api/articles` case: the prefix-dimension result must be
    // captured as typed data too, not just embedded in the finding's message/data JSON.
    let unprovided_consumes = vec![consume(
        "http",
        Some("GET /articles"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let provides = vec![provide(
        "GET /api/articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    let result = route_near_miss_results(&unprovided_consumes, &provides);
    assert_eq!(result.prefix_records.len(), 1);
    let record = &result.prefix_records[0];
    assert_eq!(record.prefix, "/api");
    assert!(record.consume_missing_prefix);
    assert_eq!(record.consume_key, "GET /articles");
    assert_eq!(record.provide_key, "GET /api/articles");

    // Case-dimension inputs must produce EMPTY prefix_records — only the prefix dimension populates it.
    let case_consumes = vec![consume(
        "http",
        Some("GET /api/Articles"),
        "fe-react",
        "Api.tsx",
        10,
    )];
    let case_provides = vec![provide(
        "GET /api/articles",
        "be-nest",
        "articles.controller.ts",
        22,
    )];
    let case_result = route_near_miss_results(&case_consumes, &case_provides);
    assert!(case_result.prefix_records.is_empty());
}
