# The Node-free host: `zzop` CLI + `zzop-mcp` MCP server

**Two** self-contained binaries over one shared library (`zzop-summary`, `crates/summary`), running the
zzop analysis engine with no Node.js runtime at all. That library CALLS the shaping entry points
(`analyze_json`/`analyze_trees_json`/`analyze_envelope_json`/`query_io_json`/`query_file_json`/
`query_coverage_json`/`version_string`/`disclosure_contract_text`) and RE-EXPORTS the rest verbatim
rather than wrapping them (`explain`, `version`, `version_string`, `validate_envelope_only_json`,
`validate_rule_pack_json`) — the distinction matters, because a re-exported entry point has no
`crates/summary` wrapper to look for. The lists are not restated anywhere else: the `pub use` block in
`crates/summary/src/lib.rs` is the re-export side and `grep -rho 'zzop_facade::[a-z_]*' crates/summary/src`
is the call side (see [Module map](#module-map) below). Either way the surface is the
`zzop-facade` contract (`crates/facade/src/lib.rs`), documented in full in [facade.md](facade.md):

- **`zzop-mcp`** — the **MCP server** over stdio (bare `zzop-mcp`, or the `zzop-mcp mcp` form a client's
  own `.mcp.json`, the Claude Code plugin's `plugin.json` `mcpServers`, and the MCPB manifest all
  register). For MCP clients.
- **`zzop`** — the **CLI** (`zzop analyze <path>` / `zzop analyze-envelope <envelope.json>` / `zzop cross
  <path>...` / `zzop file <path> <tree>...` / `zzop endpoint <pattern> <path>...` / `zzop manifest
  <path>...` / `zzop diff <a.json>
  <b.json>` / `zzop facts <path>...` / `zzop coverage <path>...` / `zzop graph <path>...` / `zzop init` /
  `zzop contract` / `zzop explain <rule-id>` / `zzop validate-…`). For direct terminal/CI use, no MCP
  client required. Seven of those (`manifest`, `diff`, `explain`, `facts`, `coverage`, `graph`, `init`)
  are CLI-ONLY lanes with no MCP tool twin — see [CLI-only lanes](#cli-only-lanes-manifest--diff--explain--facts--coverage--graph--init) below for
  why each has none.

Each is a Cargo package building exactly one thin argv-dispatch binary — `zzop` is package
`zzop-cli-bin` (`packages/cli-bin`), `zzop-mcp` is package `zzop-mcp` (`packages/mcp`) — and each calls
the shared `zzop-summary` crate's entry points DIRECTLY, so a CLI run and an MCP `tools/call` against
the same path produce the same analysis through the same code. There is deliberately no per-product
dispatch layer in between — a wrapper each host forwards through one-for-one is drift surface, not a
guard against drift. The layer that actually prevents per-host drift is `zzop-summary`.

## Module map

The per-file responsibility table moved to [`CONTRIBUTING.md`](../../CONTRIBUTING.md#repository-layout)
on 2026-08-08. It named which source file holds which test, which is a fact about working ON zzop and
not about using it — and this page is a user-facing contract document. Everything below it here is the
contract: the CLI surface, the MCP surface, config semantics, and the output shape.

## CLI surface

The canonical list is the `USAGE` constant in `packages/cli-bin/src/main.rs` (what `zzop --help` prints);
the block below is a reading aid, not a second source of truth.

```
zzop analyze <path>                  # analyze ONE repo/tree, print a JSON findings summary
zzop analyze --config <zzop.config.jsonc>  # same analysis, the ONE tree the config names (a config outside the tree root)
zzop analyze-envelope <envelope.json>  # Mode A: a Normalized-AST envelope file replaces native parsing, same summary shape
zzop cross <path>...                 # analyze 2+ trees, print the cross-layer join (paths mode)
zzop cross --config <zzop.config.jsonc>  # same, but the config's `trees` define the join
zzop file <path> <tree>...           # definitive "what does zzop know about THIS FILE?" query (uncapped)
zzop file <path> --config <zzop.config.jsonc>  # same query, the config's trees define the run
zzop endpoint <pattern> <path>...    # definitive "is io key X provided/consumed/joined?" query
zzop endpoint <pattern> --config <zzop.config.jsonc>  # same query, the config's `trees` define the join
zzop manifest <path>...              # the run's structural contract manifest (identity only) — commit it
zzop manifest --config <zzop.config.jsonc>  # same, but the config's `trees` define the join
zzop diff <a.json> <b.json> [--allow-tool-drift]  # the delta between two manifests: bucket transitions first
zzop facts <path>...                 # the run's POST-ASSEMBLY FACTS (per-tree CommonIr + the whole join, uncapped) for YOUR OWN rule program
zzop facts --config <zzop.config.jsonc>  # same, but the config's trees define the run
zzop graph <path>... [--domain <join|dep|risk|posture|cochange>] [--format <mermaid|cosmograph-nodes|cosmograph-links>] [--scope <prefix>] [--top <n>] [--fold <n>]  # a serialization for an EXTERNAL renderer, never drawn here; --domain defaults to join, --format to mermaid (a flowchart; the cosmograph-* values emit uncapped NDJSON tables for --domain dep)
zzop graph --config <zzop.config.jsonc> [--domain <join|dep|risk|posture|cochange>] [--scope <prefix>] [--top <n>] [--fold <n>]  # same, but the config's trees define the run
zzop init [<dir>] [--force]          # write the embedded starter zzop.config.jsonc into <dir> (default: the current directory; never overwrites without --force)
zzop contract                        # list the embedded authoring contracts (name, mime, description)
zzop contract <name>                 # print that contract document to stdout (raw bytes, pipe-safe)
zzop explain <rule-id>               # print one bundled DSL rule's compiled-in data (full <pack>/<rule> or an unambiguous bare id)
zzop explain <rule-id> --config <p>  # same, over the packs that config's trees load — the form a recovered pack's rule needs
zzop version [--verbose]             # print this binary's version (also: --version, -V); --verbose adds every parser's fingerprint
zzop <subcommand> --help             # print that one subcommand's own line, exit 0 (also: -h)
zzop-mcp mcp                             # the MCP server over stdio (newline-delimited JSON-RPC 2.0)
zzop-mcp version [--verbose]             # this binary's version; --verbose prints the SAME fingerprint string `zzop version --verbose` does
zzop-mcp help                            # print the usage line, exit 0 (also: --help, -h)
```

The three analysis lanes — `analyze`, `analyze-envelope`, `cross` — additionally take the findings-view
knobs `--severity <critical|warning|info>` / `--rule <id>` / `--limit <n>`: the argv spelling of the
identically-named arguments their MCP tool twins declare, parsed into the one shared `FindingFilters`, so
neither surface can filter differently. They are exactly the tools that declare those arguments —
`check_endpoint` has none on either surface.

`analyze`/`analyze-envelope`/`cross`/`endpoint`/`file` print pretty-printed JSON to stdout on success (exit
`0` — including a `not-found` verdict from `endpoint`/`file`, which is an answer, not a failure); a
failure prints `zzop: <message>` to stderr and exits `1`. `analyze-envelope <file>` reads
the given path as the envelope JSON text (an unreadable file is the same exit-`1` runtime-failure lane,
`zzop: failed to read <path>: <os error>`) and runs the identical Mode A analysis the
`analyze_envelope` MCP tool does — same handler, same output shape `analyze` produces, minus the
filesystem-only `path`/`config` fields an envelope has neither of. A missing/malformed argument (no `<path>`,
`--config` with no path following it, `endpoint` with no pattern or no path, a flag-looking argument
in a path/pattern position — the recognized flags there are `--config`, the three findings knobs, plus
`diff`'s `--allow-tool-drift`, `graph`'s `--domain`/`--format`/`--scope`/`--top`, `init`'s `--force` and
`version`'s `--verbose`, so `analyze --nope` is a usage error, never the path `--nope`; a knob with a missing,
dash-shaped or out-of-range value is the same exit-`2` lane) exits `2` with a usage line. `zzop
help`/`--help`/`-h` prints the whole usage line plus every subcommand's elaboration to stdout and exits
`0`; `zzop <subcommand> --help`/`-h` prints just that subcommand's line, also stdout, also exit `0` (a
help request is answered, never rejected — it used to fall into the dash-shaped-argument guard and exit
`2` on stderr). `zzop-mcp` has its own, shorter usage line — it serves MCP and nothing else. Path
arguments (tree roots and `--config` files alike) are absolutized against the invocation cwd before any
config handling, so `zzop analyze .` and relative `--config` paths work from anywhere. `endpoint` with ONE path is the
`check_endpoint` tool's `path` mode, with 2+ paths its multi-root `paths` mode, and with `--config`
its config-first `configPath` mode — the exact same handler each way, so a CLI query and a tool call
give the identical answer. `contract` with no name lists every embedded authoring contract (one
human-readable line each: name, mime, description); `contract <name>` prints that document's exact
embedded bytes to stdout (pipe-safe — the same bytes MCP `resources/read` serves for
`zzop://contract/<name>`, resolved through the same lookup, so the two surfaces cannot drift); an
unknown name exits `1` with an error naming every valid contract. `version`/`--version` prints the
binary's own name followed by the version (`zzop <version>` from the CLI, `zzop-mcp <version>` from the
server binary), exit `0`; `version --verbose` prints instead the DIAGNOSTIC form
(`zzop_facade::version_string()` — the same version plus every parser's derived fingerprint
`<id>/<source hash>` and a `zzop-engine=<hash>` token), identical
on both binaries. The bare form is the default because it is a one-token line scripts parse; the
fingerprints are not on the MCP wire (`serverInfo` is a spec-shaped `{name, version}` object and every
`resources/read` document is a static embedded contract), so `zzop-mcp version --verbose` is where an
operator asks the `zzop-mcp` product which parser build it carries (since 2026-08-03 each value ends in
the derived source hash that keys the per-file cache, so it separates two builds whose extraction
differs — see [facade.md](facade.md)). One owner behind all of it,
`zzop_facade::version` (`crates/facade/src/version.rs`),
which `zzop-summary` re-exports verbatim rather than wrapping (so `zzop_summary::version()` in either
binary's source IS that function) — and the exact value MCP
`initialize` reports as `serverInfo.version` (see below), so no two surfaces can disagree on the number.

**Windows Git Bash / MSYS caveat**: a leading-slash `endpoint <pattern>` (e.g. `/articles`) run from Git
Bash/MSYS gets silently path-converted by the shell BEFORE it reaches this binary — `zzop endpoint
"/articles" <path>` can become a query for `C:/Program Files/Git/articles`, producing a confusing
false not-found with no indication the pattern itself was rewritten. This is a shell behavior, not a
`zzop-mcp` bug, but it bites `endpoint` specifically because its first argument is a bare pattern rather
than a path. Work around it either by quoting AND setting `MSYS_NO_PATHCONV=1` for the invocation
(`MSYS_NO_PATHCONV=1 zzop endpoint "/articles" <path>`), or by running the command from PowerShell/
cmd instead, neither of which path-converts arguments.

### CLI-only lanes: `manifest` / `diff` / `explain` / `facts` / `coverage` / `graph` / `init`

Seven subcommands have no MCP tool twin. `explain` has none because MCP already reaches the same
compiled-in rule data through the `rule-catalog` resource below. `init` has none for the same reason
plus a stronger one: its document is already on the wire as the `config-template` resource, and the
only thing the subcommand adds is a WRITE into the caller's tree — which is precisely what a stdio
server should not be doing on a client's behalf. `manifest`/`diff`/`facts`/`coverage`/`graph` have none for
reasons recorded as a contract in [contracts/surface-parity.json](../contracts/surface-parity.json)'s
`_cliOnlyLanes` — the manifest is deliberately UNCAPPED while every MCP reply here is cap-governed (the
token-bomb guard in [Output contract](#output-contract) below), so putting it on that wire would force a
choice between breaking that doctrine and capping the one artifact whose whole reason for existing is
that the caps make two capped summaries un-diffable; the workflow is file-shaped (commit a manifest, let
a later run compare against it) rather than conversation-shaped; and an agent that wants either can shell
out to this same binary for the identical analysis.

`facts` carries the same three arguments even harder: its output is uncapped *and* grows with tree size
(the IR alone measured 107x the shipped reply on zzop's own tree, and `facts` adds the whole join on
top), its consumer is a PROGRAM rather than a conversation (`zzop facts ./api ./web > facts.json &&
./my-rule facts.json`), and an agent already has the shaped answers it usually wants through the join
buckets and `check_endpoint`. See [facade.md](facade.md#custom-rules-consumer-side-zzop-facts) for the
full field table, the always-present-key rule, and what the surface deliberately does not carry.

`coverage` is the aggregate-visibility lane — "how much of this tree does zzop actually see", as a
per-extension dispatch table (structural / lexical-only / degraded), the per-tree census with its
join-visibility spelled as a sentence, the build's compiled-in `frameworkRecognizers` capability
table (which frameworks this build recognizes, channel by channel — a fact of the binary, answered
before any run), and the axes zzop has NEVER measured on your tree (recall)
carried as a schema FIELD rather than a caveat sentence — by ruling there is no single score, so the
unmeasured part cannot be dropped in transit. Its MCP absence is the mild kind: the per-file half is
already on the wire as `check_file`, and every analyze reply carries the same census under
`coverage`; the aggregate is an authoring/ops view. The core is host-neutral
(`zzop_facade::query_coverage_json`), so a twin is one dispatch entry away if agent demand arrives.

`graph` is CLI-only for two of the same reasons: a diagram is a terminal/file act (`zzop graph ./api
./web > join.mmd`, then a renderer), and the shaped answers a PROGRAM reasons over — the same buckets as
counts, keys and sites — already ride `cross_repo`, so a tool twin would put one dataset on two surfaces
under two different shaping rules. It is also the one subcommand that prints something other than a
JSON document, because its consumer is a renderer rather than a program: a mermaid `flowchart LR` by
default, or — with `--format cosmograph-nodes|cosmograph-links` on `--domain dep` — uncapped NDJSON
tables for an interactive viewer, one JSON object per line rather than one document. See
[facade.md](facade.md#the-joins-picture-zzop-graph) for the node model, the per-bucket `--top`/`--scope`
scoping, the two-channel truncation disclosure, and the four things the format structurally cannot carry.

`zzop manifest` runs the same two source modes and the same `analyzeTrees` engine path as `cross`, then
projects the result to IDENTITY ONLY: `provides[] {kind, key, source}`, `edges[] {kind, key, from, to}`,
`buckets[] {bucket, kind, key, source}` (all sorted and deduped, so the bytes are stable run over run),
per-source `{sourceId, joinContributionZero, degraded}`, and `tool` — `zzop_facade::version_string()`
verbatim, i.e. the release version plus every parser fingerprint. It deliberately carries **no file or
line** (a rename would drown the real signal, and a pure refactor must diff empty), **no root path** (an
absolute path differs between a laptop and CI, which would make manifests un-diffable across the two
machines that most need to compare), and **no findings** (v1 is structural contract state only). zzop
never stores, names, or garbage-collects a snapshot — keeping one is the user's job.

`zzop diff` is pure: two JSON strings in, one out, no engine and no filesystem beyond the two files.
Its report is RANKED, not symmetric — read `transitions` first. A `+` is common and usually harmless; a
key whose bucket placement changed (`edges` → `unprovidedConsumes`) means the caller still calls a route
that is gone. The per-relation `added`/`removed` lists are that reading's raw evidence, not additional
facts. Two honesty gates, without which the feature would manufacture silent wrongs:

- **Tool identity.** Two manifests from different zzop builds are not comparable — our own parser
  improvement would read as the other team breaking a contract. The default is REFUSAL (exit `1`, a
  runtime failure, since both arguments were well-formed); `--allow-tool-drift` compares anyway and the
  reply then carries a `toolDrift` block naming both builds. Refuse or disclose, never silently compare.
- **Blindness vs deletion.** A source that got LESS visible between the two runs (more `degraded` files,
  newly `joinContributionZero`, or absent from the second run entirely) explains its own disappearances.
  Every removed row attributable to such a source is tagged `blindnessSuspect: true`, and
  `sources.coverageDropped` names the drop — so "the route vanished" is never reported when "we stopped
  being able to see it" is the honest reading.

`zzop explain` prints one DSL rule's data — id, pack, severity, message, the derived suppress marker,
matcher kind, and every exclusion field that kind actually carries, with its real value.

**Which packs it searches is the one thing you choose.** Bare `zzop explain <rule-id>` reads the packs
compiled into the binary. `--config <path>` reads the packs that config's trees actually load — the
compiled-in ones plus every `zzop/rules/` and `packs.extraDirs` directory those trees name. The second
form exists because a rule that leaves the bundled set does not stop running: recovered into a tree's
`zzop/rules/`, it appears in `packsLoaded` and its findings carry its id, while a compiled-in-only lookup
calls that same id unknown. The bare form's unknown-id message names `--config`, so you find out where you
find out. Two further answers beyond the happy path are worth knowing about in advance:

- **An io-scan rule's marker is printed with its run-mode condition.** An io-scan marker is read off the
  finding's anchor line in the source TEXT, and an envelope carries none (`envelope::ingest`, Mode A,
  supplies a constant-`None` `anchor_line` callback), so every io-scan marker is inert in
  `analyze-envelope` runs — disable the rule id or exclude the path there instead. Printing the bare
  token would hand an envelope-fed reader a comment that silently does nothing. (`symbol-scan` answers
  `none` for the sibling reason: its findings have no line to anchor a comment against.)
- **It answers for the BARE form of a namespaced native id.** Two native families namespace their ids
  with a `/`: `cross-layer/*` and `schema/*`. `disabledRules` / `severityOverrides` match ids exactly, so
  a reader who types only the tail (`god-model`, `route-near-miss`) would configure nothing — this lane
  resolves the tail against the live registry and names the full id to use. It is a lookup FAILURE lane —
  stderr, exit `1` — because there is no DSL rule to render; it is a guided error, not a rendering. The
  exact-match lane runs first, so `duplicate-route` (registered bare AND as `cross-layer/duplicate-route`)
  answers as the bare id it names rather than as an ambiguity. This replaced an issue-label lane that
  existed only because `schema` findings composed a `ruleId` (`schema/<label>`) that no registered id
  matched; the 12 labels are registered analyses now, so the namespaced form is answered by the ordinary
  native-id lane.

## MCP surface

`initialize` replies with `serverInfo: { name: "zzop", version }` — `version` is `CARGO_PKG_VERSION`
(`crates/facade/src/version.rs`), the workspace `[workspace.package] version`, the same value a dev build
in-tree reports. CI's release job verifies the pushed tag and `.claude-plugin/plugin.json` both match
this number, so a released build's `serverInfo.version` equals the release tag and the plugin's
published version by construction (see [`version()`](facade.md#the-zzop-facade-json-contract) and
[VERSIONING.md](../../VERSIONING.md)).

The reply also carries an optional `instructions` string, and ONLY when this build has something to
report about its own age. A `.mcpb` bundle installed from GitHub Releases is never auto-updated by
Claude Desktop, the bundle carries no origin URL for a host to poll, and MCP has no version-notification
message — so for that lane there is no delivery-layer notifier at all, and the binary reporting on
itself is the only channel left. What it does and does not say is the point:

- It says one fact it can verify offline — *this build was made from source N days old* — plus a
  recommendation to go look at the releases page. **It never says a newer release exists.** Without a
  network it cannot know that, and the binary makes no network call by decision (a version check would
  be exactly the HTTPS-in-the-binary this project has repeatedly refused).
- It stays SILENT in every state where it has nothing supportable to say: a build younger than 90 days,
  a build with no date stamp at all (a source tarball with no `.git`), a system clock before 1970, and a
  system clock reading behind the stamp. No `instructions` key appears at all in those cases — never a
  blank string.
- The threshold is 90 days, set from the measured release cadence (30 releases in the first 27 days,
  largest gap between two of them 3 days), so a current install cannot trip it. It is not configurable:
  a switch here would turn off the only notification this lane has.

The same notice — from the same function, so the two cannot disagree — is printed once to stderr under
the serve-time banner, which is where an operator reading their MCP client's server log meets it.

### Tools (`tools/list` / `tools/call`)

| Tool | Purpose |
|---|---|
| `analyze_repo` | Analyze ONE repo/tree path. |
| `cross_repo` | Analyze 2+ repos/trees and join them across the cross-layer (kind, key) boundary — zzop's headline capability (e.g. a frontend `fetch` call matched against a backend route, a shared DB table, route drift). |
| `check_file` | DEFINITIVE answer to "what does zzop know about THIS FILE?" — the targeting twin of `check_endpoint`, with a file PATH as the target instead of an io key, for a caller working IN a file rather than asking about a whole tree. Takes `target` (tree-relative or absolute, either separator style — an absolute path matches by its tail) plus exactly ONE of `path` / `paths` / `configPath`, resolved exactly as the tools above resolve theirs, and an optional `sourceId` pinning the answer to one tree. Returns which tree the file was found in (`sourceId`, plus `otherTrees` when the same relative path exists in more than one — never a silent pick), a `verdict` from a sealed four-token vocabulary with a `verdictMeaning` field spelling out THAT token's meaning in the reply itself (same self-describing discipline `check_endpoint` uses, and the same reason: no tool description or help text is a second owner of the vocabulary), the file's `loc`, `symbols` (count + exported names), `io` provides/consumes, `dependencies` in BOTH directions (`imports` and `importedBy`) with a `dependenciesMeaning` field beside them (same self-describing discipline `verdictMeaning` uses, for the same reason: an empty `imports` list is ambiguous on its own — see [File queries: `queryFile`](facade.md#file-queries-queryfile)), and every finding anchored in the file — the tree's own and the cross-layer join's merged into one list with counts by severity and rule. **Nothing is capped**: a single file's facts are bounded by the file, so this reply drops nothing and therefore never has to disclose a truncation (the one exception is a `not-found` reply's `suggestions` list, which ranks over every walked path rather than describing the target). The verdict answers whether the file was ANALYZED, not whether it is healthy — an empty findings list means "clean" only for a file the verdict says was analyzed. Runs `analyzeTrees` even for a single `path`, because the reply names the tree a file belongs to and a single-tree `analyze` output has no tree identity (see [File queries: `queryFile`](facade.md#file-queries-queryfile) for the full output contract). |
| `check_endpoint` | DEFINITIVE answer to "is io key X provided/consumed/joined?" — matches a pattern against ANY cross-layer io key (http routes, env keys, DB tables, topics) as a case-insensitive substring and returns ONE verdict from the sealed vocabulary `linked` / `provided-only` / `consumed-unprovided` / `external` / `unresolved-only` / `ambiguous` / `mixed` / `not-found`, plus a `verdictMeaning` field spelling out THAT token's meaning in the reply itself (the definitions live with the verdict computation and ride every reply on every host, so no help text or tool description is a second owner of the vocabulary), full counts, capped match lists, related findings, and key suggestions on `not-found`. Runs the shared facade query core directly — the same core any embedder driving `zzop-facade`/`zzop-summary` gets identical answers from (see [Endpoint queries: `queryIo`](facade.md#endpoint-queries-queryio) for the full output contract). |
| `analyze_envelope` | **Mode A**: a full Normalized AST envelope (a custom parser's output) REPLACES native parsing entirely for this run — contrast `validate_envelope` below, which only checks the envelope's shape and runs no analysis, and Mode B overlay/mount requests (`docs/NORMALIZED_AST.md`), which merge external symbols ON TOP of a natively-parsed tree instead of replacing it. Only symbol-scan/io-scan DSL rules can fire (no source text ships in an envelope); the native call-graph-BFS rules (`mutating-route-no-auth`, `unsafe-read-endpoint`, `non-idempotent-write`) additionally run when the envelope supplies its `calls` channel (`docs/NORMALIZED_AST.md`'s `calls` section — an envelope without it gets a `warnings` disclosure naming the silent rules). The one lane that takes NO config — an envelope carries no filesystem location, so there is none to auto-discover and none to require, and the reply has no `config`/`path`/`architecture` fields (`gitWindow` IS present, always `null` — the facade always serializes it and `null` is the "git did not run" signal); otherwise the SAME shaped summary `analyze_repo` returns (findings, `packsLoaded`, `coverage`, warnings). Same `analyzeEnvelope` facade call path documented in [Defaults (a config is required; what it does not have to say)](facade.md#defaults-a-config-is-required-what-it-does-not-have-to-say). |
| `validate_envelope` | Validate a Normalized AST envelope against its contract WITHOUT running an analysis — the authoring feedback loop. Returns `{valid, issues[], hints[]}`; never fails on bad input (same contract as the facade's `validateEnvelopeOnly` — see [Validation-only: `validateEnvelopeOnly`](facade.md#validation-only-validateenvelopeonly)). The two lists are DIFFERENT AXES: `issues` reject the envelope and alone decide `valid` (and so the `zzop validate-envelope` exit code, which this field does not change), while `hints` are accepted shapes that are almost certainly not what the producer meant. What each hint COSTS differs by shape and the prose here does not flatten that: some make the cross-layer join find nothing at all (a non-normalized `http` key; a provide key carrying a host), while others still join and instead change what the run produces (an absolute `files[].path` becomes a synthetic entry under a Mode B overlay instead of merging onto the file it names; a duplicate provide is joined once per copy, so a consume of that key gets a duplicate edge). Every hint states its own consequence and its fix, so the checks are the list, not any sentence about them — `crates/core/src/normalized/hints.rs` (`zzop_core::envelope_hints`) is where a new one is added and where the wording lives. A non-empty `hints` on a valid envelope is the more urgent signal. `hints` is always present, empty array included, so "found nothing" is never confused with a build that has no hint pass. |
| `validate_rule_pack` | Validate a DSL rule pack's STRUCTURE before loading it — the exact judgments the engine's pack loader makes at load time (bad JSON, missing field, wrong type, too-new `schema_version`) plus the full dead-rule census: every matcher regex that fails to compile, AND the two structural shapes that parse fine and still can never fire (a line-scan declaring neither `line_pattern` nor `any`; a method-scan whose `trigger` names a label no `patterns` entry declares). Such a rule would load but silently never fire. Shape only, never rule-quality semantics. Returns `{valid, issues[]}` — no `hints` list, deliberately: rule packs have no hint pass, and an always-empty array would claim a search that never ran (see the facade's `validateRulePackOnly`, whose never-fails contract this shares). Pair with the `rule-pack-schema` resource below. |

`analyze_repo`, `cross_repo`, and `analyze_envelope` share three optional drill-down arguments, described in
[Output contract](#output-contract) below: `severity` (`"critical" | "warning" | "info"`, minimum
severity to include in the findings *list* — counts always cover everything), `rule` (exact rule id),
and `limit` (list cap, default 50, min 0 — `0` is a legal "counts only, no findings listed" query —
max 1000).

**Tool annotations ride `tools/list`, and they are stated honestly rather than flatteringly.** Every
tool carries MCP's advisory `annotations` object (`title` + behavior hints, with the same string
doubled as the tool's top-level `title` for newer clients). The four tree-analyzing tools declare
`readOnlyHint: false`, because an analysis persists its cache to disk — the product front end
injects a `.zzop/cache` default resolved beside the honored config file, usually the tree root, and
a config's own `cacheDir` moves or disables it (the precise rule lives in
[ARCHITECTURE.md](../ARCHITECTURE.md#caching)) — beside `destructiveHint: false` (the writes, and
the cache's own self-eviction, stay inside zzop's `.zzop/cache/`, never touching a file a user
authored) and `idempotentHint: true` (the analysis is deterministic). The three
text-in/judgment-out tools (`analyze_envelope`, `validate_envelope`, `validate_rule_pack`) declare
`readOnlyHint: true` — their lane touches no disk at all. Every tool declares
`openWorldHint: false`: zzop makes zero network calls. These hints are consent-bearing (a host may
skip a confirmation prompt for a tool that claims to be read-only), so the classification is pinned
by a test (`tool_annotations_never_claim_read_only_for_the_cache_writing_tools`) rather than left to
prose — a new tool fails the build until it is explicitly classified.

**Argument validation is strict, not advisory.** A boundary-value round found every tool argument
here silently accepted the wrong JSON type and behaved as "not provided": a `limit` of `-1`, `1001`,
`999999`, the STRING `"50"`, or the float `3.7` all passed through as "no cap"; a `severity` NUMBER
silently dropped the severity filter instead of being rejected like an unknown severity STRING. Every
one of those is now a named `zzop error: ...` rejection instead — `limit` must be a JSON integer in
`[0, 1000]` (`zzop error: limit must be an integer between 0 and 1000 (got <value>)`), and any
non-string `severity` value hits the exact same rejection an unknown severity string gets
(`zzop error: unknown severity <value> — valid values: "critical", "warning", "info"`). The same
sweep covers every other declared-type argument across all seven tools — `path`, `paths` (and its
array elements), `configPath`, `pattern`, `target`, `sourceId`, `rule`, `envelopeJson`, `packJson` — a wrong JSON type
(a number where a string is required, an array element that isn't a string, ...) is always a named
`` `<name>` must be a string (got <value>) `` error, never a silent fallback to "argument omitted".
Only an absent key or an explicit JSON `null` means "not provided" — every other type mismatch is a
caller mistake, named.

`analyze_repo` takes **either** `path` (a tree root, whose `<path>/zzop.config.jsonc` is auto-discovered — see
[Config semantics](#config-semantics) below) **or** `configPath` (a `zzop.config.jsonc` at any location,
naming the ONE tree to analyze — for a config that does not sit at the tree root; the `zzop analyze
--config <path>` CLI twin). Passing both, or neither, is a named argument error from the shared handler,
so the two surfaces cannot drift on which combinations are legal. The reply's `config` field says whether
a config was honored (a path string) or not (`null` — either envelope mode, which has no tree, or paths mode, where N configs were loaded and no single one governs the run), and `path` always
echoes the RESOLVED absolute TREE ROOT the analysis actually ran against — never the raw argument
verbatim (a relative `path: "."` used to echo back the literal `.`, with the actual analyzed directory
never disclosed anywhere in the reply; in `configPath` mode the argument is the config FILE, which was
never a tree). A config that declares multiple trees is a guided error telling the caller to run the
cross-layer join over that config instead, or to point this single-tree analysis at one tree root
directly — worded WITHOUT either host's spelling, because the sentence is built in a shared crate that
both products speak through (machine-pinned by
`crates/engine/tests/rule_contracts/host_vocabulary.rs`).

When the underlying analysis ran git signals (the default, or a config's own `git`
settings, provided a real git history is actually present), `analyze_repo`'s reply also carries a
compact, capped `architecture` object — `{pain, painByAxis, painMeasuredWeight, painTotalWeight, painMeaning,
topRecommendation, criticalTop}` — summarizing the
facade's `health`/`recommendations`/`critical` computation (see
[Output contract](#output-contract) below for the exact shape); it is present only then, absent
(never `null`) otherwise. `cross_repo`'s `sources[].path` was audited for the same raw-path-echo gap
and found already correct (each source's path is the resolved absolute tree root, not the raw
argument) — no fix was needed there.

`cross_repo` takes **either** `configPath` (a `zzop.config.jsonc` — its `trees`, including `"auto"`,
define the join; the config-first way) **or** `paths` (2+ explicit tree roots, each loading its OWN config — each tree
tagged by its directory name). Passing both, or neither, is a named argument error. See
[Config semantics](#config-semantics) for what paths mode discloses.

`check_endpoint({ pattern, ... })` requires a non-empty `pattern` (`minLength: 1` in the schema,
matching the behavior the shared facade query core already enforced — the schema had simply
under-declared it) plus exactly ONE of `path` (a single tree,
resolved like `analyze_repo` — config auto-discovery included), `paths` (2+ tree roots, each loading its own config,
resolved like `cross_repo`'s paths mode, disclosure warnings included), or `configPath`. Every mode
runs `analyzeTrees` — even a single `path` — because a verdict is a cross-layer JOIN fact, and the
join runs fine over one tree (intra-tree edges included). The reply is the shared query core's JSON
(pretty-printed): `{pattern, verdict, counts, matches, truncated?, relatedFindings,
truncatedFindings?, suggestions?, suggestionsTruncated?, disclosure}` — see [Endpoint queries: `queryIo`](facade.md#endpoint-queries-queryio) for
every field and the sealed verdict vocabulary — plus this host's three honesty channels stamped on
top, same as the other tree-resolving tools (`analyze_repo`/`cross_repo`/`check_file`;
`analyze_envelope` and the
two validators take no filesystem-rooted config, so they carry no `config`/`configWarnings`): `config`
(which config file was honored, or null),
`configWarnings` (the config front-end's own disclosures, e.g. paths mode's ignored-config
warning, followed by every tree's engine-side config diagnostics) and `warnings` (the engine's own
self-reports — run-level join warnings first, then every tree's). The query-core fields are pinned
across every host that drives the shared facade query core.

`check_file({ target, ... })` takes the same three tree-source modes and stamps the same three
host-layer channels — its host-side half (`crates/summary/src/file.rs`) is deliberately
`endpoint_summary`'s shape, and the `zzop file` CLI subcommand calls that same function, so neither
surface can shape the answer differently. `warnings` is carried by BOTH targeted lanes because each runs
the identical `analyzeTrees` call over the identical trees that `analyze_repo`/`cross_repo` do: nothing
about a targeted question narrows what the engine had to say about the tree, so dropping them left those
users with one eye fewer and nothing in the reply saying so. Per-tree `coverage` is the one blindness
surface these two lanes still do not carry — it is a census rather than a self-report, and `cross_repo`
owns it. `target` is required and non-empty (`minLength: 1`), matched
against each tree's own relative paths: an exact tree-relative path, or an
absolute path matched by its tail (longest match first, so `src/api/users.ts` never loses to
`users.ts`). Matching is textual — the core never touches the filesystem, so it resolves no symlinks and
no `..`. Without `sourceId` every tree is searched and a path present in more than one answers from the
first with `otherTrees` naming the rest; a `not-found` reply carries `suggestions` (up to 10 nearest
walked paths, ranked deterministically) with `suggestionsTruncated` when that cap bit.

Tool-level failures (bad path, malformed envelope config, an unknown severity value) come back as a
normal MCP result with `isError: true` and a `zzop error: <message>` text block — the MCP convention.
Protocol-level errors (malformed JSON-RPC) are the JSON-RPC error responses described below, not this
channel.

### Protocol errors (stdio transport)

The server (`server.rs`) is silent-failure-free by policy: every line it reads either gets a reply or is
a spec-legal notification (a parsed object with no `id`) — a line it cannot even parse must never be
swallowed, or the client is left hanging on a reply that never comes. Two cases are answered at the
transport level, before dispatch reaches `tools/call`/`resources/read`:

- A line that isn't valid JSON at all answers JSON-RPC error `-32700` (Parse error) with `id: null` —
  the spec's reserved shape for "the request id itself is unrecoverable."
- A line that parses to a JSON **array** is a JSON-RPC batch and is served: each element goes through the
  same dispatch a lone request takes, and the replies come back as one array on one line, in request
  order. Notifications contribute no element, so a batch of only notifications produces **no reply at
  all** (not an empty array); an **empty** array is itself an invalid request and answers a single
  `-32600`; an element that is not a request object gets its own `-32600` **inside** the array rather
  than failing the whole batch. Implemented 2026-07-29, and it is a correction rather than a feature:
  the `2025-03-26` revision — one of the three this server advertises — makes receiving batches a MUST,
  so refusing them while listing that revision was a false claim of support. Only *receiving* is
  implemented; nothing here originates requests, so there is no sending side to batch.
- A line that parses but is neither an object nor an array (a bare scalar) answers `-32600`
  (Invalid Request).

Both also log one line to stderr. An unknown `method` (anything other than `initialize`/`tools/list`/
`tools/call`/`resources/list`/`resources/read`) answers `-32601` (Method not found) with the request's
own `id`. None of this is the `isError: true` tool-result channel described above — that channel is only
for a named tool call that ran and failed; these are protocol-level responses for input the server
couldn't even dispatch.

### Resources (`resources/list` / `resources/read`)

The embedded authoring contracts, addressed as `zzop://contract/<name>` — the documents a custom-parser,
DSL-rule, or config author needs, with no zzop source checkout and no Node required, since they are compiled
into the binary (`crates/summary/src/contracts.rs`, `include_str!` over the repo's own committed public
docs). The byte total is not restated here — it was written as "~180KB" and had drifted past 500KB by the
time anyone recounted, which is what a size claim does when the files it sums are edited by other
batches. Recount it instead, with the same list the binary embeds:
`grep -o 'include_str!("[^"]*")' crates/summary/src/contracts.rs | sed 's/.*("\.\.\/\.\.\/\.\.\///;s/")//' | xargs wc -c`
— which covers the `include_str!` rows only; the two `zzop_config::` rows (the starter template and
`config-surface.json`), the derived `example-pack-*` block baked by `crates/config/build.rs`, and the
rendered `disclosure-classes` have no `include_str!` in that file to grep:

| `<name>` | Content |
|---|---|
| `envelope-schema` | JSON Schema (draft-07) for the Normalized AST envelope contract — machine-validate a custom parser's output. |
| `envelope-guide` | The Normalized AST envelope contract: Mode A (full envelope) / Mode B (overlay) adapter authoring, field semantics, worked examples (`docs/NORMALIZED_AST.md`). |
| `key-normalization-fixture` | Byte-pinned HTTP key-normalization fixture — the exact `(method, path)` → join-key rows an adapter must reproduce for cross-layer joins. |
| `adapter-guide` | Adapter authoring README: key-normalization parity rules, schema/versioning policy, adapter-kit pointers (`docs/adapters/README.md`). |
| `dsl-reference` | DSL rule-pack reference: pack/rule fields and every matcher (`docs/rules/dsl-reference.md`). |
| `dsl-authoring-guide` | DSL rule authoring guide: placement, a worked example, testing conventions (`docs/rules/authoring-guide.md`). |
| `rule-pack-schema` | JSON Schema (draft-07) for the DSL rule-pack shape — pack id, rules[], the matcher kinds, severity, every property documented (`docs/contracts/rule-pack.schema.json`; the machine-readable twin of the `validate_rule_pack` tool). |
| `example-envelope` | Minimal valid Mode-A envelope example (a crude JSP parser's output). |
| `config-surface` | Machine-verified config vocabulary — every config key, dotted path, CLI flag, and embedder field zzop accepts (`crates/config/config-surface.json`, the same file `zzop-config` embeds for unknown-key warnings; its `_docs` sections self-describe). |
| `config-template` | Annotated starter `zzop.config.jsonc` (`crates/config/src/template.rs`, whose own tests check every key it names against the `config-surface` vocabulary): each optional key with a comment saying what it MEANS, set to zzop's own value — so the file documents the defaults instead of changing them. Writing it is REQUIRED once per tree: every analysis lane refuses a tree with no config, and this document is what both hosts point at when they do. `zzop init [<dir>] [--force]` (see [CLI surface](#cli-surface)) writes these exact bytes to disk; this resource is the same document without the write. |
| `rule-catalog` | Every rule id the engine ships today — the 11 DSL packs + all native analysis ids, with severity/matcher/detection prose per rule (the suppress marker is derived, `zzop-<rule id>-ok`) (`docs/rules/catalog.md`) — the discoverability gap closed: `packsLoaded` gives counts only, and the `dsl-reference` resource pointed at this file without it ever being served over MCP. Pair with the `rule` tool argument, which the three full-analysis tools take — `analyze_repo`, `cross_repo`, `analyze_envelope` (an id absent from this catalog never fires). `check_file` and `check_endpoint` declare no `rule` property and no handler reads one, so a `rule` key sent to either is an unknown argument, silently ignored rather than a narrower answer. The CLI-only `zzop explain <rule-id>` (see [CLI surface](#cli-surface)) answers "what exactly is this ONE rule" straight from the same compiled-in DSL pack data, no catalog prose parsing required — and `--config <path>` widens it to a run's loaded packs, which is the only surface that reaches a rule this catalog cannot list (a recovered pack's). No MCP twin either way, since this resource already covers the bundled ground over the wire. |
| `disclosure-classes` | Every silent-failure class zzop knows about, each with the status of how completely zzop detects it today (`asserted` / `partial` / `notYetDetected`) — the full text behind the counts every analyze reply carries (`zzop://contract/disclosure-classes`, also `zzop contract disclosure-classes`). The one row that is **rendered** from the engine's live blindness registry rather than `include_str!`'d from a committed file, so it cannot drift from the counts the replies tally off that same registry. Listed after the `include_str!`'d rows above, and ahead of the derived `example-pack-*` block below. |
| `example-pack-code-hygiene` | `examples/packs/code-hygiene.json` verbatim — an EXPORTED pack: shipped in the binary but NOT loaded, so its rules run only when a config points at them. |
| `example-pack-orm-eager` | `examples/packs/orm-eager.json` verbatim — same shape. |
| `example-pack-sql-preferences` | `examples/packs/sql-preferences.json` verbatim — same shape. |
| `example-pack-typescript-lint` | `examples/packs/typescript-lint.json` verbatim — same shape (the pack's own id is `typescript`; the resource name carries the file stem). |

The `example-pack-*` rows are DERIVED — one per file in `examples/packs/`, baked by
`crates/config/build.rs`, which owns that list — and appended after `disclosure-classes` so every older
resource keeps its position. They are therefore what the export program had reached when this page was
written, not a closed set: the next exported pack adds a row to `resources/list` and none here. **Ask
`resources/list`** rather than counting rows on this page. A hand list that claims completeness over a
derived one is a census in disguise, and this table has already been one — it enumerated twelve rows and
said "every entry above" while the binary served sixteen. To LOAD one of these packs, save its bytes as
`<stem>.json` under `<tree>/zzop/rules/` (no config key needed) or in a directory named by
`packs.extraDirs` — that key REPLACES the `zzop/rules/` default rather than adding to it.

`resources/list` returns each entry with its `uri`/`name`/`description`/`mimeType`; `resources/read`
returns the full text verbatim. Deterministic: same binary, same list, same bytes every time. An unknown `uri` is a named error listing every valid resource — an agent should never
have to guess the name. The same documents are reachable without an MCP client via
`zzop contract [<name>]` (see [CLI surface](#cli-surface)) — both surfaces resolve names through the
one embedded lookup, so they can never disagree on what exists.

## Config semantics

All config handling is delegated to the shared `zzop-config` crate (`crates/config`) — the Rust-hosted
config front end (discovery, mapping, and defaulting, ported from the removed JS CLI's
`config.js`/`mapper.js`) that a future full-CLI Rust binary would reuse. Three things to know about how
it behaves here specifically:

- **Auto-discovery.** `analyze_repo`/`zzop analyze <path>` look for `<path>/zzop.config.jsonc`
  (literal filename, no ancestor walk — the same rule the removed JS CLI used). If present, it is parsed
  (JSONC: comments and trailing commas allowed) and mapped by the same rules the JS CLI's `mapper.js`
  used to, now ported to `zzop-config`'s Rust mapper.
- **A config is required (2026-07-27, reversing the zero-config default).** A missing config used to
  produce the *same request an empty `{}` config would produce*, so an MCP tool pointed at any repo
  would still work. It is now a refusal, on both hosts identically, naming the `config-template`
  document (below) rather than either host's command — the MCP client cannot run `zzop init`, and a
  terminal cannot call `resources/read`. The reason is one key: `vocabulary` is the only knob with no
  default at all, and an undeclared one is a judgment NOT MADE, so a config-less run would analyze less
  while reporting itself complete. The refusal is total
  rather than a warning because a warning next to a shorter findings list is the disclosure readers skip.
- **What a config still does not have to say.** Everything else keeps defaulting, so a starter file
  declaring only `roots` runs the full analysis: the bundled DSL rule packs (embedded at compile time by
  `zzop-config`'s `build.rs` and injected as inline `packDefs` — see
  [Defaults](facade.md#defaults-a-config-is-required-what-it-does-not-have-to-say)'s note on this
  field), default `git: {}` collection (30-day recency), and a default `cacheDir` of `.zzop/cache`. That
  last one is the only default that writes to disk — a first run creates the directory inside the
  analyzed tree, and `cacheDir: null` opts out; the user-facing statement of that (including the
  anchored `**/.zzop/` gitignore line, and the authored `zzop/` one character away that a glob would
  swallow) is [ARCHITECTURE.md's Caching section](../ARCHITECTURE.md#caching). One thing config-loading
  cannot default is the tree set: a root that resolves to a single tree cannot run the cross-layer join
  (which needs 2+). When that root carries a workspace manifest resolving to 2+ packages, `zzop-config`
  says so as the FIRST `configWarnings` entry — naming the manifest, the exact package count, and the
  `"trees": "auto"` remedy.
- **`configPath` / paths-mode disclosure (`cross_repo`).** Config-first mode (`configPath`) loads that
  file directly (or a directory containing `zzop.config.jsonc`) and requires it to declare `trees` (2+,
  or `"auto"`) — a single-tree config there is a guided error saying to analyze it as one tree instead.
  Paths mode (`paths`) builds one tree request per path, tagged `sourceId` = that directory's name, and
  loads each path's own `zzop.config.jsonc` — a path without one is refused by the same message every
  other analysis lane gives, naming the path that lacks it. The reply's `config` field still reads `null`,
  because no single config governs the run; a `configWarnings` entry says so out loud and names every
  config that WAS loaded, so `config: null` cannot be read as "no config was read". A path whose config
  declares its own tree set is refused rather than flattened — that config's answer to "which trees?" and
  this call's `paths` are two different answers, and silently picking one would make the analyzed set
  unreadable from either.
- **Path-ish config values resolve against the config file's own directory**, not the analyzing
  process's cwd: `root`/`cacheDir`/`packsDir`. A server host's cwd is meaningless — an MCP client can
  invoke this binary from anywhere — so cwd-relative resolution would make the same config mean
  different things per caller. Overlay paths are the one exception: they stay relative to the tree root.

Every reply from `analyze_repo`/`cross_repo` carries `config` (the config file path honored, or `null`)
and `configWarnings` (the config front-end's own non-fatal notes — unknown keys, a skipped/unreadable
overlay, an `"auto"`-expansion report, the single-tree-over-a-workspace disclosure, the paths-mode
disclosure above) as a channel **separate** from
the engine's own `warnings` — two different honesty channels, never merged into one. `disabledRules`/
`severityOverrides` entries that match no known rule id (a typo, or a stale id from a different zzop
version — the "...matching no known rule id..." diagnostics) are a config-authoring mistake, not an
engine finding, so they land in `configWarnings`, never in `warnings`. `suppressions` entries with the
same problem are unaffected by this split and stay in `warnings` (the analogous `unknown_suppression_rule_ids`
self-report was not moved).

## Output contract

Every tool reply is summary-first: full counts ride along unconditionally, and any list that gets capped
says so explicitly — this is the token-bomb guard for MCP responses (`crates/summary/src/output/mod.rs`),
built to never lie by omission.

- **Findings** shape to `{total, bySeverity, byRule, shown, truncated?, testPaths?}`. `total`/`bySeverity`/`byRule`
  are always computed over the FULL set — a `severity`/`rule` filter narrows only `shown`, never the
  counts. `shown` is the filtered list, sorted severity-descending with original engine order as the
  stable tiebreak (deterministic — same analysis, byte-identical tool output), capped at `limit`
  (default 50, max 1000). `truncated` (`{shown, totalMatching, hint}`) appears **only** when `shown` is
  incomplete — its absence is itself the "you have everything" signal, so a cap is never silent. A
  `rule` filter that matches ZERO findings AND names a rule id absent from `byRule` (i.e. it never
  fired at ALL this run, not merely filtered down to nothing) gets an additive `note` field pointing
  the caller at the `rule-catalog` contract resource (`zzop://contract/rule-catalog` /
  `zzop contract rule-catalog`) to check the id — this fires through the real `analyze_repo`/
  `cross_repo`/`check_endpoint` tool-call path end to end, not just the underlying shaping helper.

  Within a severity tier, findings whose file matches the DSL's shared test-path pattern sort
  after production findings, and `testPaths` (`{count, meaning}`, additive-only like `truncated`)
  announces the demotion with the full-set count — the credential rules deliberately keep scanning
  test paths (a committed secret is a leak wherever it sits; the catalog rows say so), but a first
  screen leads with production code. Nothing is dropped and no count moves.
- **Cross-layer edges** (`cross_repo`) get the same treatment via a plain list cap (`edgesTruncated`,
  default cap 200 — edges are small rows, so most joins fit uncapped).
- **`degraded`** (`analyze_repo` only) — the size-capped/parse-failure file-path list gets the same
  cap-plus-disclosure treatment as every other list (`degradedTruncated`, default cap 50) rather than
  riding through verbatim, which would bypass this module's own token-bomb guard on a repo with
  thousands of degraded files. `coverage.degraded` (below) already carries the full, uncapped COUNT, so
  this list is supplementary detail (which files, not just how many) and is never the only source of
  the number.
- **`distinctBucketKeys`** (`cross_repo`) — alongside the numeric `buckets` counts, each of the five
  non-edge join buckets (`unconsumedProvides`, `unprovidedConsumes`, `unresolvedConsumes`,
  `externalConsumes`, `ambiguousConsumes`) lists EVERY DISTINCT key (deduped, engine order preserved; an
  unresolved consume contributes its `raw` expression when recorded), so an agent sees WHICH keys sit in
  a bucket instead of only how many. The list is uncapped since 2026-07-29, so there is no truncation
  field to check — on a large repo one of these lists can be long. A parallel
  `distinctBucketKeyFirstSites` object mirrors it with each key's FIRST recorded site as `"file:line"`
  (`null` when the fact carries no location — never guessed), so a listed key is locatable without a
  follow-up call.

  **`buckets.X` and `distinctBucketKeys.X` are two counts of one bucket, and they legitimately differ.**
  `buckets.X` counts RAW ROWS while the key list DEDUPES; measured on this repo's `cases/` corpus,
  `unprovidedConsumes` is 23 rows over 14 distinct keys. So `buckets.X >= len(distinctBucketKeys.X)`
  for the FIVE non-edge buckets, and equality only means no key repeated. `edges` is not one of them —
  it carries no key list beside it, so no such comparison exists for it, and a row there is a matched
  consume->provide PAIR rather than a call site.
  The reply says this itself in a **`bucketMeaning`** sentence rather than leaving the arithmetic to a
  reader — the same repair the graph lane's `%%` census already carries for the identical confusion
  ("60 rows are 4 relations"). Both fields were renamed on 2026-07-31 (`bucketKeys` →
  `distinctBucketKeys`, `bucketKeySites` → `distinctBucketKeyFirstSites`) so the NAME states the
  membership rule: the second one was plural but has only ever carried the FIRST site per key, which the
  tool description said and the CLI twin's bare field name did not.

  These keys are the RAW join residue: no rule vocabulary filters them (see the bucket contract in
  [facade.md](facade.md#output-data-shapes) and the `join-bucket-unfiltered` disclosure class), so a key
  listed here may have no corresponding finding — `crossLayerFindings` is the filtered view of the same
  facts, and is legitimately smaller.
- **`scores.*` detail lists** — the structural-health reports (in the raw `zzop-facade` output, and
  summarized into `architecture` on this wire) each carry a capped list of the rows behind the number:
  `fileSizeCompliance.violations`, `godFile.files`, `diamond.pairs`, `renameInstability.files`,
  `busFactor.files` cap at 50 (per-FILE / per-PAIR rows), and
  `hierarchy.violations`, `publicApi.deepImports`, `siblingCross.violations` cap at 100 (per-EDGE rows,
  which run longer over the same tree). Each ships a sibling count of what the cap dropped —
  `violationsTruncated`, `filesTruncated`, `pairsTruncated`, `deepImportsTruncated` — so
  `list.length + <list>Truncated` is the full total. Unlike the fields above it is **always present,
  `0` included**: these are fixed-shape report structs, and a disclosure that vanishes when it does not
  bite would make "complete list" and "no disclosure" the same bytes. This matters most where the SCORE
  is computed over the full row count before the cap (every list here), so counting the returned rows
  never reproduced the score and nothing said why. The cap values live in one place
  (`crates/metrics/src/scores/detail_cap.rs`), not per module.
- **`warnings` (engine) and `configWarnings` (config front-end + engine-side config diagnostics,
  e.g. unknown-rule-id overrides) are never capped** — the honest
  self-report channels outrank brevity, on the theory that a truncated warning list is worse than a long
  one.
- **`packsLoaded`** — the engine's positive pack-load confirmation (`{id, rules, source, filesInScope,
  zeroAdmissionRules?}[]`,
  id-sorted; see the [`AnalyzeOutputView` table](facade.md#the-zzop-facade-json-contract)) rides through whole on every
  `analyze_repo` reply and per-source on `cross_repo` — one entry per loaded pack, bounded by the pack
  count, so it needs no cap. `filesInScope` counts the files this tree has that the pack's rules WOULD
  scan by path-pattern candidacy alone — NOT a "matched" or "found N usages" count, and not gated on
  whether anything actually fired. It tells apart a pack that legitimately found nothing
  (`filesInScope > 0`, zero findings — the pack ran over that many eligible files and had nothing to
  report) from a pack that never had anything to check on this tree (`filesInScope: 0` — e.g. a
  frontend-only pack loaded against a backend-only tree), which a bare `packsLoaded` entry with no
  findings could not distinguish on its own. A large `filesInScope` on an otherwise-zero-finding pack
  (e.g. a `security` pack reporting `filesInScope: 116` over an all-Java tree that trips none of its
  rules) means every one of those 116 files matched the pack's `file_pattern` scope, not that a finding
  was produced in any of them — read it as "eligible", never as a usage count. `zeroAdmissionRules`
  makes the same distinction one level down, inside an in-scope pack: the sorted ids of rules whose own
  path gates (`file_pattern` plus that rule's `file_exclude_pattern`) admitted zero files here, i.e.
  rules whose zero findings are scope, not "checked and clean". In `analyze_envelope` replies the same
  field is additionally MODE-filtered: rules whose matcher kind envelope mode never evaluates
  (anything but symbol-scan/io-scan) are listed even when their path patterns match — their green is
  vacuous there. Present only when non-empty, and
  omitted on a `filesInScope: 0` pack (the pack-level zero already covers every rule of it) — see the
  facade table for the full definition. In this host the bundled packs are
  injected as inline `packDefs`, so they report `source: "inline"` (the removed JS wrapper's bundled
  packs arrived as `"dir"` instead — a packaging difference, not a behavior one).
- **`ruleOverridesApplied`** — rides through whole on every `analyze_repo` reply and per-source on
  `cross_repo`'s `sources[]` entries, same as `packsLoaded`, but omitted (not `null`) whenever the
  engine itself omits it (no `disabledRules`/`severityOverrides`/`packsOnly` requested) — see the
  [`AnalyzeOutputView` table](facade.md#the-zzop-facade-json-contract) for the field shape. Its `only`
  member is the one a reader must not skip: `packsLoaded` reports the same rows whether a pack ran or
  not, so an allowlist is invisible everywhere else in the reply.
- **`coverage`** — the engine's per-tree structural coverage census (`files`, `parserDispatched` — renamed from `sourceFiles`, `symbols`,
  `resolvedImportEdges` — renamed from `importEdges` and counting RESOLVED in-tree edges only,
  `declaredImportsByExt` — the per-extension DECLARED-specifier denominator for that edge count,
  counted before resolution; an absent extension key means "never measured", never 0,
  `ioProvides`, `ioConsumesKeyed`, `ioConsumesUnresolved`, `degraded`, `joinContributionZero` — see the
  [`AnalyzeOutputView` table](facade.md#the-zzop-facade-json-contract) for field semantics) rides through whole on every
  `analyze_repo` reply and per-source on `cross_repo`'s `sources[]` entries — a handful of scalars plus
  one small per-extension map, no cap needed. `joinContributionZero` is the engine's own blindness ASSERTION (this tree extracted no
  JOINABLE io — 0 provides and 0 keyed consumes, unresolved consumes don't count — while analyzing
  `files > 0`, so it is invisible to the cross-layer join) and must reach the summary
  reader — a "0 findings" tree that contributed nothing to the join is not a clean tree.
- **`disclosure`** — the engine's run-global, pinned silent-failure-class registry, carried on every
  `analyze_repo`/`cross_repo`/`check_endpoint` reply as a **fold** since 2026-07-29: the reply gets the
  registry's *shape* (`classes`, `asserted`, `partial`, `notYetDetected`) plus a `note`, a `resource`
  (`zzop://contract/disclosure-classes`) and a runnable `command` (`zzop contract disclosure-classes`).
  The prose itself ships once through the contract lane rather than on every call — it was byte-identical
  on every run and every tree, and measured 10,809 of 16,617 reply bytes on this repo (92% of a small
  `check_endpoint` reply), i.e. a fixed per-call tax on the agent this output is written for.
  **What did not change**: the fact that gaps exist *and their magnitude* are still in the reply without
  asking, which is what the disclosure doctrine requires; and run-VARYING disclosure — `coverage`
  (including `joinContributionZero`) and the `warnings` tripwires — is untouched. The counts and the
  served text are derived from one registry and sealed by tests on both sides, so a registry that grows
  while the counts stand still fails the build. See
  [`disclosure` — silent-failure-class registry (run-global)](facade.md#disclosure--silent-failure-class-registry-run-global)
  for the facade-level field contract, which still carries the full array (it is the derivation source).
- **`architecture`** (`analyze_repo` only) — a compact, capped summary closing a disclosure asymmetry:
  the facade output carries full `health`/`recommendations`/`critical` (git-history-dependent
  structural-debt metrics — see the [`AnalyzeOutputView` table](facade.md#the-zzop-facade-json-contract)), but this host's
  shaped reply otherwise dropped all three entirely, even though `analyze_repo`'s own description
  promises "git signals included". Present **only** when `health` rode this tree's output
  (i.e. git signals actually ran — a real `.git` history, not merely the default `git: {}`
  request) — **absent**, never `null`, when they did not (e.g. no `.git` directory at all). Shape:
  `{pain, painByAxis, painMeasuredWeight, painTotalWeight, painMeaning, topRecommendation, criticalTop}`.
  `pain` is `health.pain` — the composite structural-debt scalar, and **the only score number any
  shipped CLI or MCP surface publishes** (the full `scores` object rides the raw `zzop-facade`
  embedding lane alone, so nothing else here carries a per-metric denominator). It therefore ships
  with its OWN AXIS SPLIT since 2026-08-12 — `painByAxis` says, on the wire, that this number contains
  no rule findings at all and that most of it is a structural OPINION rather than a defect claim:
  `defect` (import cycles, the only entry), `opinion` (barrel discipline, FSD layering, SDP/Main
  Sequence, Newman modularity, LOC ceilings) and `history` (rename churn, bus factor), each on `pain`'s
  own scale and summing to it. The measurement that forced it: adding 20 `$queryRawUnsafe` files to a
  tree moved `critical` from 1 to 21 and left `pain` byte-identical. It also ships
  with its OWN denominator since 2026-08-08: `painMeasuredWeight` / `painTotalWeight` is the share of
  the weighted metric table that had a population to score over on this tree, and `painMeaning` is
  the sentence saying how to read the pair — the same self-describing-reply device as
  `verdictMeaning`, for the same reason. A bare `pain` was precisely the folded single score
  `queryCoverage` forbids ("there is deliberately NO single score field, and one must never be
  added"), and it read BETTER for a tree zzop could see less of, because every metric with nothing to
  measure scores 100 and so contributed no pain. `pain` is `null`, never `0`, when nothing was
  measurable at all; `topRecommendation` is `null`-safe `{id, severity, topItem}` built from
  `recommendations[0]` (`topItem` is that recommendation's top-ROI item's `path`, `null` when there is
  none); `criticalTop` is up to 3 file paths off the front of the engine's own **size-weighted
  blast-radius** ranking of `critical` (`blastRadius * ln(loc + 2)`, with `blastRadius` as the
  tie-break — `crates/metrics/src/criticality.rs`). The weighting is why a 400-line core outranks a
  5-line re-export barrel of equal blast, and it is also why re-sorting the raw `critical` array by
  `blastRadius` alone does not reproduce these three paths; `critical` already arrives in this order. The full arrays are never in this summary — only the raw `zzop-facade` JSON output,
  reachable by embedding the engine (Rust crate) directly, carries the complete per-file
  `recommendations`/`critical` detail; no shipped CLI/MCP surface emits it.
- **`gitWindow`** (every analysis reply, `null` when git did not run) — the engine's own `{recentDays, since}` echo of which git
  window produced the run's numbers (always serialized by the engine, `null` when git signals did not
  run — see `crates/engine/src/output.rs`'s `GitWindow`), forwarded verbatim by name so a consumer
  diffing two runs' `scores`/`health`/`architecture` numbers can tell which window produced which
  output.

## Build instructions

Neither product package has an MSVC/toolchain requirement — they build under the workspace's
default toolchain on every platform:

```sh
cargo build -p zzop-cli-bin -p zzop-mcp --release
```

The binaries land at `target/release/zzop` and `target/release/zzop-mcp` (`.exe` on Windows); drop
`--release` for debug builds at `target/debug/` during local iteration. `cargo test -p zzop-mcp` runs
the `zzop-mcp` package's own protocol/dispatch tests (`resources.rs`, `server.rs`, and the `tools/list`/
`tools/call` schema tests in `tools/tests.rs`); `cargo test -p zzop-cli-bin` runs the CLI binary's own
argv-dispatch tests. Everything both of them dispatch INTO is tested under `cargo test -p zzop-summary`:
that crate holds `output/`, the filters and the analyze/cross/endpoint summary assembly, plus
`tests/host_dispatch.rs` — the end-to-end `analyze`/`cross`/`endpoint` run over real temp trees, driving
the crate through the exact public surface both products call.

## Distribution status

Both products ship as self-contained, per-platform static binaries attached to every tagged [GitHub
Release](https://github.com/eezz4/zzop/releases) (`prebuild.yml`'s tag-triggered build); building from a
source checkout with the command above remains fully supported as an alternative. The install lanes
themselves — release asset, Claude Code plugin, Claude Desktop `.mcpb` bundle, npm `@zzop/cli` — are
listed once in the repo [README's Quick start](../../README.md#quick-start) rather than restated here,
and the `.mcp.json` worked example plus the plugin install path live in
[packages/README.md](../../packages/README.md).

See also: [facade.md](facade.md) (the request/response
shapes this host drives directly), [../NORMALIZED_AST.md](../NORMALIZED_AST.md) (the
envelope contract behind `validate_envelope`/`envelope-guide`), [../adapters/README.md](../adapters/README.md)
(adapter authoring, mirrored by the `adapter-guide` resource),
[../recipes/verify-before-fetch.md](../recipes/verify-before-fetch.md) (the workflow these tools
exist for: checking the other repo's API from inside yours, and how to read a negative answer).

## The `zzop-facade` JSON contract

Moved to [facade.md](facade.md) — the engine's request/response surface (every entry point, enumerated
there and nowhere else; the `pub use` block at the bottom of `crates/facade/src/lib.rs` is the one
place that decides the set), the `AnalyzeRequest`/`AnalyzeOutputView` field tables, the config requirement and what still defaults,
the endpoint-query, file-query and `disclosure` shapes, and the error/panic discipline). It is a separate page because it is read for a
separate reason: embedding the engine, rather than wiring an MCP client to the host documented here.

