// Typed surface for @zzop/native. The addon itself is JSON-string-in / JSON-string-out (see src/lib.rs's
// module doc — "single N-API entry"); these types describe the *shape* of that JSON on both sides so a
// TypeScript host doesn't have to hand-roll it, without this package taking on a JSON-schema/codegen step
// (a later-phase concern, same as prebuild packaging).

/** One tree's `analyze()`/`analyzeTrees()` input — see `zzop_facade::AnalyzeRequest`
 * (`crates/facade/src/request.rs`). */
export interface AnalyzeConfig {
  /** Absolute or process-relative path to the tree root. Required. */
  root: string;
  /** Tags this tree in `CommonIr.source` / cross-layer join output. Default: `""`. */
  sourceId?: string;
  /** Directory (or directories, searched in order) of `*.json` DSL rule packs to load. Default: none
   * loaded. An explicit `null` disables the bundled-directory default the JS wrapper prepends and
   * every pack directory (caller `packDefs` stay honored). */
  packsDir?: string | string[] | null;
  /** Inline rule-pack definitions (parsed rule-pack JSON objects), loaded BEFORE `packsDir` — on a
   * pack-id collision the directory pack wins whole. Additive (v0.16.0). Default: none. */
  packDefs?: Array<Record<string, unknown>>;
  /** Analysis cache directory (content-hash + parser/ruleset fingerprint keyed). Default: cache off. */
  cacheDir?: string;
  /** Enables git-history-dependent analyses (scores/health/recommendations/critical/seams). Default:
   * off (the JS wrapper injects `git: {}` when the key is absent; an explicit `null` disables git
   * collection). */
  git?: {
    /** `git log --since=<since>`. Default: full history. */
    since?: string;
    /** Window, in days, for each file's `recent_*` fields. Default: 30. */
    recentDays?: number;
    /** Custom commit-type classifier table (`{pattern: <regex>, tag: <TAG>}[]`). REPLACES the default
     * FIX/FEAT/REVERT/... table entirely when present and non-empty (match order = array order); absent
     * or empty falls back to the default table. An entry whose `pattern` fails to compile as a regex is
     * skipped (matches nothing) and reported as a `warnings` entry, never a failure. Default: the built-in
     * table. */
    commitTypePatterns?: { pattern: string; tag: string }[];
  } | null;
  /** Files larger than this (bytes) skip structural parsing, falling back to a lexical line count. */
  sizeCap?: number;
  /** Rule/pack/native-analysis ids to disable entirely (exact match). */
  disabledRules?: string[];
  /** Per-rule severity remap: rule id -> `"critical"` | `"warning"` | `"info"`. Default: no remaps. */
  severityOverrides?: Record<string, "critical" | "warning" | "info">;
  /** Finding-level accept-list: each entry drops findings for `rule` — globally (no filter), in files whose
   * path contains `path` (plain substring), or in files matching `glob` (full-path glob: `*`/`?` stay
   * within a segment, `**` spans `/`, `{a,b}` alternates). `glob` takes precedence over `path`. Default:
   * nothing suppressed. */
  suppressions?: { rule: string; path?: string; glob?: string }[];
  /** Config-wide, rule-agnostic finding-level filter (the top-level `"exclude"` config key's napi
   * exposure): each entry drops findings for EVERY rule at once — globally (no filter, when both `path`
   * and `glob` are omitted), in files whose path contains `path` (plain substring), or in files matching
   * `glob` (full-path glob, same syntax as `suppressions[].glob`; takes precedence over `path`). The file
   * is still analyzed — only findings are filtered. Default: nothing globally excluded. */
  globalExcludes?: { path?: string; glob?: string }[];
  /** Mode-B adapter overlays: partial Normalized-AST envelopes (typically just `io` + fragment channels
   * for a handful of files — see `docs/NORMALIZED_AST.md`) merged ON TOP of this tree's native analysis.
   * Left as `Record<string, unknown>[]` for the same reason `NormalizedAstEnvelope` is (see that type's
   * doc). Default: none (no overlay processing). Contrast with `analyzeEnvelope()`, where a full envelope
   * REPLACES native analysis rather than augmenting it — this field has no equivalent there. */
  adapterOverlays?: Array<Record<string, unknown>>;
  /** Deployment-topology "whole-tree" mount point: prepends this gateway prefix to every one of this
   * tree's http provide keys, as the least-specific entry (an equal or longer `mounts[]` `dir` wins).
   * Default: no implicit whole-tree mount. */
  mountedAt?: string;
  /** Deployment-topology mounts, in array order: prepends `at` to this tree's http provide keys whose
   * file path falls under `dir` (longest matching `dir` wins). Default: no mounts beyond `mountedAt`. */
  mounts?: Array<{ dir: string; at: string }>;
  /** Hosts this tree owns: re-keys absolute-URL consumes targeting these hosts into internal joinable
   * keys at cross-layer link time. Default: no hosts declared. */
  hosts?: string[];
}

