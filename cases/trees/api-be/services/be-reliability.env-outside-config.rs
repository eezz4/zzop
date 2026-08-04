// be-reliability/env-outside-config (Rust lane) — bad: two runtime environment reads, each keeping
// its own spelling (std::env::var and the use-qualified env::var are the same function and stay two
// callees — the channel's original-spelling contract). good: env!(), which resolves at COMPILE time
// and reads no process environment at run time — the channel constant's own named boundary, so no
// site exists there at all.
use std::env;

fn bad_dsn() -> String {
    std::env::var("DATABASE_URL").unwrap_or_default()
}

fn bad_port() -> Result<String, env::VarError> {
    env::var("PORT")
}

fn good_compile_time() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
