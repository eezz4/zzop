// Typed surface for @zpz/native. The addon itself is JSON-string-in / JSON-string-out (see src/lib.rs's
// module doc — "single N-API entry"); these types describe the *shape* of that JSON on both sides so a
// TypeScript host doesn't have to hand-roll it, without this package taking on a JSON-schema/codegen step
// (a later-phase concern, same as prebuild packaging).

/** One tree's `analyze()`/`analyzeTrees()` input — see `zpz_napi::api::AnalyzeRequest` (Rust). */
export interface AnalyzeConfig {
  /** Absolute or process-relative path to the tree root. Required. */
  root: string;
  /** Tags this tree in `CommonIr.source` / cross-layer join output. Default: `""`. */
  sourceId?: string;
  /** Directory of `*.json` DSL rule packs (`rules/dsl/*.json`) to load. Default: none loaded. */
  packsDir?: string;
  /** Analysis cache directory (content-hash + parser/ruleset fingerprint keyed). Default: cache off. */
  cacheDir?: string;
  /** Enables git-history-dependent analyses (scores/health/recommendations/critical/seams). Default: off. */
  git?: {
    /** `git log --since=<since>`. Default: full history. */
    since?: string;
    /** Window, in days, for each file's `recent_*` fields. Default: 30. */
    recentDays?: number;
  };
  /** Files larger than this (bytes) skip structural parsing, falling back to a lexical line count. */
  sizeCap?: number;
  /** Rule/pack/native-analysis ids to disable entirely (exact match). */
  disabledRules?: string[];
}

/** `analyzeTrees()`'s input: one `AnalyzeConfig` per tree, joined by IoFacts. */
export interface AnalyzeTreesConfig {
  trees: AnalyzeConfig[];
}

/**
 * `analyzeEnvelope()`'s config input — see `zpz_napi::api::EnvelopeAnalyzeRequest` (Rust). Unlike
 * `AnalyzeConfig` there is no `root`/`cacheDir`/`git`/`sizeCap`: an envelope carries no filesystem
 * location the engine can re-read (see the external-parser protocol doc, `docs/NORMALIZED_AST.md`).
 */
export interface EnvelopeAnalyzeConfig {
  /** Tags this tree in `CommonIr.source` / cross-layer join output. Default: `""`. */
  sourceId?: string;
  /** Directory of `*.json` DSL rule packs (`rules/dsl/*.json`) to load. Default: none loaded. Only
   * `symbol-scan`/`io-scan` rules ever fire in envelope mode (no source text is available). */
  packsDir?: string;
  /** Rule/pack/native-analysis ids to disable entirely (exact match). */
  disabledRules?: string[];
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
 * of every `zpz-core` IR type (`CommonIr`, `Finding`, `FileNode`, `Scores`, ...) — those are Rust structs
 * whose JSON field casing/shape this package does not want to fork/duplicate and risk drifting from; a
 * generated/shared `.d.ts` for the IR itself is a later-phase concern (see also `src/api.rs`'s doc on why
 * this crate mirrors rather than forks `zpz_engine::AnalyzeOutput` for serialization).
 */
export type AnalyzeOutputJson = Record<string, unknown>;

/**
 * Runs the fused engine over one tree and returns its JSON-serialized `AnalyzeOutput`
 * (`{ir, findings, degraded, fileCount, nodes, scores, health, recommendations, critical, seams,
 * folders, layerCoChurn, warnings, cache, ruleTimings}`) — every field, and every nested type's own
 * fields, are camelCase (see `docs/modules/napi.md` for the one documented exception, `Finding.data`).
 * Throws on malformed `configJson` or a missing `root` — never returns a panic.
 */
export function analyze(configJson: string): string;

/**
 * Runs `analyze()` once per tree in `{trees: AnalyzeConfig[]}`, then cross-layer-joins every tree's IoFacts.
 * Returns `{trees: [{root, sourceId, output: AnalyzeOutputJson}], crossLayer}` as JSON. Throws on malformed
 * `configJson` or an empty/invalid `trees` array.
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

/** This addon's crate version plus every parser's `PARSER_FINGERPRINT` (diagnostics). */
export function version(): string;

declare const _default: {
  analyze: typeof analyze;
  analyzeTrees: typeof analyzeTrees;
  analyzeEnvelope: typeof analyzeEnvelope;
  version: typeof version;
};
export default _default;
