//! In-binary authoring contracts — the documents a custom-parser or rule author needs, served over MCP
//! `resources/*` as `zzop://contract/<name>` and printed by `zzop contract [<name>]`.
//! Carrying them INSIDE the binary (vs. reading from disk) is
//! what makes the "author an adapter with only the binary" promise hold: no zzop source checkout, no
//! sidecar files, no install-location assumptions. Every source is committed, English and CI-guarded —
//! the public docs plus the machine-verified config-surface vocabulary and the rule catalog, ~180KB
//! embedded at compile time, plus one document rendered from a compiled-in registry (below).
//!
//! Why this table lives in the SHAPING crate rather than in either product: both surfaces resolve
//! `<name>` through it, so it is a host-shared answer like every other module here, and the embed table
//! keeps the established "reference, never re-own" discipline — the `config-surface` row points at
//! `zzop_config::CONFIG_SURFACE_JSON` instead of embedding the same bytes a second time.
//!
//! Two rows are RENDERED rather than embedded, both for the same reason and both halves of a fold:
//! `disclosure-classes`, the silent-failure-class registry's full text (`zzop_engine::
//! disclosure_contract_text`), and `reply-legends`, the four run-invariant reply legends
//! (`crate::output::legends`). Neither has a file to `include_str!`, and in both cases the reply's
//! folded half and the document are two views of ONE source, so neither can drift into claiming what
//! the other says.

use std::sync::OnceLock;

/// One embedded contract document.
#[derive(Clone)]
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

/// Re-export of the missing-config refusal's stable head (`zzop_config::MISSING_CONFIG_MARKER`), for
/// the same reason the filename above rides through here: the products depend on this crate, not on
/// `zzop-config`, and a display layer that spells the phrase itself is a copy that drifts. The CLI
/// matches on this to append its host-side `Run \`zzop init\`` line (2026-08-09 ruling — the shared
/// string stays host-neutral; each host adds its own way out).
pub const MISSING_CONFIG_MARKER: &str = zzop_config::MISSING_CONFIG_MARKER;

/// The multi-tree refusal's stable phrase, for exactly the reason the line above rides here: the
/// refusal is emitted by a crate every host shares, so it stays host-neutral — and then each host
/// has to recognize it to append its own runnable way out. That was the 2026-08-09 ruling, and it
/// was applied to the missing-config refusal only; this refusal shipped with the same
/// SPELLING-FREE comment and no host wired to it, so BOTH of its prescriptions failed when a
/// reader transcribed them (`zzop cross <dir>` exits 2; `zzop analyze <tree root>` exits 1 because the
/// declared roots carry no config of their own). Emitted by `analyze::analyze_summary_with`; matched by
/// `cli::print_or_exit` and `mcp::tools`.
pub const MULTI_TREE_MARKER: &str = "run the CROSS-LAYER JOIN over this config";

/// The same class's third member, re-exported for the same reason the two above are: the products
/// depend on this crate, not on `zzop-config`, and a display layer that spells the phrase itself is a
/// copy that drifts. Fires for PATHS mode — `cross <dirA> <dirB>` where one of those directories carries
/// a config declaring its own tree set. Its remedy was already performable ("point these paths at trees
/// whose configs declare one tree each"); what it lacked was any way to type the other one.
pub const PATHS_MODE_CONFIG_MARKER: &str = zzop_config::trees::PATHS_MODE_CONFIG_MARKER;

/// The URI space every contract document is addressed in. Owned here, next to the names it prefixes,
/// because three surfaces now spell it: MCP `resources/list`/`resources/read` (`packages/mcp`), and —
/// since the disclosure fold — every analyze-shaped reply, which prints the disclosure document's URI
/// as the pointer to the full text it stopped shipping. A pointer assembled from a second copy of this
/// prefix is a pointer that can drift.
pub const URI_PREFIX: &str = "zzop://contract/";

/// The `<name>` of the silent-failure-class document — the FULL TEXT of the registry an analyze reply
/// used to ship on every call (~10.6KB, byte-identical every run) and now folds to counts plus this
/// pointer. Named here for the same reason `CONFIG_TEMPLATE_NAME` is: the shaper that prints the
/// pointer and the table that answers it must read one constant, or the reply can name a document the
/// contract lane cannot serve.
pub const DISCLOSURE_CONTRACT_NAME: &str = "disclosure-classes";

