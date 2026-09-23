use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use zzop_core::Severity;

/// A fresh, unique scratch directory under the OS temp dir — no `tempfile` crate dependency (this
/// crate's dependency budget is `zzop-core` + `serde` + `serde_json` only), so tests roll their own via
/// the same pid+counter+nanos uniqueness scheme as `atomic::temp_sibling`.
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
        vocabulary_fingerprint: "vocab1".to_string(),
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
        exported_signature_names: Vec::new(),
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
        function_spans: Vec::new(),
        test_spans: Vec::new(),
        call_sites: Vec::new(),
        string_literals: Vec::new(),
        call_graph: Default::default(),
        export_aliases: Vec::new(),
        has_generated_banner: false,
    }
}

fn sample_findings() -> Vec<Finding> {
    vec![Finding {
        rule_id: "pack/rule".to_string(),
        severity: Severity::Warning,
        file: "a.ts".to_string(),
        line: 1,
        message: "example finding".to_string(),
        evidence_paths: Vec::new(),
        data: None,
    }]
}

// Compares two serde-serializable values structurally by round-tripping through `serde_json::Value` —
// sidesteps the fact that `SourceSymbol` (and therefore `FileIrSlice`) does not derive `PartialEq`.
fn json_eq<T: Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).unwrap() == serde_json::to_value(b).unwrap()
}

/// `open` must actually RUN the eviction pass — the one thing `evict`'s own tests cannot show, because
/// they call `evict_to_cap` directly. Deleting the call site left this whole suite green until this test
/// existed (measured 2026-08-05), so the assertion is deliberately about the WIRING and not the policy:
/// entries written under a budget of zero must not survive the next open.
#[test]
fn open_runs_the_eviction_pass_and_not_only_the_schema_wipe() {
    let dir = scratch_dir("evict-wiring");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    for i in 0..8 {
        cache
            .put_ir(
                &scoped_key("content", "p1", &format!("f{i}.ts"), "pack@1"),
                &sample_ir(i),
            )
            .unwrap();
    }
    let count = |d: &str| fs::read_dir(dir.join(d)).unwrap().count();
    assert_eq!(
        count(IR_DIR),
        8,
        "setup: the entries must be on disk to be evictable"
    );

    // A cap of 0 makes every entry over budget. The SAME schema version, so nothing here is the wipe —
    // if this passed with the eviction call removed, the wipe would be doing the work and the test lying.
    AnalysisCache::open_with_cap(&dir, "v1", 0).unwrap();
    assert_eq!(
        count(IR_DIR),
        0,
        "open did not evict — the housekeeping step is unwired, not merely idle"
    );
}

/// Eviction must not be SILENT: the count `evict_to_cap` returns has to reach the handle, because the
/// engine's only way to tell a user why the next run got slower is to read it back off the cache.
///
/// Driven through the real path (`open_with_cap` -> `evict::evict_to_cap`) rather than asserting on a
/// hand-set field: the defect being sealed is exactly that the call site DISCARDED the return value, so
/// a test that constructs the count itself would skip the line that produces the defect. The budget is
/// injected because the real one is ~256 MiB (~137,000 entries), which no test can reach honestly.
#[test]
fn open_reports_how_many_entries_its_eviction_pass_deleted() {
    let dir = scratch_dir("evict-count");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    assert_eq!(
        cache.evicted_entries(),
        0,
        "setup: a fresh under-budget cache evicts nothing"
    );
    for i in 0..5 {
        cache
            .put_ir(
                &scoped_key("content", "p1", &format!("f{i}.ts"), "pack@1"),
                &sample_ir(i),
            )
            .unwrap();
    }

    // Same schema version, so the wipe cannot be what deletes these; a budget of 0 makes every entry
    // over cap. Five entries in, five entries must be reported out.
    let reopened = AnalysisCache::open_with_cap(&dir, "v1", 0).unwrap();
    assert_eq!(
        reopened.evicted_entries(),
        5,
        "the evicted count never left `evict_to_cap` — the open path throws it away"
    );
}

