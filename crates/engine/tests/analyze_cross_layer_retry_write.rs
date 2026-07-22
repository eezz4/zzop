//! End-to-end test for `cross-layer/retrying-write-no-idempotency` (`egress-retry-v1` FE tag → cross-layer
//! join → native rule), wired from `zzop_engine::analyze_trees`. Parses real TypeScript — an `axios-retry`-
//! wired FE tree and a Hono/Express BE tree — so the whole vertical slice runs: the parser tags the retried
//! write consume, the join resolves it to the BE route, and the rule flags exactly those call sites. Two
//! positives (an `axios-retry`-file POST and a `pRetry(...)`-wrapped DELETE) fire; three controls prove
//! precision — a retried READ, a retried write with no provider (no edge), and a non-retry write all stay
//! silent.
//!
//! Increment 2 (two-sided check) coverage added here: the rule is now `critical`, and a witnessed
//! `idempotency-guarded` attribute on the resolved provider route — either NATIVELY recognized (an inline
//! Express/Hono handler reading the `Idempotency-Key` header, parser-typescript's `router_mounts::idempotency`
//! recognizer) or INJECTED (a Mode B `EngineConfig::adapter_overlays` overlay, exact `IoKey` or a covering
//! `PathScope`) — vetoes the finding while leaving the cross-layer edge itself intact, proving the join still
//! ran and only the rule's own veto suppressed the finding.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{
    Attribute, EntityRef, FileProjection, IoFacts, NormalizedEnvelope, NORMALIZED_AST_FORMAT,
};
use zzop_engine::{analyze_trees, EngineConfig};
use zzop_rules_cross_layer::IDEMPOTENCY_GUARDED_ATTR;

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn config(source_id: &str) -> EngineConfig {
    EngineConfig {
        source_id: source_id.to_string(),
        ..EngineConfig::default()
    }
}

fn find<'a>(findings: &'a [zzop_core::Finding], rule_id: &str) -> Vec<&'a zzop_core::Finding> {
    findings.iter().filter(|f| f.rule_id == rule_id).collect()
}

/// A minimal, all-empty `FileProjection` — same defaults `analyze_attribute_injection.rs`'s own
/// `projection()` helper uses. Self-contained: each `tests/*.rs` file is its own separate test
/// binary/crate, so these small overlay helpers are copied/adapted rather than shared.
fn projection_with_attrs(path: &str, loc: u32, attrs: Vec<Attribute>) -> FileProjection {
    FileProjection {
        class_shape_fragments: Vec::new(),
        path: path.to_string(),
        loc,
        symbols: Vec::new(),
        imports: zzop_core::ImportMap::new(),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: std::collections::HashMap::new(),
        procedure_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        io: IoFacts::default(),
        degraded: false,
        is_entry: false,
        attributes: attrs,
        loop_spans: Vec::new(),
    }
}

/// A single-file overlay whose one `FileProjection` carries exactly `attrs` — the placeholder path/loc
/// are irrelevant to `AttributeStore::from_overlays`, which flattens `attributes` across every overlay
/// file regardless of path (attributes are entity-addressed, not file-addressed).
fn overlay_with_attrs(parser: &str, attrs: Vec<Attribute>) -> NormalizedEnvelope {
    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: parser.to_string(),
        source: "adapter".to_string(),
        files: vec![projection_with_attrs("overlay/attrs.json", 1, attrs)],
    }
}

