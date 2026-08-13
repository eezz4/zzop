//! Tests for the shipped contract-doc table (`contracts::CONTRACT_DOCS`).
//!
//! Lives outside `contracts.rs` for the repo's per-file line cap: the source file carries the shipped
//! table, its tests pair with it from the outside.

use crate::contracts::CONTRACT_DOCS;

/// The `rule-catalog` description hardcodes the bundled DSL pack COUNT, and this exact string ships
/// over MCP `resources/list` — a reader's only pack-count signal without a source checkout. Nothing
/// checked it: it read "14" here and "15" in `docs/modules/mcp.md` while the truth was 12 (found in
/// review, 2026-07-24), the same hardcoded-inventory class as the "2 -> 44" security-rule miscount one
/// commit earlier. Pinned to the one compile-time truth so the count cannot drift again.
#[test]
fn rule_catalog_description_states_the_real_bundled_pack_count() {
    let doc = CONTRACT_DOCS
        .iter()
        .find(|d| d.name == "rule-catalog")
        .expect("the rule-catalog contract doc must exist");
    let expected = format!("({} DSL packs", zzop_config::BUNDLED_PACK_SOURCES.len());
    assert!(
        doc.description.contains(&expected),
        "rule-catalog description must state `{expected}`; it reads: {}",
        doc.description
    );
}

/// Same class as the pack-count pin above, caught the same way: this description also spells the
/// SUPPRESS MARKER form, and that string ships over MCP `resources/list` — for an agent client it is
/// the only marker spelling available without a source checkout. The 2026-07-26 `zzop-` prefix batch
/// migrated every doc and message but missed this one, so `resources/list` kept advertising the old
/// bare form while the engine had stopped honoring it: an agent following the resource would write a
/// marker that silently does not suppress. Pinned to the derivation itself, not to a literal, so a
/// future prefix change cannot leave the shipped description behind.
#[test]
fn rule_catalog_description_spells_the_real_derived_suppress_marker_form() {
    let doc = CONTRACT_DOCS
        .iter()
        .find(|d| d.name == "rule-catalog")
        .expect("the rule-catalog contract doc must exist");
    // Derive from the rule kernel's own function so the pin tracks the code, not a copy of it.
    // (`zzop-core` is a DEV-dependency of this crate for exactly this derivation — the shipped code
    // here needs nothing below `zzop-config`.)
    let marker = zzop_core::RuleDef::suppress_marker_for_id("<rule id>");
    assert!(
        doc.description.contains(&marker),
        "rule-catalog description must spell the derived marker `{marker}`; it reads: {}",
        doc.description
    );
}

/// Third instance of the same class, and the reason this one is DERIVED rather than pinned to a
/// literal: two descriptions enumerate the matcher kinds behind a totality quantifier ("every
/// matcher", "the matcher kinds"), and a closed enumeration does not grow when its set does. It
/// already failed once — `literal-scan` shipped in v0.29.0 with rules using it, `resources/list` kept
/// telling every MCP client there were five, and a pack author reading the catalog to learn the
/// vocabulary was told the sixth did not exist (found in the v0.29.0 release audit).
///
/// The subject set is read off `Matcher`'s own variant list rather than written here, so adding a
/// seventh matcher turns these descriptions red instead of leaving them quietly short
/// (working-agreements §5.5①). The floor guards the extraction itself: if the regex ever stops
/// matching, an empty subject set would make this test vacuously green (§5.5③).
#[test]
fn every_matcher_kind_appears_in_the_descriptions_that_claim_to_list_them_all() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/src/dsl/def/matcher.rs"),
    )
    .expect("zzop-core's matcher module is readable from this workspace");
    let body = src
        .split_once("pub enum Matcher {")
        .expect("`pub enum Matcher {` must exist — the derivation anchors on it")
        .1
        .split_once("\n}")
        .expect("the enum body must be brace-terminated")
        .0;
    let kinds: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter_map(|l| l.split_once('('))
        .map(|(variant, _)| variant)
        .filter(|v| {
            v.chars().next().is_some_and(char::is_uppercase) && v.chars().all(char::is_alphanumeric)
        })
        .map(kebab)
        .collect();
    assert!(
        kinds.len() >= 5,
        "extraction floor: expected at least the 5 pre-v0.29.0 matcher kinds, got {kinds:?} — the \
         regexless split above stopped seeing variants, which would make this pin vacuously green"
    );
    for name in ["dsl-reference", "rule-pack-schema"] {
        let doc = CONTRACT_DOCS
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("the {name} contract doc must exist"));
        for kind in &kinds {
            assert!(
                doc.description.contains(kind.as_str()),
                "the `{name}` description claims to list every matcher but omits `{kind}` — this \
                 string ships over MCP `resources/list` and `zzop contract`, so the omission tells \
                 every client without a checkout that the matcher does not exist. It reads: {}",
                doc.description
            );
        }
    }
}

