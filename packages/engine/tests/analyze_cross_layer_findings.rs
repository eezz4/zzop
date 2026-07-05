//! End-to-end test for the 6 `cross-layer/*` native rules (`rules/native/rules-graph::cross_layer`), wired
//! from `zzop_engine::analyze_trees` into `MultiAnalyzeOutput::cross_layer_findings`. Mirrors
//! `analyze_multi_tree.rs`'s FE/BE fixture shapes (real TypeScript `fetch` calls + Hono routes, parsed for
//! real — not hand-built `Finding`s) and exercises at least 3 of the 6 rules end to end:
//! `cross-layer/unconsumed-endpoint`, `cross-layer/method-mismatch`, and `cross-layer/duplicate-route`, plus
//! `cross-layer/version-skew` — 4 of 6, all through one small 3-tree fixture. `crossLayerFindings`
//! serialization casing (camelCase, matching every other output-facing type at the napi boundary — see
//! `Finding`'s own `#[serde(rename_all = "camelCase")]`) is asserted directly on `serde_json::to_value`, and
//! `disabledRules` union gating (disabling a cross-layer rule id in only ONE tree's config still drops that
//! rule from the joint output) is asserted by re-running the same fixture with one tree's `disabled_rules`
//! set.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_trees, EngineConfig};

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

/// FE tree: a correctly-joining consume (`GET /authen/getUserInfo`), a method-mismatch consume
/// (`POST /api/v1/orders` — the BE only provides it as `PUT`), and a version-skew consume
/// (`GET /api/v1/accounts` — the BE only provides `GET /api/v2/accounts`).
fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-fe");
    dir.write(
        "src/Ctx.tsx",
        "export function ok() { return fetch(\"/authen/getUserInfo\"); }\n\
         export function mismatch() { return fetch(\"/api/v1/orders\", { method: \"POST\" }); }\n\
         export function skew() { return fetch(\"/api/v1/accounts\"); }\n",
    );
    dir
}

/// BE tree 1: provides the route the FE correctly calls, PLUS `PUT /api/v1/orders` (method-mismatch target)
/// and `GET /api/v2/accounts` (version-skew target), PLUS a dead endpoint nobody calls
/// (`GET /authen/getGoogleRedirect` — drives `cross-layer/unconsumed-endpoint`), PLUS
/// `DELETE /api/legacy/purge` — also provided by BE tree 2, driving cross-tree `duplicate-route`.
fn be1_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-be1");
    dir.write(
        "routes/apiRoutes.ts",
        "const apiRoutes = new Hono();\n\
         apiRoutes.get(\"/authen/getUserInfo\", api.getUserInfo);\n\
         apiRoutes.put(\"/api/v1/orders\", api.updateOrder);\n\
         apiRoutes.get(\"/api/v2/accounts\", api.getAccounts);\n\
         apiRoutes.get(\"/authen/getGoogleRedirect\", api.googleRedirect);\n\
         apiRoutes.delete(\"/api/legacy/purge\", api.purge1);\n",
    );
    dir
}

/// BE tree 2: independently provides the SAME `DELETE /api/legacy/purge` route as BE tree 1 — a genuine
/// cross-tree route duplicate nobody in this fixture consumes (so it surfaces via `unconsumed_provides`).
fn be2_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-be2");
    dir.write(
        "routes/legacy.ts",
        "const legacyRoutes = new Hono();\nlegacyRoutes.delete(\"/api/legacy/purge\", api.purge2);\n",
    );
    dir
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

