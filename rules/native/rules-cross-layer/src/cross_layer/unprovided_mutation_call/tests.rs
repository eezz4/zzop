use super::*;
use zzop_core::io::IoConsume;

fn consume(kind: &str, key: Option<&str>, source: &str, file: &str, line: u32) -> TaggedConsume {
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
            retry_configured: None,
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
    // Paste-ready injection stub (full key, provide role) + the SERVING-tree caveat.
    assert_eq!(
        data["injectionStub"],
        "routes: [{ \"key\": \"POST /api/orders\", \"role\": \"provide\" }]"
    );
    assert!(out[0].message.contains("SERVING tree's `routes`"));
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
