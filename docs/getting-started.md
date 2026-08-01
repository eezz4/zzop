# Getting started

The fastest path from "nothing installed" to a report on your own repo, plus how to read that report and
quiet a false positive. For the config schema see [`docs/modules/facade.md`](modules/facade.md) (per-tree
request fields) and [`docs/modules/mcp.md`](modules/mcp.md) (config discovery/mapping semantics);
`zzop contract config-surface` prints the authoritative key vocabulary. For the CLI/MCP invocation
reference see [`packages/README.md`](../packages/README.md). This page does not duplicate them.

## Which binary do you want?

zzop's primary distribution is two Node-free binaries, driven completely differently. Pick before you
install:

| If you want | Use | How you drive it |
|---|---|---|
| An AI agent (Claude Code, Claude Desktop, any MCP client) to answer questions about your repos | `zzop-mcp` — an MCP server over stdio | Install the plugin or the `.mcpb` bundle; the agent calls the tools. You run no commands. Surface: [modules/mcp.md](modules/mcp.md#mcp-surface). |
| To run analyses yourself — a terminal, a CI job, a script | `zzop` — a plain CLI | `zzop analyze .`, `zzop cross ./web ./api`, … JSON to stdout. Surface: [modules/mcp.md](modules/mcp.md#cli-surface). |

Both dispatch to the same shared handlers over the same engine, so a tool call and a CLI run against the
same path analyze identically and reach the same verdict. What each surface lets you *ask for* differs
in one way only:

- **The findings-list filters reach both.** `analyze_repo`/`cross_repo`/`analyze_envelope` accept
  `severity` (minimum severity), `rule` (an exact rule id) and `limit` (list cap, default 50, max 1000);
  their CLI twins `analyze`/`cross`/`analyze-envelope` take the same three as
  `--severity`/`--rule`/`--limit`, parsed into the same shared filter, so neither surface can filter
  differently. Counts by severity and rule cover everything on both surfaces regardless; the cap only
  truncates the listed findings, and truncation is always disclosed.
- **Some lanes are CLI-only, with no MCP tool twin:** `manifest`, `diff`, `facts`, `coverage`, `graph`, `explain`
  and `init`. Which lanes those are, and why each one is unpaired, is answered in one place —
  [CLI-only lanes](modules/mcp.md#cli-only-lanes-manifest--diff--explain--facts--coverage--graph--init) — not by
  this sentence. (The lanes that project an analysis are recorded as a machine-readable contract in
  [surface-parity.json](contracts/surface-parity.json)'s `_cliOnlyLanes`, whose keys are that list;
  `explain` and `init` are deliberately outside that registry — neither projects an analysis — so the
  registry alone would under-report, and the linked section carries their reasons instead.)

**The rest of this page is the CLI workflow.** For an agent-driven session the equivalents are
`analyze_repo` (for `zzop analyze`), `cross_repo` (for `zzop cross`), `check_endpoint` (for
`zzop endpoint`) and `check_file` (for `zzop file`); everything below about configuration, reading the
output, severity and suppression applies unchanged, because both surfaces return the same summary from
the same handler.

## Install & first run

No Node.js, no npm, no Rust toolchain needed for either binary.
Pick one of the four install lanes listed in the repo README's
[Quick start](../README.md#quick-start) — release asset, Claude Code plugin, Claude Desktop `.mcpb`
bundle, or `npm i -g @zzop/cli`; this page does not restate them. Plugin install/troubleshooting
detail (the `SessionStart` hook, where the binary lands, how updates are reported rather than applied)
is in [`packages/README.md`](../packages/README.md#install-as-a-claude-code-plugin).

```sh
zzop init               # REQUIRED FIRST: writes the starter zzop.config.jsonc (see below)
zzop analyze .          # analyzes the current directory and prints a report
zzop cross ./frontend ./backend   # cross-layer join across 2+ trees
zzop analyze . --severity critical --limit 10   # narrow the findings LIST (counts still cover everything)
zzop analyze --config ./ci/zzop.config.jsonc    # a config that does not sit at the tree root
zzop analyze --help     # that one subcommand's line, on stdout, exit 0 — `zzop help` prints them all
```

**That first run writes one thing into your repo:** a `.zzop/cache/` directory (the default `cacheDir`),
holding the per-file analysis cache. Set `"cacheDir": null` in your config to turn caching off and write
nothing at all; see [ARCHITECTURE.md](ARCHITECTURE.md#caching).

### Two directories, one letter apart

zzop uses two on-disk locations next to your config, and the difference between them is a single
leading dot:

| Directory | Whose | In version control? |
|---|---|---|
| `.zzop/` | zzop's — derived, disposable | **No.** Delete it whenever; the next run rebuilds it. Today it holds `.zzop/cache/`. |
| `zzop/` | yours — hand-authored | **Yes.** Custom DSL rule packs in `zzop/rules/`, adapter overlays in `zzop/adapters/`. |

Because the two names are one character apart, ignore the derived one with an **anchored** pattern —
`**/.zzop/`, which is what this repo's own `.gitignore` uses — and never with a `zzop*` glob. A glob
matches both, so it would quietly take every rule pack you wrote out of version control, with no error
from git or from zzop. That is why the ignore rule is written the way it is rather than the shorter way.

`zzop/rules/` is also a **default discovery location**: if that directory exists and your config does not
declare `packs.extraDirs`, the packs in it load without your naming them. Declaring `packs.extraDirs`
takes over completely — the default is a fallback, never merged into your list, so pack directories always
have exactly one origin. Declaring it as `[]` is the explicit "no pack directories" opt-out. A repo with no
`zzop/rules/` gets no warning; there is nothing to report.

### Turning off packs you don't need

Every loaded pack costs the rule evaluations it performs, and every run already tells you which packs
had nothing to scan. `packsLoaded[].filesInScope` counts how many of this tree's files a pack's rules
would even look at; a pack sitting at `filesInScope: 0` matched no file **path** here, so it cannot
report anything on this tree. You do not have to scan that array: the run emits one `warnings` line
naming every such pack, sorted, in one place.

The loop is three steps:

1. Run once, and read the `warnings` entry beginning `N loaded pack(s) had 0 files in scope`.
2. Decide pack by pack. It is a **path** fact, computed before any file's content is read — "no file
   here matches this pack's patterns", never "you don't use redis". Add one matching file tomorrow and
   the pack applies again with no config change, so keep the ones you may grow into.
3. Drop the ones you are sure about:

```jsonc
{ "packs": { "disabled": ["redis", "browser"] } }
```

A disabled pack is dropped whole at the pack gate, before any of its rules is evaluated — not filtered
out afterwards — so the rules it holds cost nothing. Two things worth knowing: it never changes what the
remaining packs report, and, because the disabled list is part of the analysis cache's ruleset
fingerprint, the first run after you edit it re-runs rules over every file once before the cache is warm
again.

zzop never does this for you, and that is deliberate. Which stacks a repo has is something you declare;
an engine that inferred it from a `package.json` would sometimes infer wrong, and the failure mode of a
wrong inference is security rules that silently do not run.

`zzop init` writes a starter `zzop.config.jsonc` into the current directory: every optional key with a
comment saying what it means, each already set to zzop's own value, so the file documents the defaults
rather than changing them. It refuses to overwrite an existing config unless you pass `--force`.

**A config is required.** Every analysis lane refuses a tree that has none, on both the CLI and the MCP
surface, and says so with a pointer to that same document. This is not ceremony: the `vocabulary` block
the starter file writes is the set of names zzop would otherwise have to GUESS about your project — what
you call your auth guards, which banners mark your generated files, how you name your data-access
receivers — and a key you do not declare is a judgment zzop does not make. Measured across 17 open-source
repositories, running with none of it declared changes 69 findings. The lanes that exist to GET you a
config, or to describe the binary, still run without one: `init`, `contract`, `explain`, `version`,
`help`, `validate-envelope`, `validate-rule-pack`, and `analyze-envelope` (an envelope carries no
filesystem location, so there is no tree for a config to describe). The same document prints without writing anything as `zzop contract
config-template`, and the full key vocabulary is `zzop contract config-surface`. Writing one by hand is
equally fine (the smallest useful config on a monorepo is `{ "trees": "auto" }`) — then pass it
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
`trees` is **not** suppressed: it resolves to one tree, so it gets the warning too, worded for its own
remedy (`Add "trees": "auto"` to that config).

The `zzop.config.jsonc` keys (`roots`/`trees` — including per-tree `trees[].topology` connection
topology — `packs`, `rules`, `git`, `vocabulary`, `cacheDir`, `sizeCap`, `exclude`, `overlays`) are documented in
[`docs/modules/facade.md`](modules/facade.md) and [`docs/modules/mcp.md`](modules/mcp.md); the machine-
readable source of truth is `crates/config/config-surface.json`, printable via `zzop contract config-surface`.

**When the defaults do not match your repository** — an extension zzop does not parse, guard names it
does not know, a gateway prefix it cannot see, a rule that does not exist yet —
[extending.md](extending.md) walks every plug-in point in the order you meet them, cheapest first, and
says plainly which one costs real work.

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
[NORMALIZED_AST.md](NORMALIZED_AST.md) and [`examples/adapters/auth-overlay-adapter`](../examples/adapters/auth-overlay-adapter/README.md)
for a complete one); leaving the rule off is `rules: { "<pack>/<rule>": "off" }`, and saying so
explicitly stops the warning too.

**Severity.** Every finding is one of three levels:

| Severity | Roughly means |
| --- | --- |
| `critical` | A confirmed correctness/security issue — the kind of thing that should block a merge. |
| `warning` | A likely issue or architectural smell worth a look, not necessarily urgent. |
| `info` | Lower-confidence or advisory — useful context, high volume. |

**What a severity level asserts — and what it does not.** A severity is a statement about zzop's
confidence in *that* finding, and confidence here is bounded by what the run could see, never by a
census of your running system (zzop reads source; it never calls your services). Two cross-layer rules
make the bound visible by de-escalating themselves: `cross-layer/unconsumed-mutation-endpoint` and
`cross-layer/unprovided-mutation-call` fire at `info` instead of `warning` when this run contains a
source whose HTTP calls came back mostly unresolved (respectively: a source that imports a server
framework yet yielded almost no routes), and they name that source in the message. "Nobody calls this
write endpoint" is not worth a warning when the callers may simply be invisible. The finding still
fires either way — this is a confidence match, not suppression.

The converse does **not** follow, and those rules say so in the warning-branch message too. Each
blindness check is a single narrow predicate — *is one source's `http` consumes majority-unresolved?*,
*does one source import a server framework yet extract almost no routes?* — so a rule staying at
`warning` means **blindness was not witnessed**, not that coverage was proven complete. A caller in a
call shape or language this extraction does not model, or in a repository outside this run at all, is
invisible to the blindness check exactly as it is to the rule. Before treating any severity as a
verdict, read the two channels that report the run's own limits: `warnings` (above) and the per-tree
`coverage` census, whose `joinContributionZero` flag asserts outright that a tree contributed nothing
to the cross-layer join. Per-rule conditions are spelled out in
[rules/catalog.md](rules/catalog.md).

**Exit codes:**

| Code | Meaning |
| --- | --- |
| `0` | Ran successfully (regardless of what was found). |
| `1` | Analysis/runtime error. |
| `2` | Usage or config error. |

The binary does **not** gate its exit code on finding severity: it is an analysis + summary surface, not
a CI linter. To gate CI, read the JSON counts yourself (e.g. fail the job when `bySeverity.critical > 0`).
`@zzop/cli` (see [`packages/cli/README.md`](../packages/cli/README.md)) is the identical native binary,
not a separate presentation layer, so there is no other output surface to switch to.

There used to be three config keys pointing at surfaces like these — `failOn` (a severity gate), `format`
(an output selector) and `report.*` (report files) — left over from a CLI removed in 2026-07. They were
accepted and then ignored: writing one produced no warning, no error and no behavior. As of 2026-07-26
they are **removed from the recognized key set**, so writing one now produces a `configWarnings` entry
saying it was removed and what to do instead. Deleting such a key from your config changes nothing about
a run; it only removes the warning.

## Suppressing findings

There are three mechanisms, at three different scopes, plus one caveat about where none of them reach.
This section is the one place they're all listed together — each links to its authoritative doc.

**(a) Inline suppress marker (in code, per line).** Every DSL rule whose matcher anchors at a source line
— `line-scan`, `method-scan`, `io-scan` — has an inline marker derived from its id — `zzop-<rule-id>-ok`;
`symbol-scan` findings have no line to anchor a comment against and honor no marker (no shipped rule uses
that matcher today, and `zzop explain <rule-id>` says so per rule). A comment carrying that marker on the
finding's own line, or the line directly above it, silences that one finding; each rule's `message` states
the exact marker. Which comment leader is
recognized depends on the matcher, not on the file: `line-scan`/`method-scan` rules read `//` (plus `--`
in a `.sql` file), while `io-scan` rules — whose anchor line can come from any language that registers an
HTTP route — read `//` **or** `#`, so `# zzop-protected-path-no-auth-evidence-ok` on a FastAPI route
works exactly like `// zzop-protected-path-no-auth-evidence-ok` on an Express one. Those `io-scan`
markers are NATIVE-analysis only: a full-envelope
run (`zzop analyze-envelope`, Mode A) carries no source text, so there is no anchor line to read and the
marker is inert there. It fails toward firing, not toward silence — a matching finding reports
unsuppressed rather than being guessed away. `dev-path-no-guard-hint`'s guard-hint carve-out reads that
same anchor line and goes inert with it, so under a full envelope a matching route fires even when its
registration line carries a guard-hint argument. Clear one there by injecting the attribute the rule
reads (`auth-guarded` for `protected-path-no-auth-evidence`) or by disabling the rule in config
(mechanism (b) below).
Mode B adapter overlays are unaffected: they merge onto a natively-parsed tree whose source text is on
disk, so both channels stay live. A marker this rule doesn't
honor no longer fails silently: whether it's a typo or a marker borrowed from another rule, a comment
shaped like a marker is called out in the finding's message, which names both the token you wrote and the
marker this rule actually honors. Example (the
`sql/nplus1` rule's marker is `zzop-nplus1-ok`):

```ts
const items = list.map(x => db.find(x.id)); // zzop-nplus1-ok: batched below, false positive
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
  full sightline (including why it is narrower than `mutating-route-no-auth`, which also walks Java and
  Python) is in [rules/catalog.md](rules/catalog.md).
- `dead-candidates` / `unimported-export` skip any file whose first 8 lines carry a generated-file banner
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

(There is no severity-threshold key to reach for here — see "Reading the output" above for why `failOn`
was removed rather than kept as a knob that did nothing.) Full schema in
[`packages/README.md`](../packages/README.md).

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
finding is about — but no marker is read there: derived `zzop-<rule-id>-ok` markers are a DSL-rule mechanism and are
never wired into a native finding, and the finding itself is a joint-analysis result no single tree owns,
so a comment in one tree could not speak for it. Silence one only via `disabledRules`/config `rules`
`"off"`, never a comment. See
[modules/facade.md](modules/facade.md#the-zzop-facade-json-contract) for why (no single tree owns a cross-layer finding).

## Where to next

- [ARCHITECTURE.md](ARCHITECTURE.md) — how a tree gets processed: the IR, route/IO extraction, caching, degraded files.
- [modules/facade.md](modules/facade.md#the-zzop-facade-json-contract) — embed the engine directly (the `zzop-facade` JSON contract, request/response shapes).
- [rules/authoring-guide.md](rules/authoring-guide.md) — write and ship a new DSL rule pack.
- [NORMALIZED_AST.md](NORMALIZED_AST.md) and [../examples/adapters/](../examples/adapters/README.md) — extend zzop to a new language or framework via an external parser/adapter.