/** `analyzeTrees()`'s input: one `AnalyzeConfig` per tree, joined by IoFacts. */
export interface AnalyzeTreesConfig {
  trees: AnalyzeConfig[];
}

/**
 * `analyzeEnvelope()`'s config input — see `zzop_facade::EnvelopeAnalyzeRequest`
 * (`crates/facade/src/request.rs`). Unlike `AnalyzeConfig` there is no `root`/`cacheDir`/`git`/
 * `sizeCap`: an envelope carries no filesystem location the engine can re-read (see the
 * external-parser protocol doc, `docs/NORMALIZED_AST.md`).
 */
export interface EnvelopeAnalyzeConfig {
  /** Tags this tree in `CommonIr.source` / cross-layer join output. Default: `""`. */
  sourceId?: string;
  /** Directory (or directories, searched in order) of `*.json` DSL rule packs to load. The engine
   * facade additionally seeds the BUNDLED packs as inline `packDefs` on every envelope analysis
   * (a directory pack with a bundled id wins the collision whole, unchanged); an explicit
   * `packsDir: null` opts out of the bundled seed and all pack directories (caller `packDefs` are
   * still honored). Only `symbol-scan`/`io-scan`
   * rules ever fire in envelope mode (no source text is available). */
  packsDir?: string | string[] | null;
  /** Inline rule-pack definitions — same shape and packsDir-collision semantics as
   * `AnalyzeConfig.packDefs`, seeded AFTER the facade's bundled defaults (a caller def with a
   * bundled id wins that collision whole). Default: none beyond the bundled packs. */
  packDefs?: Array<Record<string, unknown>>;
  /** Rule/pack/native-analysis ids to disable entirely (exact match). */
  disabledRules?: string[];
  /** Per-rule severity remap: rule id -> `"critical"` | `"warning"` | `"info"`. Default: no remaps. */
  severityOverrides?: Record<string, "critical" | "warning" | "info">;
  /** Finding-level accept-list: each entry drops findings for `rule` — globally (no filter), in files whose
   * path contains `path` (plain substring), or in files matching `glob` (full-path glob: `*`/`?` stay
   * within a segment, `**` spans `/`, `{a,b}` alternates). `glob` takes precedence over `path`. Default:
   * nothing suppressed. */
  suppressions?: { rule: string; path?: string; glob?: string }[];
  /** Config-wide, rule-agnostic finding-level filter — same shape and semantics as
   * `AnalyzeConfig.globalExcludes`. Default: nothing globally excluded. */
  globalExcludes?: { path?: string; glob?: string }[];
  /** Deployment-topology "whole-tree" mount point — same shape and semantics as
   * `AnalyzeConfig.mountedAt`: prepends this gateway prefix to every one of the envelope's http
   * provide keys, as the least-specific entry (an equal or longer `mounts[]` `dir` wins). Mounts
   * apply uniformly to Mode A envelopes and natively-parsed trees alike (see
   * `docs/NORMALIZED_AST.md`). Default: no implicit whole-tree mount. */
  mountedAt?: string;
  /** Deployment-topology mounts, in array order — same shape and semantics as
   * `AnalyzeConfig.mounts`: prepends `at` to the envelope's http provide keys whose file path falls
   * under `dir` (longest matching `dir` wins). Default: no mounts beyond `mountedAt`. */
  mounts?: Array<{ dir: string; at: string }>;
}

/**
 * `analyzeEnvelope()`'s envelope input — the `docs/NORMALIZED_AST.md` v1 Normalized AST contract an
 * external/custom parser (Java, Python, JSP, ...) emits per source tree. Left as `Record<string,
 * unknown>` for the same reason `AnalyzeOutputJson` is (see that type's doc) — the Rust `NormalizedEnvelope`/
 * `FileProjection` types are the authoritative schema.
 */
export type NormalizedAstEnvelope = Record<string, unknown>;

/**
 * `analyze(configJson)` -> parsed JSON shape. Left as `Record<string, unknown>` rather than a full 1:1 port
 * of every `zzop-core` IR type (`CommonIr`, `Finding`, `FileNode`, `Scores`, ...) — those are Rust structs
 * whose JSON field casing/shape this package does not want to fork/duplicate and risk drifting from; a
 * generated/shared `.d.ts` for the IR itself is a later-phase concern (see also
 * `crates/facade/src/output.rs`'s doc on why the facade mirrors rather than forks
 * `zzop_engine::AnalyzeOutput` for serialization).
 */
