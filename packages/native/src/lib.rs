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
//! cdylib links normally.
//!
//! The actual `analyze` / `analyzeTrees` / `version` logic does not live in this crate at all: it is
//! `zzop-facade` (`crates/facade/src/lib.rs`), a separate napi-free `rlib` crate re-exported below
//! so the JS surface and any Rust caller of `zzop_napi::*` see zero change. It was split out because
//! cargo builds a dependency's `cdylib` target even on an `rlib` dependency edge, and this crate's
//! `cdylib` half fails to link under gnu once `#[napi]` exports are compiled in — see
//! `zzop_facade`'s crate doc for the full reasoning. `addon.rs` (feature `addon` only) is a thin
//! `#[napi]` pass-through to `zzop_facade`'s functions.
//!
//! The real addon builds MSVC-only:
//! ```text
//! cargo +stable-x86_64-pc-windows-msvc build -p zzop-napi --release --features addon
//! ```
//! `node packages/native/smoke.mjs` exercises the built `.node` afterward.

#[cfg(feature = "addon")]
mod addon;

pub use zzop_facade::{
    analyze_envelope_json, analyze_json, analyze_trees_json, query_io_json,
    validate_envelope_only_json, validate_rule_pack_json, version_string,
};