/// FE tree: `checkout.ts` wires `axios-retry`, so its POST is a retried write (FLAGGED once it joins a BE
/// route); its GET is a retried READ (never tagged). `noretry.ts` does NOT wire retry, so its PUT — though it
/// also joins a BE route — is not flagged. `orphan.ts` wires retry and POSTs to a path NO backend provides,
/// so there is no edge and nothing to flag.
fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-retry-fe");
    dir.write(
        "src/checkout.ts",
        "import axiosRetry from \"axios-retry\";\n\
         axiosRetry(axios, { retries: 3 });\n\
         export function pay() { return axios.post(\"/api/orders\", body); }\n\
         export function peek() { return axios.get(\"/api/orders\"); }\n",
    );
    dir.write(
        "src/noretry.ts",
        "export function save() { return axios.put(\"/api/profile\", body); }\n",
    );
    dir.write(
        "src/wrapped.ts",
        "export function del() { return pRetry(() => axios.delete(\"/api/session\", body)); }\n",
    );
    dir.write(
        "src/orphan.ts",
        "import axiosRetry from \"axios-retry\";\n\
         axiosRetry(axios, { retries: 3 });\n\
         export function ghost() { return axios.post(\"/api/nowhere\", body); }\n",
    );
    dir
}

/// BE tree: provides the two routes the FE writes to (`POST /api/orders`, `PUT /api/profile`) and the read
/// (`GET /api/orders`). It does NOT provide `/api/nowhere`. Handlers are named references (not inline), so
/// none of these routes ever carry a native `idempotency-guarded` witness — this fixture is deliberately
/// pre-increment-2-veto-shaped, isolating the base flagging behavior from the veto (covered by the
/// Express-shaped fixtures below).
fn be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-retry-be");
    dir.write(
        "routes/api.ts",
        "const r = new Hono();\n\
         r.post(\"/api/orders\", api.createOrder);\n\
         r.get(\"/api/orders\", api.listOrders);\n\
         r.put(\"/api/profile\", api.updateProfile);\n\
         r.delete(\"/api/session\", api.logout);\n",
    );
    dir
}

/// Express-shaped BE tree providing ONLY `POST /api/orders`, registered with an INLINE handler whose body
/// reads the `Idempotency-Key` header — the native witness `parser_typescript::router_mounts::idempotency`
/// recognizes (see that module: only an inline `Arrow`/`Fn` handler is ever judged, never a named
/// reference). The existing `be_tree()` fixture above registers routes via Hono with named-reference
/// handlers (`api.createOrder`), which can never carry this native witness — an inline Express handler is a
/// different registration shape, so it gets its own fixture here rather than mutating `be_tree()` and
/// disturbing the base test's other routes/assertions.
fn be_tree_express_guarded() -> TempDir {
    let dir = TempDir::new("zzop-engine-retry-be-express-guarded");
    dir.write(
        "routes/api.ts",
        "const app = express();\n\
         app.post(\"/api/orders\", async (req, res) => {\n\
         \x20 const key = req.get(\"Idempotency-Key\");\n\
         \x20 res.json({ ok: true });\n\
         });\n",
    );
    dir
}

/// Same Express registration shape as [`be_tree_express_guarded`], but the inline handler never reads the
/// idempotency-key header — no native witness, so `retrying-write-no-idempotency` fires unless something
/// else (a Mode B injection) vetoes it. Shared by the UNGUARDED-still-fires test and both INJECTION-VETO
/// tests below, so all three exercise byte-identical BE source and differ only in the injected overlay.
fn be_tree_express_unguarded() -> TempDir {
    let dir = TempDir::new("zzop-engine-retry-be-express-unguarded");
    dir.write(
        "routes/api.ts",
        "const app = express();\n\
         app.post(\"/api/orders\", async (req, res) => {\n\
         \x20 res.json({ ok: true });\n\
         });\n",
    );
    dir
}