/// The `<name>` of the reply-legends document — the FULL TEXT of the four RUN-INVARIANT legends an
/// analyze reply used to ship on every call (8,474 bytes, byte-identical across four corpus trees) and
/// now folds to a short note plus this pointer. Named here for [`DISCLOSURE_CONTRACT_NAME`]'s reason,
/// which is the whole point of a pointer: the shaper that prints it and the table that answers it read
/// ONE constant, so a reply cannot name a document this lane will not serve.
pub const REPLY_LEGENDS_CONTRACT_NAME: &str = "reply-legends";

/// The `<name>` of the framework-recognizer document — the CAPABILITY table (*"can this build read my
/// stack"*) served with no tree to walk. Named here for the reason the two constants above are: any
/// surface that points at it and the table that answers must read one constant.
pub const FRAMEWORK_RECOGNIZERS_CONTRACT_NAME: &str = "framework-recognizers";

/// The doc's bytes as a READER should receive them: a markdown contract is served with a one-line
/// provenance banner naming the build that baked it. JSON and JSONC contracts are returned untouched —
/// a banner is not valid in either, and both already carry their own version channel (the envelope
/// schema's `version`, a rule pack's spliced `exported_from`).
///
/// ## Why this exists (2026-09-07, review ledger V74)
///
/// These documents are compiled into the binary, so a reader holds whatever was baked at that release
/// and nothing in the bytes said which. Measured on v0.34.0..HEAD: `docs/rules/catalog.md` moved 153
/// lines, `config-template.jsonc` was created (+287), `authoring-guide.md` moved 43. A v0.34.0 user
/// running `zzop contract rule-catalog` holds 68-commits-stale bytes that read exactly like current
/// ones. The repository's own `docs/` is ahead of them, which is the pair that actually disagrees —
/// not "binary vs site", since the site ships on the same push as the release.
///
/// The device is not new: `examples/packs/*.json` have carried `exported_from: {zzop_version,
/// contract}` since the pack contracts were minted, for this exact reason ("a copy saved from this
/// binary is then undatable"). This is that stamp reaching the documents that had none.
///
/// It is prepended at SERVE time rather than baked, because the alternative is a version string
/// hand-written into 15 committed files — the rot this repo has a guard against.
pub fn served_content(doc: &ContractDoc) -> std::borrow::Cow<'static, str> {
    if doc.mime != "text/markdown" {
        return std::borrow::Cow::Borrowed(doc.content);
    }
    std::borrow::Cow::Owned(format!(
        "<!-- served by zzop {version} as `{uri}{name}`. These bytes are compiled INTO that build: a \
newer release may carry a different document, and this repository's own docs/ may already be ahead of \
it. Re-read this resource from the binary you are actually running. -->\n{content}",
        version = env!("CARGO_PKG_VERSION"),
        uri = URI_PREFIX,
        name = doc.name,
        content = doc.content
    ))
}

/// Looks up an embedded contract document by its `<name>` (the `zzop://contract/<name>` URI tail).
/// The ONE lookup both surfaces share — the MCP `resources/read` handler (package `zzop-mcp`'s
/// `resources.rs`) and the `zzop contract <name>` CLI path (package `zzop-cli-bin`'s `main.rs`) resolve
/// names through this function, so the two surfaces cannot drift on which names exist. Both reach it as
/// `zzop_summary::contracts::find`.
pub fn find(name: &str) -> Option<&'static ContractDoc> {
    docs().iter().find(|doc| doc.name == name)
}

/// Every embedded contract name, in `CONTRACT_DOCS` (= `resources/list`) order — the shared "valid
/// names" vocabulary both the unknown-URI resource error and the unknown-name CLI error enumerate.
pub fn names() -> impl Iterator<Item = &'static str> {
    docs().iter().map(|doc| doc.name)
}

/// The `disclosure-classes` document's text, rendered ONCE from the engine's live blindness registry
/// (`zzop_facade::disclosure_contract_text`, itself a re-export of the engine's own render) instead of
/// embedded from a committed file. It has no file to embed: the registry it describes lives in Rust,
/// and a checked-in copy would be exactly the second hand-maintained list the fold is not allowed to
/// have — the run reply's `disclosure` counts are tallied off that same registry.
fn disclosure_classes_text() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(zzop_facade::disclosure_contract_text)
}

/// The `reply-legends` document's text, rendered ONCE from the same list the reply's pointers are
/// built from (`crate::output::legends`). The second RENDERED row, for the first one's reason: there is
/// no file to embed, and a checked-in copy would be the second hand-maintained text the fold is not
/// allowed to have — the reply's short notes and this document are two views of one list.
fn reply_legends_text() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(crate::output::legends::contract_text)
}

