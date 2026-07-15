# `zzop-napi`

The Node.js binding surface over the zzop analysis engine. Five functions, all JSON-string-in /
JSON-string-out (except `version`), are actually DEFINED in the shared `zzop-facade` crate
(`crates/facade/src/lib.rs`) — plain, napi-free Rust that compiles and has a normal `#[test]` surface
under the workspace's default `gnu` toolchain with no feature flags at all. This crate, `zzop-napi`
(published as the npm binding `@zzop/native` from `packages/native/`), is one of two consumers:
`src/lib.rs` re-exports every `zzop-facade` function unchanged, and `src/addon.rs` — compiled only
under the default-off `addon` feature (MSVC-only; see [Building the addon from source](#building-the-addon-from-source)
below) — wraps each one in a thin `#[napi]` shim. The other consumer is the Node-free `zzop-mcp`
binary (`packages/mcp/`, see [modules/mcp.md](mcp.md)), which calls the same `zzop-facade` functions
directly with no napi and no Node process at all. `zzop-facade` lives in its own `rlib`-only crate,
separate from `zzop-napi`, because cargo builds a dependency's `cdylib` target even on an `rlib`
dependency edge — `zzop-napi`'s `cdylib` half (the Node addon artifact) fails to link under the local
`gnu` toolchain once its `#[napi]` surface is compiled in, and that failure would poison any crate that
merely depended on `zzop-napi` for its plain-Rust logic, `zzop-mcp` included.

## Functions

| JS name | Rust signature | Request → Response |
|---|---|---|
| `analyze` | `(configJson: string) -> string` | `AnalyzeRequest` → `AnalyzeOutputView` |
| `analyzeTrees` | `(configJson: string) -> string` | `AnalyzeTreesRequest{trees: [AnalyzeRequest]}` → `MultiAnalyzeOutputView` |
| `analyzeEnvelope` | `(envelopeJson: string, configJson: string) -> string` | `NormalizedEnvelope` + `EnvelopeAnalyzeRequest` → `AnalyzeOutputView` |
| `validateEnvelopeOnly` | `(envelopeJson: string) -> string` | envelope JSON → `{valid: boolean, issues: string[]}` — see [below](#validation-only-validateenvelopeonly). |
| `version` | `() -> string` | none (cannot fail, no `Result`) |

`AnalyzeRequest` (`#[serde(rename_all="camelCase", default)]`, unknown fields ignored):

| Field | Type | Notes |
|---|---|---|
| `root` | `String` (required — empty → `Err`) | Tree root to walk. |
| `sourceId` | `String` (default `""`) | Free-form label carried through into cross-tree output. |
| `packsDir` | `Option<String \| String[]>` | Directory (or directories) of `*.json` DSL rule packs to load — see [rules/authoring-guide.md](../rules/authoring-guide.md). Multiple directories are loaded and MERGED (see [Defaults](#defaults-zero-config--full-analysis) below for the collision rule). A bad/missing directory is a non-fatal `warnings` entry, not a failure — other directories in the list still load. |
| `packDefs` | `RulePackDef[]` (default `[]`) | Inline rule-pack definitions handed to the engine as data instead of a filesystem directory — the self-contained-binary alternative to `packsDir` (e.g. `zzop-mcp`'s bundled packs, embedded at compile time). Loaded BEFORE `packsDir` directories, so a directory pack with the same id wins the collision (mirrors the JS wrapper's bundled-first `packsDir` ordering below). A same-id collision among `packDefs` entries themselves: the later array entry wins whole. The JS CLI/wrapper never sends this field — it stays on the `packsDir`-only path unchanged; this is purely additive for non-JS hosts like `zzop-mcp` (see [modules/mcp.md](mcp.md)). Not available on `analyzeEnvelope`'s config (`EnvelopeAnalyzeRequest` has no equivalent field — envelope mode takes packs via `packsDir` only). |
| `cacheDir` | `Option<String>` | See [Caching](../ARCHITECTURE.md#caching). Omit to run uncached. |
| `git` | `Option<{ since: Option<String>, recentDays: Option<u32>, commitTypePatterns: Option<Array<{ pattern: String, tag: String }>> }>` | Enables git-derived scores/health/recommendations/criticality/seams. `recentDays` default is 30. `commitTypePatterns`, when present and non-empty, REPLACES the default FIX/FEAT/REVERT/... classifier table entirely (match order = array order, mirroring the default table's REVERT-first rationale); an entry whose `pattern` fails to compile as a regex is skipped (matches nothing) and reported as a `warnings` entry, never a failure. |
| `sizeCap` | `Option<usize>` | Default 1,500,000 bytes (~1.5MB) — see [degraded files](../ARCHITECTURE.md#degraded-files). |
| `disabledRules` | `Vec<String>` | Rule/analysis ids to turn off — see [rules/catalog.md](../rules/catalog.md) for the id list. |
| `severityOverrides` | `BTreeMap<String, "critical" \| "warning" \| "info">` (default `{}`) | Per-rule severity remap, keyed by rule id (same id space as `disabledRules`). Promotes/demotes a rule's findings without editing the pack — applied post-merge, so it also re-sorts the finding into its new severity band. |
| `suppressions` | `Vec<{ rule: String, path?, glob? }>` (default `[]`) | Finding-level accept-list. Each entry drops findings for `rule` either everywhere (no filter), only in files whose path CONTAINS `path` as a plain substring (case-sensitive), or only in files matching `glob` (full-path shell glob; `glob` takes precedence over `path`). Multiple entries for one rule are OR-ed. |
| `globalExcludes` | `Vec<{ path?, glob? }>` (default `[]`) | Config-wide, rule-agnostic finding-level filter — the top-level `"exclude"` config key. Same `path`/`glob` matching as `suppressions`, but drops matching findings from EVERY rule at once (rather than one named `rule`); the file itself is still analyzed, only its findings are filtered. |
| `adapterOverlays` | `Vec<NormalizedEnvelope>` (default `[]`) | Mode-B adapter overlays: partial Normalized-AST envelopes merged ON TOP of native analysis (each re-validated, soft-skipped with a warning if invalid). How a framework/SDK adapter adds IoFacts the engine does not parse natively without reimplementing the parser — contrast `analyzeEnvelope`, where a full envelope REPLACES native analysis. Post-cache, so it does not affect the cache key. See [../NORMALIZED_AST.md](../NORMALIZED_AST.md). |
| `mountedAt` | `Option<String>` | Deployment-topology whole-tree gateway/ingress mount prefix — shorthand for a `mounts` entry with `dir: ""`, folded in LAST (after every `mounts` entry) so an explicit equal-length `mounts` entry wins a tie. `None` (default) adds no implicit mount. Applied to `kind=http` provides only, stacking on top of any code-extracted prefix. See [../ARCHITECTURE.md](../ARCHITECTURE.md#cross-layer-join) / [packages/cli/README.md](../../packages/cli/README.md#connection-topology). |
| `mounts` | `Vec<{ dir: String, at: String }>` (default `[]`) | Deployment-topology per-directory mounts: prepends `at` to a `kind=http` provide's key when its file path falls under `dir` (longest matching `dir` wins per provide). Shape is validated fail-fast by the CLI mapper (`ConfigError`); the engine itself defensively skips+warns on a malformed value as a backstop. |
| `hosts` | `Vec<String>` (default `[]`) | Hosts this tree owns. An absolute-URL consume from any tree targeting one of these hosts (`http`/`https` only) is re-keyed to an internal joinable key at cross-layer link time instead of falling into `externalConsumes` — see `hostRekeyCounts` below. |

### Defaults (zero-config = full analysis)

The JS wrapper (`index.js`, before the config JSON crosses into Rust) injects two defaults into every
`analyze` config and every `analyzeTrees` tree entry, so a bare `{ root }` request runs the full
analysis instead of silently degrading to native-analyses-only:

- `packsDir` — when the key is absent, defaults to the bundled rule packs: `<repo root>/rules/dsl`
  when running from a source checkout (the live truth, so a rule edit is never shadowed by a stale
  copy), else the `rules/` directory inside the installed package (populated at publish time by the
  `prepack` script). When the caller supplies their own `packsDir` (a single
  directory string, or an array of directories), the JS wrapper (`index.js`) PREPENDS the bundled
  default directory rather than replacing it — the effective load order sent to Rust is
  `[bundled, ...yourDirs]`. All listed directories are loaded and merged: a pack id that appears in
  more than one directory is taken WHOLE from the LATER directory in that order — not a rule-level
  merge inside that pack id — so a caller's pack always wins a collision against a shipped pack with
  the same id, while every distinctly-id'd pack from every directory stays loaded. A bad/unreadable
  directory anywhere in the list is a non-fatal `warnings` entry; every other directory still loads.
  An explicit `packsDir: null` disables pack loading entirely (bundled included) — this is the one
  case the wrapper leaves untouched, since `null` means "no DSL packs at all", not "no defaults".
- `git` — when the key is absent, defaults to `git: {}` (the engine applies its own `recentDays: 30`
  default). An explicit value wins; `git: null` disables git collection. If `root` is not a git
  repository, the engine degrades gracefully with a "git collection skipped" warning.

This bundled-packs default is JS-specific: `index.js` injects it as a `packsDir` pointing at a directory
on disk (a source checkout's `rules/dsl`, or the installed package's copied `rules/`). A non-JS host with
no such directory to point at — `zzop-mcp` is the only one today — gets the same "bundled packs always
load" guarantee through `packDefs` instead: the shared `zzop-config` crate embeds the bundled pack JSON
at compile time and injects it as inline `packDefs` before mapping a config to a request (see
[modules/mcp.md](mcp.md)). `packDefs` is additive to this crate's own contract — the JS wrapper never
sends it, so this default-injection description is unchanged for JS callers.

`analyzeEnvelope`'s config gets only the `packsDir` default — envelope mode has no `root`/git. To turn
off individual rules rather than a whole channel, use `disabledRules`
(see [rules/catalog.md](../rules/catalog.md)).

When the engine itself runs with a narrowed scope anyway (explicit opt-out, or a non-JS consumer
calling the Rust engine directly), it self-reports on `warnings` instead of staying silent:

- `git history not requested (git option omitted): scores, health, recommendations, criticality, seams and layerCoChurn are null. Pass git: {} to enable them.`
- `no DSL rule packs loaded: only the N built-in native analyses ran. Set packsDir to a directory of *.json rule packs to enable the shipped DSL rules.` (N = the engine's actual native-analysis count.)

These are capability notes, not errors — the analysis still completes normally. The zero-packs note
also applies to `analyzeEnvelope`; the git note never does (envelope mode has no git by design).

`EnvelopeAnalyzeRequest { sourceId: String, packsDir: Option<String | Vec<String>>, disabledRules: Vec<String>, severityOverrides: BTreeMap<String, Severity>, suppressions: Vec<{ rule, path?, glob? }>, globalExcludes: Vec<{ path?, glob? }> }` —
deliberately no `root`/`cacheDir`/`git`/`sizeCap`/`packDefs` (envelope mode has no filesystem root or git
repo, and takes packs via `packsDir` only — see `packDefs`'s row above).
`severityOverrides`/`suppressions`/`globalExcludes` behave identically to their `AnalyzeRequest`
counterparts above. `NormalizedEnvelope` shape: see `../NORMALIZED_AST.md`.

### Validation-only: `validateEnvelopeOnly`

`validateEnvelopeOnly(envelopeJson)` runs the same structural/semantic checks `analyzeEnvelope` applies
to its envelope argument (`zzop_core::validate_envelope`) but stops there — no `configJson`, no pack
loading, no engine run — so an external adapter author gets fast, offline "is my envelope well-formed"
feedback without a full analysis. It returns `{"valid": boolean, "issues": string[]}` and, unlike every
other function on this page, **never fails**: an unparseable or semantically invalid envelope still
produces an ordinary `{"valid": false, "issues": [...]}` result rather than a rejected `Result`/thrown
`Error` — a validity check cannot itself be "wrong" the way a malformed request can.

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
| `warnings` | `string[]` | Non-fatal issues (e.g. a bad `packsDir`) plus the capability self-report notes — see [Defaults](#defaults-zero-config--full-analysis). |
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
| `joinContributionZero` | `boolean` | `true` when this tree analyzed files>0 but extracted zero IO (0 `ioProvides`, 0 consumes) — the active-blindness fact: this tree is structurally invisible to `analyzeTrees`'s cross-layer join, so any join finding referencing it (`unconsumedProvides`/`unprovidedConsumes`/edges) is not meaningful for it. A framework/SDK client the extractor cannot see is a common cause; see `adapterOverlays` above (Mode B) to restore visibility. |

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
`"zzop-napi/{CARGO_PKG_VERSION} zzop-parser-typescript={PARSER_FINGERPRINT} zzop-parser-prisma={PARSER_FINGERPRINT}"`
(Java's fingerprint is not currently surfaced here).

## Output data shapes

The `ir` field is the Common IR every file gets projected into — language-neutral, and the same shape
an external parser adapter must produce (see [NORMALIZED_AST.md](../NORMALIZED_AST.md)):

| Type | Fields | Notes |
|---|---|---|
| `CommonIr` | `source`, `parser: string`, plus the fields below (flattened) | `parser` = producing adapter id (`"typescript"`, `"prisma"`, ...). |
| — `dep` | `{ [path]: string[] }` | Import graph: path → imported paths. |
| — `symbols` | `SourceSymbol[]` | See below. |
| — `loc` | `{ [path]: number }` | Physical line count per file. |
| — `io` | `IoFacts \| null` | `provides`/`consumes` HTTP/DB/tRPC facts, joined cross-tree by `analyzeTrees`. |
| `SourceSymbol` | `id, file, name, kind, line, exported, isDefault, bodyStart?, bodyEnd?` | `kind` is one of `function\|class\|const\|type\|interface`; `bodyStart`/`bodyEnd` (1-based, inclusive) are set only for functions/classes with a recoverable body span. camelCase on output like every other type here. On the way IN, `SourceSymbol` is also reused verbatim as the deserialize target for [NORMALIZED_AST.md](../NORMALIZED_AST.md)'s frozen v1 external-parser envelope input contract (`FileProjection.symbols`), so it additionally *accepts* that contract's snake_case names (`is_default`, `body_start`, `body_end`) via `#[serde(alias = ...)]` — a conforming envelope producer's JSON keeps working unchanged. |

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

## Error/panic discipline

Two layers. `zzop-facade` (`crates/facade/src/lib.rs`) never panics by contract — every fallible path
(bad JSON, missing `root`, invalid envelope) returns `Result<String, String>`. `addon.rs` wraps each of
the three fallible calls (analyze/analyzeTrees/analyzeEnvelope — validateEnvelopeOnly is wrapped only for panic-safety and never itself returns Err) in:

```rust
fn catch<F: FnOnce() -> Result<String, String> + UnwindSafe>(f: F) -> napi::Result<String> {
    match std::panic::catch_unwind(f) {
        Ok(Ok(json)) => Ok(json),
        Ok(Err(message)) => Err(Error::from_reason(message)),
        Err(_) => Err(Error::from_reason("zzop-napi: internal panic (this is a bug — please report it)")),
    }
}
```

This is a second, outer `catch_unwind` — the engine already isolates a single bad file's parse/rule
failure internally (see [degraded files](../ARCHITECTURE.md#degraded-files)); this outer one exists
because unwinding across a `#[napi]`-exported `extern "C"` boundary is undefined behavior and must
never happen. In practice: a JS caller sees either a resolved value or a rejected/thrown `Error`, never
a process crash. `version` has no `Result`/`catch` wrapper (cannot fail).

## Building the addon from source

The real `.node` addon requires the MSVC toolchain on Windows (Node-API's delay-load linking has no
MinGW/`ld` equivalent):

```
cargo +stable-x86_64-pc-windows-msvc build -p zzop-napi --release --features addon
```

## Packaging layout

Main package `@zzop/native` (publishes to npm on a `v*` git tag): `main: index.js`,
`types: index.d.ts`, `optionalDependencies` on 5 platform packages under `npm/`:

| Directory | Package | os/cpu/libc |
|---|---|---|
| `npm/win32-x64-msvc` | `@zzop/native-win32-x64-msvc` | win32/x64 |
| `npm/darwin-x64` | `@zzop/native-darwin-x64` | darwin/x64 |
| `npm/darwin-arm64` | `@zzop/native-darwin-arm64` | darwin/arm64 |
| `npm/linux-x64-gnu` | `@zzop/native-linux-x64-gnu` | linux/x64/glibc |
| `npm/linux-arm64-gnu` | `@zzop/native-linux-arm64-gnu` | linux/arm64/glibc |

Each ships only a gitignored `zzop-napi.node` placed by CI (`scripts/place-artifacts.mjs`, mapping
Rust target triples to these 5 platform dirs). `scripts/sync-versions.mjs` propagates the root
version into every sub-package and the root's `optionalDependencies` pins.

**Loader cascade** (`index.js`): build `${platform}-${arch}`, look up the matching platform package →
`require()` it; on failure/absence, fall back to `require('./zzop-napi.node')` (local dev build); if
both fail, throw with the attempted paths, supported-platform table, and the correct build command for
the current platform. musl/Alpine and WASM have no table entry (fall to the local-build/throw path).
`smoke.mjs` (package-root, not `cargo test`) exercises the loader end-to-end against a real built
addon: `version()`, `analyze()` (2-file cycle fixture), `analyzeTrees()`.

## Calling from ESM

This is a CommonJS package. A bare specifier import (`import native from '@zzop/native'`) works from an
ES module, but a dynamic `import()` of a raw Windows file path (e.g. `import('C:\\...\\index.js')`)
fails with `ERR_UNSUPPORTED_ESM_URL_SCHEME` — Node requires a `file://` URL or `createRequire`:

```js
import { createRequire } from 'node:module';
const native = createRequire(import.meta.url)('/abs/path/to/packages/native/index.js');
// or: import { pathToFileURL } from 'node:url';
//     const native = (await import(pathToFileURL('/abs/path/to/index.js').href)).default;
```

See also: [../ARCHITECTURE.md](../ARCHITECTURE.md) (how a tree is processed, degrade/cache behavior),
[../rules/catalog.md](../rules/catalog.md) (every rule/analysis id `disabledRules` can reference),
[mcp.md](mcp.md) (the Node-free host that shares this page's `zzop-facade` request/response contract
end-to-end, over an MCP tool surface and a CLI instead of a JS binding).
