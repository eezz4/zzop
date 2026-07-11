//! zzop-napi — the Node<->Rust boundary (N-API binding). Builds the addon itself, plus the
//! loader/smoke test a prebuild pipeline wraps (see `docs/modules/napi.md`).
//!
//! ## Feature gating: `addon` is off by default
//! napi-rs's `#[napi]` exports reference Node-API C symbols (`napi_get_undefined`,
//! `napi_create_string_utf8`, ...) that only exist inside a running `node` process. On Windows
//! these resolve via a delay-load import table that `napi-build`'s `build.rs` wires into the MSVC
//! linker (`link.exe /DELAYLOAD:node.exe`); the `x86_64-pc-windows-gnu` toolchain has no equivalent
//! mechanism, so a cdylib with `#[napi]` exports compiled in fails to link under gnu with
//! unresolved `napi_*` externs.
//!
//! Since `cargo test --workspace` must pass under gnu, the napi dependencies (`napi`,
//! `napi-derive`, `napi-build`) are optional, gathered under a default-off `addon` feature (see
//! `Cargo.toml`). Without it, `mod addon` isn't compiled — no `napi_*` symbol references, so the
//! cdylib links normally — while `mod api` (plain Rust, no napi types) still compiles and runs its
//! `#[cfg(test)]` module under gnu like any other workspace crate.
//!
//! The real addon builds MSVC-only:
//! ```text
//! cargo +stable-x86_64-pc-windows-msvc build -p zzop-napi --release --features addon
//! ```
//! `node packages/napi/smoke.mjs` exercises the built `.node` afterward.

#[cfg(feature = "addon")]
mod addon;
mod api;

pub use api::{
    analyze_envelope_json, analyze_json, analyze_trees_json, validate_envelope_only_json,
    version_string,
};