/// The `framework-recognizers` document's text, rendered ONCE from the compiled-in recognizer
/// aggregator (`zzop_facade::framework_recognizer_contract_text`). The THIRD rendered row, for the two
/// before it's reason: there is no file to embed, and a checked-in copy would be a second
/// hand-maintained list of what this binary can read — the drift the aggregator exists to prevent.
fn framework_recognizers_text() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(zzop_facade::framework_recognizer_contract_text)
}

/// Every contract resource this binary serves, in `resources/list` order — [`EMBEDDED_DOCS`], the
/// disclosure registry's RENDERED row, the exported packs, and the reply-legends RENDERED row. Built
/// once at first use, then shared; deterministic (same binary, same list, same bytes) because both
/// non-const rows render from pinned, run-free sources.
fn docs() -> &'static [ContractDoc] {
    static DOCS: OnceLock<Vec<ContractDoc>> = OnceLock::new();
    DOCS.get_or_init(|| {
        let mut docs = EMBEDDED_DOCS.to_vec();
        docs.push(ContractDoc {
            name: DISCLOSURE_CONTRACT_NAME,
            description: "Every silent-failure class zzop knows about — the ways its own output can be silently MISREAD, each with the status of how completely zzop detects it today (asserted / partial / notYetDetected). This is the full text of the `disclosure` block every analyze reply used to ship verbatim; the reply now carries the counts plus a pointer here, so the numbers stay unmissable and the paragraphs cost nothing per call. Rendered from the engine's live registry, never a copy.",
            mime: "text/markdown",
            content: disclosure_classes_text(),
        });
        // The EXPORTED packs, one row each, DERIVED from `examples/packs/*.json` by
        // `crates/config/build.rs` rather than written out here — see that function's doc for why a
        // hand-written row per pack is a list guaranteed to rot (exporting more packs is the standing
        // plan). Appended last so every existing resource keeps its `resources/list` position: the MCP
        // handler's own doc records that an ordinal in prose about this table is unfalsifiable, and a
        // reorder would still be a gratuitous change to a published listing.
        docs.extend(
            zzop_config::EXAMPLE_PACK_CONTRACTS
                .iter()
                .map(|(name, description, content)| ContractDoc {
                    name,
                    description,
                    mime: "application/json",
                    content,
                }),
        );
        // Appended AFTER the packs for the reason stated one comment up, applied to this row too: every
        // resource that already had a position keeps it. A second rendered row that landed beside the
        // first would have shifted every exported pack by one, which is a change to a published listing
        // in exchange for reading order in a file nobody reads by ordinal.
        docs.push(ContractDoc {
            name: REPLY_LEGENDS_CONTRACT_NAME,
            description: "The run-INVARIANT legends of an analyze-shaped reply, in full: what a `coverageGaps` row is and what its zero can and cannot mean, what `findings.byRule` counts (findings, not places) and why its entries are not comparable across rules, when a native analysis's absence from `findings` is NOT a measured zero, and what each `packsLoaded` row's numbers count — including the one that means NOT ANALYZED. These sentences are byte-identical on every run and for every repository, so a reply carries this run's measurement plus a short note and points here for the vocabulary. Rendered from the same list the reply builds those notes from, never a copy.",
            mime: "text/markdown",
            content: reply_legends_text(),
        });
        // Appended LAST, the same discipline the two rows above state: every resource that already had
        // a `resources/list` position keeps it, so a published listing never shifts to buy reading order.
        docs.push(ContractDoc {
            name: FRAMEWORK_RECOGNIZERS_CONTRACT_NAME,
            description: "Every framework recognizer compiled into THIS build, grouped by the parser crate that owns the adapter, with the extensions it runs on and the cross-layer channel it fills. A fact of the BUILD, true before any tree is walked: this is the one surface that answers \"will zzop read my stack\" WITHOUT running an analysis — the same table also rides in a `coverage` reply, but that needs a tree first, and every framework-silence warning is per-run and fires only on a tree already showing the symptom. A row means the recognizer runs on those extensions, never that every idiom is modelled; an absent framework means no recognizer for it exists in this build at all. Rendered from the compiled-in registry, never a copy.",
            mime: "text/markdown",
            content: framework_recognizers_text(),
        });
        docs
    })
}

