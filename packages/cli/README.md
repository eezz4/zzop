# @zzop/cli

Config-driven CLI for the **zzop** multi-language SAST/architecture analysis engine. Write a
`zzop.config.jsonc`, run `npx zzop` — no code, ESLint-style.

The analysis engine ships as [`@zzop/native`](https://www.npmjs.com/package/@zzop/native) and is
installed automatically as a dependency of this package. `@zzop/cli` is the thin config-driven front end;
`@zzop/native` is the engine/SDK layer for embedders.

## Install

```sh
# one-off (no install)
npx @zzop/cli init
npx @zzop/cli

# or as a dev dependency (then the command is `zzop`)
npm i -D @zzop/cli
```

Requires Node.js >= 18.

## Quick start

```sh
zzop init      # writes an annotated zzop.config.jsonc to the current directory
zzop           # analyzes using that config and prints a report
```

`zzop init` refuses to overwrite an existing config; pass `--force` to replace it.

## Commands

| Command | Description |
| --- | --- |
| `zzop init [--force]` | Write an annotated `zzop.config.jsonc` to the current directory. |
| `zzop [run] [options]` | Load the config, analyze, and print. This is the default command. |

### `run` options

| Option | Description |
| --- | --- |
| `--config <path>` | Config file to load (default `./zzop.config.jsonc`). |
| `--format <pretty\|json>` | Output format, overriding the config's `format`. |
| `--json` | Alias for `--format json`. |
| `--out <dir>` | Override the report base directory (default `./zzop-reports`; equivalent to config `report.dir`). Each run writes to `<dir>/zzop.<epoch>/`, a fresh subdir per run so runs accumulate. |
| `-a, --all` | Expand info-level findings. By default they are folded to a per-rule count so warnings/errors stay visible. |
| `--severity <critical\|warning\|info\|off>` | Only display findings at or above this severity (default `off` = show all). This is a display filter only — the exit code is always computed from the unfiltered findings and the config's `failOn`, never from `--severity`. |
| `-h, --help` | Show help. |
| `--version` | Show the CLI and engine versions. |

Stdout is the default *interactive* output. On top of that, **every run also persists a Markdown report to
disk by default** — this is the delivery surface for handing an analysis to someone else (e.g. a
cross-repo review, or attaching results to a PR): `./zzop-reports/zzop.<epoch-seconds>/` gets one
`<sourceId>.md` per analyzed tree, plus a `cross-repo.md` summary (edges, unresolved/unprovided/unconsumed
buckets, coverage self-reports) when the run covers more than one tree. `--out <dir>` (or config
`report.dir`) overrides the base directory.

Set config `report.formats` to change which formats are written — e.g. `["md", "json", "sarif"]` to also
emit machine-readable reports alongside the default Markdown, or `["json"]` to switch off Markdown
entirely. `sarif` is [SARIF 2.1.0](https://sarifweb.azurewebsites.net/), which GitHub code scanning and the
VS Code SARIF viewer read directly. To disable report writing altogether (e.g. a CI job that only cares
about the exit code), set config `report.enabled: false`.

### Warnings

zzop follows a "narrowed scope self-reports in `warnings`, never silently" contract, so the CLI prints
warnings to **stderr** (stdout stays clean — pretty or JSON):

- **Unknown config keys** — a key the CLI doesn't recognize (a typo, or a key from a different zzop
  version) is *ignored* (never rejected), but reported: `zzop: warning: unknown config key "rulez" …`.
- **Engine self-reports** — a narrowed scope (git not requested, no rule packs found, a file that couldn't
  be parsed structurally, …) is surfaced rather than swallowed.

### Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Ran successfully; no finding at or above `failOn`. |
| `1` | At least one finding at or above `failOn` (CI gate). |
| `2` | Config or usage error. |

## Configuration

`zzop.config.jsonc` is JSON with comments (and trailing commas) allowed. `zzop init` generates a fully
annotated copy; the reference below summarizes each option.

```jsonc
{
  // What to analyze: one or more directory roots. Multiple roots run a
  // cross-layer (multi-tree) analysis.
  "roots": ["."],

  // Or name each tree explicitly (takes precedence over "roots"):
  // "trees": [
  //   { "root": "./api", "sourceId": "api" },
  //   { "root": "./web", "sourceId": "web" }
  // ],

  "packs": {
    // Extra directories of custom DSL rule packs (rules/dsl/*.json). These MERGE
    // with the bundled packs; a custom pack whose id matches a bundled one wins.
    "extraDirs": ["./zzop-packs"],
    // Whole packs to disable, by pack id.
    "disabled": ["browser"]
  },

  "rules": {
    // "off"                        -> disable the rule
    "no-explicit-any": "off",
    // "info" | "warn" | "critical" -> override severity
    "n-plus-one": "warn",
    // object form -> override severity AND drop findings by file path.
    // Each `exclude` entry is a plain substring, OR a glob if it contains
    // `*`/`?`/`{}` (full-path: `*`/`?` stay within a segment, `**` spans `/`,
    // `{a,b}` alternates). `[...]` stays literal so raw `app/[id]/` paths work.
    "toctou": { "severity": "warn", "exclude": ["legacy/"] },
    "dead-candidates": { "exclude": ["**/app/**/{page,layout,route}.tsx"] }
  },

  // Top-level exclude: path globs/substrings dropped from EVERY rule's findings
  // (files are still parsed for the dep graph). Same glob rules as per-rule
  // exclude above — a `*` stays within a path segment, use `**/` to cross
  // directories.
  "exclude": ["**/*.stories.tsx", "legacy/"],

  // Enables git-history-derived signals. Omit to use engine defaults.
  // "recentDays" windows the recent-activity fields (default 30).
  // "commitTypePatterns" teaches a non-English/non-conventional commit convention: an array of
  // { "pattern": "<regex>", "tag": "FIX"|"FEAT"|... }, checked in array order (earlier entries win,
  // mirroring the built-in REVERT-before-FIX ordering). When present and non-empty it REPLACES the
  // default FIX/FEAT/REVERT/.../STYLE table entirely; a pattern that fails to compile as a regex is
  // skipped (matches nothing) and reported as a warning, never a crash. Omit for the default table.
  "git": { "recentDays": 30 },
  // "git": {
  //   "recentDays": 30,
  //   "commitTypePatterns": [{ "pattern": "^\\s*corrige\\b", "tag": "FIX" }]
  // },

  // Analysis cache directory (omit to disable caching).
  "cacheDir": ".zzop-cache",

  // Files larger than this many bytes skip structural parsing.
  "sizeCap": 500000,

  // "pretty" or "json"; overridden by --format / --json.
  "format": "pretty",

  // Reports are persisted to disk by default (Markdown: one file per tree, plus
  // cross-repo.md for a multi-tree run) in addition to stdout. Each run writes to
  // <dir>/zzop.<epoch>/ so runs accumulate. Omit "report" entirely to keep the
  // defaults (dir "zzop-reports", formats ["md"]).
  // "report": {
  //   "dir": "zzop-reports",
  //   "formats": ["md", "json", "sarif"],
  //   "enabled": true // set false to disable report writing entirely
  // },

  // Exit non-zero when any finding is at or above this severity, or "off" to
  // always exit 0.
  "failOn": "warn"
}
```

### Severity values

Config severities are normalized to the engine's three levels:

| You write | Becomes |
| --- | --- |
| `off`, `none`, `disabled` | rule disabled |
| `info`, `note`, `low` | `info` |
| `warn`, `warning`, `medium` | `warning` |
| `error`, `critical`, `high` | `critical` |

`failOn` uses the same names (plus `off` to never fail). Ordering: `info` < `warning` < `critical`.

On a multi-tree run (`roots` with 2+ entries, or `trees`), `failOn` also gates the cross-layer findings —
the `cross-layer/*` rules run over the join between trees (`duplicate-route`, `route-shadowing`,
`unprovided-mutation-call`, `external-secret-in-url`, and others; see
[docs/rules/catalog.md](../../docs/rules/catalog.md)). Most are `warning`-tier, so they fail CI under the
default `failOn: "warn"` exactly like a per-tree finding does; a handful of `info`-tier self-reports
(`unconsumed-endpoint`, `route-near-miss`, the coverage/blindness notes) never do under the default. In the
pretty terminal report, cross-layer findings print in their own "Cross-layer findings:" section after the
per-tree file groups, since the same relative file path can exist in two different trees.

## Examples

Analyze the current directory, fail CI on any warning or worse:

```sh
zzop
```

Analyze a monorepo's two layers and emit JSON for a downstream tool:

```jsonc
// zzop.config.jsonc
{
  "trees": [
    { "root": "./services/api", "sourceId": "api" },
    { "root": "./apps/web", "sourceId": "web" }
  ],
  "format": "json",
  "failOn": "critical"
}
```

```sh
zzop --config zzop.config.jsonc > report.json
```

Turn off one noisy rule and downgrade another to info, using a custom pack directory:

```jsonc
{
  "roots": ["."],
  "packs": { "extraDirs": ["./zzop-packs"] },
  "rules": {
    "n-plus-one": "off",
    "toctou": "info"
  }
}
```

## License

MIT
