# Getting started

The fastest path from "nothing installed" to a report on your own repo, plus how to read that report and
quiet a false positive. For the config schema see [`docs/modules/facade.md`](modules/facade.md) (per-tree
request fields) and [`docs/modules/mcp.md`](modules/mcp.md) (config discovery/mapping semantics);
`zzop contract config-surface` prints the authoritative key vocabulary. For the CLI/MCP invocation
reference see [`crates/host/README.md`](../crates/host/README.md). This page does not duplicate them.

## Install & first run

zzop's primary distribution is two Node-free binaries — `zzop` (the CLI, for the terminal workflow
below) and `zzop-mcp` (the MCP server, for AI-agent clients) — no Node.js, no npm, no Rust toolchain
needed. Pick one of the four install lanes listed in the repo README's
[Quick start](../README.md#quick-start) — release asset, Claude Code plugin, Claude Desktop `.mcpb`
bundle, or `npm i -g @zzop/cli`; this page does not restate them. Plugin install/troubleshooting
detail (the `SessionStart` hook, where the binary lands, how updates are reported rather than applied)
is in [`crates/host/README.md`](../crates/host/README.md#install-as-a-claude-code-plugin).

```sh
zzop analyze .          # analyzes the current directory and prints a report
zzop cross ./frontend ./backend   # cross-layer join across 2+ trees
```

**That first run writes one thing into your repo:** a `.zzop/cache/` directory (the default `cacheDir`),
holding the per-file analysis cache. It is pure derived state — safe to delete whenever — so ignore it
with an anchored `/.zzop/` line in `.gitignore` (not a `zzop*` glob: that would also swallow an authored
`zzop/` rule-pack directory). Set `"cacheDir": null` in your config to turn caching off and write
nothing at all; see [ARCHITECTURE.md](ARCHITECTURE.md#caching).

There is no scaffolding subcommand — write `zzop.config.jsonc` by hand (the smallest useful one on a
monorepo is `{ "trees": "auto" }`; the full key vocabulary is `zzop contract config-surface`) and pass it
explicitly:

```sh
zzop cross --config zzop.config.jsonc
```

**Analyzing a monorepo root as ONE tree is a trap the tool now calls out.** A root that resolves to a single
tree cannot run the cross-layer join — it needs 2+ trees. If that root carries a workspace manifest
(`pnpm-workspace.yaml`, or a `package.json` with `workspaces`) resolving to 2+ packages, the run's first
`configWarnings` entry names that manifest, the exact package count, the fact that the join did not run, and
the one-line remedy: `"trees": "auto"`, which expands every workspace package into its own tree.
You see this on `zzop analyze .` (and the `analyze_repo` tool), and on `zzop endpoint`/`check_endpoint`
when you pass a single path — the entry points that resolve one tree from a root. `zzop cross` never shows
it: it always has 2+ trees, so the join really did run. Three deliberate silences: a manifest resolving to
fewer than 2 packages says nothing (`"trees": "auto"` could not deliver the join there either, so the advice
would be false); a config that explicitly declares `trees` (an array, or `"auto"`) says nothing — naming the
tree set IS your answer to "which trees?", and `"auto"` prints its own expansion report instead; and a
multi-root config says nothing, because the join actually ran. A config that exists but never declares
`trees` is **not** suppressed: it resolves to one tree exactly like no config at all, so it gets the same
warning, worded for its own remedy (`Add "trees": "auto"` to that config).

The `zzop.config.jsonc` keys (`roots`/`trees` — including per-tree `mountedAt`/`mounts`/`hosts` connection
topology — `packs`, `rules`, `git`, `cacheDir`, `sizeCap`, `format`, `report`, `failOn`) are documented in
[`docs/modules/facade.md`](modules/facade.md) and [`docs/modules/mcp.md`](modules/mcp.md); the machine-
readable source of truth is `crates/config/config-surface.json`, printable via `zzop contract config-surface`.

## Reading the output

`zzop analyze` prints a single JSON object to stdout — a shaped summary, not a raw dump. It carries
full finding counts by severity and by rule, engine warnings, and a capped findings list (default 50;
truncation is always disclosed). When git signals ran, it also carries a compact `architecture` object
(pain score, top recommendation, top critical files). This is the exact same summary the `analyze_repo`
MCP tool returns — the CLI subcommand and the tool share one handler, so they never disagree.

**`warnings` is where "what did NOT run" is reported**, and it is worth reading before the findings: a
short findings list can mean a clean repo or a blind engine, and only this array tells the two apart. It
carries unparsed extensions, framework surfaces zzop could not see, minified files skipped — and rules
that deliberately stood down. A few rules rest on a fact your source cannot state (which directory is the
config module, for instance); rather than guess it, they emit nothing and say so here, naming what to
declare and how many sites they left unjudged. Declaring it is the `overlays` key (see
[NORMALIZED_AST.md](NORMALIZED_AST.md) and [`examples/auth-overlay-adapter`](../examples/auth-overlay-adapter/README.md)
for a complete one); leaving the rule off is `rules: { "<pack>/<rule>": "off" }`, and saying so
explicitly stops the warning too.

**Severity.** Every finding is one of three levels:

| Severity | Roughly means |
| --- | --- |
| `critical` | A confirmed correctness/security issue — the kind of thing that should block a merge. |
| `warning` | A likely issue or architectural smell worth a look, not necessarily urgent. |
| `info` | Lower-confidence or advisory — useful context, high volume. |

**Exit codes:**

| Code | Meaning |
| --- | --- |
| `0` | Ran successfully (regardless of what was found). |
| `1` | Analysis/runtime error. |
| `2` | Usage or config error. |

The binary does **not** gate its exit code on finding severity: it is an analysis + summary surface, not
a CI linter. To gate CI, read the JSON counts yourself (e.g. fail the job when `bySeverity.critical > 0`).
The `format`, `report`, and `failOn` config keys are *recognized* (they do not trigger unknown-key
warnings) but nothing acts on them today — no shipped surface renders a terminal report, writes a
report file, or gates its exit code on them; `@zzop/cli` (see
[`packages/cli/README.md`](../packages/cli/README.md)) is the identical native binary, not a separate
presentation layer.

## Suppressing findings

There are three mechanisms, at three different scopes, plus one caveat about where none of them reach.
This section is the one place they're all listed together — each links to its authoritative doc.

**(a) Inline suppress marker (in code, per line).** Every DSL rule whose matcher anchors at a source line
— `line-scan`, `method-scan`, `io-scan` — has an inline marker derived from its id — `<rule-id>-ok`;
`symbol-scan` findings have no line to anchor a comment against and honor no marker (no shipped rule uses
that matcher today, and `zzop explain <rule-id>` says so per rule). A comment carrying that marker on the
finding's own line, or the line directly above it, silences that one finding; each rule's `message` states
the exact marker. Which comment leader is
recognized depends on the matcher, not on the file: `line-scan`/`method-scan` rules read `//` (plus `--`
in a `.sql` file), while `io-scan` rules — whose anchor line can come from any language that registers an
HTTP route — read `//` **or** `#`, so `# auth-gates-ok` on a FastAPI route works exactly like
`// auth-gates-ok` on an Express one. Those `io-scan` markers are NATIVE-analysis only: a full-envelope
run (`zzop analyze-envelope`, Mode A) carries no source text, so there is no anchor line to read and the
marker is inert there. It fails toward firing, not toward silence — a matching finding reports
unsuppressed rather than being guessed away. `route-exposure`'s guard-hint carve-out reads that same
anchor line and goes inert with it, so under a full envelope a matching route fires even when its
registration line carries a guard-hint argument. Clear one there by injecting the attribute the rule
reads (`auth-guarded` for `auth-gates`) or by disabling the rule in config (mechanism (b) below).
Mode B adapter overlays are unaffected: they merge onto a natively-parsed tree whose source text is on
disk, so both channels stay live. A marker this rule doesn't
honor no longer fails silently: whether it's a typo or a marker borrowed from another rule, a comment
shaped like a marker is called out in the finding's message, which names both the token you wrote and the
marker this rule actually honors. Example (the
`sql/nplus1` rule's marker is `nplus1-ok`):

```ts
const items = list.map(x => db.find(x.id)); // nplus1-ok: batched below, false positive
```

Full semantics (lookback window, regex-escaping, which matchers support it) in
[rules/dsl-reference.md](rules/dsl-reference.md#suppress-marker-semantics).

Native analyses are disable-only (mechanism (b) below) with TWO comment-driven exceptions, neither derived
from a rule id:

- `non-idempotent-write` / `unsafe-read-endpoint` honor a hand-written `// idempotent-ok: <reason>` on the
  handler's body-start line or up to 3 lines above it. The trailing colon is REQUIRED here, unlike every
  derived marker, and a multi-line signature counts from the body's `{`, not the `function` keyword. A
  near-miss IS disclosed here too: a comment in that window shaped like a suppression marker but not
  matching this one (a missing colon, a different id) is named in the finding's message rather than
  silently ignored — the native scanner runs its own near-miss pass, separate from the derived-marker
  one described in (a). Neither rule needs suppressing outside TypeScript, because neither can fire
  there: each needs store-write evidence that only the TypeScript parser produces
  (ts/tsx/js/jsx/mjs/cjs/mts/cts), so a Python/Go/Rust/C#/Java repo gets zero findings from both
  regardless of what its handlers write. Zero there means NOT ANALYZED, not "no risky write" — the
  full sightline (including why it is one extension narrower than `mutating-route-no-auth`, which does
  walk Java) is in [rules/catalog.md](rules/catalog.md).
- `dead-candidates` / `dead-exports` skip any file whose first 8 lines carry a generated-file banner
  (`@generated`, `auto-generated`, `code generated by`, "this file was generated", …). A bare
  "DO NOT EDIT" deliberately does NOT count — the marker must name generation.

**(b) Config-level (per project, in `zzop.config.jsonc`).** Turn a rule off, override its severity, or
drop it for matching file paths. Keys are matched by exact rule id: a DSL rule's id is the full
`"{pack}/{rule}"` string (e.g. `sql/nplus1`, `sql/race-condition-toctou`), while a native analysis id
is used as-is (e.g. `dead-candidates` — and note some native ids contain a slash of their own, like
`cross-layer/unconsumed-endpoint`; that slash is part of the native id, not a pack prefix):

```jsonc
"rules": {
  "typescript/no-explicit-any": "off",
  "dead-candidates": { "exclude": ["**/app/**/{page,layout,route}.tsx"] }
}
```

(`failOn` is a recognized severity-threshold key but the Node-free binary does not act on it today — see
"Reading the output" above.) Full schema in [`crates/host/README.md`](../crates/host/README.md).

**(c) SDK/embedding-level (per call, when embedding the engine directly).** Callers embedding
`zzop-facade` directly — or driving the engine JSON contract via `zzop`'s subcommands — pass
`suppressions` (finding-level accept-list by rule + path/glob), `disabledRules`, or
`severityOverrides` on the request:

```json
{ "suppressions": [{ "rule": "sql/nplus1", "path": "legacy/" }] }
```

Full field shapes in [modules/facade.md](modules/facade.md#the-zzop-facade-json-contract) (see `AnalyzeRequest`).

**(d) Caveat: native cross-layer analyses are disable-only.** The `cross-layer/*` native rules (run
over the cross-layer join, `zzop cross`) DO anchor at a `file:line` — the provide or consume site the
finding is about — but no marker is read there: derived `<id>-ok` markers are a DSL-rule mechanism and are
never wired into a native finding, and the finding itself is a joint-analysis result no single tree owns,
so a comment in one tree could not speak for it. Silence one only via `disabledRules`/config `rules`
`"off"`, never a comment. See
[modules/facade.md](modules/facade.md#the-zzop-facade-json-contract) for why (no single tree owns a cross-layer finding).

## Where to next

- [ARCHITECTURE.md](ARCHITECTURE.md) — how a tree gets processed: the IR, route/IO extraction, caching, degraded files.
- [modules/facade.md](modules/facade.md#the-zzop-facade-json-contract) — embed the engine directly (the `zzop-facade` JSON contract, request/response shapes).
- [rules/authoring-guide.md](rules/authoring-guide.md) — write and ship a new DSL rule pack.
- [NORMALIZED_AST.md](NORMALIZED_AST.md) and [../examples/](../examples/README.md) — extend zzop to a new language or framework via an external parser/adapter.
