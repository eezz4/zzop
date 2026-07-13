'use strict';

// The `zzop init` template — a tsc-style annotated zzop.config.jsonc. Every option carries an inline
// comment describing what it does, like `tsc --init`. The file must be immediately runnable: after
// comment-stripping it parses to a valid config (roots: ["."], format/failOn set; rules/packs are
// illustrative-but-inert defaults).

const CONFIG_TEMPLATE = `{
  // zzop configuration — https://github.com/eezz4/zzop
  // Run \`npx zzop\` in this directory to analyze. Edit below, then re-run.

  // ── What to analyze ────────────────────────────────────────────────────────
  // One or more directory roots to scan. A single root runs the engine once;
  // multiple roots run a cross-layer (multi-tree) analysis and join their I/O.
  "roots": ["."],

  // Alternatively, name each tree so cross-layer output can attribute findings.
  // If "trees" is present it takes precedence over "roots". Remove "roots" above
  // if you use this form. A tree root may also point OUTSIDE this repo — attach the
  // sibling checkout that serves this repo's calls (e.g. "../provider-repo": a backend,
  // a peer service, or a module-federation remote) to review both sides together.
  // "trees": [
  //   { "root": "./api", "sourceId": "api" },
  //   { "root": "./web", "sourceId": "web" }
  // ],

  // Monorepo tip: analyzing a repo as a SINGLE root mixes every app's I/O into one
  // tree, which can MASK an app whose routes zzop cannot extract — its blindness is
  // hidden by the other apps' signals, so it looks like "only part of the repo ran".
  // Split apps into "trees" above so each app's coverage/blindness is reported per
  // tree — that is where the "this tree is IO-blind; inject a Mode B adapter to
  // restore visibility" guidance surfaces. Attach a Mode B adapter's output via the
  // "overlays" config key (top level, or per-tree as "trees[i].overlays") — an array
  // of paths to overlay envelope JSON files, e.g. "overlays": ["./my-adapter/envelope.json"].

  // ── Global exclude (all rules) ─────────────────────────────────────────────
  // Path globs/substrings dropped from EVERY rule's findings at once (files are
  // still parsed, so the dep graph / dead-code analysis stays correct). Use this
  // instead of repeating the same glob under each rule below. A "*" stays within one
  // path segment — use "**/" to cross directories (so "**/*.stories.tsx", NOT
  // "*.stories.tsx", which would only match a stories file at the repo root).
  // "exclude": ["**/*.stories.tsx", ".storybook/"],

  // ── Rule packs (plugins) ───────────────────────────────────────────────────
  "packs": {
    // Extra local directories of custom DSL rule packs (rules/dsl/*.json). These
    // MERGE with the bundled packs; a custom pack whose id matches a bundled one
    // replaces it. Leave empty to use only the bundled packs.
    "extraDirs": [],

    // Whole packs to turn off entirely, by pack id.
    "disabled": []
  },

  // ── Per-rule overrides ─────────────────────────────────────────────────────
  // Map a rule id to either a severity string or an object.
  //   "off"                        -> disable the rule
  //   "info" | "warn" | "critical" -> override its severity
  //   { "severity": "warn",
  //     "exclude": ["legacy/"] }   -> override severity AND drop findings by file
  //                                   path. Each entry is a substring, or a glob if
  //                                   it has *, ?, or {} (e.g. **/app/**/page.tsx).
  "rules": {
    // "typescript/no-explicit-any": "off",
    // "sql/nplus1": "warn",
    // "sql/race-condition-toctou": { "severity": "warn", "exclude": ["legacy/"] }
  },

  // ── Git-derived signals ────────────────────────────────────────────────────
  // Enables history-dependent analyses (churn/health/recommendations). Omit to
  // skip git-derived analyses entirely; set {} to enable them with defaults.
  "git": {
    // Window, in days, for each file's recent-activity fields (default 30).
    "recentDays": 30
  },

  // ── Performance / caching ──────────────────────────────────────────────────
  // Analysis cache directory (content-hash keyed). Omit to disable caching.
  "cacheDir": ".zzop-cache",

  // Files larger than this many bytes skip structural parsing (lexical fallback). Omitted here so
  // the engine default applies; uncomment to override.
  // "sizeCap": 1500000,

  // ── Output ─────────────────────────────────────────────────────────────────
  // "pretty" (grouped, human-readable) or "json" (raw engine output). Overridden
  // by --format / --json on the command line.
  "format": "pretty",

  // Reports are persisted to disk by default (Markdown: one file per tree, plus
  // cross-repo.md for a multi-tree run) in addition to stdout. Each run writes to
  // <dir>/zzop.<epoch>/ so runs accumulate. Omit "report" entirely to keep the
  // defaults (dir "zzop-reports", formats ["md"]); --out <dir> overrides "dir".
  // "sarif" is read by GitHub code scanning and the VS Code SARIF viewer.
  // "report": {
  //   "dir": "zzop-reports",
  //   "formats": ["md", "json", "sarif"],
  //   "enabled": true // set false to disable report writing entirely
  // },

  // Exit non-zero when any finding is at or above this severity — for CI gating.
  //   "info" | "warn" | "critical", or "off" to always exit 0.
  // The FIRST run on an untuned repo will likely exit 1 (findings exist) — that is
  // expected, not a tool error. Triage the output, exclude non-deployed surface
  // (tests/stories/generated) via "exclude" above, then keep "warn" gating in CI.
  "failOn": "warn"
}
`;

module.exports = { CONFIG_TEMPLATE };
