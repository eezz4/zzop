# @zzop/cli

**zzop** joins your repos on their contracts — frontend calls matched against backend routes across a
repo boundary, with near-misses named instead of left for a human to diff by hand — and does it
deterministically: same code in, same findings out, byte-stable across runs, so you can gate a PR on
contract drift (`failOn`) without flaky rechecks. Alongside that join, the same engine runs as a
multi-language SAST/architecture analyzer over each repo individually. `@zzop/cli` is the config-driven
front end: install it (`npm i -D @zzop/cli`), write a `zzop.config.jsonc`, run `npx zzop` — no code,
ESLint-style. (A bare one-off `npx zzop` without the install fails — no package named `zzop` exists;
use `npx @zzop/cli` for one-off runs.)

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
| `zzop init adapter --mode <a\|b> --kind <consume\|provide> [--force]` | Scaffold a self-contained starter adapter into `./zzop-adapter/` (`main.mjs`, bundled `lib/keys.mjs` + `lib/envelope.mjs`, `README.md`). `--mode a` = full envelope (replaces native analysis for the tree); `--mode b` = io-only overlay (merged via the `overlays` config key). `--kind` selects which side's extraction TODOs are stubbed in. Refuses to overwrite an existing `zzop-adapter/` without `--force`. See [docs/adapters/README.md](../../docs/adapters/README.md). |
| `zzop [run] [options]` | Load the config, analyze, and print. This is the default command. |
| `zzop endpoint <pattern>` | Definitive io-key query: is `<pattern>` (a case-insensitive substring of any io key — http routes, env keys, DB tables, topics) provided, consumed, or joined? Runs the same config-driven analysis as `zzop run` (honors `--config`; a configured `cacheDir` makes the re-run cheap) and prints ONE verdict — `linked` \| `provided-only` \| `consumed-unprovided` \| `external` \| `unresolved-only` \| `ambiguous` \| `mixed` \| `not-found` — with the matching sites (`file:line (source)`), related findings, and key suggestions on `not-found`. `--json` prints the raw query JSON (the same shape the zzop-mcp `check_endpoint` tool returns — both run one shared query core). Exits `0` regardless of verdict (a query is not a gate); `2` = config/usage error. |
| `zzop adapter validate <envelope.json>` | Check an adapter envelope offline: structural validation against the v1 envelope contract plus lint hints (unnormalized `http` keys, host-carrying provide keys, duplicate provides, absolute file paths). Exits non-zero if the envelope is structurally invalid; hints never affect the exit code. Attach a valid overlay envelope to a run via the `overlays` config key. |
| `zzop pack validate <pack.json>` | Check a DSL rule-pack JSON offline, before loading it: the same judgments the engine's pack loader makes at load time (bad JSON, missing field, wrong type, too-new `schema_version`) plus any matcher regex that fails to compile (such a rule would load but silently never fire). Structure only — it never judges rule quality or semantics. Exits non-zero if the pack is invalid. The machine-readable shape contract is [docs/contracts/rule-pack.schema.json](../../docs/contracts/rule-pack.schema.json); see [docs/rules/dsl-reference.md](../../docs/rules/dsl-reference.md) for the fields. |

Every command also answers `--help`/`-h` with a focused help block for that command (e.g.
`zzop init adapter --help`), exiting `0`; a bare `zzop --help` prints the full usage.

### `run` options

