//! Only wires up napi-rs's linker setup when the `addon` feature is active. Under the workspace's default
//! `x86_64-pc-windows-gnu` toolchain (no `addon` feature -> `napi-build` is not even a build-dependency —
//! see `Cargo.toml`'s `[features]`), this is a no-op build script. See `src/lib.rs`'s module doc for why.
fn main() {
    #[cfg(feature = "addon")]
    napi_build::setup();
}
