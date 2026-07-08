# Getting started

The fastest path from "nothing installed" to a report on your own repo, plus how to read that report and
quiet a false positive. For the full config schema and CLI flag reference, see
[`packages/cli/README.md`](../packages/cli/README.md) — this page does not duplicate it.

## Install & first run

Requires Node.js >= 18. The engine (`@zzop/native`) ships a prebuilt binary for your platform and
installs automatically as a dependency of the CLI — no Rust toolchain needed.

```sh
npm i -D @zzop/cli
npx zzop init      # writes an annotated zzop.config.jsonc to the current directory
npx zzop           # analyzes using that config and prints a report
```

`zzop init` refuses to overwrite an existing config; pass `--force` to replace it. Once installed as a
dev dependency, drop the `npx` prefix if you add a `package.json` script, or keep it for one-off runs.

See [`packages/cli/README.md`](../packages/cli/README.md) for the full `zzop.config.jsonc` schema
(`roots`/`trees`, `packs`, `rules`, `git`, `cacheDir`, `sizeCap`, `format`, `report`, `failOn`).

## Reading the output

**Severity.** Every finding is one of three levels:

| Severity | Roughly means |
| --- | --- |
| `critical` | A confirmed correctness/security issue — the kind of thing that should block a merge. |
| `warning` | A likely issue or architectural smell worth a look, not necessarily urgent. |
| `info` | Lower-confidence or advisory — useful context, high volume. |

**Info-folding.** By default `info`-level findings are folded into a per-rule count so `warning`/
`critical` findings stay visible in the terminal. Pass `-a`/`--all` to expand every `info` finding.

**`--severity <critical|warning|info|off>`** filters what's *displayed* (default `off` = show all). It
never changes the exit code — that's always computed from the full, unfiltered finding set against
`failOn`.

**Exit codes:**

| Code | Meaning |
| --- | --- |
| `0` | Ran successfully; no finding at or above `failOn`. |
| `1` | At least one finding at or above `failOn` (CI gate). |
| `2` | Config or usage error. |

**Every run also writes a Markdown report to disk by default** — the hand-off surface for a cross-repo
review (e.g. a frontend agent reviewing a backend, or attaching results to a PR): `./zzop-reports/
zzop.<epoch-seconds>/` gets one `<sourceId>.md` per analyzed tree, plus a `cross-repo.md` summary (edges,
unresolved/unprovided/unconsumed buckets, and any "this tree is blind to its own consumes" self-reports)
when the run covers more than one tree. `--out <dir>` (or config `report.dir`) overrides the base
directory; each run gets a fresh `zzop.<epoch>/` subdir, so runs accumulate rather than overwrite. Set
config `report.formats` (e.g. `["md", "json", "sarif"]`) to also emit
[SARIF 2.1.0](https://sarifweb.azurewebsites.net/) or raw JSON, or `report.enabled: false` to turn report
writing off entirely.

## Suppressing findings

There are four mechanisms, at three different scopes. This section is the one place they're all listed
together — each links to its authoritative doc.

**(a) Inline suppress marker (in code, per line).** Some DSL rules define a `suppress_marker` — a
`//`-comment on the finding's own line, or the line directly above it, silences that one finding. The
marker name is rule-specific; when a rule has one, its `message` tells you what to write. Example (the
n+1 rule's marker is `n+1-ok`):

```ts
const items = list.map(x => db.find(x.id)); // n+1-ok: batched below, false positive
```

Full semantics (lookback window, regex-escaping, which matchers support it) in
[rules/dsl-reference.md](rules/dsl-reference.md#suppress-marker-semantics).

**(b) Config-level (per project, in `zzop.config.jsonc`).** Turn a rule off, override its severity, or
drop it for matching file paths:

```jsonc
"rules": {
  "no-explicit-any": "off",
  "dead-candidates": { "exclude": ["**/app/**/{page,layout,route}.tsx"] }
}
```

`failOn` controls which severity fails CI (`"warn"`, `"critical"`, or `"off"` to never fail). Full schema
in [`packages/cli/README.md`](../packages/cli/README.md#configuration).

**(c) SDK/embedding-level (per call, when embedding the engine directly).** Callers of `@zzop/native`
pass `suppressions` (finding-level accept-list by rule + path/glob), `disabledRules`, or
`severityOverrides` on the request:

```json
{ "suppressions": [{ "rule": "sql/nplus1", "path": "legacy/" }] }
```

Full field shapes in [modules/napi.md](modules/napi.md) (see `AnalyzeRequest`).

**(d) Caveat: native cross-layer analyses are disable-only.** The 20 `cross-layer/*` native rules (run
over `analyzeTrees`' joined IO graph) have no source line to anchor an inline marker against — silence
one only via `disabledRules`/config `rules` `"off"`, never a comment. See
[modules/napi.md](modules/napi.md) for why (no single tree owns a cross-layer finding).

## Where to next

- [ARCHITECTURE.md](ARCHITECTURE.md) — how a tree gets processed: the IR, route/IO extraction, caching, degraded files.
- [modules/napi.md](modules/napi.md) — embed the engine directly (Node API surface, request/response shapes).
- [rules/authoring-guide.md](rules/authoring-guide.md) — write and ship a new DSL rule pack.
- [NORMALIZED_AST.md](NORMALIZED_AST.md) and [../examples/](../examples/README.md) — extend zzop to a new language or framework via an external parser/adapter.
