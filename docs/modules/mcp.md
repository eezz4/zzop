# `zzop-mcp`

The Node-free host: **two** self-contained binaries over one shared library, running the zzop analysis
engine with no Node.js runtime at all. [`napi.md`](napi.md) documents the same `zzop-facade` JSON contract
they drive — historically exposed to Node via the `@zzop/native` binding, part of the npm distribution
removed 2026-07-20. These binaries are the Node-free side of that same contract — they call
`analyze_json`/`analyze_envelope_json`/`analyze_trees_json`/`validate_envelope_only_json`/`validate_rule_pack_json`/`query_io_json`
(`crates/facade/src/lib.rs`) directly, with no napi and no JS in between:

- **`zzop-mcp`** — the **MCP server** over stdio (bare `zzop-mcp`, or the `zzop-mcp mcp` form `.mcp.json`
  and the MCPB manifest register). For MCP clients.
- **`zzop`** — the **CLI** (`zzop analyze <path>` / `zzop analyze-envelope <envelope.json>` / `zzop cross
  <path>...` / `zzop endpoint <pattern> <path>...` / `zzop contract` / `zzop validate-…`). For direct
  terminal/CI use, no MCP client required.

Both are thin `src/bin/` arg-dispatch shims over the same `zzop_mcp` lib and share the exact same handlers
(`packages/mcp/src/tools.rs`), so a CLI run and an MCP `tools/call` against the same path produce the same
analysis through the same code.

## Module map

| Module | Responsibility |
|---|---|
| `bin/zzop.rs` | The `zzop` CLI entry — thin argument dispatch: `analyze` / `cross` / `endpoint` / `contract` / `validate-*` / `version` subcommands over the library below. |
| `bin/zzop-mcp.rs` | The `zzop-mcp` server entry — thin: bare / `mcp` serve stdio; `version` / `help` / unknown-arg lanes. |
| `server.rs` | The stdio JSON-RPC 2.0 loop (`initialize`, `tools/*`, `resources/*`). |
| `tools.rs` | Pure dispatch: match tool name → extract arguments → call the shared `zzop-summary` function → wrap the MCP result. No shaping logic lives here. |
| `tools/definitions.rs` | MCP tool descriptions + input schemas (`tools/list`). |
| `resources.rs` | MCP resource handlers (`resources/list`, `resources/read`) over the embedded authoring contracts. |
| `embedded.rs` | The embedded contract documents themselves — compiled into the binary via `include_str!`. |

