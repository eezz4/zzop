// The two method-scan rules. Both need the Rust parser's `SourceSymbol` body spans, so a `syn` parse
// failure in THIS file would silence them while changing nothing else in the score — which is the
// failure mode a purely lexical fixture cannot detect, and the reason one of the two fixtures here is
// deliberately span-dependent rather than line-dependent.

use std::process::Command;

/// security/command-and-interpolation — `Command::new` and a `format!` in ONE function body. The good
/// form passes each argument on its own and interpolates nothing, so the trigger never matches.
pub fn run_report(period: &str) -> std::io::Result<std::process::Output> {
    let script = format!("/usr/local/bin/report --period {}", period);
    Command::new("sh").arg("-c").arg(script).output()
}

pub fn good_run_report(period: &str) -> std::io::Result<std::process::Output> {
    Command::new("/usr/local/bin/report")
        .arg("--period")
        .arg(period)
        .output()
}

/// reliability/reqwest-no-timeout — a client built from reqwest's timeout-less default. The good form
/// sets one on the builder, which is what the rule's `absent` veto reads.
pub async fn fetch_profile(url: &str) -> Result<String, reqwest::Error> {
    let client = reqwest::Client::new();
    client.get(url).send().await?.text().await
}

pub async fn good_fetch_profile(url: &str) -> Result<String, reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    client.get(url).send().await?.text().await
}

/// The 2026-08-03 additions, one per direction the original matcher missed. `reqwest::get` builds a
/// fresh timeout-less default client on every call and was invisible to the `Client::{new,builder}`
/// trigger; `connect_timeout` caps only the HANDSHAKE, so under the old bare-word `timeout` veto it
/// silenced the rule while leaving the stalled-peer failure wide open. The good form carries a
/// request-level `.timeout(` — the method spelling the tightened veto reads — on a client the rule
/// would otherwise flag.
pub async fn ping_status(url: &str) -> Result<String, reqwest::Error> {
    reqwest::get(url).await?.text().await
}

pub async fn fetch_profile_connect_timeout_only(url: &str) -> Result<String, reqwest::Error> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(2))
        .build()?;
    client.get(url).send().await?.text().await
}

pub async fn good_fetch_profile_request_timeout(url: &str) -> Result<String, reqwest::Error> {
    let client = reqwest::Client::new();
    client
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?
        .text()
        .await
}