/// The disclosure itself. Zero must produce NOTHING (a line on every run trains readers to ignore the
/// channel), and a real eviction must produce a line that says what happened, that it is housekeeping
/// rather than a failure, and what it costs the next run.
#[test]
fn the_eviction_warning_is_emitted_only_when_something_was_actually_evicted() {
    let dir = scratch_dir("evict-warning");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    assert_eq!(
        cache.eviction_warning(),
        None,
        "an ordinary open must disclose nothing — 0 evicted is not news"
    );
    cache
        .put_ir(
            &scoped_key("content", "p1", "a.ts", "pack@1"),
            &sample_ir(1),
        )
        .unwrap();

    let warning = AnalysisCache::open_with_cap(&dir, "v1", 0)
        .unwrap()
        .eviction_warning()
        .expect("an open that evicted must disclose it");
    assert!(
        warning.contains("1 entry evicted"),
        "the warning must state how many entries went: {warning}"
    );
    assert!(
        warning.contains("Not an error"),
        "the warning must say this is cap enforcement, not a failure: {warning}"
    );
    assert!(
        warning.contains("recomputes"),
        "the warning must state the re-analysis cost the next run pays: {warning}"
    );
}

/// The other half: a normal open must NOT evict. Without this, the test above would still pass if `open`
/// deleted entries unconditionally, which is the failure that would actually hurt a user.
#[test]
fn a_normal_open_evicts_nothing_because_the_cache_is_under_budget() {
    let dir = scratch_dir("evict-no-op");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = scoped_key("content", "p1", "a.ts", "pack@1");
    cache.put_ir(&k, &sample_ir(1)).unwrap();

    let reopened = AnalysisCache::open(&dir, "v1").unwrap();
    assert!(
        reopened.get_ir(&k).is_some(),
        "an under-budget entry was evicted by an ordinary open"
    );
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
fn roundtrip_preserves_function_spans() {
    // `FileIrSlice::function_spans` — same isolation as `roundtrip_preserves_loop_spans` above, and it
    // matters more here: the absent-fact degrade for `MethodScan::after_in_same_function` is "remove no
    // pairings", so a dropped field on a warm run does not go silent, it silently RESTORES the false
    // positives the gate exists to remove.
    let dir = scratch_dir("roundtrip-function-spans");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("content", "ts+swc1+logic1", "pack@1");
    let mut ir = sample_ir(10);
    ir.function_spans = vec![(1, 9), (3, 5)];

    cache.put_ir(&k, &ir).unwrap();
    let got = cache.get_ir(&k).expect("expected IR hit after put");
    assert_eq!(
        got.function_spans,
        vec![(1, 9), (3, 5)],
        "function_spans must round-trip exactly"
    );
}

#[test]
fn roundtrip_preserves_call_sites() {
    // `FileIrSlice::call_sites` — same isolation as the two span fields above. Its degrade direction is
    // `loop_spans`', i.e. SILENCE, so a warm run that dropped this field would report every `CallScan`
    // rule as having found nothing, which in the output is indistinguishable from a clean tree. The
    // vector's ORDER is asserted too: the producer's source order is this channel's determinism contract,
    // and a serializer that reordered would move every finding's line without changing any count.
    let dir = scratch_dir("roundtrip-call-sites");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("content", "ts+swc1+logic1", "pack@1");
    let mut ir = sample_ir(10);
    ir.call_sites = vec![
        zzop_core::CallSite {
            kind: zzop_core::CALL_KIND_CONSOLE_WRITE.to_string(),
            line: 4,
            callee: "console.error".to_string(),
            algorithm: None,
        },
        zzop_core::CallSite {
            kind: zzop_core::CALL_KIND_ENV_READ.to_string(),
            line: 2,
            callee: "process.env.HOME".to_string(),
            algorithm: None,
        },
        // W4's `algorithm`, in BOTH of its states, because they serialize differently: `Some` is a
        // key on the wire and `None` is the key's ABSENCE (`skip_serializing_if`). A round trip that
        // silently turned the second into `Some("")` would make an `algorithm_pattern` rule match a
        // site whose algorithm the source never spelled — the never-guess contract broken by the
        // cache rather than by a producer, which no producer test could catch.
        zzop_core::CallSite {
            kind: zzop_core::CALL_KIND_HASH_CALL.to_string(),
            line: 7,
            callee: "createHash".to_string(),
            algorithm: Some("md5".to_string()),
        },
        zzop_core::CallSite {
            kind: zzop_core::CALL_KIND_HASH_CALL.to_string(),
            line: 9,
            callee: "createHash".to_string(),
            algorithm: None,
        },
    ];
    let expected = ir.call_sites.clone();

    cache.put_ir(&k, &ir).unwrap();
    let got = cache.get_ir(&k).expect("expected IR hit after put");
    assert_eq!(
        got.call_sites, expected,
        "call_sites must round-trip exactly, in order"
    );
    assert_eq!(
        got.call_sites[3].algorithm, None,
        "an absent `algorithm` must read back as None, never as Some(\"\") — the never-guess \
         contract has to survive the cache, not only the producer"
    );
}

#[test]
fn a_cached_call_site_written_before_the_algorithm_field_existed_still_loads() {
    // The additive-field promise, exercised against the WIRE rather than against a struct: a warm
    // cache written by a pre-W4 build has no `algorithm` key at all, and `#[serde(default)]` is what
    // keeps that entry loadable instead of silently unreadable (which would degrade to a cold re-parse
    // — recoverable, but a bump this field deliberately does not need).
    let json = r#"{"kind":"console-write","line":4,"callee":"console.error"}"#;
    let site: zzop_core::CallSite = serde_json::from_str(json).expect("pre-W4 shape must load");
    assert_eq!(site.algorithm, None);
}

#[test]
fn roundtrip_preserves_string_literals_bit_exactly() {
    // `FileIrSlice::string_literals` — same isolation and same SILENCE degrade as `call_sites` above.
    // Two extra assertions specific to this channel: the f32 `entropy` must round-trip BIT-exactly
    // (its producer quantizes to 1/8 bit precisely so serialization cannot move it — a cache that
    // nudged it could flip an `entropy_min` threshold between cold and warm runs), and the stored
    // payload must carry the value HASH, never a value field (the no-plaintext contract is about
    // exactly this file-on-disk).
    let dir = scratch_dir("roundtrip-string-literals");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("content", "ts+swc1+logic1", "pack@1");
    let mut ir = sample_ir(10);
    ir.string_literals = vec![
        zzop_core::BoundStringLiteral {
            name: "apiKey".to_string(),
            line: 3,
            value_hash: zzop_core::value_hash_hex("correct-horse-battery-staple"),
            entropy: zzop_core::shannon_entropy_bits("correct-horse-battery-staple"),
        },
        zzop_core::BoundStringLiteral {
            name: "kind".to_string(),
            line: 1,
            value_hash: zzop_core::value_hash_hex("refresh_token"),
            entropy: zzop_core::shannon_entropy_bits("refresh_token"),
        },
    ];
    let expected = ir.string_literals.clone();

    cache.put_ir(&k, &ir).unwrap();
    let got = cache.get_ir(&k).expect("expected IR hit after put");
    assert_eq!(
        got.string_literals, expected,
        "string_literals must round-trip exactly, in order"
    );
    assert_eq!(
        got.string_literals[0].entropy.to_bits(),
        expected[0].entropy.to_bits(),
        "entropy must round-trip bit-exactly, not merely approximately"
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
    let path = cache.ir_path(&IrKey::from(&k));
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
    let path = cache.ir_path(&IrKey::from(&k));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let wrong_entry = IrEntry {
        format_version: FORMAT_VERSION,
        // Irrelevant to this test: the key comparison rejects the entry before the digest is
        // consulted. Left obviously wrong rather than computed, so a reader does not mistake this for
        // an integrity fixture — `a_hand_edited_ir_payload_is_refused` is that.
        payload_digest: String::new(),
        key: IrKey {
            content_hash: "not-the-right-hash".to_string(),
            parser_fingerprint: "parser1".to_string(),
            scope: "a.ts".to_string(),
            vocabulary_fingerprint: "vocab1".to_string(),
        },
        ir: sample_ir(1),
    };
    fs::write(&path, serde_json::to_vec(&wrong_entry).unwrap()).unwrap();

    assert!(cache.get_ir(&k).is_none());
}

/// The stored entry's on-disk JSON is FLAT — the key's fields sit beside `format_version`/`ir`, not
/// nested under a `key` object. `IrEntry`/`FindingsEntry` hold a key VALUE (so the read-path comparison
/// is a `PartialEq` that no new field can escape) and reach that layout via `#[serde(flatten)]`; this
/// pins that the refactor was shape-preserving, i.e. that entries written before it still load and that
/// no `CACHE_SCHEMA_VERSION` bump was owed.
#[test]
fn stored_entries_keep_the_flat_on_disk_key_layout() {
    let dir = scratch_dir("flat-layout");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("content", "parser1", "ruleset1");
    cache.put_ir(&k, &sample_ir(3)).unwrap();
    cache.put_findings(&k, &sample_findings()).unwrap();

    for (path, extra) in [
        (cache.ir_path(&IrKey::from(&k)), None),
        (cache.findings_path(&k), Some("ruleset_fingerprint")),
    ] {
        let raw: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).expect("entry must be valid JSON");
        let obj = raw.as_object().expect("entry must be a JSON object");
        assert!(obj.get("key").is_none(), "the key must not be nested");
        for field in [
            "format_version",
            "content_hash",
            "parser_fingerprint",
            "scope",
        ] {
            assert!(obj.contains_key(field), "{field} must sit at the top level");
        }
        assert_eq!(obj.contains_key("ruleset_fingerprint"), extra.is_some());
    }
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

/// The wipe rule as ONE property instead of sampled cases: for any stored version A and any opening
/// version B, entries survive **iff A == B**. Direction and distance are both irrelevant, and stating
/// it as a table is what makes that irrelevance testable — two named cases (`v1 -> v2`, `v1 -> v1`)
/// could not distinguish equality from ordering, which is exactly the substitution this pins against.
///
/// A `stored < mine` "improvement" fails the cells below the diagonal: it would let an OLDER binary
/// serve entries written by a NEWER one, which is a stale HIT rather than a miss — the one outcome
/// this cache must never produce. Adding a version to the list widens coverage for free.
#[test]
fn entries_survive_a_reopen_exactly_when_the_schema_version_is_unchanged() {
    // Shaped like the real value: a hash, with one legacy `{release}+{hash}` spelling so the
    // transition off that format is covered too.
    const VERSIONS: &[&str] = &[
        "42f0dc253ae9e705",
        "aa11bb22cc33dd44",
        "0.29.1+42f0dc253ae9e705",
    ];
    let k = key("content", "parser1", "ruleset1");

    for stored in VERSIONS {
        for opening in VERSIONS {
            let dir = scratch_dir("schema-matrix");
            {
                let cache = AnalysisCache::open(&dir, stored).unwrap();
                cache.put_ir(&k, &sample_ir(1)).unwrap();
                cache.put_findings(&k, &sample_findings()).unwrap();
            }
            let cache = AnalysisCache::open(&dir, opening).unwrap();
            let survived = cache.get_ir(&k).is_some();
            assert_eq!(
                survived,
                stored == opening,
                "stored={stored} opening={opening}: survival must track EQUALITY, not order"
            );
            assert_eq!(
                cache.get_findings(&k).is_some(),
                stored == opening,
                "stored={stored} opening={opening}: findings must follow ir"
            );
        }
    }
}

// --- Entry integrity: a well-formed hand-edit is the one staleness trick that got through ---

/// The measured attack, exactly. Seven other tricks (identical mtime + identical byte length, a
/// byte-identical file's cross-file invalidation, truncated / zero-byte / garbage entries, a changed
/// config fingerprint, four concurrent runs) all recomputed correctly. This one did not: rewrite
/// `findings` to `[]`, leave every fingerprint alone, and the real finding vanished from three
/// consecutive runs with no warning. The key describes the INPUTS; nothing described the OUTPUT.
#[test]
fn a_hand_edited_findings_payload_is_refused_and_counted() {
    let dir = scratch_dir("integrity-findings");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("source", "p1", "r1");
    cache.put_findings(&k, &sample_findings()).unwrap();
    assert_eq!(cache.get_findings(&k).unwrap().len(), 1, "baseline hit");

    // Blank the findings the way a hand-edit would, leaving every key field intact.
    let path = cache.findings_path(&k);
    let mut entry: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    entry["findings"] = serde_json::json!([]);
    fs::write(&path, serde_json::to_vec(&entry).unwrap()).unwrap();

    assert!(
        cache.get_findings(&k).is_none(),
        "a payload that does not match its own digest must be a MISS, so the file is re-analyzed \
         rather than served an emptied findings list"
    );
    assert_eq!(cache.rejected_entries(), 1);
    let w = cache
        .integrity_warning()
        .expect("the refusal must be disclosed");
    assert!(w.contains("1 cache entry"), "{w}");
    // The claim must not overstate itself: the digest is keyless, so this detects edits that did not
    // know about it, not a determined forger.
    assert!(w.contains("detector, not an authenticity check"), "{w}");
}

/// The IR half of the same axis. An IR payload is not where a finding is deleted, but it is where a
/// symbol, an import edge or an io fact can be — and every rule downstream reads those.
#[test]
fn a_hand_edited_ir_payload_is_refused() {
    let dir = scratch_dir("integrity-ir");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("source", "p1", "r1");
    cache.put_ir(&k, &sample_ir(10)).unwrap();
    assert!(cache.get_ir(&k).is_some(), "baseline hit");

    let path = cache.ir_path(&IrKey::from(&k));
    let mut entry: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    entry["ir"]["symbols"] = serde_json::json!([]);
    fs::write(&path, serde_json::to_vec(&entry).unwrap()).unwrap();

    assert!(cache.get_ir(&k).is_none());
    assert_eq!(cache.rejected_entries(), 1);
}

/// The invalidation that keeps this from being "reject everything": an untouched entry must still be
/// served, and must cost no warning. Without it, a digest that never matched would pass the two tests
/// above while making the cache useless.
#[test]
fn an_untouched_entry_still_hits_and_reports_no_integrity_problem() {
    let dir = scratch_dir("integrity-clean");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("source", "p1", "r1");
    cache.put_ir(&k, &sample_ir(10)).unwrap();
    cache.put_findings(&k, &sample_findings()).unwrap();

    assert!(cache.get_ir(&k).is_some());
    assert_eq!(cache.get_findings(&k).unwrap().len(), 1);
    assert_eq!(cache.rejected_entries(), 0);
    assert!(
        cache.integrity_warning().is_none(),
        "a healthy run must be silent here, or a reader learns to skip the channel"
    );
}

/// A valid entry COPIED onto another key's file must not verify. This is why the digest covers the key
/// as well as the payload: both entries are internally consistent, and serving the copy would answer
/// one file's question with another file's findings.
#[test]
fn an_entry_copied_onto_another_keys_file_is_refused() {
    let dir = scratch_dir("integrity-swap");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let a = key("source-a", "p1", "r1");
    let b = key("source-b", "p1", "r1");
    cache.put_findings(&a, &sample_findings()).unwrap();
    cache.put_findings(&b, &Vec::new()).unwrap();

    // Take a's entry, retarget its stored key to b's, and drop it on b's path — the key comparison
    // passes, since the key inside the file now says b.
    let mut entry: serde_json::Value =
        serde_json::from_slice(&fs::read(cache.findings_path(&a)).unwrap()).unwrap();
    entry["content_hash"] = serde_json::json!(b.content_hash);
    fs::write(cache.findings_path(&b), serde_json::to_vec(&entry).unwrap()).unwrap();

    assert!(
        cache.get_findings(&b).is_none(),
        "a's payload under b's key must not be served as b's answer"
    );
    assert_eq!(cache.rejected_entries(), 1);
}

// --- The detector must fire at TAMPERING, never at zzop's own writes ---
//
// `FileIrSlice::const_map_fragment` is a `std::collections::HashMap`, whose serde_json object-key order
// follows a per-INSTANCE randomized seed. The digest is computed from a serialization of the payload on
// both sides, so unless that serialization is canonicalized, WRITING hashes one key order and READING
// hashes the order of the map instance deserialization happened to build — a healthy entry accuses
// itself. Measured on a real getredash/redash checkout (1289 files, 42 entries carrying a 2+-key
// `const_map_fragment`): seven consecutive runs over a cache zzop had just written reported 33, 33, 32,
// 38, 38, 34 and 35 refused entries, with findings identical every run.
//
// The pair below is one test design, not two tests: the "no warning" half is worth nothing on its own
// (a detector wired to a constant `true` would pass it), so it is followed by the SAME payload, the SAME
// read path, and a real hand-edit — which must still be refused.

/// A `const_map_fragment` with enough keys that a fresh map instance essentially never reproduces the
/// written order. 8 keys, so a coincidental match is ~1/8! per read.
fn ir_with_multi_key_const_map() -> FileIrSlice {
    let mut ir = sample_ir(10);
    ir.const_map_fragment = (0..8)
        .map(|i| (format!("CONST_{i}"), format!("value-{i}")))
        .collect();
    ir
}

/// The "0" half. Reading the same untouched entry many times must refuse it ZERO times: every read
/// deserializes a NEW `HashMap` with a new hash seed, so a digest taken over a non-canonical
/// serialization flags a random subset of the reads — which is exactly the shape of the field measurement
/// above. One read would be a coin flip; the loop makes the assertion mean what it says.
#[test]
fn a_healthy_entry_holding_a_map_is_never_refused_however_often_it_is_read() {
    let dir = scratch_dir("integrity-map-clean");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("source", "p1", "r1");
    let ir = ir_with_multi_key_const_map();
    cache.put_ir(&k, &ir).unwrap();

    const READS: usize = 100;
    for _ in 0..READS {
        let got = cache
            .get_ir(&k)
            .expect("an entry zzop just wrote must be served");
        assert!(
            json_eq(&got, &ir),
            "the served slice must equal what was put"
        );
    }
    assert_eq!(
        cache.rejected_entries(),
        0,
        "zzop's own write was accused of corruption on some of {READS} reads — the payload never \
         changed, so the digest is being taken over bytes that are not stable for one value"
    );
    assert!(cache.integrity_warning().is_none());
}

/// The half that gives the half above its meaning: the SAME payload, the SAME read path, one byte of a
/// `const_map_fragment` VALUE rewritten on disk with the stored digest left alone. If this does not fire,
/// the test above is proving nothing.
#[test]
fn the_same_read_path_still_refuses_a_hand_edited_map_value() {
    let dir = scratch_dir("integrity-map-tampered");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("source", "p1", "r1");
    cache.put_ir(&k, &ir_with_multi_key_const_map()).unwrap();
    assert!(cache.get_ir(&k).is_some(), "baseline hit");

    let path = cache.ir_path(&IrKey::from(&k));
    let mut entry: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    entry["ir"]["const_map_fragment"]["CONST_3"] = serde_json::json!("tampered");
    fs::write(&path, serde_json::to_vec(&entry).unwrap()).unwrap();

    assert!(
        cache.get_ir(&k).is_none(),
        "a rewritten map VALUE must still be refused — canonicalizing the digest input may drop key \
         ORDER from what is hashed, never content"
    );
    assert_eq!(cache.rejected_entries(), 1);
    assert!(cache.integrity_warning().is_some());
}

/// The exact — and only — edit class the canonical digest input gives up, pinned so it is a decision
/// somebody made rather than a surprise somebody finds. Re-emitting the same object's keys in another
/// order is not a modification of the entry: it deserializes to the identical value, so serving it is
/// serving what zzop wrote. Everything that changes a key, a value, an array's order or an element's
/// presence still moves the digest (the test above, and the four `payload_digest` unit tests).
#[test]
fn a_pure_key_reordering_is_not_treated_as_tampering() {
    let dir = scratch_dir("integrity-map-reordered");
    let cache = AnalysisCache::open(&dir, "v1").unwrap();
    let k = key("source", "p1", "r1");
    let ir = ir_with_multi_key_const_map();
    cache.put_ir(&k, &ir).unwrap();

    // Re-emit the map object's members in reverse order, value-for-value identical. Done on the raw
    // TEXT: parsing into `serde_json::Value` would sort the keys (its `Map` is a `BTreeMap` here) and
    // silently undo the very thing under test.
    let path = cache.ir_path(&IrKey::from(&k));
    let text = fs::read_to_string(&path).unwrap();
    let marker = "\"const_map_fragment\":{";
    let start = text.find(marker).unwrap() + marker.len();
    let end = start + text[start..].find('}').unwrap();
    let mut members: Vec<&str> = text[start..end].split(',').collect();
    members.reverse();
    let reordered = format!("{}{}{}", &text[..start], members.join(","), &text[end..]);
    assert_ne!(
        reordered, text,
        "the reordering must actually move the bytes"
    );
    fs::write(&path, reordered).unwrap();

    let got = cache
        .get_ir(&k)
        .expect("a re-ordered but value-identical map is the same entry, not a modified one");
    assert!(json_eq(&got, &ir));
    assert_eq!(cache.rejected_entries(), 0);
}