/// The contract table as every surface reads it (`for doc in CONTRACT_DOCS`, `.iter()`, `.len()`) —
/// a zero-size handle over [`docs`] rather than a plain `&'static [ContractDoc]`, because one row is
/// RENDERED at first use (see [`disclosure_classes_text`]) and a `static` slice can only hold
/// const-evaluated rows. The read shape is unchanged on purpose: the MCP `resources/list` handler, the
/// `zzop contract` listing and the name lookups all keep seeing ONE table, so a document that resolves
/// but is not listed — half a pointer — stays impossible.
#[derive(Clone, Copy)]
pub struct ContractDocs;

/// See [`ContractDocs`]. Still a `static`, as the slice it replaced was — a `const` here would put a
/// brand-new const TYPE in front of the policy-census guard, which reads const shapes and fails on one
/// it has neither been taught nor had waived.
pub static CONTRACT_DOCS: ContractDocs = ContractDocs;

impl std::ops::Deref for ContractDocs {
    type Target = [ContractDoc];

    fn deref(&self) -> &Self::Target {
        docs()
    }
}

impl IntoIterator for ContractDocs {
    type Item = &'static ContractDoc;
    type IntoIter = std::slice::Iter<'static, ContractDoc>;

    fn into_iter(self) -> Self::IntoIter {
        docs().iter()
    }
}

/// The contract documents embedded from committed files — every row but the rendered one.
static EMBEDDED_DOCS: &[ContractDoc] = &[
    ContractDoc {
        name: "envelope-schema",
        description: "JSON Schema (draft-07) for the Normalized AST envelope contract — machine-validate a custom parser's output.",
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
        // The matcher names below are DERIVED-CHECKED, not hand-kept: the test
        // `contracts_tests::every_matcher_kind_appears_in_the_descriptions_that_claim_to_list_them_all`
        // reads `Matcher`'s own variants and reds when this string — or the `rule-pack-schema` one
        // further down — omits one. It is pinned in Rust rather than left to a reader because these
        // bytes FREEZE into every prebuilt binary: a new variant would otherwise keep telling every
        // client without a source checkout that the shape does not exist. HOW MANY names belong here
        // is not written in this comment either — that count would be the same second copy one level
        // out, and the enum plus that test are what make the list complete.
        description: "DSL rule-pack reference: pack/rule fields and every matcher (line-scan, method-scan, symbol-scan, io-scan, call-scan, literal-scan).",
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
        // Second subject of the derived matcher-kind pin described on the `dsl-reference` row above.
        description: "JSON Schema (draft-07) for the DSL rule-pack shape — pack id, rules[], the matcher kinds (line-scan, method-scan, symbol-scan, io-scan, call-scan, literal-scan), severity; every property documented. Machine-check a pack with the rule-pack validator (structure only — the same loader judgments, never rule-quality semantics).",
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
        description: "Annotated starter zzop.config.jsonc: every optional key with a comment saying what it means, filled in with zzop's own suggested value for that key so the reader can see and judge it. A config is REQUIRED — every analysis lane refuses a tree that has none, so this is the file that makes a first run possible, not an optional tuning layer. The values are SUGGESTIONS WRITTEN DOWN, not defaults the engine would apply behind the config: an undeclared vocabulary key makes no judgment at all, so writing this block is what turns those questions on and deleting a key turns one off. Usage: save these exact bytes as zzop.config.jsonc at the tree root; the vocabulary it draws from is the config-surface resource.",
        mime: "application/jsonc",
        // Owned by `zzop-config` (which also machine-checks every key it names against
        // `config-surface.json`), referenced here — the same one-embed-one-truth rule as the
        // `config-surface` row above.
        content: zzop_config::template::CONFIG_TEMPLATE_JSONC,
    },
    ContractDoc {
        name: "rule-catalog",
        description: "Every rule id the engine ships today (8 DSL packs + all native analysis ids), with severity/matcher/detection prose per rule (a DSL rule's suppress marker is derived, `zzop-<rule id>-ok`) — the ONE place a rule id can be looked up without a source checkout. The EXPORTED packs are documented here too, under their own `Exported packs` heading, one row per rule: those ship in the repository but not in this binary and run only when a config points at them, so an id found in that section is a real id that did not LOAD — never a misspelling, and never absent from this document. The three FULL-ANALYSIS lanes — whole-repo analysis, the cross-repo join, and envelope analysis — take a `rule` findings filter to pair with it, and an id absent here never fires; the two LOOKUP lanes (one file, one endpoint) take no rule filter at all, so a rule id sent to those is not a narrower answer. Pair with the dsl-reference resource for matcher semantics.",
        mime: "text/markdown",
        content: include_str!("../../../docs/rules/catalog.md"),
    },
];
