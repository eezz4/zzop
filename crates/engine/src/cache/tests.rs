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
    let sql = parser_fingerprint(Some(Language::Sql), &config);
    let csharp = parser_fingerprint(Some(Language::CSharp), &config);
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
    assert_ne!(sql, ts);
    assert_ne!(sql, prisma);
    assert_ne!(sql, java);
    assert_ne!(sql, python);
    assert_ne!(sql, rust);
    assert_ne!(sql, go);
    assert_ne!(sql, none);
    assert_ne!(csharp, ts);
    assert_ne!(csharp, prisma);
    assert_ne!(csharp, java);
    assert_ne!(csharp, python);
    assert_ne!(csharp, rust);
    assert_ne!(csharp, go);
    assert_ne!(csharp, sql);
    assert_ne!(csharp, none);
}

#[test]
fn every_parser_fingerprint_arm_carries_the_engine_source_hash() {
    // The gap this closes: `crates/engine` produces cached bytes (`pipeline/io_projection.rs` fills
    // `FileIrSlice.io`, `pipeline/fresh.rs` decides what a slice contains, `pipeline/findings.rs`
    // authors the stored message) but is structurally absent from every `FP_*` closure, because it is
    // the crate that DEPENDS on them. An engine-only fix used to leave every warm cache serving the old
    // answer. The suffix is applied once after the match so an arm cannot skip it — this asserts the
    // suffix stays SHARED if anyone ever moves it back into the arms.
    let config = EngineConfig::default();
    for language in [
        Some(Language::TypeScript),
        Some(Language::Prisma),
        Some(Language::Java21),
        Some(Language::Python),
        Some(Language::Rust),
        Some(Language::Go),
        Some(Language::Sql),
        Some(Language::CSharp),
        None,
    ] {
        let fp = parser_fingerprint(language, &config);
        assert!(
            fp.contains(&format!("+engine={FP_ENGINE}")),
            "{language:?} arm is missing the engine source hash: {fp}"
        );
    }
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
    let sql_before = parser_fingerprint(Some(Language::Sql), &config);
    let csharp_before = parser_fingerprint(Some(Language::CSharp), &config);
    let none_before = parser_fingerprint(None, &config);

    config.io.router_names = vec!["customRouter".to_string()];

    let ts_after = parser_fingerprint(Some(Language::TypeScript), &config);
    assert_ne!(
        ts_before, ts_after,
        "an io.router_names change must invalidate cached TypeScript entries"
    );
    // Scoped to the TypeScript branch only — Prisma/Java/Python/Rust/Go/Sql/CSharp/lexical-fallback
    // fingerprints never consult `config.io`, so they must be unaffected by an `io` change (no needless
    // invalidation).
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
    assert_eq!(sql_before, parser_fingerprint(Some(Language::Sql), &config));
    assert_eq!(
        csharp_before,
        parser_fingerprint(Some(Language::CSharp), &config)
    );
    assert_eq!(none_before, parser_fingerprint(None, &config));
}

/// The invalidation contract of the surfaced stamp, pinned at the composition seam: change one
/// character of the hash input (what a one-character parser-source edit does to `FP_*` via
/// `build.rs`) and the surfaced value moves; leave it alone and two runs are byte-identical. The
/// constants themselves cannot be perturbed inside a test — they are compile-time — so the pin holds
/// the seam every constant flows through.
#[test]
fn surfaced_stamp_moves_with_the_source_hash_and_is_byte_stable_without_it() {
    assert_eq!(
        derived("rust/syn-2", "aaaaaaaaaaaaaaaa"),
        derived("rust/syn-2", "aaaaaaaaaaaaaaaa"),
        "same id + same source hash must stamp byte-identically"
    );
    assert_ne!(
        derived("rust/syn-2", "aaaaaaaaaaaaaaaa"),
        derived("rust/syn-2", "aaaaaaaaaaaaaaab"),
        "a moved source hash must move the surfaced stamp"
    );
    // Two calls of the whole surface in one build: byte-identical, so nothing nondeterministic
    // (map iteration, time, randomness) can ever leak into the stamp.
    assert_eq!(
        surface::parser_fingerprints(),
        surface::parser_fingerprints()
    );
}

/// The surfaced list and the cache key describe ONE set of parsers: every per-language arm of
/// [`parser_fingerprint`] starts with exactly one surfaced value (its config-independent base), and
/// every surfaced entry backs some arm. A ninth parser wired into dispatch fails here until it is
/// surfaced; a surfaced entry no arm produces fails here too (a stamp for a parser that cannot run).
#[test]
fn parser_fingerprints_surface_matches_every_dispatch_arm() {
    let surfaced = surface::parser_fingerprints();
    // Sorted + unique by crate name — the determinism the doc promises is checkable, not asserted.
    let names: Vec<&str> = surfaced.iter().map(|(n, _)| *n).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted, "entries must be sorted and unique by name");

    let config = EngineConfig::default();
    let mut matched: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for language in [
        Language::TypeScript,
        Language::Prisma,
        Language::Java21,
        Language::Python,
        Language::Rust,
        Language::Go,
        Language::Sql,
        Language::CSharp,
    ] {
        let fp = parser_fingerprint(Some(language), &config);
        let (name, _) = surfaced
            .iter()
            .find(|(_, v)| fp.starts_with(&format!("{v}+")))
            .unwrap_or_else(|| {
                panic!("no surfaced fingerprint is the base of {language:?}'s cache-key arm: {fp}")
            });
        matched.insert(name);
    }
    assert_eq!(
        matched.len(),
        surfaced.len(),
        "every surfaced entry must back a dispatch arm (unbacked: {:?})",
        surfaced
            .iter()
            .map(|(n, _)| *n)
            .filter(|n| !matched.contains(n))
            .collect::<Vec<_>>()
    );
    // The engine suffix on every arm and the surfaced engine fingerprint are the same value — the
    // stamp cannot claim an engine the cache key does not use.
    assert_eq!(surface::engine_fingerprint(), FP_ENGINE);
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
