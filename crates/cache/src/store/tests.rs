use super::*;
use zzop_core::Severity;

/// A fresh, unique scratch directory under the OS temp dir — no `tempfile` crate dependency (this
/// crate's dependency budget is `zzop-core` + `serde` + `serde_json` only), so tests roll their own via
/// the same pid+counter+nanos uniqueness scheme as `temp_sibling`.
fn scratch_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "zzop-cache-test-{tag}-{}-{nanos}-{n}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn key(content: &str, parser: &str, ruleset: &str) -> CacheKey {
    scoped_key(content, parser, "a.ts", ruleset)
}

fn scoped_key(content: &str, parser: &str, scope: &str, ruleset: &str) -> CacheKey {
    CacheKey {
        content_hash: AnalysisCache::content_hash(content.as_bytes()),
        parser_fingerprint: parser.to_string(),
        scope: scope.to_string(),
        ruleset_fingerprint: ruleset.to_string(),
    }
}

fn sample_ir(loc: u32) -> FileIrSlice {
    FileIrSlice {
        symbols: vec![zzop_core::SourceSymbol {
            id: "a.ts#foo".to_string(),
            file: "a.ts".to_string(),
            name: "foo".to_string(),
            kind: zzop_core::SourceSymbolKind::Function,
            line: 1,
            exported: true,
            is_default: false,
            body_start: Some(1),
            body_end: Some(3),
            write_sites: Vec::new(),
        }],
        imports: Some(zzop_core::ImportMap::new()),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        asset_refs: Vec::new(),
        loc,
        degraded: false,
        io: None,
        used_names: Vec::new(),
        minified_or_generated: false,
        const_map_fragment: std::collections::HashMap::new(),
        procedure_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        wrapper_def_fragments: Vec::new(),
        wrapper_call_fragments: Vec::new(),
        controller_prefix_route_fragments: Vec::new(),
        class_shape_fragments: Vec::new(),
        query_call_sites: Vec::new(),
        field_usage_tokens: Vec::new(),
        loop_spans: Vec::new(),
    }
}

fn sample_findings() -> Vec<Finding> {
    vec![Finding {
        rule_id: "pack/rule".to_string(),
        severity: Severity::Warning,
        file: "a.ts".to_string(),
        line: 1,
        message: "example finding".to_string(),
        data: None,
    }]
}

// Compares two serde-serializable values structurally by round-tripping through `serde_json::Value` —
// sidesteps the fact that `SourceSymbol` (and therefore `FileIrSlice`) does not derive `PartialEq`.
fn json_eq<T: Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).unwrap() == serde_json::to_value(b).unwrap()
}

#[test]
fn roundtrip_ir_put_get() {
    let dir = scratch_dir("roundtrip-ir");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("content", "ts+swc1+logic1", "pack@1");
    let ir = sample_ir(10);

    assert!(cache.get_ir(&k).is_none());
    cache.put_ir(&k, &ir).unwrap();
    let got = cache.get_ir(&k).expect("expected IR hit after put");
    assert!(json_eq(&got, &ir));
}

#[test]
fn roundtrip_preserves_the_minified_flag() {
    // `FileIrSlice::minified_or_generated` (added alongside `zzop-cache-v6`) must survive a put/get round
    // trip exactly like every other field `roundtrip_ir_put_get` already covers in aggregate — this test
    // isolates just that one field so a future regression that quietly serializes/deserializes it wrong
    // (e.g. an errant `#[serde(skip)]`) fails here specifically, not just as an unexplained diff in the
    // broader `json_eq` comparison above.
    let dir = scratch_dir("roundtrip-minified-flag");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("content", "ts+swc1+logic1", "pack@1");
    let mut ir = sample_ir(10);
    ir.minified_or_generated = true;

    cache.put_ir(&k, &ir).unwrap();
    let got = cache.get_ir(&k).expect("expected IR hit after put");
    assert!(
        got.minified_or_generated,
        "the minified_or_generated flag must round-trip as true"
    );
}

