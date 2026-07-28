//! D18 end-to-end: `.rs` inside `CALL_GRAPH_COVERED_EXTENSIONS`, proved through the real engine.
//!
//! Three halves have to hold together or the lift is dishonest, and each has its own test below:
//! - **LIFT** — a Rust mutating route with no auth anywhere is now FOUND. Before this batch the rule was
//!   structurally silent on every Rust tree (`symbol_graph` restricted to `.rs` was provably empty).
//! - **GUARD** — a route whose handler takes an auth EXTRACTOR is cleared. This is the half that makes
//!   the lift safe: without it, every idiomatically-guarded axum route becomes a false positive, which is
//!   exactly what D8's "both halves in the same batch" rule exists to prevent.
//! - **VETO** — an OPTIONAL extractor does NOT clear it. `MaybeAuthUser` contains `auth` and would sail
//!   through the name vocabulary; it admits anonymous callers, so it must never reach the graph.
//!
//! Plus the range disclosure (S10) that the lift made necessary.
//!
//! Fixtures are distilled from `corpus/oss/be-axum`, whose `src/http/extractor.rs` really does export
//! `AuthUser` and `MaybeAuthUser(pub Option<AuthUser>)`, and whose five mutating routes are all guarded
//! by the signature shape and none by a body call.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

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

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &EngineConfig::default())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings.iter().filter(|f| f.rule_id == rule).collect()
}

/// `src/http/extractor.rs` — the guard types themselves, so the tree really DECLARES the symbol the
/// handler's signature names (an undeclared name resolves to nothing and the edge is dropped).
const EXTRACTOR_RS: &str = concat!(
    "pub struct AuthUser {\n",
    "    pub user_id: String,\n",
    "}\n\n",
    "pub struct MaybeAuthUser(pub Option<AuthUser>);\n",
);

/// An axum router file whose one mutating route is handled by `create_article`, whose signature is
/// `param`. Passing the whole parameter list keeps the three tests below differing in EXACTLY the one
/// thing each is about.
fn axum_tree(dir: &TempDir, param: &str) {
    dir.write("src/http/extractor.rs", EXTRACTOR_RS);
    dir.write(
        "src/http/articles.rs",
        &format!(
            concat!(
                "use axum::routing::post;\n",
                "use axum::Router;\n",
                "use crate::http::extractor::{{AuthUser, MaybeAuthUser}};\n\n",
                "pub fn router() -> Router {{\n",
                "    Router::new().route(\"/api/articles\", post(create_article))\n",
                "}}\n\n",
                "async fn create_article({param}) -> String {{\n",
                "    String::new()\n",
                "}}\n"
            ),
            param = param
        ),
    );
}

/// The LIFT half, on its own. Before `.rs` entered `CALL_GRAPH_COVERED_EXTENSIONS`, this route was
/// exempted before the BFS ever ran and the rule could not fire on any Rust tree at all.
#[test]
fn a_rust_mutating_route_with_no_auth_evidence_is_flagged() {
    let dir = TempDir::new("zzop-callgraph-rust-unguarded");
    axum_tree(&dir, "body: String");
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "src/http/articles.rs");
    assert_eq!(found[0].data.as_ref().unwrap()["method"], "POST");
}

/// The GUARD half, end-to-end through the real engine: parser producer
/// (`zzop_parser_rust::parse_extractor_guards`) + the engine's Rust loop + cross-file resolution of
/// `crate::http::extractor::AuthUser` + the rule's existing name vocabulary. No `decorator_guarded`
/// entry is involved — this clears as an ordinary graph edge, which is the whole design claim.
#[test]
fn an_auth_extractor_in_the_handler_signature_clears_the_route() {
    let dir = TempDir::new("zzop-callgraph-rust-guarded");
    axum_tree(&dir, "auth_user: AuthUser, body: String");
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "an AuthUser extractor is the idiomatic Rust guard and must exempt the route: {:?}",
        out.findings
    );
}

/// The VETO half. `MaybeAuthUser` holds an `Option`, so the route is reachable anonymously — and its
/// name contains `auth`, so nothing downstream would have caught it. If this regresses, the engine
/// silently stops reporting the routes most likely to be genuinely open.
#[test]
fn an_optional_extractor_does_not_clear_the_route() {
    let dir = TempDir::new("zzop-callgraph-rust-optional");
    axum_tree(&dir, "maybe_user: MaybeAuthUser, body: String");
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "mutating-route-no-auth").len(),
        1,
        "an optional extractor admits anonymous callers, so it is not a gate: {:?}",
        out.findings
    );
}

/// S10 — the range disclosure the lift made necessary. A tower `.route_layer` guard is invisible to the
/// BFS, and in Rust that is a mainstream idiom rather than an edge case, so every run whose Rust routes
/// are IN the rule's range says so.
#[test]
fn a_rust_tree_in_range_discloses_the_router_layer_blind_spot() {
    let dir = TempDir::new("zzop-callgraph-rust-disclosure");
    axum_tree(&dir, "auth_user: AuthUser, body: String");
    let out = scan(&dir);
    let disclosed = out
        .warnings
        .iter()
        .any(|w| w.contains("Rust auth-range gap"));
    assert!(disclosed, "warnings: {:?}", out.warnings);
}

/// The disclosure is gated on the RULE's range, not on "Rust is present" — a read-only Rust tree gets
/// no warning, because a disclosure that fires where the rule cannot is the noise that makes real
/// disclosures ignorable.
#[test]
fn a_read_only_rust_tree_gets_no_range_disclosure() {
    let dir = TempDir::new("zzop-callgraph-rust-readonly");
    dir.write("src/http/extractor.rs", EXTRACTOR_RS);
    dir.write(
        "src/http/articles.rs",
        concat!(
            "use axum::routing::get;\n",
            "use axum::Router;\n\n",
            "pub fn router() -> Router {\n",
            "    Router::new().route(\"/api/articles\", get(list_articles))\n",
            "}\n\n",
            "async fn list_articles() -> String {\n",
            "    String::new()\n",
            "}\n"
        ),
    );
    let out = scan(&dir);
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("Rust auth-range gap")),
        "warnings: {:?}",
        out.warnings
    );
}
