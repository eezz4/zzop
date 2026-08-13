//! The RETRIEVAL stamp a pack carries when it came out of a shipped binary's contract lane.
//!
//! ## The defect this exists for
//! A pack exported out of the bundle (`examples/packs/*.json`) is retrieved by name from any binary
//! (`example-pack-<stem>`, served over MCP `resources/read` and by the CLI's contract lane), saved to
//! `<tree>/zzop/rules/`, and then it just sits there. Its rules are the ones that build shipped —
//! forever. Two ways of going stale already fail LOUD: an unknown `${NAME}` fragment fails the load,
//! and a `schema_version` newer than the engine is rejected by name. Everything else was undetectable:
//! a copy taken from a v0.30 binary keeps running its v0.30 rule set under a v0.40 engine with no
//! surface saying so, because nothing in the file recorded where it came from.
//!
//! ## Why this is NOT `schema_version`
//! `schema_version` is the pack FORMAT's version: it says which shape the file is written against, it
//! gates loading (`pack_loader::SUPPORTED_DSL_SCHEMA_VERSION`), and it moves only when the matcher/pack
//! shape does — which is almost never. This says which zzop BUILD handed the file over. It gates
//! nothing and can never reject a pack; the only thing it buys is that the drift becomes *sayable*.
//!
//! ## Why nothing in the repository carries one
//! The stamp is minted at RETRIEVAL, by `crates/config/build.rs`, into the bytes the contract resource
//! serves — the committed `examples/packs/*.json` files stay byte-identical. A value written into the
//! committed file would have to be re-typed at every release to stay true, and this repository has an
//! unbroken record of losing that bet. `CARGO_PKG_VERSION` (the `[workspace.package] version` in
//! Cargo.toml, this repo's version SSOT) is read at compile time instead, so the stamp cannot lag.
//!
//! ## Absent means silent
//! A hand-written pack, or one copied out of a source checkout, has no stamp and gets no complaint.
//! Nobody derived a provenance for it, so there is nothing to compare against — inventing one would be
//! the same guess this module exists to replace. See `pack_loader::pack_export_staleness` for the
//! judgment side.

use serde::Deserialize;

/// Where a retrieved pack came from — the value of `RulePackDef::exported_from`.
///
/// Both fields are REQUIRED once the object is present: a stamp naming a version but not the resource
/// to re-fetch from states a problem and withholds the fix, and a stamp naming a resource but no
/// version cannot be compared against anything. Absent altogether is the supported "no provenance"
/// state (see the module doc); a half-filled stamp is not.
#[derive(Debug, Clone, Deserialize)]
pub struct PackExport {
    /// The `zzop` version whose binary served these bytes — `CARGO_PKG_VERSION` at the time the
    /// serving build was compiled. Compared against the RUNNING engine's own version, as a plain
    /// string: any difference at all is reported, including a patch-level one. Deliberately not a
    /// release-line (major.minor) comparison — a rule moving between packs is an ordinary commit here,
    /// not a minor-version event, so a comparison that ignored the patch digit would be silent about
    /// exactly the drift it was built to name.
    pub zzop_version: String,
    /// The contract-resource name these bytes were served under (`example-pack-<stem>`), carried so the
    /// remedy can be exact. It is NOT derivable from the pack `id`: `examples/packs/typescript-lint.json`
    /// declares `"id": "typescript"`, so a message that assembled `example-pack-<id>` would name a
    /// resource that does not resolve.
    pub contract: String,
}
