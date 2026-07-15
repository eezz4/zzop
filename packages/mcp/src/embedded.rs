//! Compile-time embedded authoring contracts — the documents a custom-parser or rule author needs,
//! served over MCP `resources/*` as `zzop://contract/<name>`. Embedding (vs. reading from disk) is
//! what makes the "author an adapter with only the binary" promise hold: no zzop source checkout, no
//! sidecar files, no install-location assumptions. All sources are the repo's committed public docs
//! (English, CI-guarded), ~105KB total.

/// One embedded contract document.
pub struct ContractDoc {
    /// URI tail: the resource is addressed as `zzop://contract/<name>`.
    pub name: &'static str,
    /// One-line human/agent description shown in `resources/list`.
    pub description: &'static str,
    pub mime: &'static str,
    pub content: &'static str,
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
        name: "example-envelope",
        description: "Minimal valid Mode-A envelope example (a crude JSP parser's output) — the smallest starting point for a custom parser.",
        mime: "application/json",
        content: include_str!("../../../examples/jsp-envelope.example.json"),
    },
];