#[test]
fn retrying_write_flagged_only_when_retried_write_joins_a_provider() {
    let fe = fe_tree();
    let be = be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    // The retried POST resolves to the BE route — a normal edge exists.
    assert!(
        out.cross_layer
            .edges
            .iter()
            .any(|e| e.key == "POST /api/orders"),
        "expected the retried write to join the provider: {:?}",
        out.cross_layer.edges
    );

    let flagged = find(
        &out.cross_layer_findings,
        "cross-layer/retrying-write-no-idempotency",
    );
    // Exactly the two retried writes that JOIN a provider: the axios-retry-file POST and the pRetry-wrapped
    // DELETE. The retried GET (read verb), the non-retry PUT, and the orphan POST (no provider) stay silent.
    let mut hit: Vec<(&str, &str)> = flagged
        .iter()
        .map(|f| {
            // Increment 2: severity is now CRITICAL (was Warning in increment 1) — both sides of the
            // claim (a real retry trigger AND a real absence of a witnessed provider guard) are now
            // checked, so the finding no longer needs the increment-1 hedge.
            assert_eq!(f.severity, zzop_core::Severity::Critical);
            assert!(
                f.message.contains("routes/api.ts"),
                "cites provider: {}",
                f.message
            );
            // Increment 2: the message now names the veto vocabulary directly.
            assert!(
                f.message.contains("idempotency-guarded"),
                "message names the veto attribute: {}",
                f.message
            );
            // The increment-1 honesty hedge ("does NOT verify ...") is gone now that the provider side
            // is actually checked via the attribute store.
            assert!(
                !f.message.contains("does NOT verify"),
                "old increment-1 guard-not-checked disclosure must be gone: {}",
                f.message
            );
            (
                f.file.as_str(),
                if f.message.contains("POST /api/orders") {
                    "POST"
                } else {
                    "DELETE"
                },
            )
        })
        .collect();
    hit.sort();
    assert_eq!(
        hit,
        vec![("src/checkout.ts", "POST"), ("src/wrapped.ts", "DELETE")],
        "only the two retried writes that resolve to a provider are flagged: {:?}",
        flagged
    );
}

/// NATIVE VETO: the same FE retry setup, but the BE route is registered Express-style with an inline
/// handler that reads the `Idempotency-Key` header — the native witness. `retrying-write-no-idempotency`
/// must not fire for `POST /api/orders`, while the cross-layer edge itself still exists (proving the join
/// ran and the ABSENCE of a finding is the rule's own veto, not a broken join).
#[test]
fn native_idempotency_guarded_witness_vetoes_the_finding() {
    let fe = fe_tree();
    let be = be_tree_express_guarded();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    assert!(
        out.cross_layer
            .edges
            .iter()
            .any(|e| e.key == "POST /api/orders"),
        "the join must still resolve the retried write to the provider — only the FINDING is vetoed: {:?}",
        out.cross_layer.edges
    );

    let flagged = find(
        &out.cross_layer_findings,
        "cross-layer/retrying-write-no-idempotency",
    );
    assert!(
        flagged.is_empty(),
        "a natively recognized inline handler reading the idempotency-key header must veto the finding: {:?}",
        flagged
    );
}

/// UNGUARDED still fires: the same Express registration shape as the native-veto test, but the inline
/// handler never reads the header — no native witness, no overlay, so the finding fires at `critical` with
/// a provider `file:line` citation and a paste-ready `data.injectionStub`.
#[test]
fn unguarded_express_inline_handler_still_fires_critical_with_injection_stub() {
    let fe = fe_tree();
    let be = be_tree_express_unguarded();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    let flagged = find(
        &out.cross_layer_findings,
        "cross-layer/retrying-write-no-idempotency",
    );
    assert_eq!(flagged.len(), 1, "{:?}", flagged);
    let f = flagged[0];
    assert_eq!(f.severity, zzop_core::Severity::Critical);
    assert!(
        f.message.contains("routes/api.ts:2"),
        "message cites the provider file:line: {}",
        f.message
    );

    let data = f.data.as_ref().expect("finding carries data");
    let stub_str = data["injectionStub"]
        .as_str()
        .expect("injectionStub is a JSON string");
    let stub: serde_json::Value =
        serde_json::from_str(stub_str).expect("injectionStub parses as JSON");
    assert_eq!(stub["target"]["ioKey"]["kind"], "http");
    assert_eq!(stub["target"]["ioKey"]["key"], "POST /api/orders");
    assert_eq!(stub["key"], "idempotency-guarded");
    assert_eq!(stub["value"], true);
}

