//! `reqwest-no-timeout` — the Rust counterpart of `fetch-no-timeout`, and the reason it is a separate
//! rule rather than a widened `file_pattern`: `fetch-no-timeout`'s `require_file` pre-gate looks for a
//! Node/Workers server shape (an `express`/`fastify`/`hono` import, `createServer(`, a D1
//! `prepare(` call, ...), none of which a Rust file can ever satisfy — so admitting `.rs` there would
//! have shipped a rule that is structurally silent on every Rust tree while the catalog listed it.

use crate::{hits, scan, TempDir};

#[test]
fn a_reqwest_client_with_no_timeout_in_the_function_is_flagged() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "pub async fn fetch(url: &str) -> Result<String, reqwest::Error> {\n    let client = reqwest::Client::new();\n    client.get(url).send().await?.text().await\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "reqwest-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn a_client_builder_carrying_a_timeout_is_not_flagged() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "pub async fn fetch(url: &str) -> Result<String, reqwest::Error> {\n    let client = reqwest::Client::builder()\n        .timeout(std::time::Duration::from_secs(5))\n        .build()?;\n    client.get(url).send().await?.text().await\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "reqwest-no-timeout").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The nearest benign lookalike: some OTHER library's `Client::new()`. The `require_file` pre-gate is
/// the whole defense here, so this pins that the rule is not a bare `Client::new` scan.
#[test]
fn a_client_new_in_a_file_that_never_names_reqwest_is_not_flagged() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "use aws_sdk_s3::Client;\npub async fn head(bucket: &str) -> String {\n    let client = Client::new(&aws_config::load_from_env().await);\n    format!(\"{}/{}\", bucket, client.config().region().unwrap())\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "reqwest-no-timeout").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The disclosed FUNCTION-LOCAL limit, pinned rather than only described: a timeout configured in a
/// sibling constructor does NOT clear a client built here. This asserts the rule's stated blind spot, so
/// a future change that silently widens the veto to file scope goes red instead of quietly landing.
#[test]
fn a_timeout_set_in_a_different_function_does_not_clear_this_one() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "pub fn shared() -> reqwest::Client {\n    reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build().unwrap()\n}\npub async fn fetch(url: &str) -> Result<String, reqwest::Error> {\n    let client = reqwest::Client::new();\n    client.get(url).send().await?.text().await\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "reqwest-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

/// The 2026-08-03 veto tightening, positive direction: `connect_timeout` caps the HANDSHAKE only — the
/// message's headline failure (a peer that accepts and then stalls) happens after the handshake
/// succeeds, so a connect-timeout-only builder must NOT clear the rule. Under the old bare-word
/// `(?i)timeout` veto this exact shape was silent.
#[test]
fn a_builder_with_only_a_connect_timeout_is_flagged() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "pub async fn fetch(url: &str) -> Result<String, reqwest::Error> {\n    let client = reqwest::Client::builder()\n        .connect_timeout(std::time::Duration::from_secs(2))\n        .build()?;\n    client.get(url).send().await?.text().await\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "reqwest-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

/// The 2026-08-03 veto widening, silencing direction: `.read_timeout(..)` bounds exactly the
/// stalled-peer read the message names as the failure, so a read-timeout-only builder must clear
/// the rule. Under the `\.timeout\s*\(` spelling-only veto this shape fired — the rule warned about
/// a stall the code already capped.
#[test]
fn a_builder_with_only_a_read_timeout_is_not_flagged() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "pub async fn fetch(url: &str) -> Result<String, reqwest::Error> {\n    let client = reqwest::Client::builder()\n        .read_timeout(std::time::Duration::from_secs(5))\n        .build()?;\n    client.get(url).send().await?.text().await\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "reqwest-no-timeout").is_empty(),
        "{:?}",
        out.findings
    );
}

/// Same tightening, other lexical direction: the WORD `timeout` inside an unrelated string no longer
/// vetoes — only the method spelling `.timeout(` does.
#[test]
fn the_word_timeout_in_an_error_string_no_longer_clears_the_finding() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "pub async fn fetch(url: &str) -> Result<String, reqwest::Error> {\n    let context = \"gateway timeout budget is owned by the caller\";\n    let client = reqwest::Client::new();\n    client.get(url).send().await.map_err(|e| { let _ = context; e })?.text().await\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "reqwest-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

/// The 2026-08-03 trigger widening: `reqwest::get(url)` builds a fresh timeout-less default client on
/// every call and was previously invisible to the `Client::{new,builder}` trigger.
#[test]
fn the_reqwest_get_convenience_call_is_flagged() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "pub async fn ping(url: &str) -> Result<String, reqwest::Error> {\n    reqwest::get(url).await?.text().await\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "reqwest-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

/// `Client::default()` is the same timeout-less default client as `Client::new()` by another spelling.
#[test]
fn a_client_built_via_default_is_flagged() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "pub async fn fetch(url: &str) -> Result<String, reqwest::Error> {\n    let client = reqwest::Client::default();\n    client.get(url).send().await?.text().await\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "reqwest-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

/// A per-request `.timeout(..)` on the request builder is the message's second recommended fix, and the
/// tightened veto still reads it: same method spelling, different receiver.
#[test]
fn a_request_level_timeout_still_clears_the_finding() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "pub async fn fetch(url: &str) -> Result<String, reqwest::Error> {\n    let client = reqwest::Client::new();\n    client.get(url).timeout(std::time::Duration::from_secs(5)).send().await?.text().await\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "reqwest-no-timeout").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn the_reqwest_no_timeout_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "pub async fn fetch(url: &str) -> Result<String, reqwest::Error> {\n    // zzop-reqwest-no-timeout-ok: the caller wraps this in tokio::time::timeout\n    let client = reqwest::Client::new();\n    client.get(url).send().await?.text().await\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "reqwest-no-timeout").is_empty(),
        "{:?}",
        out.findings
    );
}
