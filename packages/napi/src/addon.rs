//! The `#[napi]`-exported surface — compiled only under the `addon` feature (MSVC builds; see
//! `lib.rs`'s module doc). Each export is a thin shim over `crate::api`: parse the `String`
//! argument, run the panic-caught `api::*` call, map `Result<String, String>` to napi's
//! `Result<String>`.
//!
//! ## Never a panic across the FFI boundary
//! `std::panic::catch_unwind` wraps every call here, one layer outside `api.rs`'s own internal
//! `catch_unwind` (the engine's per-file pass already isolates a single bad file — see
//! `zzop_engine`'s crate doc). Unwinding across a `#[napi]`-exported `extern "C"` function is
//! undefined behavior, so a caught panic becomes an ordinary JS `Error` instead.
use napi::{Error, Result};
use napi_derive::napi;

fn catch<F: FnOnce() -> std::result::Result<String, String> + std::panic::UnwindSafe>(
    f: F,
) -> Result<String> {
    match std::panic::catch_unwind(f) {
        Ok(Ok(json)) => Ok(json),
        Ok(Err(message)) => Err(Error::from_reason(message)),
        Err(_) => Err(Error::from_reason(
            "zzop-napi: internal panic (this is a bug — please report it)".to_string(),
        )),
    }
}

/// `analyze(configJson: string) -> string`. See `crate::api::analyze_json` for the config shape
/// and error modes.
#[napi]
pub fn analyze(config_json: String) -> Result<String> {
    catch(move || crate::api::analyze_json(&config_json))
}

/// `analyzeTrees(configJson: string) -> string` — multi-tree/cross-layer analysis. See
/// `crate::api::analyze_trees_json`.
#[napi(js_name = "analyzeTrees")]
pub fn analyze_trees(config_json: String) -> Result<String> {
    catch(move || crate::api::analyze_trees_json(&config_json))
}

/// `analyzeEnvelope(envelopeJson: string, configJson: string) -> string` — the
/// `docs/NORMALIZED_AST.md` external-parser protocol receiver. See
/// `crate::api::analyze_envelope_json` for the envelope/config shapes and error modes.
#[napi(js_name = "analyzeEnvelope")]
pub fn analyze_envelope(envelope_json: String, config_json: String) -> Result<String> {
    catch(move || crate::api::analyze_envelope_json(&envelope_json, &config_json))
}

/// `version() -> string` — crate version + parser fingerprints (diagnostics). See
/// `crate::api::version_string`.
#[napi]
pub fn version() -> String {
    crate::api::version_string()
}