Everything functional — output shaping (full counts, capped lists, explicit truncation disclosure;
see [Output contract](#output-contract) below), finding filters, bucket keys/sites, typo
suggestions, the architecture summary, config-warnings merging, tree resolution, sibling-scope
disclosure, path absolutization — is **not** in this crate: it lives in the shared `zzop-summary`
crate (`crates/summary`), whose crate doc states the rule: hosts are thin protocol facades, ALL
summary logic is shared so it cannot drift per-host (the surface-parity contract at
[contracts/surface-parity.json](../contracts/surface-parity.json) machine-checks the complement:
every engine output field is either carried or documented-omitted per surface). The config
front-end (`zzop.config.jsonc` discovery, JSONC parsing, config→request mapping, `trees: "auto"`
workspace expansion) likewise lives in the shared `zzop-config` crate (`crates/config`).
`zzop-mcp`'s own `Cargo.toml` depends on `zzop-summary` (which re-exports the facade call path) and
`zzop-config` (embedded config-surface bytes) and nothing else beyond `serde_json`.

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

`initialize` replies with `serverInfo: { name: "zzop", version }` — release binaries report the release
version (stamped at build time from the release tag — `ZZOP_RELEASE_VERSION` in `prebuild.yml` — so it
matches the tag and the plugin's `plugin.json` version); in-tree dev builds report the `0.0.0` workspace
placeholder.

### Tools (`tools/list` / `tools/call`)

| Tool | Purpose |
|---|---|
| `analyze_repo` | Analyze ONE repo/tree path. |
| `cross_repo` | Analyze 2+ repos/trees and join them across the cross-layer (kind, key) boundary — zzop's headline capability (e.g. a frontend `fetch` call matched against a backend route, a shared DB table, route drift). |
| `check_endpoint` | DEFINITIVE answer to "is io key X provided/consumed/joined?" — matches a pattern against ANY cross-layer io key (http routes, env keys, DB tables, topics) as a case-insensitive substring and returns ONE verdict from the sealed vocabulary `linked` / `provided-only` / `consumed-unprovided` / `external` / `unresolved-only` / `ambiguous` / `mixed` / `not-found`, plus full counts, capped match lists, related findings, and key suggestions on `not-found`. Runs the shared facade query core directly — the same core any embedder driving `zzop-facade`/`zzop-summary` gets identical answers from (see [napi.md](napi.md#endpoint-queries-queryio) for the full output contract). |
| `analyze_envelope` | **Mode A**: a full Normalized AST envelope (a custom parser's output) REPLACES native parsing entirely for this run — contrast `validate_envelope` below, which only checks the envelope's shape and runs no analysis, and Mode B overlay/mount requests (`docs/NORMALIZED_AST.md`), which merge external symbols ON TOP of a natively-parsed tree instead of replacing it. Only symbol-scan/io-scan rules can fire (no source text ships in an envelope). Zero-config only — an envelope carries no filesystem location, so the reply has no `config`/`path`/`architecture`/`gitWindow` fields; otherwise the SAME shaped summary `analyze_repo` returns (findings, `packsLoaded`, `coverage`, warnings). Same `analyzeEnvelope` facade call path documented in [napi.md](napi.md#defaults-zero-config--full-analysis). |
| `validate_envelope` | Validate a Normalized AST envelope against the v1 contract WITHOUT running an analysis — the authoring feedback loop. Returns `{valid, issues[]}`; never fails on bad input (same contract as the facade's `validateEnvelopeOnly` — see [napi.md](napi.md#validation-only-validateenvelopeonly)). |
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
truncatedFindings?, suggestions?, disclosure}` — see [napi.md](napi.md#endpoint-queries-queryio) for
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
| `rule-catalog` | Every rule id the engine ships today — 14 DSL packs + all native analysis ids, with severity/matcher/suppress-marker/detection prose per rule (`docs/rules/catalog.md`) — the discoverability gap closed: `packsLoaded` gives counts only, and the `dsl-reference` resource pointed at this file without it ever being served over MCP. Pair with the `rule` tool argument on `analyze_repo`/`cross_repo`/`check_endpoint` (an id absent from this catalog never fires). |

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
  [napi.md](napi.md#defaults-zero-config--full-analysis)'s note on this field) and default `git: {}`
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
  id-sorted; see [napi.md](napi.md)'s `AnalyzeOutputView` table) rides through whole on every
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
  engine itself omits it (no `disabledRules`/`severityOverrides` requested) — see
  [napi.md](napi.md)'s `AnalyzeOutputView` table for the field shape.
- **`coverage`** — the engine's per-tree structural coverage census (`files`, `symbols`, `importEdges`,
  `ioProvides`, `ioConsumesKeyed`, `ioConsumesUnresolved`, `degraded`, `joinContributionZero` — see
  [napi.md](napi.md)'s `AnalyzeOutputView` table for field semantics) rides through whole on every
  `analyze_repo` reply and per-source on `cross_repo`'s `sources[]` entries — a handful of scalars, no
  cap needed. `joinContributionZero` is the engine's own blindness ASSERTION (this tree extracted no
  JOINABLE io — 0 provides and 0 keyed consumes, unresolved consumes don't count — while analyzing
  `files > 0`, so it is invisible to the cross-layer join) and must reach the summary
  reader — a "0 findings" tree that contributed nothing to the join is not a clean tree.
- **`disclosure`** — the engine's run-global, pinned silent-failure-class registry (identical every run;
  see [napi.md](napi.md#disclosure--silent-failure-class-registry-run-global) for the full field
  contract) rides through unfiltered on every `analyze_repo`/`cross_repo` reply, the same meta-honesty
  channel the shared facade exposes to every host.
- **`architecture`** (`analyze_repo` only) — a compact, capped summary closing a disclosure asymmetry:
  the facade output carries full `health`/`recommendations`/`critical` (git-history-dependent
  structural-debt metrics — see [napi.md](napi.md)'s `AnalyzeOutputView` table), but this host's
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

`zzop-mcp` has no napi dependency and no MSVC/toolchain requirement — it builds under the workspace's
default toolchain on every platform:

```sh
cargo build -p zzop-mcp --release
```

The binary lands at `target/release/zzop-mcp` (`target/release/zzop-mcp.exe` on Windows); drop `--release`
for a debug build at `target/debug/zzop-mcp` during local iteration. `cargo test -p zzop-mcp` runs the
crate's own protocol/dispatch tests (`resources.rs`, `server.rs`, and the end-to-end tool tests in
`tools/tests.rs`); the shaping logic it dispatches to is tested under `cargo test -p zzop-summary`
(that crate holds `output/`, filters, and the analyze/cross/endpoint summary assembly since the
facade-thinning split — see the module map above).

## Distribution status

Prebuilt, per-platform `zzop-mcp` binaries are attached to every tagged [GitHub
Release](https://github.com/eezz4/zzop/releases) (`prebuild.yml`'s tag-triggered build — see
[napi.md](napi.md#packaging-layout)): named `zzop-mcp-<platform>[.exe]`
for the 5 platforms `win32-x64-msvc`, `darwin-x64`, `darwin-arm64`, `linux-x64-gnu`, `linux-arm64-gnu` —
a self-contained static binary per platform, no Node required. Download the asset for your platform, rename/link
it to `zzop-mcp` (`zzop-mcp.exe` on Windows), and point `.mcp.json` at it (see
[packages/mcp/README.md](../../packages/mcp/README.md) for the worked example, including the Claude Code
plugin install path). Building from a source checkout with the command above remains fully supported as
an alternative to downloading a release asset.

See also: [napi.md](napi.md) (the `zzop-facade` request/response contract this host drives directly —
that page's own filename note explains the historical Node binding it once documented),
[../NORMALIZED_AST.md](../NORMALIZED_AST.md) (the
envelope contract behind `validate_envelope`/`envelope-guide`), [../adapters/README.md](../adapters/README.md)
(adapter authoring, mirrored by the `adapter-guide` resource).