export type AnalyzeOutputJson = Record<string, unknown>;

/**
 * Runs the fused engine over one tree and returns its JSON-serialized `AnalyzeOutput`
 * (`{ir, findings, degraded, fileCount, nodes, scores, health, recommendations, critical, seams,
 * folders, layerCoChurn, packsLoaded, warnings, coverage, disclosure, cache, ruleTimings}`) — every
 * field, and every nested type's own
 * fields, are camelCase (see `docs/modules/napi.md` for the one documented exception, `Finding.data`).
 * Throws on malformed `configJson` or a missing `root` — never returns a panic.
 */
export function analyze(configJson: string): string;

/**
 * Runs `analyze()` once per tree in `{trees: AnalyzeConfig[]}`, then cross-layer-joins every tree's IoFacts.
 * Returns `{trees: [{root, sourceId, output: AnalyzeOutputJson}], crossLayer, crossLayerFindings,
 * disclosure}` as JSON. Throws on malformed `configJson` or an empty/invalid `trees` array.
 */
export function analyzeTrees(configJson: string): string;

/**
 * The `docs/NORMALIZED_AST.md` external-parser protocol receiver: validates `envelopeJson` against the
 * v1 Normalized AST contract, projects it into the same per-file artifact shape a native parser
 * produces, and runs every language-neutral analysis (dep graph, `circular`/`dead-candidates`, DSL
 * `symbol-scan`/`io-scan` rules) over it. Returns JSON matching `AnalyzeOutputJson`'s shape. Throws on a
 * malformed envelope (wrong `format`, unsupported `version`, empty/duplicate `path`, an inverted
 * `body_start`/`body_end` span) or malformed `configJson` — never a panic.
 */
export function analyzeEnvelope(envelopeJson: string, configJson: string): string;

/**
 * Fast offline `{valid, issues}` check for a `NormalizedAstEnvelope` against the v1 Normalized AST
 * contract — `zzop_core::validate_envelope` alone, no `configJson` and no engine analysis. Never throws
 * on an invalid envelope: an unparseable/semantically-invalid envelope still returns an ordinary
 * `'{"valid":false,"issues":[...]}'` JSON string, never an `Err`.
 */
export function validateEnvelopeOnly(envelopeJson: string): string;

/**
 * Pre-load, structure-only `{valid, issues}` check for a DSL rule-pack JSON text (one
 * `rules/dsl/*.json` / `packsDir` file, or one `packDefs` entry) — the exact judgments the engine's
 * pack loader makes at load time (bad JSON, missing field, wrong type, too-new `schema_version`)
 * plus every matcher regex that fails to compile (such a rule loads but silently never fires).
 * NEVER judges rule quality or semantics. Like `validateEnvelopeOnly`, never throws on invalid
 * input: an unparseable pack still returns an ordinary `'{"valid":false,"issues":[...]}'` string.
 * The machine-readable shape contract ships as `docs/contracts/rule-pack.schema.json`
 * (`zzop://contract/rule-pack-schema` over MCP).
 */
export function validateRulePackOnly(packJson: string): string;

/**
 * Definitive endpoint/io-key query over an ALREADY-PRODUCED `analyzeTrees()` output — pure
 * post-processing, no re-analysis. `analysisJson` is the string `analyzeTrees()` returned;
 * `queryJson` is `{"pattern": string}` (non-empty; case-insensitive substring matched against
 * every cross-layer io key — http routes, env keys, DB tables, topics — and against `raw` for unresolved
 * consumes). Returns `{pattern, verdict, counts, matches, truncated?, relatedFindings,
 * truncatedFindings?, suggestions?, disclosure}` where `verdict` is the sealed vocabulary
 * `"linked" | "provided-only" | "consumed-unprovided" | "external" | "unresolved-only" |
 * "ambiguous" | "mixed" | "not-found"`. Throws on a malformed query (empty/missing/unknown key)
 * and on a single-tree `analyze()` output (a guided error: verdicts are join facts — run
 * `analyzeTrees()`, which joins even a single tree).
 */
export function queryIo(analysisJson: string, queryJson: string): string;

/** This addon's crate version plus every parser's `PARSER_FINGERPRINT` (diagnostics). */
export function version(): string;

declare const _default: {
  analyze: typeof analyze;
  analyzeTrees: typeof analyzeTrees;
  analyzeEnvelope: typeof analyzeEnvelope;
  validateEnvelopeOnly: typeof validateEnvelopeOnly;
  validateRulePackOnly: typeof validateRulePackOnly;
  queryIo: typeof queryIo;
  version: typeof version;
};
export default _default;