| Option | Description |
| --- | --- |
| `--config <path>` | Config file to load (default `./zzop.config.jsonc`). |
| `--format <pretty\|json>` | Output format, overriding the config's `format`. |
| `--json` | Alias for `--format json`. |
| `--out <dir>` | Override the report base directory (default `./zzop-reports`; equivalent to config `report.dir`). Each run writes to `<dir>/zzop.<epoch>/`, a fresh subdir per run so runs accumulate. |
| `-a, --all` | Show everything expanded: info-level findings (folded to a per-rule count by default so warnings/errors stay visible) AND each finding's full message (folded to a one-line summary by default), plus a one-line rule-pack load confirmation (`N packs loaded (M rules): ...` — the output's `packsLoaded` field). The complete message is always in the JSON output and markdown reports regardless of this flag. |
| `--severity <critical\|warning\|info\|off>` | Only display findings at or above this severity (default `off` = show all). This is a display filter only — the exit code is always computed from the unfiltered findings and the config's `failOn`, never from `--severity`. |
| `--debug-io` | After the normal output, dump every cross-layer join bucket (`edges`, `unconsumedProvides`, `unprovidedConsumes`, `unresolvedConsumes`, `externalConsumes`, `ambiguousConsumes`) as deterministic plain text, one section per bucket and one line per entry — the join-debug surface for troubleshooting an adapter/overlay. A no-op single-tree run still prints every section, each at count 0. |
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

These apply to `zzop [run]` (the default analysis command):

| Code | Meaning |
| --- | --- |
| `0` | Ran successfully; no finding at or above `failOn`. |
| `1` | At least one finding at or above `failOn` (CI gate). |
| `2` | Config or usage error. |

`failOn` defaults to `warn` when omitted from the config, so a first run on an untuned repo exiting `1` is
normal and expected — not a tool error. Triage the output, exclude non-deployed surface via `exclude`, and
keep `failOn` gating in CI from there.

`zzop adapter validate <path>` does not read `failOn` — its `0`/`1` mean the envelope passed/failed
structural validation instead (see the Commands table above); `2` is still a usage error (bad path,
malformed JSON). `zzop pack validate <path>` follows the same rule: `0`/`1` mean the rule pack
passed/failed its structure check, `2` is a usage error. `zzop endpoint <pattern>` does not read
`failOn` either — it exits `0` on any successful query regardless of verdict (a query is not a
gate); `2` is still a config/usage error.

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
  //   {
  //     "root": "./api", "sourceId": "api",
  //     // Gateway/ingress prefix this tree is served behind (shorthand for a
  //     // whole-tree mount).
  //     "mountedAt": "/api",
  //     // Monorepo: per-directory prefixes for sub-apps mounted at different
  //     // paths. Longest matching "dir" (tree-relative) wins per file. Stacks
  //     // on top of prefixes zzop extracts from code (e.g. Nest setGlobalPrefix).
  //     "mounts": [{ "dir": "apps/settle", "at": "/settle" }],
  //     // Hosts that serve this tree: an absolute-URL call to one of these
  //     // from another tree joins as a call into this tree, keyed by path.
  //     "hosts": ["api.foo.com"]
  //   },
  //   { "root": "./web", "sourceId": "web" }
  // ],

  // Monorepo shortcut: "trees": "auto" expands to one tree per workspace package
  // (sourceId = each package's name), detected from pnpm-workspace.yaml or
  // package.json "workspaces". Turns the cross-layer join on with no hand-authoring;
  // run zzop from the workspace root.
  // "trees": "auto",

  "packs": {
    // Extra directories of custom DSL rule packs (rules/dsl/*.json). These MERGE
    // with the bundled packs; a custom pack whose id matches a bundled one wins.
    "extraDirs": ["./zzop-packs"],
    // Whole packs to disable, by pack id.
    "disabled": ["browser"]
  },

  "rules": {
    // "off"                        -> disable the rule
    "typescript/no-explicit-any": "off",
    // "info" | "warn" | "critical" -> override severity
    "sql/nplus1": "warn",
    // object form -> override severity AND drop findings by file path.
    // Each `exclude` entry is a plain substring, OR a glob if it contains
    // `*`/`?`/`{}` (full-path: `*`/`?` stay within a segment, `**` spans `/`,
    // `{a,b}` alternates). `[...]` stays literal so raw `app/[id]/` paths work.
    "sql/race-condition-toctou": { "severity": "warn", "exclude": ["legacy/"] },
    "dead-candidates": { "exclude": ["**/app/**/{page,layout,route}.tsx"] }
  },

  // Top-level exclude: path globs/substrings dropped from EVERY rule's findings
  // (files are still parsed for the dep graph). Same glob rules as per-rule
  // exclude above — a `*` stays within a path segment, use `**/` to cross
  // directories.
  "exclude": ["**/*.stories.tsx", "legacy/"],

  // Attach a Mode B adapter's output: an array of paths to overlay envelope JSON
  // files. Valid at the top level (applies to every tree) and/or per-tree as
  // "trees[i].overlays" (adds to that tree only). See
  // docs/adapters/README.md for the envelope format.
  // "overlays": ["./my-adapter/envelope.json"],

  // Enables git-history-derived signals. Omit to use engine defaults.
  // "since" windows history collection to a git-log-style time filter (e.g.
  // "2 weeks ago", "1.year", an ISO date); omitted = full history.
  // "recentDays" windows the recent-activity fields (default 30).
  // "commitTypePatterns" teaches a non-English/non-conventional commit convention: an array of
  // { "pattern": "<regex>", "tag": "FIX"|"FEAT"|... }, checked in array order (earlier entries win,
  // mirroring the built-in REVERT-before-FIX ordering). When present and non-empty it REPLACES the
  // default FIX/FEAT/REVERT/.../STYLE table entirely; a pattern that fails to compile as a regex is
  // skipped (matches nothing) and reported as a warning, never a crash. Omit for the default table.
  "git": { "recentDays": 30 },
  // "git": {
  //   "since": "1.year",
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

