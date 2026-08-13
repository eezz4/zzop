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

/// The INLINE-`mod` half of the same method-scan question, added 2026-08-11 with the qualified-id
/// change (`zzop_parser_rust::lang::symbols`'s module doc). A method-scan rule rides `SourceSymbol` body
/// spans, and until nested items got a symbol of their own the entire body of an inline `mod` sat in no
/// span at all — so this exact `Command::new` + `format!` pair, which scores at the top of this file,
/// was invisible here. It is NOT a duplicate of `run_report`: deleting the `Item::Mod` walk from
/// `symbols.rs` leaves that anchor scoring and takes only this one away.
///
/// The good twin sits in the same module, so the module cannot be cleared wholesale either.
pub mod legacy {
    use std::process::Command;

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
}

/// The `trait`-DEFAULT half of the same method-scan question, added 2026-08-13. A default body is code
/// that runs unless an impl overrides it, and until `zzop_parser_rust::lang::symbols` walked a trait's
/// associated items it emitted only the trait's own span-less `Interface` symbol — so this exact
/// `Command::new` + `format!` pair sat in no span at all, while the identical one at file top level
/// (`run_report`, :11) and the one inside `mod legacy` (:76) both scored. Reverting the trait walk takes
/// this anchor and leaves those two.
///
/// Two things beside it are deliberate rather than decorative. `label` is a BODY-LESS signature: it
/// encloses nothing, so it must carry no span at all — the span contract's own worked example of what
/// `None` claims. And `good_run_report` is a default of the SAME trait, so giving the trait one span
/// over its whole item list instead of a leaf per method does not pass here either: the two bodies
/// would share a region, and the pair would report at the `trait` line rather than at this one.
pub trait Reporting {
    fn label(&self) -> &str;

    fn run_report(&self, period: &str) -> std::io::Result<std::process::Output> {
        let script = format!("/usr/local/bin/report --period {}", period);
        Command::new("sh").arg("-c").arg(script).output()
    }

    fn good_run_report(&self, period: &str) -> std::io::Result<std::process::Output> {
        Command::new("/usr/local/bin/report")
            .arg("--period")
            .arg(period)
            .output()
    }
}