/// `Matcher`'s serde tag is `rename_all = "kebab-case"`, so a variant's wire spelling is derivable.
fn kebab(variant: &str) -> String {
    let mut out = String::new();
    for (i, c) in variant.char_indices() {
        if c.is_uppercase() {
            if i != 0 {
                out.push('-');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The RETRIEVAL channel, pinned end to end at the table level: every exported pack in the repository
/// must be servable by name, and what comes out must be the pack itself.
///
/// The derivation is what needs pinning, not the one row that exists today. `crates/config/build.rs`
/// walks `examples/packs/*.json`; if that walk breaks — a moved directory, a changed extension filter
/// — the table simply gets shorter, the binary still builds, and the retrieval path goes back to the
/// 404 it was before 2026-08-12 with nothing saying so. A floor of one is the smallest assertion that
/// catches it, and `scripts/check-embedded-contract-docs.sh` covers the other direction (a pack baked
/// but absent from VERSIONING.md's release list).
#[test]
fn every_exported_pack_is_served_as_a_contract_resource_carrying_a_loadable_pack() {
    assert!(
        !zzop_config::EXAMPLE_PACK_CONTRACTS.is_empty(),
        "no `examples/packs/*.json` was baked — crates/config/build.rs's walk found nothing, so the \
         retrieval path for an exported rule is a 404 again. Re-point that walk in the same commit as \
         whatever moved the directory."
    );

    for (name, description, content) in zzop_config::EXAMPLE_PACK_CONTRACTS {
        let stem = name
            .strip_prefix("example-pack-")
            .unwrap_or_else(|| panic!("{name} must be addressable as example-pack-<stem>"));
        let doc = CONTRACT_DOCS
            .iter()
            .find(|d| d.name == *name)
            .unwrap_or_else(|| panic!("{name} is baked but absent from the served table"));
        assert_eq!(doc.mime, "application/json");
        assert_eq!(
            doc.content, *content,
            "the served bytes must be the pack's own"
        );

        // The bytes a reader saves must be a pack the loader accepts — otherwise the resource is a
        // pointer to a file that cannot do the job it is offered for.
        let pack = zzop_core::parse_dsl_pack(content)
            .unwrap_or_else(|e| panic!("{name} does not parse as a rule pack: {e}"));
        assert!(
            !pack.rules.is_empty(),
            "{name} carries zero rules — retrieving it would restore nothing"
        );

        // The description has to name the FILE STEM, because the instruction it gives is "save these
        // bytes as <stem>.json"; a description that names the wrong file sends the reader to a path
        // the loader will not read.
        assert!(
            description.contains(&format!("{stem}.json")),
            "{name}'s description must name the filename to save it as: {description}"
        );
    }
}

/// The RETRIEVAL STAMP, pinned where it is minted: every pack this binary serves must carry an
/// `exported_from` naming THIS build and THIS resource, and the committed file must carry none.
///
/// Both directions are load-bearing. Without the first, `crates/config/build.rs` could stop splicing —
/// or splice a value that no longer tracks `CARGO_PKG_VERSION` — and every retrieved pack would go back
/// to being undatable, with `parse_dsl_pack` still green because the stamp is an additive field nothing
/// requires. Without the second, someone "helpfully" writing the stamp into `examples/packs/*.json`
/// would create exactly the hand-maintained version string the stamp was designed around: `examples/`
/// sits outside `scripts/check-release-version-propagation.sh`'s subject, so it would rot unwatched.
///
/// The pack-id assertion is the one that catches the tempting shortcut: `typescript-lint.json` declares
/// `"id": "typescript"`, so a resource name assembled from the pack id instead of the file stem would
/// send a reader to a name that does not resolve.
#[test]
fn every_served_pack_is_stamped_with_this_build_and_no_committed_pack_is() {
    for (name, _, content) in zzop_config::EXAMPLE_PACK_CONTRACTS {
        let pack = zzop_core::parse_dsl_pack(content)
            .unwrap_or_else(|e| panic!("{name} does not parse as a rule pack: {e}"));
        let export = pack.exported_from.as_ref().unwrap_or_else(|| {
            panic!(
                "{name} is served without an `exported_from` stamp — a copy saved from this binary is \
                 then undatable, and the staleness warning it exists to enable can never fire"
            )
        });
        assert_eq!(
            export.zzop_version,
            env!("CARGO_PKG_VERSION"),
            "{name}'s stamp must name the build that serves it"
        );
        assert_eq!(
            &export.contract, name,
            "{name}'s stamp must name its own resource — that string is the remedy the warning prints"
        );

        // The committed file, read from disk: the served bytes are the file PLUS the stamp, never the
        // file itself.
        let stem = name
            .strip_prefix("example-pack-")
            .expect("example-pack-<stem>");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/packs")
            .join(format!("{stem}.json"));
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        assert!(
            zzop_core::parse_dsl_pack(&committed)
                .expect("committed pack must load")
                .exported_from
                .is_none(),
            "{} carries a committed `exported_from` — that is a version string a human must retype \
             every release, in a directory no version-propagation guard watches. The stamp belongs to \
             the retrieval moment (crates/config/build.rs), not to the file.",
            path.display()
        );
        let after_brace = committed
            .strip_prefix('{')
            .expect("a pack file opens with a brace");
        assert!(
            content.ends_with(after_brace),
            "{name}'s served bytes must be the committed file with the stamp spliced in and \
             everything else passed through unchanged"
        );
    }
}
