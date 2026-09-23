//! Java's call-graph HOP DEPTH, proved end-to-end on one tree that contains all three depths at once.
//!
//! 📏 Why one tree and not three: the defect this pins was a FALSE FINDING, and a false finding is only
//! visible next to the true one it looks identical to. Before `callgraph::java_bridge` (2026-09-07, review
//! ledger V30) this exact tree produced TWO `mutating-route-no-auth` findings — hop-0 (real: no guard
//! anywhere) and hop-2 (the bug: handler -> helper in ANOTHER file -> guard, whose second hop the graph
//! could not draw). The engine's own module doc named the single-hop limit honestly but recorded it as
//! missing coverage; it was worse than that, because the rule spent the gap on an accusation.
//!
//! The three hops, and what each one is evidence of:
//! - **hop-0** — no guard on any path. Must FIRE, before and after. This is the control: a fix that
//!   silences hop-2 by weakening the rule silences this too.
//! - **hop-1** — the handler calls the guard directly. Already passed, and passes through the guard
//!   vocabulary's QUALIFIER arm (`AuthorizationService`), not the method name (`checkUser` matches no
//!   guard pattern). The bridge is additive precisely so this keeps working: it ADDS a node, it never
//!   replaces the specifier-shaped one the qualifier is read from.
//! - **hop-2** — the guard is one file further away. Must NOT fire.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, EngineConfig};

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

/// The whole tree: one guard, one helper that calls it, and three controllers at hop 0, 1 and 2.
fn three_hop_tree() -> TempDir {
    let dir = TempDir::new("zzop-java-hops");
    dir.write(
        "src/main/java/com/ex/svc/AuthorizationService.java",
        "package com.ex.svc;\npublic class AuthorizationService {\n    public void checkUser(String id) { }\n}\n",
    );
    dir.write(
        "src/main/java/com/ex/svc/OrderHelper.java",
        "package com.ex.svc;\nimport com.ex.svc.AuthorizationService;\npublic class OrderHelper {\n    private AuthorizationService auth = new AuthorizationService();\n    public void persist(String id) { auth.checkUser(id); }\n}\n",
    );
    dir.write(
        "src/main/java/com/ex/web/Hop0Controller.java",
        "package com.ex.web;\nimport org.springframework.web.bind.annotation.*;\n@RestController\npublic class Hop0Controller {\n    @PostMapping(\"/hop0/orders\")\n    public String create(@RequestBody String body) { return \"ok\"; }\n}\n",
    );
    dir.write(
        "src/main/java/com/ex/web/Hop1Controller.java",
        "package com.ex.web;\nimport com.ex.svc.AuthorizationService;\nimport org.springframework.web.bind.annotation.*;\n@RestController\npublic class Hop1Controller {\n    private AuthorizationService auth = new AuthorizationService();\n    @PostMapping(\"/hop1/orders\")\n    public String create(@RequestBody String body) { auth.checkUser(body); return \"ok\"; }\n}\n",
    );
    dir.write(
        "src/main/java/com/ex/web/Hop2Controller.java",
        "package com.ex.web;\nimport com.ex.svc.OrderHelper;\nimport org.springframework.web.bind.annotation.*;\n@RestController\npublic class Hop2Controller {\n    private OrderHelper helper = new OrderHelper();\n    @PostMapping(\"/hop2/orders\")\n    public String create(@RequestBody String body) { helper.persist(body); return \"ok\"; }\n}\n",
    );
    dir
}

fn unguarded_controllers(dir: &TempDir) -> Vec<String> {
    let out = analyze_tree(dir.path(), &EngineConfig::default());
    let mut files: Vec<String> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "mutating-route-no-auth")
        .map(|f| {
            f.file
                .rsplit('/')
                .next()
                .unwrap_or(f.file.as_str())
                .to_string()
        })
        .collect();
    files.sort();
    files.dedup();
    files
}

#[test]
fn a_java_guard_two_hops_away_clears_the_route_and_an_absent_one_still_fires() {
    let dir = three_hop_tree();
    assert_eq!(
        unguarded_controllers(&dir),
        vec!["Hop0Controller.java".to_string()],
        "hop-0 is the only route with no guard on any path. Hop2Controller appearing here is the \
         pre-bridge false positive (review ledger V30): its guard is real, just one file further away. \
         Hop1Controller appearing here would mean the bridge REPLACED the specifier-shaped node the \
         guard vocabulary reads its qualifier from, instead of adding to it."
    );
}

#[test]
fn the_hop_two_helper_is_what_carries_the_guard_not_the_handler_itself() {
    // The counterfactual for the test above: strip the helper's own call to the guard and hop-2 becomes
    // a genuine finding. Without this, "hop-2 does not fire" is equally consistent with the bridge
    // clearing every Java route that reaches ANY second file — the failure mode a reach fix invites.
    let dir = three_hop_tree();
    dir.write(
        "src/main/java/com/ex/svc/OrderHelper.java",
        "package com.ex.svc;\npublic class OrderHelper {\n    public void persist(String id) { }\n}\n",
    );
    assert_eq!(
        unguarded_controllers(&dir),
        vec![
            "Hop0Controller.java".to_string(),
            "Hop2Controller.java".to_string()
        ],
        "with the helper's guard call removed, hop-2 has no guard on any path and must fire again"
    );
}