#[test]
fn roundtrip_preserves_loop_spans() {
    // `FileIrSlice::loop_spans` (added alongside `zzop-cache-v18`) must survive a put/get round trip —
    // isolated the same way `roundtrip_preserves_the_minified_flag` isolates its own field, so a
    // regression that quietly drops/mis-serializes just this field fails here specifically.
    let dir = scratch_dir("roundtrip-loop-spans");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("content", "ts+swc1+logic1", "pack@1");
    let mut ir = sample_ir(10);
    ir.loop_spans = vec![(2, 4), (7, 7)];

    cache.put_ir(&k, &ir).unwrap();
    let got = cache.get_ir(&k).expect("expected IR hit after put");
    assert_eq!(
        got.loop_spans,
        vec![(2, 4), (7, 7)],
        "loop_spans must round-trip exactly"
    );
}

#[test]
fn roundtrip_findings_put_get() {
    let dir = scratch_dir("roundtrip-findings");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("content", "ts+swc1+logic1", "pack@1");
    let findings = sample_findings();

    assert!(cache.get_findings(&k).is_none());
    cache.put_findings(&k, &findings).unwrap();
    let got = cache
        .get_findings(&k)
        .expect("expected findings hit after put");
    assert!(json_eq(&got, &findings));
}

#[test]
fn miss_on_content_change() {
    let dir = scratch_dir("miss-content");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k1 = key("content-a", "parser1", "ruleset1");
    let k2 = key("content-b", "parser1", "ruleset1");
    cache.put_ir(&k1, &sample_ir(1)).unwrap();
    cache.put_findings(&k1, &sample_findings()).unwrap();

    assert!(cache.get_ir(&k2).is_none());
    assert!(cache.get_findings(&k2).is_none());
    // original key is unaffected
    assert!(cache.get_ir(&k1).is_some());
    assert!(cache.get_findings(&k1).is_some());
}

#[test]
fn miss_on_parser_fingerprint_change() {
    let dir = scratch_dir("miss-parser");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k1 = key("content", "parser-v1", "ruleset1");
    let k2 = key("content", "parser-v2", "ruleset1");
    cache.put_ir(&k1, &sample_ir(1)).unwrap();

    assert!(cache.get_ir(&k2).is_none());
    assert!(cache.get_ir(&k1).is_some());
}

#[test]
fn ir_preserved_when_only_ruleset_changes_but_findings_are_not() {
    let dir = scratch_dir("ruleset-split");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k1 = key("content", "parser1", "ruleset-a");
    let k2 = key("content", "parser1", "ruleset-b");
    let ir = sample_ir(42);
    cache.put_ir(&k1, &ir).unwrap();
    cache.put_findings(&k1, &sample_findings()).unwrap();

    // Same content + parser, different ruleset: IR is still a hit (ruleset-independent key)...
    let got_ir = cache
        .get_ir(&k2)
        .expect("IR must be reusable across ruleset change");
    assert!(json_eq(&got_ir, &ir));
    // ...but findings, keyed on the full triple, are a miss until re-run and stored under the new key.
    assert!(cache.get_findings(&k2).is_none());
}

#[test]
fn schema_version_mismatch_wipes_existing_entries() {
    let dir = scratch_dir("schema-wipe");
    let k = key("content", "parser1", "ruleset1");
    {
        let cache = AnalysisCache::open(&dir, "schema-v1").unwrap();
        cache.put_ir(&k, &sample_ir(1)).unwrap();
        cache.put_findings(&k, &sample_findings()).unwrap();
        assert!(cache.get_ir(&k).is_some());
    }
    // Reopening with a different schema version wipes prior entries.
    let cache = AnalysisCache::open(&dir, "schema-v2").unwrap();
    assert!(cache.get_ir(&k).is_none());
    assert!(cache.get_findings(&k).is_none());
}

#[test]
fn schema_version_match_preserves_entries_across_reopen() {
    let dir = scratch_dir("schema-keep");
    let k = key("content", "parser1", "ruleset1");
    {
        let cache = AnalysisCache::open(&dir, "schema-v1").unwrap();
        cache.put_ir(&k, &sample_ir(7)).unwrap();
    }
    let cache = AnalysisCache::open(&dir, "schema-v1").unwrap();
    assert!(cache.get_ir(&k).is_some());
}