Rule ids under `rules` follow one format rule: a DSL-pack rule uses its full `pack/rule` id (e.g.
`typescript/no-explicit-any`, `sql/nplus1`), while a native analysis id is used as-is, with no pack prefix
(e.g. `dead-candidates`).

A default run writes to disk: the report notice (`Wrote N reports to <dir>`) goes to **stderr**, not
stdout, so `--format json`/`--json` output on stdout stays parseable even when reports are also being
written. Two directories appear by default — `zzop-reports/` (persisted reports; `--out`/config
`report.dir` relocates it, `report.enabled: false` disables it) and `.zzop-cache/` (the analysis cache;
`zzop init`'s generated config enables it by default, and omitting config `cacheDir` disables caching).
Add both to `.gitignore`.

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

### Connection topology

For a cross-repo/cross-tree join, some facts about how trees connect at runtime live only in deployment
infra — a gateway's ingress rules, which host serves which service — never in either repo's own code.
That is the one class of join information zzop cannot recover by reading source, so declare it per tree,
alongside `sourceId`:

- `mountedAt: "/api"` — shorthand for a whole-tree gateway/ingress prefix (equivalent to a `mounts` entry
  with `dir: ""`, applied after every `mounts` entry — an explicit `dir: ""` mount of your own wins a tie
  against this shorthand).
- `mounts: [{ "dir": "apps/settle", "at": "/settle" }]` — for monorepos where different sub-apps are
  mounted at different paths; per file, the longest matching `dir` (tree-relative) wins. Mounts stack ON
  TOP of any prefix zzop already extracts from code (e.g. Nest's `setGlobalPrefix`) — the gateway sits
  outside the app.
- `hosts: ["api.foo.com"]` — an absolute-URL call from another tree to one of these hosts (`http`/`https`
  only) stops counting as external egress and is treated as an internal call (re-keyed to its path) that
  joins normally instead (a call to `https://api.foo.com/users` re-keys to `/users` and can match any
  tree's provide at that path, not only this tree's).

A mount or host that ends up with zero effect on the join produces a warning (stale config self-discloses
instead of silently doing nothing); a prefix that's simply wrong shows up in the near-miss/prefix-drift
findings rather than failing silently. Values are literal paths, not rewrite patterns. A path that doesn't
start with `/`, or contains `://`, fails config loading with an error.

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
    "sql/nplus1": "off",
    "sql/race-condition-toctou": "info"
  }
}
```

## License

MIT
