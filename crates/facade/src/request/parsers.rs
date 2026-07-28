//! Parser routing — the request-side shape of the config's `parsers` object.
//!
//! Split out of `request.rs` on 2026-07-27 to stay under the repo's per-file line cap. A natural seam:
//! these two types are the only ones in the request surface that describe HOW a file is read rather
//! than WHAT to analyze or WHICH judgments to make.

use serde::Deserialize;

/// Parser routing, the request-side shape of the config's `parsers` object.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParsersRequest {
    /// `{ glob, language }` entries, applied in order — first match wins, ahead of the extension map.
    /// An entry naming a language this build does not have is skipped with a warning rather than
    /// failing the run: an unknown language is a config-authoring mistake, and the run's other trees
    /// still have honest answers to give.
    pub glob_overrides: Vec<GlobOverrideRequest>,
}

/// One `parsers.globOverrides[]` entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobOverrideRequest {
    pub glob: String,
    pub language: String,
}
