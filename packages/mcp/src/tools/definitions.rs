//! `tools/list` result payload — the tool definitions (names, descriptions, input JSON Schemas),
//! split out of `tools.rs` unchanged. The strings here are the MCP contract; see the parent module
//! doc for the surface overview.

/// `tools/list` result: every tool this server exposes, with input JSON Schemas. Shared filter
/// arguments (`severity`/`rule`/`limit`) are the drill-down knobs the truncation hint points at.
pub fn list() -> serde_json::Value {
    let filter_props = serde_json::json!({
        "severity": { "type": "string", "enum": ["critical", "warning", "info"], "description": "Minimum severity to include in the findings list (counts always cover everything)." },
        "rule": { "type": "string", "description": "Exact rule id to include in the findings list. Valid ids are listed in the rule-catalog resource (zzop://contract/rule-catalog); a filter matching zero findings for an id absent from this run's own fired ids gets a disclosure note." },
        "limit": { "type": "integer", "minimum": 0, "maximum": 1000, "description": "Findings list cap (default 50, max 1000). 0 is legal — \"counts only, no findings listed\". Must be a JSON integer in range; an out-of-range or wrong-type value is a named error, not a silent no-op." }
    });
    serde_json::json!({
        "tools": [
            {
                "name": "analyze_repo",
                "description": "Run zzop's deterministic analysis on ONE repository/tree path. Auto-discovers <path>/zzop.config.jsonc (rules, packs, overlays, mounts — the reply's `config` field says whether one was honored); without one, zero-config defaults apply (bundled rule packs + git signals included). Returns a summary (full counts by severity/rule, engine warnings) plus a capped findings list — truncation is always disclosed. A config declaring multiple trees returns a guided error pointing at cross_repo (with the configPath to pass). Cross-layer (`cross-layer/*`) findings come from the multi-tree join and surface only in cross_repo/check_endpoint replies — this tool reports this tree's own per-tree findings only. Any honored config's rule overrides (disabled rules, remapped severities) are positively acknowledged in the reply's `ruleOverridesApplied` field ({disabled, severityRemapped} id lists, omitted when none were requested), alongside the honored config file echoed in `config`. This reply, and the sibling `zzop analyze <path>` CLI form (same handler), are both a shaped summary that deliberately omits the raw `ir` block some engine disclosures point at — the full raw io facts (`ir.io`'s provide/consume lists) are only in the raw `zzop-facade` JSON output you get by embedding the engine directly, never in this Node-free binary's replies. When the underlying analysis ran git signals (zero-config default, or a config's own `git` settings), the reply also carries a compact, capped `architecture` object summarizing the engine's health/recommendations/critical-file computation: {pain: the composite structural-debt score, topRecommendation: {id, severity, topItem} or null when no recommendation cleared threshold, criticalTop: up to 3 paths off the blast-radius-ranked critical list — NOT the churn hotspot ranking (see the `rule-catalog` contract's criticality entry)}. The whole object is absent (not null) when git signals did not run. Full per-file scores/recommendations/critical-file detail is never in this summary — only the raw `zzop-facade` JSON output (direct engine embedding) carries the complete arrays.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Absolute path to the repo/tree to analyze." },
                        "severity": filter_props["severity"],
                        "rule": filter_props["rule"],
                        "limit": filter_props["limit"]
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "cross_repo",
                "description": "Analyze 2+ repos/trees and join them across the layer boundary — the cross-layer (kind,key) join (e.g. a React consume matching a Spring provide, a shared DB table, route drift). Pass EITHER `configPath` (a zzop.config.jsonc — its `trees`, including \"auto\", define the join; the config-first way) OR `paths` (explicit tree roots; config-free, each tagged by directory name — any zzop.config.jsonc inside them is NOT loaded and says so in configWarnings). Returns per-tree summaries with engine warnings, the join buckets, matched edges, and cross-layer findings (capped lists disclose truncation); `bucketKeys` lists which distinct keys sit in each non-edge bucket, and the parallel `bucketKeySites` gives the first call site (`file:line`) behind each listed key, so e.g. an unresolvedConsumes key is locatable without a further query. The honored config file, if any, is echoed at the top level (`config`), and each source's rule overrides, if any, are positively acknowledged per-tree (`ruleOverridesApplied`: {disabled, severityRemapped} id lists) rather than left implicit. Like analyze_repo, this reply (and the sibling `zzop cross` CLI form) is a shaped summary per source that omits the raw `ir` block — full raw io facts live only in the raw `zzop-facade` JSON output (direct engine embedding).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Absolute paths to the repos/trees to join (config-free mode).",
                            "minItems": 2
                        },
                        "configPath": { "type": "string", "description": "Path to a zzop.config.jsonc (or a directory containing one) whose trees define the join (config-first mode)." },
                        "severity": filter_props["severity"],
                        "rule": filter_props["rule"],
                        "limit": filter_props["limit"]
                    },
                    // The handler's real contract, expressed in the schema (not just the prose):
                    // exactly ONE of `paths` / `configPath` — both branches matching (or neither)
                    // fails `oneOf`, mirroring the "not both" / "pass one" handler errors.
                    "oneOf": [
                        { "required": ["paths"] },
                        { "required": ["configPath"] }
                    ]
                }
            },
            {
                "name": "check_endpoint",
                "description": "DEFINITIVE answer to \"is io key X provided/consumed/joined?\" — matches `pattern` against ANY cross-layer io key (http routes, env keys, DB tables, topics) as a case-insensitive substring, over a fresh analysis of the given tree(s). Returns one verdict from a sealed vocabulary: \"linked\" (consume joined to a provide), \"provided-only\" (provided, nothing consumes it), \"consumed-unprovided\" (consumed, nothing provides it — drift/bug), \"external\" (third-party egress), \"unresolved-only\" (call sites whose key could not be statically determined), \"ambiguous\" (the key is provided by more than one source tree — every candidate provider is listed in `candidates`), \"mixed\" (matches span 2+ of those classes — counts disambiguate), or \"not-found\" (with key suggestions). Full per-bucket counts ride along uncapped; matched objects (file/line/source intact) and related findings are capped with disclosed truncation. Pass exactly ONE of `path` (one tree — the join still runs, intra-tree edges included), `paths` (2+ tree roots, config-free), or `configPath`.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "minLength": 1, "description": "Non-empty, case-insensitive substring to match against every io key (and against the raw expression of unresolved consumes)." },
                        "path": { "type": "string", "description": "Absolute path to ONE repo/tree (auto-discovers its zzop.config.jsonc, like analyze_repo)." },
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Absolute paths to 2+ repos/trees to join (config-free mode, like cross_repo).",
                            "minItems": 2
                        },
                        "configPath": { "type": "string", "description": "Path to a zzop.config.jsonc (or a directory containing one) whose trees define the analysis." }
                    },
                    "required": ["pattern"],
                    // The "exactly ONE of path/paths/configPath" rule from the description,
                    // expressed in the schema: each branch requires `pattern` plus one source, so
                    // two sources (two branches match) or zero (none match) both fail `oneOf` —
                    // mirroring the handler's own "pass exactly ONE" error.
                    "oneOf": [
                        { "required": ["pattern", "path"] },
                        { "required": ["pattern", "paths"] },
                        { "required": ["pattern", "configPath"] }
                    ]
                }
            },
            {
                "name": "analyze_envelope",
                "description": "Run Mode A full-envelope analysis: a complete Normalized AST envelope (a custom parser's output, already validated against the v1 contract) REPLACES native parsing entirely for this run — contrast validate_envelope, which only checks the envelope's shape and runs no analysis at all, and Mode B overlay/mount requests, which merge external symbols ON TOP of a natively-parsed tree instead of replacing it. Only symbol-scan/io-scan rules can fire (an envelope carries no source text, so text-scan/regex-body rules never match — the zzop://contract/rule-catalog resource says which rules are which kind). Zero-config: bundled rule packs load the same way every other zzop-mcp tool's zero-config default does; an envelope carries no filesystem location, so there is no `config` file to auto-discover and the reply has no `config`/`path` field at all. Returns the SAME shaped summary analyze_repo/cross_repo return otherwise: full findings counts by severity/rule, engine warnings, `packsLoaded` confirmation, and the structural coverage census — capped lists always disclose truncation. Never carries an `architecture`/`gitWindow` field: git signals need a working tree to diff, which an envelope does not have. Pair with the zzop://contract/envelope-guide and example-envelope resources.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "envelopeJson": { "type": "string", "description": "The Normalized AST envelope JSON text to analyze (see the zzop://contract/envelope-schema resource)." },
                        "severity": filter_props["severity"],
                        "rule": filter_props["rule"],
                        "limit": filter_props["limit"]
                    },
                    "required": ["envelopeJson"]
                }
            },
            {
                "name": "validate_envelope",
                "description": "Validate a Normalized AST envelope (a custom parser's output) against the v1 contract WITHOUT running an analysis — the authoring feedback loop. Returns {valid, issues[]}; never fails on bad input. Pair with the zzop://contract/* resources (schema, guide, key-normalization fixture).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "envelopeJson": { "type": "string", "description": "The envelope JSON text to validate." }
                    },
                    "required": ["envelopeJson"]
                }
            },
            {
                "name": "validate_rule_pack",
                "description": "Validate a DSL rule pack's STRUCTURE before loading it — the exact judgments the engine's pack loader makes at load time (bad JSON, missing field, wrong type, too-new schema_version) plus every matcher regex that fails to compile (such a rule would load but silently never fire). This checks shape ONLY — it never judges rule quality or semantics (whether a pattern over-matches, whether a rule is useful). Validation is also PACK-LOCAL: it cannot see any other pack, so a pack `id` colliding with a bundled or another loaded pack (which replaces it WHOLE) is invisible here — that only surfaces at load time via `packsLoaded` (and its shadow warning, when one fires); check `packsLoaded` after loading a pack this tool passed. Returns {valid, issues[]}; never fails on bad input. Pair with the zzop://contract/rule-pack-schema resource (the machine-readable shape) and the dsl-reference/dsl-authoring-guide resources.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "packJson": { "type": "string", "description": "The rule-pack JSON text to validate (one pack file — rules/dsl/<pack>/<pack>.json in-repo or a packsDir file — or one packDefs entry)." }
                    },
                    "required": ["packJson"]
                }
            }
        ]
    })
}
