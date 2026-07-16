use super::*;
use zzop_core::RuleConfig;

fn pack(id: &str) -> RulePackDef {
    let json = format!(r#"{{"id": "{id}", "framework": "any", "rules": []}}"#);
    serde_json::from_str(&json).unwrap()
}

#[test]
fn ruleset_fingerprint_is_order_independent_over_pack_list() {
    let a = pack("a");
    let b = pack("b");
    let config = EngineConfig::default();
    let fp1 = ruleset_fingerprint(&[&a, &b], &config);
    let fp2 = ruleset_fingerprint(&[&b, &a], &config);
    assert_eq!(fp1, fp2);
}

#[test]
fn ruleset_fingerprint_changes_when_pack_set_changes() {
    let a = pack("a");
    let b = pack("b");
    let config = EngineConfig::default();
    let fp_a = ruleset_fingerprint(&[&a], &config);
    let fp_ab = ruleset_fingerprint(&[&a, &b], &config);
    assert_ne!(fp_a, fp_ab);
}

#[test]
fn ruleset_fingerprint_changes_when_disabled_rules_changes() {
    let a = pack("a");
    let mut config = EngineConfig::default();
    let fp_before = ruleset_fingerprint(&[&a], &config);
    config.rule_config = RuleConfig {
        disabled_rules: vec!["something".to_string()],
        ..RuleConfig::default()
    };
    let fp_after = ruleset_fingerprint(&[&a], &config);
    assert_ne!(fp_before, fp_after);
}

#[test]
fn parser_fingerprint_differs_by_language() {
    let config = EngineConfig::default();
    let ts = parser_fingerprint(Some(Language::TypeScript), &config);
    let prisma = parser_fingerprint(Some(Language::Prisma), &config);
    let java = parser_fingerprint(Some(Language::Java21), &config);
    let python = parser_fingerprint(Some(Language::Python), &config);
    let rust = parser_fingerprint(Some(Language::Rust), &config);
    let go = parser_fingerprint(Some(Language::Go), &config);
    let none = parser_fingerprint(None, &config);
    assert_ne!(ts, prisma);
    assert_ne!(ts, none);
    assert_ne!(prisma, none);
    assert_ne!(java, ts);
    assert_ne!(java, prisma);
    assert_ne!(java, none);
    assert_ne!(python, ts);
    assert_ne!(python, prisma);
    assert_ne!(python, java);
    assert_ne!(python, none);
    assert_ne!(rust, ts);
    assert_ne!(rust, prisma);
    assert_ne!(rust, java);
    assert_ne!(rust, python);
    assert_ne!(rust, none);
    assert_ne!(go, ts);
    assert_ne!(go, prisma);
    assert_ne!(go, java);
    assert_ne!(go, python);
    assert_ne!(go, rust);
    assert_ne!(go, none);
}

#[test]
fn parser_fingerprint_changes_with_size_cap() {
    let mut config = EngineConfig::default();
    let fp1 = parser_fingerprint(Some(Language::TypeScript), &config);
    config.size_cap += 1;
    let fp2 = parser_fingerprint(Some(Language::TypeScript), &config);
    assert_ne!(fp1, fp2);
}

#[test]
fn parser_fingerprint_changes_with_io_router_names_for_typescript_only() {
    let mut config = EngineConfig::default();
    let ts_before = parser_fingerprint(Some(Language::TypeScript), &config);
    let prisma_before = parser_fingerprint(Some(Language::Prisma), &config);
    let java_before = parser_fingerprint(Some(Language::Java21), &config);
    let python_before = parser_fingerprint(Some(Language::Python), &config);
    let rust_before = parser_fingerprint(Some(Language::Rust), &config);
    let go_before = parser_fingerprint(Some(Language::Go), &config);
    let none_before = parser_fingerprint(None, &config);

    config.io.router_names = vec!["customRouter".to_string()];

    let ts_after = parser_fingerprint(Some(Language::TypeScript), &config);
    assert_ne!(
        ts_before, ts_after,
        "an io.router_names change must invalidate cached TypeScript entries"
    );
    // Scoped to the TypeScript branch only — Prisma/Java/Python/Rust/Go/lexical-fallback fingerprints
    // never consult `config.io`, so they must be unaffected by an `io` change (no needless invalidation).
    assert_eq!(
        prisma_before,
        parser_fingerprint(Some(Language::Prisma), &config)
    );
    assert_eq!(
        java_before,
        parser_fingerprint(Some(Language::Java21), &config)
    );
    assert_eq!(
        python_before,
        parser_fingerprint(Some(Language::Python), &config)
    );
    assert_eq!(
        rust_before,
        parser_fingerprint(Some(Language::Rust), &config)
    );
    assert_eq!(go_before, parser_fingerprint(Some(Language::Go), &config));
    assert_eq!(none_before, parser_fingerprint(None, &config));
}

#[test]
fn cache_scope_differs_by_rel_for_the_same_source_id() {
    // Two different files with identical content/parser/ruleset must not collide on a cache entry.
    let config = EngineConfig::default();
    let a = cache_scope(&config, "routes/a.ts");
    let b = cache_scope(&config, "other/a.ts");
    assert_ne!(a, b);
}

#[test]
fn cache_scope_differs_by_source_id_for_the_same_rel() {
    // The multi-tree-sharing-one-cache_dir case: two trees with the same rel path must not collide
    // either.
    let fe_config = EngineConfig {
        source_id: "fe".to_string(),
        ..EngineConfig::default()
    };
    let be_config = EngineConfig {
        source_id: "be".to_string(),
        ..EngineConfig::default()
    };
    let fe = cache_scope(&fe_config, "src/types.ts");
    let be = cache_scope(&be_config, "src/types.ts");
    assert_ne!(fe, be);
}

#[test]
fn cache_scope_does_not_let_source_id_and_rel_bleed_into_each_other() {
    // NUL-separator regression guard: `source_id = "ab"` + `rel = "c"` must differ from
    // `source_id = "a"` + `rel = "bc"` even though naive concatenation would collide.
    let left_config = EngineConfig {
        source_id: "ab".to_string(),
        ..EngineConfig::default()
    };
    let right_config = EngineConfig {
        source_id: "a".to_string(),
        ..EngineConfig::default()
    };
    let left = cache_scope(&left_config, "c");
    let right = cache_scope(&right_config, "bc");
    assert_ne!(left, right);
}
