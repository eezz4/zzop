//! Compile-time embedded authoring contracts — the documents a custom-parser or rule author needs,
//! served over MCP `resources/*` as `zzop://contract/<name>` and printed by `zzop contract [<name>]`.
//! Embedding (vs. reading from disk) is
//! what makes the "author an adapter with only the binary" promise hold: no zzop source checkout, no
//! sidecar files, no install-location assumptions. All sources are committed, English, CI-guarded repo
//! files — the public docs plus the machine-verified config-surface vocabulary and the rule catalog —
//! ~180KB total.
//!
//! Why this table lives in the SHAPING crate rather than in either product: both surfaces resolve
//! `<name>` through it, so it is a host-shared answer like every other module here, and the embed table
//! keeps the established "reference, never re-own" discipline — the `config-surface` row points at
//! `zzop_config::CONFIG_SURFACE_JSON` instead of embedding the same bytes a second time.

/// One embedded contract document.
pub struct ContractDoc {
    /// URI tail: the resource is addressed as `zzop://contract/<name>`.
    pub name: &'static str,
    /// One-line human/agent description shown in `resources/list`.
    pub description: &'static str,
    pub mime: &'static str,
    pub content: &'static str,
}

/// The `<name>` of the starter-config document, and the filename a host writes it to — the two values
/// `zzop init` needs, kept here rather than spelled in the CLI for the reason the whole table exists:
/// the products reach every shared answer through this crate, and neither ships a dependency below it.
/// The filename is re-exported from its one owner (the config front end discovers exactly this name),
/// so the file `init` writes and the file a run looks for can never drift apart.
pub const CONFIG_TEMPLATE_NAME: &str = "config-template";
pub const CONFIG_TEMPLATE_FILENAME: &str = zzop_config::DEFAULT_CONFIG_FILENAME;

/// Looks up an embedded contract document by its `<name>` (the `zzop://contract/<name>` URI tail).
/// The ONE lookup both surfaces share — the MCP `resources/read` handler (package `zzop-mcp`'s
/// `resources.rs`) and the `zzop contract <name>` CLI path (package `zzop-cli-bin`'s `main.rs`) resolve
/// names through this function, so the two surfaces cannot drift on which names exist. Both reach it as
/// `zzop_summary::contracts::find`.
pub fn find(name: &str) -> Option<&'static ContractDoc> {
    CONTRACT_DOCS.iter().find(|doc| doc.name == name)
}

/// Every embedded contract name, in `CONTRACT_DOCS` (= `resources/list`) order — the shared "valid
/// names" vocabulary both the unknown-URI resource error and the unknown-name CLI error enumerate.
pub fn names() -> impl Iterator<Item = &'static str> {
    CONTRACT_DOCS.iter().map(|doc| doc.name)
}

/// Every contract resource this binary serves. Order is the `resources/list` order (deterministic).
pub static CONTRACT_DOCS: &[ContractDoc] = &[
    ContractDoc {
        name: "envelope-schema",
        description: "JSON Schema (draft-07) for the Normalized AST envelope v1 — machine-validate a custom parser's output.",
        mime: "application/json",
        content: include_str!("../../../docs/adapters/envelope.schema.json"),
    },
    ContractDoc {
        name: "envelope-guide",
        description: "The Normalized AST envelope contract: Mode A (full envelope) / Mode B (overlay) adapter authoring, field semantics, worked examples.",
        mime: "text/markdown",
        content: include_str!("../../../docs/NORMALIZED_AST.md"),
    },
    ContractDoc {
        name: "key-normalization-fixture",
        description: "Byte-pinned HTTP key-normalization fixture — the exact (method, path) -> join-key rows an adapter must reproduce for cross-layer joins.",
        mime: "application/json",
        content: include_str!("../../../docs/adapters/key-normalization.fixture.json"),
    },
    ContractDoc {
        name: "adapter-guide",
        description: "Adapter authoring README: key-normalization parity rules, schema/versioning policy, adapter-kit pointers.",
        mime: "text/markdown",
        content: include_str!("../../../docs/adapters/README.md"),
    },
    ContractDoc {
        name: "dsl-reference",
        description: "DSL rule-pack reference: pack/rule fields and all four matchers (line-scan, method-scan, symbol-scan, io-scan).",
        mime: "text/markdown",
        content: include_str!("../../../docs/rules/dsl-reference.md"),
    },
    ContractDoc {
        name: "dsl-authoring-guide",
        description: "DSL rule authoring guide: placement, a worked example pack, testing conventions, recurring defect checklist, when a rule does NOT fit the DSL.",
        mime: "text/markdown",
        content: include_str!("../../../docs/rules/authoring-guide.md"),
    },
    ContractDoc {
        name: "rule-pack-schema",
        description: "JSON Schema (draft-07) for the DSL rule-pack shape — pack id, rules[], the four matcher kinds (line-scan, method-scan, symbol-scan, io-scan), severity; every property documented. Machine-check a pack with the rule-pack validator (structure only — the same loader judgments, never rule-quality semantics).",
        mime: "application/json",
        content: include_str!("../../../docs/contracts/rule-pack.schema.json"),
    },
    ContractDoc {
        name: "example-envelope",
        description: "Minimal valid Mode-A envelope example (a crude JSP parser's output) — the smallest starting point for a custom parser.",
        mime: "application/json",
        content: include_str!("../../../docs/contracts/example-envelope.json"),
    },
    ContractDoc {
        name: "config-surface",
        description: "Machine-verified config vocabulary — every config key, dotted path, CLI flag, and embedder field zzop accepts (the purpose/configKeys/configPaths/embedderFields sections self-describe). Usage: config lives in zzop.config.jsonc at the repo root; multi-tree analysis declares trees[] (or trees: \"auto\"), where one DB/schema directory joins as its own tree; unknown keys warn, never fail.",
        mime: "application/json",
        // Reused from `zzop-config` (this crate already depends on it), which embeds the same
        // `crates/config/config-surface.json` for unknown-key warnings — one embed, one truth.
        content: zzop_config::CONFIG_SURFACE_JSON,
    },
    ContractDoc {
        name: CONFIG_TEMPLATE_NAME,
        description: "Annotated starter zzop.config.jsonc: every optional key with a comment saying what it means, set to the value a config-less run already uses (so writing it changes nothing). Usage: save these exact bytes as zzop.config.jsonc at the tree root; the vocabulary it draws from is the config-surface resource.",
        mime: "application/jsonc",
        // Owned by `zzop-config` (which also machine-checks every key it names against
        // `config-surface.json`), referenced here — the same one-embed-one-truth rule as the
        // `config-surface` row above.
        content: zzop_config::template::CONFIG_TEMPLATE_JSONC,
    },
    ContractDoc {
        name: "rule-catalog",
        description: "Every rule id the engine ships today (12 DSL packs + all native analysis ids), with severity/matcher/detection prose per rule (a DSL rule's suppress marker is derived, `zzop-<rule id>-ok`) — the ONE place a rule id can be looked up without a source checkout. Pair with the `rule` findings filter every analysis surface takes (an id absent here never fires) and the dsl-reference resource for matcher semantics.",
        mime: "text/markdown",
        content: include_str!("../../../docs/rules/catalog.md"),
    },
];

#[cfg(test)]
mod tests {
    use super::CONTRACT_DOCS;

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
}