#[test]
fn cross_layer_findings_cover_at_least_four_of_the_six_rules_end_to_end() {
    let fe = fe_tree();
    let be1 = be1_tree();
    let be2 = be2_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be1.path().to_path_buf(), config("be1")),
        (be2.path().to_path_buf(), config("be2")),
    ];
    let out = analyze_trees(&trees);

    // Sanity: the correctly-matching route still joins as a normal edge, not a cross-layer finding source.
    assert!(
        out.cross_layer
            .edges
            .iter()
            .any(|e| e.key == "GET /authen/getUserInfo"),
        "expected the correctly-matching route to still join: {:?}",
        out.cross_layer.edges
    );

    // 1. cross-layer/unconsumed-endpoint — the dead Google-redirect route.
    let unconsumed = find(&out.cross_layer_findings, "cross-layer/unconsumed-endpoint");
    assert!(
        unconsumed
            .iter()
            .any(|f| f.message.contains("GET /authen/getGoogleRedirect")),
        "expected an unconsumed-endpoint finding for the dead route: {:?}",
        unconsumed
    );

    // 2. cross-layer/method-mismatch — FE calls POST, BE only provides PUT, same path.
    let mismatch = find(&out.cross_layer_findings, "cross-layer/method-mismatch");
    assert_eq!(mismatch.len(), 1, "{:?}", mismatch);
    assert_eq!(mismatch[0].file, "src/Ctx.tsx");
    assert!(mismatch[0].message.contains("/api/v1/orders"));
    assert!(mismatch[0].message.contains("PUT"));

    // 3. cross-layer/version-skew — FE calls v1, BE only provides v2, rest of the path identical.
    let skew = find(&out.cross_layer_findings, "cross-layer/version-skew");
    assert_eq!(skew.len(), 1, "{:?}", skew);
    assert_eq!(skew[0].file, "src/Ctx.tsx");
    assert!(skew[0].message.contains("`v1`"));
    assert!(skew[0].message.contains("`v2`"));

    // 4. cross-layer/duplicate-route — DELETE /api/legacy/purge provided by both be1 and be2.
    let dup = find(&out.cross_layer_findings, "cross-layer/duplicate-route");
    assert_eq!(dup.len(), 1, "{:?}", dup);
    assert!(dup[0].message.contains("DELETE /api/legacy/purge"));
    assert!(dup[0].message.contains("be1"));
    assert!(dup[0].message.contains("be2"));

    // Deterministic (severity, file, line, ruleId) sort — the same order `merge_findings` gives per-tree
    // findings. Every one of these 4 rules is `Warning` except `unconsumed-endpoint` (`Info`), so `Info`
    // entries must all sort after every `Warning` entry.
    let mut saw_info = false;
    for f in &out.cross_layer_findings {
        if f.severity == zzop_core::Severity::Info {
            saw_info = true;
        } else {
            assert!(
                !saw_info,
                "a non-info finding appeared after an info finding — severity sort violated: {:?}",
                out.cross_layer_findings
            );
        }
    }
}

#[test]
fn cross_layer_findings_serialize_camel_case_like_every_other_output_type() {
    let fe = fe_tree();
    let be1 = be1_tree();
    let be2 = be2_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be1.path().to_path_buf(), config("be1")),
        (be2.path().to_path_buf(), config("be2")),
    ];
    let out = analyze_trees(&trees);
    assert!(!out.cross_layer_findings.is_empty());

    let value = serde_json::to_value(&out.cross_layer_findings).unwrap();
    let first = value.as_array().unwrap().first().unwrap();
    let obj = first.as_object().unwrap();
    assert!(obj.contains_key("ruleId"), "{obj:?}");
    assert!(!obj.contains_key("rule_id"), "{obj:?}");
    assert!(obj.contains_key("severity"));
    assert!(obj.contains_key("file"));
    assert!(obj.contains_key("line"));
    assert!(obj.contains_key("message"));
}

#[test]
fn disabling_a_cross_layer_rule_in_only_one_tree_drops_it_from_the_union() {
    let fe = fe_tree();
    let be1 = be1_tree();
    let be2 = be2_tree();

    let mut fe_config = config("fe");
    fe_config.rule_config.disabled_rules = vec!["cross-layer/method-mismatch".to_string()];

    let trees = vec![
        (fe.path().to_path_buf(), fe_config),
        (be1.path().to_path_buf(), config("be1")), // does NOT disable it itself
        (be2.path().to_path_buf(), config("be2")),
    ];
    let out = analyze_trees(&trees);

    assert!(
        find(&out.cross_layer_findings, "cross-layer/method-mismatch").is_empty(),
        "one tree disabling a cross-layer rule must drop it from the joint output: {:?}",
        out.cross_layer_findings
    );
    // Sibling rules untouched by the disable — still present.
    assert!(!find(&out.cross_layer_findings, "cross-layer/version-skew").is_empty());
    assert!(!find(&out.cross_layer_findings, "cross-layer/duplicate-route").is_empty());
}
