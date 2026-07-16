//! Compile-time embedded authoring contracts — the documents a custom-parser or rule author needs,
//! served over MCP `resources/*` as `zzop://contract/<name>`. Embedding (vs. reading from disk) is
//! what makes the "author an adapter with only the binary" promise hold: no zzop source checkout, no
//! sidecar files, no install-location assumptions. All sources are committed, English, CI-guarded repo
//! files — the public docs plus the machine-verified config-surface vocabulary — ~130KB total.

/// One embedded contract document.
pub struct ContractDoc {
    /// URI tail: the resource is addressed as `zzop://contract/<name>`.
    pub name: &'static str,
    /// One-line human/agent description shown in `resources/list`.
    pub description: &'static str,
    pub mime: &'static str,
    pub content: &'static str,
}

/// Looks up an embedded contract document by its `<name>` (the `zzop://contract/<name>` URI tail).
/// The ONE lookup both surfaces share — the MCP `resources/read` handler (`crate::resources`) and the
/// `zzop-mcp contract <name>` CLI path (`main.rs`) resolve names through this function, so the two
/// surfaces cannot drift on which names exist.
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
        description: "JSON Schema (draft-07) for the DSL rule-pack shape — pack id, rules[], the four matcher kinds (line-scan, method-scan, symbol-scan, io-scan), severity; every property documented. Machine-check a pack with the validate_rule_pack tool (structure only — the same loader judgments, never rule-quality semantics).",
        mime: "application/json",
        content: include_str!("../../../docs/contracts/rule-pack.schema.json"),
    },
    ContractDoc {
        name: "example-envelope",
        description: "Minimal valid Mode-A envelope example (a crude JSP parser's output) — the smallest starting point for a custom parser.",
        mime: "application/json",
        content: include_str!("../../../examples/jsp-envelope.example.json"),
    },
    ContractDoc {
        name: "config-surface",
        description: "Machine-verified config vocabulary — every config key, dotted path, CLI flag, and embedder field zzop accepts (the purpose/configKeys/configPaths/embedderFields sections self-describe). Usage: config lives in zzop.config.jsonc at the repo root; multi-tree analysis declares trees[] (or trees: \"auto\"), where one DB/schema directory joins as its own tree; unknown keys warn, never fail.",
        mime: "application/json",
        // Reused from `zzop-config` (this crate already depends on it), which embeds the same
        // `packages/cli/lib/config-surface.json` for unknown-key warnings — one embed, one truth.
        content: zzop_config::CONFIG_SURFACE_JSON,
    },
];
