# The Node-free host: `zzop` CLI + `zzop-mcp` MCP server

**Two** self-contained binaries over one shared library (`zzop-host`, `crates/host`), running the zzop analysis
engine with no Node.js runtime at all. These binaries call
`analyze_json`/`analyze_envelope_json`/`analyze_trees_json`/`validate_envelope_only_json`/`validate_rule_pack_json`/`query_io_json`
(`crates/facade/src/lib.rs`) directly — the `zzop-facade` JSON contract documented in full under
[The `zzop-facade` JSON contract](#the-zzop-facade-json-contract) below:

- **`zzop-mcp`** — the **MCP server** over stdio (bare `zzop-mcp`, or the `zzop-mcp mcp` form `.mcp.json`
  and the MCPB manifest register). For MCP clients.
- **`zzop`** — the **CLI** (`zzop analyze <path>` / `zzop analyze-envelope <envelope.json>` / `zzop cross
  <path>...` / `zzop endpoint <pattern> <path>...` / `zzop contract` / `zzop validate-…`). For direct
  terminal/CI use, no MCP client required.

Each is a Cargo package building exactly one thin argv-dispatch binary — `zzop` is package
`zzop-cli-bin` (`packages/cli-bin`), `zzop-mcp` is package `zzop-mcp` (`packages/mcp`) — over the shared
`zzop-host` lib crate (`crates/host`), whose `tools.rs` handlers both dispatch to, so a CLI run and an
MCP `tools/call` against the same path produce the same analysis through the same code.

## Module map

| Location | Responsibility |
|---|---|
| `packages/cli-bin/src/main.rs` | The `zzop` CLI entry — thin argument dispatch: `analyze` / `cross` / `endpoint` / `contract` / `validate-*` / `version` subcommands over the shared `zzop-host` lib. |
| `packages/cli-bin/src/cli.rs` | The CLI's own argv-parsing/usage/exit-code helpers. |
| `packages/mcp/src/bin/zzop-mcp.rs` | The `zzop-mcp` server entry — thin: bare / `mcp` serve stdio; `version` / `help` / unknown-arg lanes. |
| `packages/mcp/src/server.rs` | The stdio JSON-RPC 2.0 loop (`initialize`, `tools/*`, `resources/*`); re-exports `version()` from `crates/host`. |
| `packages/mcp/src/tools.rs` | Pure dispatch: match tool name → extract arguments → call the shared `zzop-summary` function → wrap the MCP result. No shaping logic lives here. |
| `packages/mcp/src/tools/definitions.rs` | MCP tool descriptions + input schemas (`tools/list`). |
| `packages/mcp/src/resources.rs` | MCP resource handlers (`resources/list`, `resources/read`) over the embedded authoring contracts (from `crates/host::embedded`). |
| `crates/host/src/tools.rs` | Shared dispatch: the typed `analyze`/`analyze_envelope`/`cross_repo`/`check_endpoint`/`validate_*` functions both the CLI subcommands and the MCP tool dispatch above call into. |
| `crates/host/src/embedded.rs` | The embedded contract documents themselves — compiled into both binaries via `include_str!`, the ONE table both surfaces resolve `<name>` through. |
| `crates/host/src/server.rs` | `version()` only — `CARGO_PKG_VERSION`, shared by both binaries so they can never disagree. |

Everything functional — output shaping (full counts, capped lists, explicit truncation disclosure;
see [Output contract](#output-contract) below), finding filters, bucket keys/sites, typo
suggestions, the architecture summary, config-warnings merging, tree resolution, sibling-scope
disclosure, path absolutization — is **not** in either product crate: it lives in the shared
`zzop-summary` crate (`crates/summary`), whose crate doc states the rule: hosts are thin protocol
facades, ALL summary logic is shared so it cannot drift per-host (the surface-parity contract at
[contracts/surface-parity.json](../contracts/surface-parity.json) machine-checks the complement:
every engine output field is either carried or documented-omitted per surface). The config
front-end (`zzop.config.jsonc` discovery, JSONC parsing, config→request mapping, `trees: "auto"`
workspace expansion) likewise lives in the shared `zzop-config` crate (`crates/config`).
`zzop-host`'s own `Cargo.toml` depends on `zzop-summary` (which re-exports the facade call path) and
`zzop-config` (embedded config-surface bytes) and nothing else beyond `serde_json`; the two product
packages each depend on `zzop-host` plus whatever else their own surface needs (`zzop-mcp` also
depends on `zzop-summary`/`zzop-config` directly for its own tool dispatch).

## CLI surface

```
zzop analyze <path>                  # analyze ONE repo/tree, print a JSON findings summary
zzop analyze-envelope <envelope.json>  # Mode A: a Normalized-AST envelope file replaces native parsing, same summary shape
zzop cross <path>...                 # analyze 2+ trees, print the cross-layer join (paths mode)
zzop cross --config <zzop.config.jsonc>  # same, but the config's `trees` define the join
zzop endpoint <pattern> <path>...    # definitive "is io key X provided/consumed/joined?" query
zzop endpoint <pattern> --config <zzop.config.jsonc>  # same query, the config's `trees` define the join
zzop contract                        # list the embedded authoring contracts (name, mime, description)
zzop contract <name>                 # print that contract document to stdout (raw bytes, pipe-safe)
zzop version                         # print this binary's version (also: --version)
zzop-mcp mcp                             # the MCP server over stdio (newline-delimited JSON-RPC 2.0)
zzop-mcp help                            # print the usage line, exit 0 (also: --help, -h)
```

`analyze`/`analyze-envelope`/`cross`/`endpoint` print pretty-printed JSON to stdout on success (exit
`0`); a failure prints `zzop-mcp: <message>` to stderr and exits `1`. `analyze-envelope <file>` reads
the given path as the envelope JSON text (an unreadable file is the same exit-`1` runtime-failure lane,
`zzop-mcp: failed to read <path>: <os error>`) and runs the identical Mode A analysis the
`analyze_envelope` MCP tool does — same handler, same output shape `analyze` produces, minus the
filesystem-only `path`/`config` fields an envelope has neither of. A missing/malformed argument (no `<path>`,
`--config` with no path following it, `endpoint` with no pattern or no path, a flag-looking argument
in a path/pattern position — the only recognized flag there is `--config`, so `analyze --help` is a
usage error, never the path `--help`) exits `2` with a usage line. `zzop-mcp help`/`--help`/`-h`
prints the same usage line to stdout and exits `0`. Path arguments (tree roots and `--config` files
alike) are absolutized against the invocation cwd before any config handling, so `zzop analyze .`
and relative `--config` paths work from anywhere. The analysis subcommands run the unfiltered default view (no `severity`/`rule`/`limit`
narrowing — that's an MCP-tool-only argument surface today). `endpoint` with ONE path is the
`check_endpoint` tool's `path` mode, with 2+ paths its config-free `paths` mode, and with `--config`
its config-first `configPath` mode — the exact same handler each way, so a CLI query and a tool call
give the identical answer. `contract` with no name lists every embedded authoring contract (one
human-readable line each: name, mime, description); `contract <name>` prints that document's exact
embedded bytes to stdout (pipe-safe — the same bytes MCP `resources/read` serves for
`zzop://contract/<name>`, resolved through the same lookup, so the two surfaces cannot drift); an
unknown name exits `1` with an error naming every valid contract. `version`/`--version` prints
`zzop-mcp <version>` (exit `0`) — the exact value MCP `initialize` reports as `serverInfo.version`
(see below), so the two surfaces can never disagree.

**Windows Git Bash / MSYS caveat**: a leading-slash `endpoint <pattern>` (e.g. `/articles`) run from Git
Bash/MSYS gets silently path-converted by the shell BEFORE it reaches this binary — `zzop endpoint
"/articles" <path>` can become a query for `C:/Program Files/Git/articles`, producing a confusing
false not-found with no indication the pattern itself was rewritten. This is a shell behavior, not a
`zzop-mcp` bug, but it bites `endpoint` specifically because its first argument is a bare pattern rather
than a path. Work around it either by quoting AND setting `MSYS_NO_PATHCONV=1` for the invocation
(`MSYS_NO_PATHCONV=1 zzop endpoint "/articles" <path>`), or by running the command from PowerShell/
cmd instead, neither of which path-converts arguments.

## MCP surface

`initialize` replies with `serverInfo: { name: "zzop", version }` — `version` is `CARGO_PKG_VERSION`
(`crates/host/src/server.rs`), the workspace `[workspace.package] version`, the same value a dev build
in-tree reports. CI's release job verifies the pushed tag and `.claude-plugin/plugin.json` both match
this number, so a released build's `serverInfo.version` equals the release tag and the plugin's
published version by construction (see [`version()`](#the-zzop-facade-json-contract) below and
[VERSIONING.md](../../VERSIONING.md)).

### Tools (`tools/list` / `tools/call`)

| Tool | Purpose |
|---|---|
| `analyze_repo` | Analyze ONE repo/tree path. |
| `cross_repo` | Analyze 2+ repos/trees and join them across the cross-layer (kind, key) boundary — zzop's headline capability (e.g. a frontend `fetch` call matched against a backend route, a shared DB table, route drift). |
| `check_endpoint` | DEFINITIVE answer to "is io key X provided/consumed/joined?" — matches a pattern against ANY cross-layer io key (http routes, env keys, DB tables, topics) as a case-insensitive substring and returns ONE verdict from the sealed vocabulary `linked` / `provided-only` / `consumed-unprovided` / `external` / `unresolved-only` / `ambiguous` / `mixed` / `not-found`, plus full counts, capped match lists, related findings, and key suggestions on `not-found`. Runs the shared facade query core directly — the same core any embedder driving `zzop-facade`/`zzop-summary` gets identical answers from (see [Endpoint queries: `queryIo`](#endpoint-queries-queryio) for the full output contract). |
| `analyze_envelope` | **Mode A**: a full Normalized AST envelope (a custom parser's output) REPLACES native parsing entirely for this run — contrast `validate_envelope` below, which only checks the envelope's shape and runs no analysis, and Mode B overlay/mount requests (`docs/NORMALIZED_AST.md`), which merge external symbols ON TOP of a natively-parsed tree instead of replacing it. Only symbol-scan/io-scan rules can fire (no source text ships in an envelope). Zero-config only — an envelope carries no filesystem location, so the reply has no `config`/`path`/`architecture`/`gitWindow` fields; otherwise the SAME shaped summary `analyze_repo` returns (findings, `packsLoaded`, `coverage`, warnings). Same `analyzeEnvelope` facade call path documented in [Defaults (zero-config = full analysis)](#defaults-zero-config--full-analysis). |
| `validate_envelope` | Validate a Normalized AST envelope against the v1 contract WITHOUT running an analysis — the authoring feedback loop. Returns `{valid, issues[]}`; never fails on bad input (same contract as the facade's `validateEnvelopeOnly` — see [Validation-only: `validateEnvelopeOnly`](#validation-only-validateenvelopeonly)). |
| `validate_rule_pack` | Validate a DSL rule pack's STRUCTURE before loading it — the exact judgments the engine's pack loader makes at load time (bad JSON, missing field, wrong type, too-new `schema_version`) plus every matcher regex that fails to compile (such a rule would load but silently never fire). Shape only, never rule-quality semantics. Returns `{valid, issues[]}`; never fails on bad input (same contract as the facade's `validateRulePackOnly`). Pair with the `rule-pack-schema` resource below. |

`analyze_repo`, `cross_repo`, and `analyze_envelope` share three optional drill-down arguments, described in
[Output contract](#output-contract) below: `severity` (`"critical" | "warning" | "info"`, minimum
severity to include in the findings *list* — counts always cover everything), `rule` (exact rule id),
and `limit` (list cap, default 50, min 0 — `0` is a legal "counts only, no findings listed" query —
max 1000).

**Argument validation is strict, not advisory.** A boundary-value round found every tool argument
here silently accepted the wrong JSON type and behaved as "not provided": a `limit` of `-1`, `1001`,
`999999`, the STRING `"50"`, or the float `3.7` all passed through as "no cap"; a `severity` NUMBER
silently dropped the severity filter instead of being rejected like an unknown severity STRING. Every
one of those is now a named `zzop error: ...` rejection instead — `limit` must be a JSON integer in
`[0, 1000]` (`zzop error: limit must be an integer between 0 and 1000 (got <value>)`), and any
non-string `severity` value hits the exact same rejection an unknown severity string gets
(`zzop error: unknown severity <value> — valid values: "critical", "warning", "info"`). The same
sweep covers every other declared-type argument across all six tools — `path`, `paths` (and its
array elements), `configPath`, `pattern`, `rule`, `envelopeJson`, `packJson` — a wrong JSON type
(a number where a string is required, an array element that isn't a string, ...) is always a named
`` `<name>` must be a string (got <value>) `` error, never a silent fallback to "argument omitted".
Only an absent key or an explicit JSON `null` means "not provided" — every other type mismatch is a
caller mistake, named.

`analyze_repo({ path })` auto-discovers `<path>/zzop.config.jsonc` (see
[Config semantics](#config-semantics) below); the reply's `config` field says whether one was honored
(a path string) or not (`null`, zero-config defaults applied), and `path` always echoes the RESOLVED
absolute directory the analysis actually ran against — never the raw argument verbatim (a relative
`path: "."` used to echo back the literal `.`, with the actual analyzed directory never disclosed
anywhere in the reply). A config that declares multiple trees is a guided error telling the caller to
use `cross_repo` with `configPath` instead, or to point `analyze_repo` at one tree root directly.

When the underlying analysis ran git signals (the zero-config default, or a config's own `git`
settings, provided a real git history is actually present), `analyze_repo`'s reply also carries a
compact, capped `architecture` object — `{pain, topRecommendation, criticalTop}` — summarizing the
facade's `health`/`recommendations`/`critical` computation (see
[Output contract](#output-contract) below for the exact shape); it is present only then, absent
(never `null`) otherwise. `cross_repo`'s `sources[].path` was audited for the same raw-path-echo gap
and found already correct (each source's path is the resolved absolute tree root, not the raw
argument) — no fix was needed there.

`cross_repo` takes **either** `configPath` (a `zzop.config.jsonc` — its `trees`, including `"auto"`,
define the join; the config-first way) **or** `paths` (2+ explicit tree roots; config-free — each tree
tagged by its directory name). Passing both, or neither, is a named argument error. See
[Config semantics](#config-semantics) for what paths mode discloses.

`check_endpoint({ pattern, ... })` requires a non-empty `pattern` (`minLength: 1` in the schema,
matching the behavior the shared facade query core already enforced — the schema had simply
under-declared it) plus exactly ONE of `path` (a single tree,
resolved like `analyze_repo` — config auto-discovery included), `paths` (2+ config-free tree roots,
resolved like `cross_repo`'s paths mode, disclosure warnings included), or `configPath`. Every mode
runs `analyzeTrees` — even a single `path` — because a verdict is a cross-layer JOIN fact, and the
join runs fine over one tree (intra-tree edges included). The reply is the shared query core's JSON
(pretty-printed): `{pattern, verdict, counts, matches, truncated?, relatedFindings,
truncatedFindings?, suggestions?, disclosure}` — see [Endpoint queries: `queryIo`](#endpoint-queries-queryio) for
every field and the sealed verdict vocabulary — plus this host's two honesty channels stamped on
top, same as the other tree-resolving tools (`analyze_repo`/`cross_repo`; `analyze_envelope` and the
two validators take no filesystem-rooted config, so they carry neither field): `config` (which
config file was honored, or null) and
`configWarnings` (the config front-end's own disclosures, e.g. paths mode's ignored-config
warning). The query-core fields are pinned across every host that drives the shared facade query core.

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
- A line that parses but isn't a single JSON object (e.g. a batch array — unsupported by this server, and
  unused by MCP clients) answers `-32600` (Invalid Request).

Both also log one line to stderr. An unknown `method` (anything other than `initialize`/`tools/list`/
`tools/call`/`resources/list`/`resources/read`) answers `-32601` (Method not found) with the request's
own `id`. None of this is the `isError: true` tool-result channel described above — that channel is only
for a named tool call that ran and failed; these are protocol-level responses for input the server
couldn't even dispatch.

### Resources (`resources/list` / `resources/read`)

Ten embedded authoring contracts, addressed as `zzop://contract/<name>` — the documents a custom-parser,
DSL-rule, or config author needs, with no zzop source checkout and no Node required, since they are compiled
into the binary (`embedded.rs`, `include_str!` over the repo's own committed public docs, ~180KB total):

| `<name>` | Content |
|---|---|
| `envelope-schema` | JSON Schema (draft-07) for the Normalized AST envelope v1 — machine-validate a custom parser's output. |
| `envelope-guide` | The Normalized AST envelope contract: Mode A (full envelope) / Mode B (overlay) adapter authoring, field semantics, worked examples (`docs/NORMALIZED_AST.md`). |
| `key-normalization-fixture` | Byte-pinned HTTP key-normalization fixture — the exact `(method, path)` → join-key rows an adapter must reproduce for cross-layer joins. |
| `adapter-guide` | Adapter authoring README: key-normalization parity rules, schema/versioning policy, adapter-kit pointers (`docs/adapters/README.md`). |
| `dsl-reference` | DSL rule-pack reference: pack/rule fields and all four matchers (`docs/rules/dsl-reference.md`). |
| `dsl-authoring-guide` | DSL rule authoring guide: placement, a worked example, testing conventions (`docs/rules/authoring-guide.md`). |
| `rule-pack-schema` | JSON Schema (draft-07) for the DSL rule-pack shape — pack id, rules[], the four matcher kinds, severity, every property documented (`docs/contracts/rule-pack.schema.json`; the machine-readable twin of the `validate_rule_pack` tool). |
| `example-envelope` | Minimal valid Mode-A envelope example (a crude JSP parser's output). |
| `config-surface` | Machine-verified config vocabulary — every config key, dotted path, CLI flag, and embedder field zzop accepts (`crates/config/config-surface.json`, the same file `zzop-config` embeds for unknown-key warnings; its `_docs` sections self-describe). |
| `rule-catalog` | Every rule id the engine ships today — 15 DSL packs + all native analysis ids, with severity/matcher/suppress-marker/detection prose per rule (`docs/rules/catalog.md`) — the discoverability gap closed: `packsLoaded` gives counts only, and the `dsl-reference` resource pointed at this file without it ever being served over MCP. Pair with the `rule` tool argument on `analyze_repo`/`cross_repo`/`check_endpoint` (an id absent from this catalog never fires). |

`resources/list` returns every entry above (in this order) with its `uri`/`name`/`description`/
`mimeType`; `resources/read` returns the full text verbatim. Deterministic: same binary, same list, same
bytes every time. An unknown `uri` is a named error listing every valid resource — an agent should never
have to guess the name. The same ten documents are reachable without an MCP client via
`zzop contract [<name>]` (see [CLI surface](#cli-surface)) — both surfaces resolve names through the
one embedded lookup, so they can never disagree on what exists.

## Config semantics

All config handling is delegated to the shared `zzop-config` crate (`crates/config`) — the Rust-hosted
config front end (discovery, mapping, and zero-config defaulting, ported from the removed JS CLI's
`config.js`/`mapper.js`) that a future full-CLI Rust binary would reuse. Three things to know about how
it behaves here specifically:

- **Auto-discovery.** `analyze_repo`/`zzop analyze <path>` look for `<path>/zzop.config.jsonc`
  (literal filename, no ancestor walk — the same rule the removed JS CLI used). If present, it is parsed
  (JSONC: comments and trailing commas allowed) and mapped by the same rules the JS CLI's `mapper.js`
  used to, now ported to `zzop-config`'s Rust mapper.
- **Zero-config defaults.** Unlike the removed JS CLI (which errored without a config file), a missing
  config produces the *same request an empty `{}` config would produce* — an MCP tool pointed at any repo
  must still work. This still gets the full default treatment: the bundled DSL rule packs (embedded at
  compile time by `zzop-config`'s `build.rs` and injected as inline `packDefs` — see
  [Defaults (zero-config = full analysis)](#defaults-zero-config--full-analysis)'s note on this field) and default `git: {}`
  collection (30-day recency) both apply, the same defaulting the removed JS wrapper's `withDefaults`
  used to inject.
- **`configPath` / paths-mode disclosure (`cross_repo`).** Config-first mode (`configPath`) loads that
  file directly (or a directory containing `zzop.config.jsonc`) and requires it to declare `trees` (2+,
  or `"auto"`) — a single-tree config there is a guided error pointing at `analyze_repo` instead.
  Paths mode (`paths`) builds one zero-config tree request per path, tagged `sourceId` = that directory's
  name; critically, it does **not** load a `zzop.config.jsonc` sitting inside any of those paths — and it
  says so. Each such path adds a `configWarnings` entry (`"<path> contains a zzop.config.jsonc that
  paths mode does NOT load — pass configPath to honor it"`) rather than silently ignoring it, or
  silently loading it and surprising the caller with rules/overlays/mounts they never asked this call to
  apply.
- **Path resolution deviates from the removed JS CLI in one documented way.** The JS CLI used to leave
  `root`/`cacheDir`/`packsDir` as literal strings for the analyzing process's cwd to resolve — which, in
  normal CLI use, *was* the config file's own directory. A server host's cwd is meaningless (an MCP
  client can invoke this binary from anywhere), so `zzop-config` resolves these path-ish config values
  against the **config file's own directory** instead. Overlay paths are the one exception and keep the
  same relative-to-tree-root resolution the JS mapper used.

Every reply from `analyze_repo`/`cross_repo` carries `config` (the config file path honored, or `null`)
and `configWarnings` (the config front-end's own non-fatal notes — unknown keys, a skipped/unreadable
overlay, an `"auto"`-expansion report, the paths-mode disclosure above) as a channel **separate** from
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

- **Findings** shape to `{total, bySeverity, byRule, shown, truncated?}`. `total`/`bySeverity`/`byRule`
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
- **Cross-layer edges** (`cross_repo`) get the same treatment via a plain list cap (`edgesTruncated`,
  default cap 200 — edges are small rows, so most joins fit uncapped).
- **`degraded`** (`analyze_repo` only) — the size-capped/parse-failure file-path list gets the same
  cap-plus-disclosure treatment as every other list (`degradedTruncated`, default cap 50) rather than
  riding through verbatim, which would bypass this module's own token-bomb guard on a repo with
  thousands of degraded files. `coverage.degraded` (below) already carries the full, uncapped COUNT, so
  this list is supplementary detail (which files, not just how many) and is never the only source of
  the number.
- **`bucketKeys`** (`cross_repo`) — alongside the numeric `buckets` counts, each of the five non-edge
  join buckets (`unconsumedProvides`, `unprovidedConsumes`, `unresolvedConsumes`, `externalConsumes`,
  `ambiguousConsumes`) lists up to 20 DISTINCT keys (deduped, engine order preserved; an unresolved
  consume contributes its `raw` expression when recorded), so an agent sees WHICH keys sit in a bucket
  instead of only how many. A capped bucket discloses its remainder in `bucketKeysTruncated`
  (`{bucket: remainingDistinctCount}`, present only when something was capped) — for the definitive
  per-key answer, follow up with `check_endpoint`. A parallel `bucketKeySites` object mirrors
  `bucketKeys` with each key's first recorded site as `"file:line"` (`null` when the fact carries no
  location — never guessed), so a listed key is locatable without a follow-up call.
- **`warnings` (engine) and `configWarnings` (config front-end + engine-side config diagnostics,
  e.g. unknown-rule-id overrides) are never capped** — the honest
  self-report channels outrank brevity, on the theory that a truncated warning list is worse than a long
  one.
- **`packsLoaded`** — the engine's positive pack-load confirmation (`{id, rules, source, filesInScope}[]`,
  id-sorted; see the [`AnalyzeOutputView` table](#the-zzop-facade-json-contract) below) rides through whole on every
  `analyze_repo` reply and per-source on `cross_repo` — one entry per loaded pack, bounded by the pack
  count, so it needs no cap. `filesInScope` counts the files this tree has that the pack's rules WOULD
  scan by path-pattern candidacy alone — NOT a "matched" or "found N usages" count, and not gated on
  whether anything actually fired. It tells apart a pack that legitimately found nothing
  (`filesInScope > 0`, zero findings — the pack ran over that many eligible files and had nothing to
  report) from a pack that never had anything to check on this tree (`filesInScope: 0` — e.g. a
  frontend-only pack loaded against a backend-only tree), which a bare `packsLoaded` entry with no
  findings could not distinguish on its own. A large `filesInScope` on an otherwise-zero-finding pack
  (e.g. a `be-security` pack reporting `filesInScope: 116` over an all-Java tree that trips none of its
  rules) means every one of those 116 files matched the pack's `file_pattern` scope, not that a finding
  was produced in any of them — read it as "eligible", never as a usage count. In this host's zero-config paths the bundled packs are
  injected as inline `packDefs`, so they report `source: "inline"` (the removed JS wrapper's bundled
  packs arrived as `"dir"` instead — a packaging difference, not a behavior one).
- **`ruleOverridesApplied`** — rides through whole on every `analyze_repo` reply and per-source on
  `cross_repo`'s `sources[]` entries, same as `packsLoaded`, but omitted (not `null`) whenever the
  engine itself omits it (no `disabledRules`/`severityOverrides` requested) — see the
  [`AnalyzeOutputView` table](#the-zzop-facade-json-contract) below for the field shape.
- **`coverage`** — the engine's per-tree structural coverage census (`files`, `symbols`, `importEdges`,
  `ioProvides`, `ioConsumesKeyed`, `ioConsumesUnresolved`, `degraded`, `joinContributionZero` — see the
  [`AnalyzeOutputView` table](#the-zzop-facade-json-contract) below for field semantics) rides through whole on every
  `analyze_repo` reply and per-source on `cross_repo`'s `sources[]` entries — a handful of scalars, no
  cap needed. `joinContributionZero` is the engine's own blindness ASSERTION (this tree extracted no
  JOINABLE io — 0 provides and 0 keyed consumes, unresolved consumes don't count — while analyzing
  `files > 0`, so it is invisible to the cross-layer join) and must reach the summary
  reader — a "0 findings" tree that contributed nothing to the join is not a clean tree.
- **`disclosure`** — the engine's run-global, pinned silent-failure-class registry (identical every run;
  see [`disclosure` — silent-failure-class registry (run-global)](#disclosure--silent-failure-class-registry-run-global) for the full field
  contract) rides through unfiltered on every `analyze_repo`/`cross_repo` reply, the same meta-honesty
  channel the shared facade exposes to every host.
- **`architecture`** (`analyze_repo` only) — a compact, capped summary closing a disclosure asymmetry:
  the facade output carries full `health`/`recommendations`/`critical` (git-history-dependent
  structural-debt metrics — see the [`AnalyzeOutputView` table](#the-zzop-facade-json-contract) below), but this host's
  shaped reply otherwise dropped all three entirely, even though `analyze_repo`'s own description
  promises zero-config "git signals included". Present **only** when `health` rode this tree's output
  (i.e. git signals actually ran — a real `.git` history, not merely the zero-config default `git: {}`
  request) — **absent**, never `null`, when they did not (e.g. no `.git` directory at all). Shape:
  `{pain, topRecommendation, criticalTop}` — `pain` is `health.pain` (the composite structural-debt
  scalar); `topRecommendation` is `null`-safe `{id, severity, topItem}` built from
  `recommendations[0]` (`topItem` is that recommendation's top-ROI item's `path`, `null` when there is
  none); `criticalTop` is up to 3 file paths off the front of the engine's own blast-radius-ranked
  `critical` list. The full arrays are never in this summary — only the raw `zzop-facade` JSON output,
  reachable by embedding the engine (Rust crate) directly, carries the complete per-file
  `recommendations`/`critical` detail; no shipped CLI/MCP surface emits it.
- **`gitWindow`** (`analyze_repo` only) — the engine's own `{recentDays, since}` echo of which git
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
`tools/call` schema tests in `tools/tests.rs`); `cargo test -p zzop-host` runs the shared crate's own
dispatch tests (the end-to-end `analyze`/`cross_repo`/`check_endpoint` tool tests in `tools/tests.rs`);
`cargo test -p zzop-cli-bin` runs the CLI binary's own argv-dispatch tests. The shaping logic every
dispatch calls into is tested under `cargo test -p zzop-summary` (that crate holds `output/`, filters,
and the analyze/cross/endpoint summary assembly since the facade-thinning split — see the module map
above).

## Distribution status

Prebuilt, per-platform `zzop-mcp` binaries are attached to every tagged [GitHub
Release](https://github.com/eezz4/zzop/releases) (`prebuild.yml`'s tag-triggered build): named
`zzop-mcp-<platform>[.exe]`
for the 5 platforms `win32-x64-msvc`, `darwin-x64`, `darwin-arm64`, `linux-x64-gnu`, `linux-arm64-gnu` —
a self-contained static binary per platform, no Node required. Download the asset for your platform, rename/link
it to `zzop-mcp` (`zzop-mcp.exe` on Windows), and point `.mcp.json` at it (see
[crates/host/README.md](../../crates/host/README.md) for the worked example, including the Claude Code
plugin install path). Building from a source checkout with the command above remains fully supported as
an alternative to downloading a release asset. Separately, this same engine is packaged for npm as
`@zzop/cli` (a zero-logic shim spawning the native `zzop` binary above) plus its 5 platform sub-packages —
see [packages/cli/README.md](../../packages/cli/README.md).

See also: [The `zzop-facade` JSON contract](#the-zzop-facade-json-contract) below (the request/response
shapes this host drives directly), [../NORMALIZED_AST.md](../NORMALIZED_AST.md) (the
envelope contract behind `validate_envelope`/`envelope-guide`), [../adapters/README.md](../adapters/README.md)
(adapter authoring, mirrored by the `adapter-guide` resource).

## The `zzop-facade` JSON contract

The zzop analysis engine's request/response surface. The functions below are all JSON-string-in /
JSON-string-out (except `version`), DEFINED in the shared `zzop-facade` crate
(`crates/facade/src/lib.rs`) — plain Rust that compiles and has a normal `#[test]` surface under the
workspace's default `gnu` toolchain with no feature flags. The two host products documented on this
page reach these `zzop-facade` functions through the shared `zzop-summary` crate — no Node process at
all — and any embedder can drive the same JSON contract directly. This section documents those
request/response shapes.

### Functions

| Name | Rust signature | Request → Response |
|---|---|---|
| `analyze` | `(configJson: string) -> string` | `AnalyzeRequest` → `AnalyzeOutputView` |
| `analyzeTrees` | `(configJson: string) -> string` | `AnalyzeTreesRequest{trees: [AnalyzeRequest]}` → `MultiAnalyzeOutputView` |
| `analyzeEnvelope` | `(envelopeJson: string, configJson: string) -> string` | `NormalizedEnvelope` + `EnvelopeAnalyzeRequest` → `AnalyzeOutputView` |
| `validateEnvelopeOnly` | `(envelopeJson: string) -> string` | envelope JSON → `{valid: boolean, issues: string[]}` — see [below](#validation-only-validateenvelopeonly). |
| `validateRulePackOnly` | `(packJson: string) -> string` | rule-pack JSON → `{valid: boolean, issues: string[]}` — see [below](#validation-only-validaterulepackonly). |
| `queryIo` | `(analysisJson: string, queryJson: string) -> string` | an `analyzeTrees` OUTPUT + `{pattern}` → the definitive endpoint-query result — see [below](#endpoint-queries-queryio). |
| `version` | `() -> string` | none (cannot fail, no `Result`) |

`AnalyzeRequest` (`#[serde(rename_all="camelCase", default)]`, unknown fields ignored):

| Field | Type | Notes |
|---|---|---|
| `root` | `String` (required — empty → `Err`) | Tree root to walk. |
| `sourceId` | `String` (default `""`) | Free-form label carried through into cross-tree output. |
| `packsDir` | `Option<String \| String[]>` | Directory (or directories) of `*.json` DSL rule packs to load — see [rules/authoring-guide.md](../rules/authoring-guide.md). Multiple directories are loaded and MERGED (see [Defaults](#defaults-zero-config--full-analysis) below for the collision rule). A bad/missing directory is a non-fatal `warnings` entry, not a failure — other directories in the list still load. |
| `packDefs` | `RulePackDef[]` (default `[]`) | Inline rule-pack definitions handed to the engine as data instead of a filesystem directory — the self-contained-binary alternative to `packsDir` (`zzop-mcp`'s bundled packs, embedded at compile time). Loaded BEFORE `packsDir` directories, so a directory pack with the same id wins the collision. A same-id collision among `packDefs` entries themselves: the later array entry wins whole. Also accepted on `analyzeEnvelope`'s config — `EnvelopeAnalyzeRequest` carries the same field with the identical contract. |
| `cacheDir` | `Option<String>` | See [Caching](../ARCHITECTURE.md#caching). Omit to run uncached. |
| `git` | `Option<{ since: Option<String>, recentDays: Option<u32>, commitTypePatterns: Option<Array<{ pattern: String, tag: String }>> }>` | Enables git-derived scores/health/recommendations/criticality/seams. `recentDays` default is 30. `commitTypePatterns` is an ARRAY of `{ pattern, tag }` objects (NOT a map) — e.g. `[{ "pattern": "^hotfix:", "tag": "FIX" }]` — and, when present and non-empty, REPLACES the default FIX/FEAT/REVERT/... classifier table entirely (match order = array order, mirroring the default table's REVERT-first rationale); an entry whose `pattern` fails to compile as a regex is skipped (matches nothing) and reported as a `warnings` entry, never a failure. |
| `sizeCap` | `Option<usize>` | Default 1,500,000 bytes (~1.5MB) — see [degraded files](../ARCHITECTURE.md#degraded-files). |
| `disabledRules` | `Vec<String>` | Rule/analysis ids to turn off — see [rules/catalog.md](../rules/catalog.md) for the id list. |
| `severityOverrides` | `BTreeMap<String, "critical" \| "warning" \| "info">` (default `{}`) | Per-rule severity remap, keyed by rule id (same id space as `disabledRules`). Promotes/demotes a rule's findings without editing the pack — applied post-merge, so it also re-sorts the finding into its new severity band. |
| `suppressions` | `Vec<{ rule: String, path?, glob? }>` (default `[]`) | Finding-level accept-list. Each entry drops findings for `rule` either everywhere (no filter), only in files whose path CONTAINS `path` as a plain substring (case-sensitive), or only in files matching `glob` (full-path shell glob; `glob` takes precedence over `path`). Multiple entries for one rule are OR-ed. |
| `globalExcludes` | `Vec<{ path?, glob? }>` (default `[]`) | Config-wide, rule-agnostic finding-level filter — the top-level `"exclude"` config key. Same `path`/`glob` matching as `suppressions`, but drops matching findings from EVERY rule at once (rather than one named `rule`); the file itself is still analyzed, only its findings are filtered. |
| `adapterOverlays` | `Vec<NormalizedEnvelope>` (default `[]`) | Mode-B adapter overlays: partial Normalized-AST envelopes merged ON TOP of native analysis (each re-validated, soft-skipped with a warning if invalid). How a framework/SDK adapter adds IoFacts the engine does not parse natively without reimplementing the parser — contrast `analyzeEnvelope`, where a full envelope REPLACES native analysis. Post-cache, so it does not affect the cache key. See [../NORMALIZED_AST.md](../NORMALIZED_AST.md). |
| `mountedAt` | `Option<String>` | Deployment-topology whole-tree gateway/ingress mount prefix — shorthand for a `mounts` entry with `dir: ""`, folded in LAST (after every `mounts` entry) so an explicit equal-length `mounts` entry wins a tie. `None` (default) adds no implicit mount. Applied to `kind=http` provides only, stacking on top of any code-extracted prefix. See [../ARCHITECTURE.md](../ARCHITECTURE.md#cross-layer-join). |
| `mounts` | `Vec<{ dir: String, at: String }>` (default `[]`) | Deployment-topology per-directory mounts: prepends `at` to a `kind=http` provide's key when its file path falls under `dir` (longest matching `dir` wins per provide). Shape is validated fail-fast by the CLI mapper (`ConfigError`); the engine itself defensively skips+warns on a malformed value as a backstop. |
| `hosts` | `Vec<String>` (default `[]`) | Hosts this tree owns. An absolute-URL consume from any tree targeting one of these hosts (`http`/`https` only) is re-keyed to an internal joinable key at cross-layer link time instead of falling into `externalConsumes` — see `hostRekeyCounts` below. |
| `routes` | `Vec<{ key: String, role?: "provide" \| "consume" }>` (default `[]`) | Lightweight route-fact injection — the ergonomic counterpart of `adapterOverlays` for the common "inject one route zzop could not resolve from source" case (a non-literal path, a dynamic verb, a computed URL). `key` is a `"METHOD PATH"` interface key (`"GET /api/users"`), normalized through the same transform the extractors use for that side (`http_interface_key` for a provide; the query/fragment-dropping `http_consume_interface_key` for a consume, so `"GET /articles?limit=10"` joins a native `GET /articles`); `role` picks whether the route is SERVED here (`provide`, default) or CALLED from here (`consume`). The whole array expands into ONE synthetic adapter overlay of `http` provides/consumes, so it composes through the identical cross-layer join path as a hand-authored overlay. A `key` that is not a `METHOD`+`PATH` pair is soft-skipped with a warning (never a hard error). See [../ARCHITECTURE.md](../ARCHITECTURE.md#cross-layer-join). |

### Defaults (zero-config = full analysis)

The `analyze`/`analyzeTrees` facade functions inject no defaults themselves — default-injection is
each host's own config front end's job, applied before the request ever reaches `zzop-facade`.
`zzop-mcp` does this through the shared `zzop-config` crate (`crates/config`), which embeds the bundled
rule packs at compile time (`build.rs`, `BUNDLED_PACK_SOURCES`) and injects them as inline `packDefs` —
carrying over the two defaults that make a bare `{ root }` request run the full analysis instead of
silently degrading to native-analyses-only:

- **Bundled DSL packs.** A single-binary host has no sidecar `rules/` directory to point a `packsDir`
  string at, so `zzop-config` embeds and injects the bundled packs as inline `packDefs` directly. A
  caller-supplied pack directory (config `packs.extraDirs`, or an embedder's own `packsDir`) is loaded
  alongside the bundled inline `packDefs`: a pack id present in both is taken WHOLE from the directory
  pack — a caller's pack always wins a collision against a shipped pack with the same id, while every
  distinctly-id'd pack from either source stays loaded. A bad/unreadable directory is a non-fatal
  `warnings` entry; every other directory still loads. An explicit `packsDir: null` disables
  directory-based pack loading — `null` means "no DSL packs from a directory", not "no defaults".
- `git` — when the key is absent, defaults to `git: {}` (the engine applies its own `recentDays: 30`
  default). An explicit value wins; `git: null` disables git collection. If `root` is not a git
  repository, the engine degrades gracefully with a "git collection skipped" warning.

`analyzeEnvelope`'s config gets only the pack default — envelope mode has no `root`/git — and gets it
at a single layer: the engine facade itself (`zzop_facade::analyze_envelope_json`) seeds the bundled
packs as inline `packDefs` on EVERY envelope analysis, whatever the host — the envelope path has no
per-host config front-end on the Rust side (an envelope carries no filesystem root for one to attach
to), so its "zero-config = full analysis" default lives at this shared chokepoint instead. The seed
order keeps the same collision rule as above: bundled inline defs load first, a caller `packDefs`
entry with a bundled id wins whole (later inline def wins), and any `packsDir` directory pack wins
whole over both — so a raw facade/binary caller with no explicit `packsDir` sees `packsLoaded`
`source: "inline"`. An explicit `packsDir: null` disables the bundled seed and all pack directories —
caller-supplied `packDefs` are still honored, per the standing "packDefs always load" contract — the
facade distinguishes an absent key from an explicit `null` for exactly this opt-out. Note only
`symbol-scan`/`io-scan` rules can fire in envelope mode (no source text); every current bundled rule is
`line-scan`/`method-scan`, so today the bundled default changes `packsLoaded` (and removes the spurious
zero-packs warning), not findings. To turn off individual rules rather than a whole channel, use
`disabledRules` (see [rules/catalog.md](../rules/catalog.md)).

When the engine itself runs with a narrowed scope anyway (explicit opt-out, or a non-host consumer
calling the Rust engine directly), it self-reports on `warnings` instead of staying silent:

- `git history not requested (git option omitted): scores, health, recommendations, criticality, seams and layerCoChurn are null. Pass git: {} to enable them.`
- ``no DSL rule packs loaded: only the N built-in native analyses ran. If you expected the bundled packs, reinstall/check the package (the bundled packs directory may be missing); to add your own, set `packs: { extraDirs: [...] }` in zzop.config.jsonc (embedders: `packsDir`).`` (N = the engine's actual native-analysis count.)

These are capability notes, not errors — the analysis still completes normally. The zero-packs note
can reach `analyzeEnvelope` only via the explicit `packsDir: null` opt-out now (the facade's bundled
default otherwise guarantees a non-empty pack set); the git note never does (envelope mode has no git
by design).

`EnvelopeAnalyzeRequest { sourceId: String, packsDir: Option<String | Vec<String>> (absent ≠ null), packDefs: Vec<RulePackDef>, disabledRules: Vec<String>, severityOverrides: BTreeMap<String, Severity>, suppressions: Vec<{ rule, path?, glob? }>, globalExcludes: Vec<{ path?, glob? }>, mountedAt: Option<String>, mounts: Vec<{ dir, at }> }` —
deliberately no `root`/`cacheDir`/`git`/`sizeCap` (envelope mode has no filesystem root or git repo).
`packDefs`/`severityOverrides`/`suppressions`/`globalExcludes`/`mountedAt`/`mounts` behave identically
to their `AnalyzeRequest` counterparts above (for `packDefs` that includes the seed order: inline defs
load BEFORE `packsDir` directories, so a directory pack with the same id wins the collision whole; for
`mountedAt`/`mounts` that includes the fold order — every `mounts[]` entry first, `mountedAt` as the
implicit whole-tree `dir: ""` entry last — with the engine applying them uniformly to Mode A envelopes,
per `../NORMALIZED_AST.md`'s deployment-topology note).
Unlike `AnalyzeRequest`, `packsDir` here distinguishes an ABSENT key from an explicit `null`: absent
(or a directory value) keeps the facade's bundled-pack default (see [Defaults](#defaults-zero-config--full-analysis)
above); `null` opts out of the bundled seed and all pack directories (caller `packDefs` are still
honored). `NormalizedEnvelope` shape: see `../NORMALIZED_AST.md`.

### Validation-only: `validateEnvelopeOnly`

`validateEnvelopeOnly(envelopeJson)` runs the same structural/semantic checks `analyzeEnvelope` applies
to its envelope argument (`zzop_core::validate_envelope`) but stops there — no `configJson`, no pack
loading, no engine run — so an external adapter author gets fast, offline "is my envelope well-formed"
feedback without a full analysis. It returns `{"valid": boolean, "issues": string[]}` and, unlike every
other function on this page, **never fails**: an unparseable or semantically invalid envelope still
produces an ordinary `{"valid": false, "issues": [...]}` result rather than a rejected `Result`/thrown
`Error` — a validity check cannot itself be "wrong" the way a malformed request can.

### Validation-only: `validateRulePackOnly`

`validateRulePackOnly(packJson)` is the same idea for a DSL rule pack: the pre-load, structure-only
check behind the `validate_rule_pack` tool and `zzop validate-rule-pack <file>` CLI subcommand (one
shared facade core, `zzop_facade::validate_rule_pack_json` — identical answers from every host). Its
`issues` surface exactly the judgments the engine's pack loader makes when it loads a
`packsDir`/`packDefs` pack — bad JSON, a missing field, a wrong type (serde's own messages, verbatim),
a too-new `schema_version` — plus every matcher regex that fails to compile, which the DSL interpreter
otherwise reports by silently never firing that rule. It never judges rule QUALITY or semantics: a
structurally sound pack with a useless rule is `valid: true`. Same `{"valid": boolean, "issues":
string[]}` shape and never-fails contract as `validateEnvelopeOnly` above. The machine-readable shape
contract ships as [`docs/contracts/rule-pack.schema.json`](../contracts/rule-pack.schema.json)
(`zzop://contract/rule-pack-schema` over MCP); the human-readable field reference is
[rules/dsl-reference.md](../rules/dsl-reference.md).

### Endpoint queries: `queryIo`

`queryIo(analysisJson, queryJson)` answers "is io key X provided/consumed/joined?" DEFINITIVELY —
pure post-processing over an ALREADY-PRODUCED `analyzeTrees` output (no re-analysis, no cache
interaction). It is the one shared query core: the `check_endpoint`
tool and `zzop endpoint` CLI subcommand (above) both call this exact function, so
every host driving it gives identical answers for the same analysis.

- `analysisJson` — the string `analyzeTrees` returned. A single-tree `analyze` output is a guided
  error: it carries raw io facts (`ir.io`) but no cross-layer join, and every verdict below is a
  join fact — run `analyzeTrees` instead (the join runs even over one tree, intra-tree edges
  included; the error reports how many raw provides/consumes matched so the guidance is concrete).
- `queryJson` — `{"pattern": "<non-empty string>"}`. The pattern is matched as a case-insensitive
  substring against every cross-layer io key (http routes, env keys, DB tables, topics — every
  bucket plus `edges`), and against the `raw` expression of an unresolved consume (`key: null`);
  an unresolved consume with no `raw` recorded is unmatched, never guessed. An unknown query key
  (a typo like `"patern"`) is a named error, not a silent `not-found`.

The result (camelCase):

| Field | Meaning |
|---|---|
| `pattern` | Echo of the query pattern. |
| `verdict` | ONE token from the sealed vocabulary below. |
| `counts` | FULL match counts per bucket (`{edges, unconsumedProvides, unprovidedConsumes, unresolvedConsumes, externalConsumes, ambiguousConsumes}`) — never capped. |
| `matches` | The same six keys, each an array of the ORIGINAL matched objects (`file`/`line`/`source` intact), capped at 20 per bucket. |
| `truncated` | `{bucket: remainingCount}` — present only when a bucket's `matches` list was capped. |
| `relatedFindings` | Findings (from every tree's `findings` AND `crossLayerFindings`) whose message contains the pattern or any matched key, case-insensitively — capped at 20, with a sibling `truncatedFindings: N` only when capped. |
| `suggestions` | Up to 10 candidate keys, present ONLY on a `not-found` verdict: keys whose last path segment equals the pattern's (case-insensitively), falling back to keys containing any single `/`-segment of the pattern. |
| `disclosure` | Forwarded verbatim from the analysis output (the run-global registry below). |

`verdict` is a **sealed wire vocabulary** (`crates/facade/src/query.rs`), derived deterministically
from which join buckets contain a match: `edges` → `"linked"`, `unconsumedProvides` →
`"provided-only"`, `unprovidedConsumes` → `"consumed-unprovided"`, `unresolvedConsumes` →
`"unresolved-only"`, `externalConsumes` → `"external"`, `ambiguousConsumes` → `"ambiguous"`.
Exactly one class matching yields its token; two or more yield `"mixed"` (the `counts`
disambiguate); zero yield `"not-found"`.

`AnalyzeOutputView` (`camelCase`, a zero-copy borrowing view) is the shape every successful `analyze`/
`analyzeEnvelope` call returns:

| Field | Type | Meaning |
|---|---|---|
| `ir` | `CommonIr` | The language-neutral IR — see [Output data shapes](#output-data-shapes) below. |
| `findings` | `Finding[]` (merged, sorted) | See [Output data shapes](#output-data-shapes) for the `Finding` shape and sort order. |
| `degraded` | `string[]` (sorted) | Paths that hit the size cap or failed to parse — see [ARCHITECTURE.md](../ARCHITECTURE.md#degraded-files). |
| `fileCount` | `number` | Files walked. |
| `nodes` | `FileNode[]` | Per-file git/graph metrics (churn, fan-in/out, risk score, ...) — populated fully only when `git` is set. `riskScore`/`hotspotScore` are always `0` for non-source files (data/config/assets — anything outside the "Language support" table in [ARCHITECTURE.md](../ARCHITECTURE.md#language-support)); `churn`/`loc`/`changeCount` stay real for them, so a large data file's edit history is still visible without it dominating a risk-sorted view. |
| `scores` | `object \| null` | 17 structural health sub-scores, 0–100; `null` unless `git` is set. |
| `health` | `object \| null` | One composite index rolled up from `scores`. |
| `recommendations` | `object[]` | ROI-ranked improvement suggestions. An item whose file carries a rule-confirmed critical finding is moved (never copied) into a synthetic `urgent-bug-risk` group that sorts first, and gains a `bugEvidence: string[]` explaining why — this never changes the item's `roi` number, which always stays a pure reduction/cost estimate. |
| `critical` | `object[]` | Files ranked by blast-radius (transitive dependents). |
| `seams` | `object[]` | Folders that are good first-extraction candidates (low boundary-crossing coupling). |
| `folders` | `object \| null` | Folder-granularity rollup of `nodes`/the dep graph. Not git-gated — `nodes`/dep graph are built unconditionally, so this is always non-null (an empty tree still gets an object with empty arrays, never `null`). |
| `layerCoChurn` | `object[] \| null` | Cross-layer commit co-churn pairs (files in different architectural layers that change together). `null` unless `git` is set and collection succeeded — same git-gating as `scores`/`health`; `[]` (not `null`) when git is active but no pair meets the co-change threshold. |
| `gitWindow` | `{ recentDays: number, since: string \| null } \| null` | Echoes the resolved git-history collection window — ALWAYS serialized (unlike `ruleOverridesApplied`'s omit-when-untouched convention); `null` on the wire IS the "git didn't run" signal (`git` not set, or collection failed), same gating as `scores`/`health`. When non-null: `recentDays` is always a resolved number (the caller's value, or the engine's `30` default when omitted); `since` is the caller's raw filter string (e.g. `"1.year"`, an ISO date) verbatim, or `null` when omitted (full history). |
| `packsLoaded` | `{ id, rules, source, filesInScope }[]` | Positive pack-load confirmation: one entry per loaded DSL pack (sorted by `id`), with its rule count as loaded and its provenance — `source` is `"dir"` (read from a `packsDir` directory) or `"inline"` (`packDefs` — how `zzop-config`'s bundled defaults arrive for `zzop-mcp`). `filesInScope` counts the files this tree has that a pack's rules WOULD scan by path-pattern candidacy alone (`file_pattern`/`file_exclude_pattern` — see [rules/dsl-reference.md](../rules/dsl-reference.md)), computed before any content/pattern check runs — it is never a "matched" or "found N usages" count. A large `filesInScope` (e.g. every `.java` file in an all-Java tree) means "eligible", nothing more; pair it with zero findings to read "this pack ran, found no evidence" (`filesInScope > 0`, zero findings) versus "this pack has nothing to say about this tree" (`filesInScope: 0`, e.g. a redis pack over a tree with no redis-shaped file paths at all). Always present; `[]` is the honest "zero DSL packs loaded" state (the same condition the `warnings` self-report names). Reflects loading, not gating: a pack disabled via `disabledRules` still appears — it did load. |
| `ruleOverridesApplied` | `{ disabled: string[], severityRemapped: string[] }` | Positive confirmation that `disabledRules`/`severityOverrides` were applied: `disabled` lists the affected rule ids, `severityRemapped` likewise for the severity remap. Omitted (or empty) when neither override was requested — a consumer must treat an absent key the same as "no overrides," never as `null`. |
| `warnings` | `string[]` | Non-fatal issues (e.g. a bad `packsDir`) plus the capability self-report notes — see [Defaults](#defaults-zero-config--full-analysis). |
| `configWarnings` | `string[]` | Config-authoring problems computed at analysis time, kept OUT of `warnings`: a `disabledRules`/`severityOverrides` entry matching no known rule id (a typo, or a stale id from a different zzop version) did nothing, and is reported here instead — only analysis time has the known-rule-id set (native analysis ids + loaded DSL pack ids) a config parser never sees. Always present; `[]` means neither knob had a matching-nothing entry. A `suppressions` entry with the same problem is unaffected by this split and still reports on `warnings`. This host's own `zzop-config` crate (see [Config semantics](#config-semantics) above) attaches ITS OWN parse-time config problems (unknown config keys, a malformed overlay) to the same `configWarnings` name on its own reply; this facade-level field is the analysis-time half of that one channel, never a rename of `warnings`. |
| `cache` | `{ hits, misses } \| null` | Set only when `cacheDir` was given. |
| `ruleTimings` | `object[] \| null` | Per-rule id + elapsed time + finding count; set only when the caller requests profiling. |
| `coverage` | `object` | Per-tree coverage census — always present. See below. |

`coverage` fields (all plain counts over this tree, always present — a `0` means "counted and found
none", not "not run"):

| Field | Type | Meaning |
|---|---|---|
| `files` | `number` | Files walked (same as `fileCount`). |
| `symbols` | `number` | `SourceSymbol` entries extracted (`ir.symbols[]` length). |
| `importEdges` | `number` | Resolved import-graph edges — sum of `ir.dep` out-degrees (edge count, not source-file count). |
| `ioProvides` | `number` | `ir.io.provides` entries. |
| `ioConsumesKeyed` | `number` | `ir.io.consumes` entries whose key resolved statically. |
| `ioConsumesUnresolved` | `number` | `ir.io.consumes` entries whose key could not be statically determined. |
| `degraded` | `number` | Same count as `degraded.length`. |
| `joinContributionZero` | `boolean` | `true` when this tree analyzed files>0 but extracted zero JOINABLE io (0 `ioProvides` and 0 keyed consumes — unresolved consumes don't count, they cannot join) — the active-blindness fact: this tree is structurally invisible to `analyzeTrees`'s cross-layer join, so any join finding referencing it (`unconsumedProvides`/`unprovidedConsumes`/edges) is not meaningful for it. A framework/SDK client the extractor cannot see is a common cause; see `adapterOverlays` above (Mode B) to restore visibility. |

### `disclosure` — silent-failure-class registry (run-global)

`analyze`, `analyzeEnvelope` and `analyzeTrees` all emit a top-level `disclosure` array: zzop's pinned,
honest list of the ways its own output can be silently misread. It is **run-global** (identical every
run, emitted once — on the multi-tree output it sits beside `trees`/`crossLayer`, never repeated per
tree) and static, so a consumer learns not just what zzop found but which *classes* of blindness zzop
does and does not yet actively detect. Each entry:

| Field | Type | Meaning |
|---|---|---|
| `id` | `string` | Stable kebab-case class id (part of the contract). |
| `group` | `string` | Taxonomy group: `extraction-blind` \| `analysis-dark` \| `input-config` \| `trust-calibration`. |
| `summary` | `string` | The concrete way an agent could misread zzop's output for this class (phrased as the misreading). |
| `status` | `string` | `asserted` (surfaced from a structural fact every run — cannot be silently missed) \| `partial` (detected in common cases, a member can still slip past) \| `notYetDetected` (a real class zzop does **not** yet detect — declared so you do not assume coverage). |

The whole JSON tree is camelCase — every nested type (`Finding`, `FileNode`, `Scores` and its ~30
sub-structs, `HealthIndex`, `Recommendation`, `CriticalFile`, `SeamCandidate`, `FolderAggregates`,
`CrossLayerCoChurn`, `CrossLayerResult`, `RuleTiming`, `IoFacts`/`IoProvide`/`IoConsume`, and now also
`SourceSymbol`, `ir.symbols[]`'s entry type) carries its own `#[serde(rename_all = "camelCase")]`, not
just this top-level view — so e.g. a `Finding`'s rule id key is `ruleId`, not `rule_id`, and a
`SourceSymbol`'s are `isDefault`/`bodyStart`/`bodyEnd`, not `is_default`/`body_start`/`body_end`. One
deliberate exception remains:
- `Finding.data` is opaque, rule-authored JSON with no uniform casing rule — see the "Every finding..."
  table below.

`SourceSymbol` still *accepts* the old snake_case names (`is_default`, `body_start`, `body_end`) on the
way IN, via `#[serde(alias = ...)]` — it doubles as the deserialize target for
`docs/NORMALIZED_AST.md`'s frozen v1 external-parser envelope input contract
(`FileProjection.symbols`), and zzop only ever receives an envelope, never emits one, so widening the
accepted input names costs nothing. See [Output data shapes](#output-data-shapes) below.

`MultiAnalyzeOutputView` (from `analyzeTrees`) wraps `{ trees: [{ root, sourceId, output }],
crossLayer: CrossLayerResult, crossLayerFindings: Finding[] }`, where `crossLayer` carries the cross-tree IO
join result across six buckets (camelCase like everything else), plus a per-edge confidence flag:
- `edges` — a consume matched to a provide across sources.
- `unconsumedProvides` — a provide no analyzed source consumes.
- `unprovidedConsumes` — a consume no analyzed source provides.
- `unresolvedConsumes` — a consume whose URL/key could not be statically determined.
- `externalConsumes` — a consume targeting an absolute external host URL (e.g.
  `GET https://vendor.com/api/users`): third-party egress, not joined, not treated as drift.
- `ambiguousConsumes` — a consume matching provides in 2+ distinct source trees: not
  auto-linked (no edge emitted), every candidate provider listed so the ambiguity can be resolved by hand.
- `edges[].lowConfidenceReason` (string, omitted when not set) — the edge's key matched a generic-path
  pattern (health checks, `/login`, etc.) that many unrelated services could share, so the match is lower
  confidence than a distinctively-named route; the edge is still emitted.

`crossLayer` also carries `hostRekeyCounts`, an additional field present only when at least one tree in
the request declares topology `hosts` — one `[host, rekeyedConsumeCount]` pair (a plain 2-element JSON
array of `[string, number]`, since it serializes a Rust `Vec<(String, usize)>`) per distinct declared
host, in declaration order. `rekeyedConsumeCount` is how many absolute-URL consumes targeting that host
were re-keyed to internal and joined via the normal `edges`/`ambiguousConsumes`/`unprovidedConsumes` path
instead of falling into `externalConsumes`; a count of `0` means the declared host is stale or every
consumer used a relative path. The field is omitted entirely (not an empty array) when no tree declares
any hosts.

`crossLayerFindings` is the output of the `cross-layer/*` native rules run over `crossLayer` (see the
"Native analyses" table in [docs/rules/catalog.md](../rules/catalog.md) for the full id list) — sorted the
same `(severity, file, line, ruleId)` way as every per-tree `findings` array, and gated by the UNION of
every tree's `disabledRules` (any one tree disabling a cross-layer rule id drops it from this array
entirely, since it is a joint-analysis output no single tree fully owns).

`version()` returns
`"zzop/{version} zzop-parser-typescript={FP} zzop-parser-prisma={FP} zzop-parser-python-3={FP} zzop-parser-java-21={FP} zzop-parser-rust={FP} zzop-parser-go={FP} zzop-parser-sql={FP} zzop-parser-csharp={FP}"`
— every native parser's `PARSER_FINGERPRINT`, in that order. `{version}` is `CARGO_PKG_VERSION` — the
workspace `[workspace.package] version`, the release SSOT since the 2026-07-22 version reform — the
same value `zzop-mcp`'s `serverInfo.version` reports (see [MCP surface](#mcp-surface) above); CI's
release job verifies the release tag matches it, so a released build's reported version equals its tag
by construction.

### Output data shapes

The `ir` field is the Common IR every file gets projected into — language-neutral, and the same shape
an external parser adapter must produce (see [NORMALIZED_AST.md](../NORMALIZED_AST.md)):

| Type | Fields | Notes |
|---|---|---|
| `CommonIr` | `source`, `parser: string`, plus the fields below (flattened) | `parser` = producing adapter id (`"typescript"`, `"prisma"`, ...). |
| — `dep` | `{ [path]: string[] }` | Import graph: path → imported paths. |
| — `symbols` | `SourceSymbol[]` | See below. |
| — `loc` | `{ [path]: number }` | Physical line count per file. |
| — `io` | `IoFacts \| null` | `provides`/`consumes` HTTP/DB/tRPC facts, joined cross-tree by `analyzeTrees`. |
| `SourceSymbol` | `id, file, name, kind, line, exported, isDefault, bodyStart?, bodyEnd?, writeSites?` | `kind` is one of `function\|class\|const\|type\|interface`; `bodyStart`/`bodyEnd` (1-based, inclusive) are set only for functions/classes with a recoverable body span; `writeSites` (skipped when empty; camelCase-only, no snake_case alias — see [NORMALIZED_AST.md](../NORMALIZED_AST.md)) lists pre-computed store-write call sites within the symbol's body span (TS only; feeds the `unsafe-read-endpoint`/`non-idempotent-write` call-graph scanners). camelCase on output like every other type here. On the way IN, `SourceSymbol` is also reused verbatim as the deserialize target for [NORMALIZED_AST.md](../NORMALIZED_AST.md)'s frozen v1 external-parser envelope input contract (`FileProjection.symbols`), so it additionally *accepts* that contract's snake_case names (`is_default`, `body_start`, `body_end`) via `#[serde(alias = ...)]` — a conforming envelope producer's JSON keeps working unchanged. |

Every finding — from a DSL rule pack or a native analysis alike — has this shape:

| Field | Value |
|---|---|
| `ruleId` | `"{pack}/{rule}"` for a DSL rule (e.g. `"sql/nplus1"`), or a plain id for a native analysis (e.g. `"circular"`) — see [rules/catalog.md](../rules/catalog.md) for the full id list. |
| `severity` | `"critical" \| "warning" \| "info"` — the rule's default severity (see [rules/catalog.md](../rules/catalog.md)). |
| `file` | The finding's file, relative to `root`. |
| `line` | 1-based line number. |
| `message` | Human-facing cause/fix-hint, copied verbatim from the rule definition. |
| `data` | Matcher-specific JSON payload (e.g. `{snippet, label}` for a line-scan hit) — opaque, rule-specific; DSL packs author their own keys ad hoc (mostly camelCase already, e.g. `handlerSymbol`), so no uniform casing rule applies inside `data` itself. |

`findings` is sorted by `(severity, file, line, ruleId)` ascending (critical first). A finding
suppressed by an inline `// <marker>-ok` comment (see [rules/dsl-reference.md](../rules/dsl-reference.md#suppress-marker-semantics))
is dropped before sorting — it never appears in the output at all, with no suppressed flag.

### Error/panic discipline

`zzop-facade` (`crates/facade/src/lib.rs`) never panics by contract — every fallible path (bad JSON,
missing `root`, invalid envelope, a malformed query) returns `Result<String, String>`. The engine
itself already isolates a single bad file's parse/rule failure internally (see [degraded
files](../ARCHITECTURE.md#degraded-files)), so any caller — `zzop-mcp` or a direct `zzop-facade`
embedder — gets either a value or a `Result::Err`, never a process crash, with no extra
unwind-catching wrapper needed: an in-process Rust call has no FFI boundary to protect. `version` has
no `Result` (cannot fail).