#[test]
fn corrupted_ir_entry_is_treated_as_miss_not_panic() {
    let dir = scratch_dir("corrupt-ir");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("content", "parser1", "ruleset1");
    let path = cache.ir_path(&k);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{ not valid json at all").unwrap();

    assert!(cache.get_ir(&k).is_none());
}

#[test]
fn key_mismatch_inside_entry_is_treated_as_miss() {
    // Defends the "compare the stored key, not just the digest" guard documented in hash.rs: even if
    // an entry file exists at the expected path with valid JSON, a stored key that does not match the
    // requested key must not be returned.
    let dir = scratch_dir("key-mismatch");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("content", "parser1", "ruleset1");
    let path = cache.ir_path(&k);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let wrong_entry = IrEntry {
        format_version: FORMAT_VERSION,
        content_hash: "not-the-right-hash".to_string(),
        parser_fingerprint: "parser1".to_string(),
        scope: "a.ts".to_string(),
        ir: sample_ir(1),
    };
    fs::write(&path, serde_json::to_vec(&wrong_entry).unwrap()).unwrap();

    assert!(cache.get_ir(&k).is_none());
}

#[test]
fn miss_on_scope_change() {
    // The bug this field exists to close: two DIFFERENT files (different `scope`) with byte-identical
    // content, the same parser fingerprint, and the same ruleset must NOT alias each other's cache
    // entry — see `CacheKey::scope`'s doc for why a file's cached IR/findings embed its own path.
    let dir = scratch_dir("miss-scope");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k1 = scoped_key("same-content", "parser1", "routes/a.ts", "ruleset1");
    let k2 = scoped_key("same-content", "parser1", "other/a.ts", "ruleset1");
    cache.put_ir(&k1, &sample_ir(1)).unwrap();
    cache.put_findings(&k1, &sample_findings()).unwrap();

    assert!(cache.get_ir(&k2).is_none());
    assert!(cache.get_findings(&k2).is_none());
    // original key is unaffected
    assert!(cache.get_ir(&k1).is_some());
    assert!(cache.get_findings(&k1).is_some());
}

#[test]
fn ir_is_scope_sensitive_not_just_findings() {
    // Unlike `ruleset_fingerprint` (findings-only), `scope` gates the IR lookup too: a `FileIrSlice`'s
    // `symbols`/`io` carry their own originating path, so IR is not purely a function of
    // (content, parser) the way it is for `ruleset_fingerprint`.
    let dir = scratch_dir("scope-gates-ir");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k1 = scoped_key("content", "parser1", "a.ts", "ruleset1");
    let k2 = scoped_key("content", "parser1", "b.ts", "ruleset1");
    cache.put_ir(&k1, &sample_ir(5)).unwrap();

    assert!(
        cache.get_ir(&k2).is_none(),
        "same content+parser but different scope must miss on IR too"
    );
}

#[test]
fn truncated_empty_file_is_treated_as_miss() {
    let dir = scratch_dir("truncated");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("content", "parser1", "ruleset1");
    let path = cache.findings_path(&k);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"").unwrap();

    assert!(cache.get_findings(&k).is_none());
}

#[test]
fn concurrent_puts_from_multiple_threads_all_land() {
    let dir = scratch_dir("concurrent");
    let cache = std::sync::Arc::new(AnalysisCache::open(&dir, "v1").unwrap());
    let mut handles = Vec::new();
    for i in 0..16u32 {
        let cache = cache.clone();
        handles.push(std::thread::spawn(move || {
            let k = key(&format!("content-{i}"), "parser1", "ruleset1");
            cache.put_ir(&k, &sample_ir(i)).unwrap();
            cache.put_findings(&k, &sample_findings()).unwrap();
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    for i in 0..16u32 {
        let k = key(&format!("content-{i}"), "parser1", "ruleset1");
        let ir = cache
            .get_ir(&k)
            .unwrap_or_else(|| panic!("missing ir for {i}"));
        assert_eq!(ir.loc, i);
        assert!(cache.get_findings(&k).is_some());
    }
}

#[test]
fn concurrent_puts_to_the_same_key_are_harmless() {
    // Every writer for the same key produces byte-identical output, so racing on the exact same
    // target path (the Windows rename caveat documented in the module doc) must never error.
    let dir = scratch_dir("concurrent-same-key");
    let cache = std::sync::Arc::new(AnalysisCache::open(&dir, "v1").unwrap());
    let k = key("same-content", "parser1", "ruleset1");
    let mut handles = Vec::new();
    for _ in 0..16 {
        let cache = cache.clone();
        let k = k.clone();
        handles.push(std::thread::spawn(move || {
            cache.put_ir(&k, &sample_ir(99)).unwrap();
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let ir = cache
        .get_ir(&k)
        .expect("entry must exist after concurrent puts");
    assert_eq!(ir.loc, 99);
}