/// INJECTION VETO (exact `IoKey`): the same unguarded Express BE fixture, but the BE tree's `EngineConfig`
/// carries a Mode B adapter overlay injecting `idempotency-guarded` on the exact route `IoKey` — the
/// injection-last proof for provider languages/frameworks with no native recognizer at all.
#[test]
fn injected_idempotency_guarded_iokey_vetoes_the_unguarded_handler() {
    let fe = fe_tree();
    let be = be_tree_express_unguarded();
    let mut be_cfg = config("be");
    be_cfg.adapter_overlays = vec![overlay_with_attrs(
        "idempotency-overlay-adapter/1",
        vec![Attribute {
            target: EntityRef::IoKey {
                kind: "http".to_string(),
                key: "POST /api/orders".to_string(),
            },
            key: IDEMPOTENCY_GUARDED_ATTR.to_string(),
            value: serde_json::json!(true),
        }],
    )];
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), be_cfg),
    ];
    let out = analyze_trees(&trees);

    assert!(
        out.cross_layer
            .edges
            .iter()
            .any(|e| e.key == "POST /api/orders"),
        "the join must still resolve — only the finding is vetoed: {:?}",
        out.cross_layer.edges
    );
    let flagged = find(
        &out.cross_layer_findings,
        "cross-layer/retrying-write-no-idempotency",
    );
    assert!(
        flagged.is_empty(),
        "an injected exact-IoKey idempotency-guarded attribute must veto the finding: {:?}",
        flagged
    );
}

/// INJECTION VETO (`PathScope`): same unguarded Express BE fixture, but the injected attribute targets a
/// covering `PathScope` (`/api`) instead of the exact route `IoKey` — the router-level-middleware shape of
/// this same veto channel (mirrors `mutating-route-no-auth`'s `auth-guarded` PathScope coverage).
#[test]
fn injected_idempotency_guarded_pathscope_vetoes_the_unguarded_handler() {
    let fe = fe_tree();
    let be = be_tree_express_unguarded();
    let mut be_cfg = config("be");
    be_cfg.adapter_overlays = vec![overlay_with_attrs(
        "idempotency-overlay-adapter/1",
        vec![Attribute {
            target: EntityRef::PathScope {
                prefix: "/api".to_string(),
            },
            key: IDEMPOTENCY_GUARDED_ATTR.to_string(),
            value: serde_json::json!(true),
        }],
    )];
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), be_cfg),
    ];
    let out = analyze_trees(&trees);

    assert!(
        out.cross_layer
            .edges
            .iter()
            .any(|e| e.key == "POST /api/orders"),
        "the join must still resolve — only the finding is vetoed: {:?}",
        out.cross_layer.edges
    );
    let flagged = find(
        &out.cross_layer_findings,
        "cross-layer/retrying-write-no-idempotency",
    );
    assert!(
        flagged.is_empty(),
        "an injected covering-PathScope idempotency-guarded attribute must veto the finding: {:?}",
        flagged
    );
}

/// PAIRING PIN: `zzop_rules_cross_layer::IDEMPOTENCY_GUARDED_ATTR` is the rule-side half of a
/// producer/consumer vocabulary pairing with parser-typescript's PRIVATE
/// `router_mounts::idempotency::IDEMPOTENCY_GUARDED_ATTR_KEY` const — both must spell the same literal
/// string or the native channel silently goes dark (the rule would query a key the parser never emits).
/// Since the parser-side const is intentionally private (producer-owned vocabulary), this test only pins
/// the rule-side spelling directly; `native_idempotency_guarded_witness_vetoes_the_finding` above is the
/// BEHAVIORAL half of the pin — if the two consts ever drifted, that test would fail (the native witness
/// would stop reaching the rule and the finding would fire instead of veto). Same convention as
/// `analyze_native_middleware.rs`'s pin for `auth-guarded`/`AUTH_GUARDED_ATTR_KEY`.
#[test]
fn idempotency_guarded_attr_literal_is_pinned() {
    assert_eq!(IDEMPOTENCY_GUARDED_ATTR, "idempotency-guarded");
}
