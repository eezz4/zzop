//! The retrieval-stamp judgment: a loaded pack that a DIFFERENT zzop build handed over.
//!
//! The stamp itself is [`crate::dsl::def::PackExport`] — read that module for what it records, why it
//! is not a second `schema_version`, and why no committed file carries one. This module is the reader:
//! it is the whole reason the stamp is worth minting, because a provenance nobody compares against is
//! decoration.
//!
//! ## What it compares
//! The stamp's `zzop_version` against THIS build's own `CARGO_PKG_VERSION` — both sides read the one
//! version SSOT (`[workspace.package] version` in Cargo.toml), one at the serving build's compile time
//! and one at this build's. Any difference is reported; equality and absence are both silent.
//!
//! ## What it is NOT allowed to do
//! Reject, skip, or downgrade the pack. A retrieved pack is a perfectly loadable pack, and a user who
//! deliberately pins an older rule set is doing something legitimate — the tolerance contract
//! (`docs/contracts/rule-pack.schema.json`'s TOLERANCE CONTRACT comment) is what lets that copy keep
//! working across engine versions in the first place. The only defect being named is the SILENCE, so
//! the only fix is a sentence.

use crate::dsl::RulePackDef;

/// One warning line for a pack whose retrieval stamp names a zzop version other than this build's, or
/// `None` when the versions match or the pack carries no stamp.
///
/// Read `env!("CARGO_PKG_VERSION")` inline rather than through a named constant: this is not policy
/// vocabulary with an axis to triage, it is the build's own identity, and every other crate that needs
/// it (`zzop_facade::version`, both hosts' `serverInfo`) reads it the same way.
///
/// ## The message shape, and what it refuses to say
/// It does not say "outdated", does not prescribe an upgrade as the only move, and does not claim any
/// rule is missing — this layer knows the two version strings and nothing else about what changed
/// between them. It states the fact, names the consequence class (rules move between packs, and in and
/// out of the bundle, between releases), gives the exact resource to re-fetch from, and says out loud
/// that keeping the copy is a valid choice. Host-neutral by construction: it names a contract DOCUMENT,
/// never a command line, because the identical sentence reaches an MCP client with no argv.
pub fn pack_export_staleness(pack: &RulePackDef) -> Option<String> {
    let export = pack.exported_from.as_ref()?;
    let running = env!("CARGO_PKG_VERSION");
    if export.zzop_version == running {
        return None;
    }
    Some(format!(
        "pack \"{}\" was retrieved from zzop {} and this engine is {} — it still carries the rule set \
         that build served, and rules move between packs (and in and out of the bundle) from release to \
         release, so this copy can be missing rules this engine ships and running rules it has since \
         dropped. Nothing is skipped and nothing is wrong with the file. To take the current bytes, \
         re-read the \"{}\" contract document and overwrite this pack; to stay on the older rule set \
         deliberately, ignore this line — it names a difference, not an error.",
        pack.id, export.zzop_version, running, export.contract
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_with_stamp(stamp: &str) -> RulePackDef {
        let json = format!(
            r#"{{"id":"orm-eager"{stamp},"rules":[{{"id":"r","severity":"warning","message":"m",
               "matcher":{{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"x"}}}}]}}"#
        );
        crate::parse_dsl_pack(&json).expect("fixture pack must load")
    }

    /// The whole point: a copy from another build SAYS SO, and the line carries both versions plus the
    /// resource that answers it. Without all three the reader learns there is a problem and not what to
    /// do about it.
    #[test]
    fn a_pack_stamped_by_another_build_is_named_with_both_versions_and_its_resource() {
        let pack = pack_with_stamp(
            r#","exported_from":{"zzop_version":"0.1.0","contract":"example-pack-orm-eager"}"#,
        );
        let warning = pack_export_staleness(&pack).expect("a differing stamp must be reported");
        assert!(warning.contains("orm-eager"), "{warning}");
        assert!(
            warning.contains("0.1.0"),
            "names the serving build: {warning}"
        );
        assert!(
            warning.contains(env!("CARGO_PKG_VERSION")),
            "names the running build: {warning}"
        );
        assert!(
            warning.contains("example-pack-orm-eager"),
            "names the resource to re-fetch from: {warning}"
        );
    }

    /// The non-vacuous leg. A function that reported every stamped pack would pass the test above and
    /// turn every correctly-retrieved pack into noise, which is how a real signal gets tuned out.
    #[test]
    fn a_pack_stamped_by_this_very_build_is_silent() {
        let stamp = format!(
            r#","exported_from":{{"zzop_version":"{}","contract":"example-pack-orm-eager"}}"#,
            env!("CARGO_PKG_VERSION")
        );
        assert!(pack_export_staleness(&pack_with_stamp(&stamp)).is_none());
    }

    /// Absence is not staleness. Every bundled pack and every hand-written one arrives here unstamped;
    /// complaining about them would drown the one case that means something.
    #[test]
    fn an_unstamped_pack_is_silent() {
        assert!(pack_export_staleness(&pack_with_stamp("")).is_none());
    }

    /// A half-filled stamp is a load failure, not a silently-ignored object: both fields are required,
    /// so a stamp that cannot be compared (no version) or cannot be acted on (no resource) is refused
    /// at the door rather than degrading into a warning nobody can follow.
    #[test]
    fn a_stamp_missing_either_field_fails_the_load() {
        for partial in [
            r#","exported_from":{"zzop_version":"0.1.0"}"#,
            r#","exported_from":{"contract":"example-pack-orm-eager"}"#,
        ] {
            let json = format!(
                r#"{{"id":"p"{partial},"rules":[{{"id":"r","severity":"warning","message":"m",
                   "matcher":{{"type":"line-scan","file_pattern":"x","line_pattern":"x"}}}}]}}"#
            );
            assert!(
                crate::parse_dsl_pack(&json).is_err(),
                "half a stamp must not load: {partial}"
            );
        }
    }
}
