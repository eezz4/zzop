# The `zzop-facade` JSON contract

The zzop analysis engine's request/response surface. The functions below are all JSON-string-in /
JSON-string-out (except `version`), DEFINED in the shared `zzop-facade` crate
(`crates/facade/src/lib.rs`) — plain Rust that compiles and has a normal `#[test]` surface under the
workspace's default `gnu` toolchain with no feature flags. The two host products documented in
[mcp.md](mcp.md) reach these `zzop-facade` functions through the shared `zzop-summary` crate — no Node
process at all — and any embedder can drive the same JSON contract directly. This page documents those
request/response shapes.

## Functions

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
| `cacheDir` | `Option<String>` | See [Caching](../ARCHITECTURE.md#caching). Omit to run uncached — **this is the facade-wire answer**, and it is the one place the two dialects differ: the facade injects nothing, so an embedder that names no directory gets no cache and no directory created in its tree. A `zzop`/`zzop-mcp`/`zzop.config.jsonc` run reaches this field through `zzop-config`, which defaults it to `.zzop/cache` (see [Defaults](#defaults-zero-config--full-analysis) below). |
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

## Defaults (zero-config = full analysis)

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
- `cacheDir` — when the key is absent, defaults to `.zzop/cache`, resolved against the config file's
  directory (or the analyzed root, with no config file). This is the one default that WRITES: a first
  run creates that directory inside the analyzed tree, which is why the fact is spelled out for users in
  [ARCHITECTURE.md](../ARCHITECTURE.md#caching) (including the anchored `/.zzop/` gitignore line) rather
  than only here. A JSON-falsy value (`null` canonically) turns caching off and emits no `cacheDir` at
  all — byte-identical to the request an omitted key produced before this default existed. Note this
  bullet describes the front end only: the `AnalyzeRequest` field itself still means "omit to run
  uncached", per its row above.

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
`symbol-scan`/`io-scan` rules can fire in envelope mode (no source text) — and the bundled packs do
ship two `io-scan` rules (`http/auth-gates`, `http/route-exposure`), so the bundled default changes
**findings** too, not just `packsLoaded` and the spurious zero-packs warning. What those two lose in
`analyze_envelope` (Mode A) is not the firing but three ANCHOR-LINE-derived channels: inline
suppress markers, the `anchor_exclude_pattern` guard-hint exception, and the near-miss marker
disclosure all go quiet, because Mode A has no source text to locate a finding's line in
(`crates/engine/src/envelope/ingest.rs` passes an `anchor_line` callback that always answers `None` —
"no info available", never a guess). `apply_adapter_overlays` (Mode B) is unaffected: an overlay is
merged BEFORE assemble, so its facts keep real anchor lines. To turn off individual rules rather than
a whole channel, use `disabledRules` (see [rules/catalog.md](../rules/catalog.md)).

Whenever the engine can see less than a caller would assume — a narrowed scope from an explicit
opt-out, a non-host consumer calling the Rust engine directly, or simply a tree the loaded packs and
parsers do not cover — it self-reports on `warnings` instead of staying silent:

- `git history not requested (git option omitted): scores, health, recommendations, criticality, seams and layerCoChurn are null. Pass git: {} to enable them.`
- ``no DSL rule packs loaded: only the N built-in native analyses ran. If you expected the bundled packs, reinstall/check the package (the bundled packs directory may be missing); to add your own, set `packs: { extraDirs: [...] }` in zzop.config.jsonc (embedders: `packsDir`).`` (N = the engine's actual native-analysis count.)
- ``rule "<pack>/<rule>": `<field>` is not a valid regex … — that rule is SKIPPED and can never fire; the pack's other rules are unaffected by it. Run `zzop validate-rule-pack <pack.json>` to catch this before a scan.`` One line per offending field, for every loaded pack. The structural variants ("declares neither `line_pattern` nor `any`", "`trigger` names label X, which no `patterns` entry declares") report the same way: a rule that parses but cannot fire is disclosed rather than left to read as a clean scan.
- ``N DSL rule(s) loaded across M pack(s), but 0 have a `file_pattern` matching any file in this tree — the loaded packs target other filetypes. Native structural/whole-graph analyses still ran; zero DSL findings in this tree means "no applicable rules", not "clean".`` Fires only when packs ARE loaded and not one loaded rule's `file_pattern` matches any analyzed file (e.g. a Go-only tree against TS/Python-oriented packs) — the distinction that keeps "112 rules loaded, 0 findings" from reading identically to "ran, tree is genuinely clean". The per-pack half of the same census is `packsLoaded[].filesInScope` below.
- ``N file(s) with extension .<ext> have no native parser — no io/symbol facts were extracted from them: <up to 3 sample paths, +N more>. …`` One line per distinct unparsed extension, pointing at the `overlays: [...]` Mode B escape hatch. Excludes non-source extensions and any extension an adapter overlay already covers (the overlay IS the parser for those); extension-less files (README, Dockerfile) are deliberately never named.

These are capability notes, not errors — the analysis still completes normally. The zero-packs note
can reach `analyzeEnvelope` only via the explicit `packsDir: null` opt-out now (the facade's bundled
default otherwise guarantees a non-empty pack set); the dead-rule and no-applicable-rule notes reach
it normally; the git and unparsed-extension notes never do (envelope mode has no git and no
filesystem walk by design).

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

## Validation-only: `validateEnvelopeOnly`

`validateEnvelopeOnly(envelopeJson)` runs the same structural/semantic checks `analyzeEnvelope` applies
to its envelope argument (`zzop_core::validate_envelope`) but stops there — no `configJson`, no pack
loading, no engine run — so an external adapter author gets fast, offline "is my envelope well-formed"
feedback without a full analysis. It returns `{"valid": boolean, "issues": string[]}` and, unlike every
other function on this page, **never fails**: an unparseable or semantically invalid envelope still
produces an ordinary `{"valid": false, "issues": [...]}` result rather than a rejected `Result`/thrown
`Error` — a validity check cannot itself be "wrong" the way a malformed request can.

## Validation-only: `validateRulePackOnly`

`validateRulePackOnly(packJson)` is the same idea for a DSL rule pack: the pre-load, structure-only
check behind the `validate_rule_pack` tool and `zzop validate-rule-pack <file>` CLI subcommand (one
shared facade core, `zzop_facade::validate_rule_pack_json` — identical answers from every host). Its
`issues` surface exactly the judgments the engine's pack loader makes when it loads a
`packsDir`/`packDefs` pack — bad JSON, a missing field, a wrong type (serde's own messages, verbatim),
a too-new `schema_version` — plus the full dead-rule census (`zzop_core::pack_regex_issues`, the same
judgment the engine's own per-run `warnings` report): every matcher regex that fails to compile, which
the DSL interpreter otherwise reports by silently never firing that rule, AND the two STRUCTURAL
shapes that parse fine and still can never fire — a line-scan declaring neither `line_pattern` nor
`any`, and a method-scan whose `trigger` names a label no `patterns` entry declares. It never judges rule QUALITY or semantics: a
structurally sound pack with a useless rule is `valid: true`. Same `{"valid": boolean, "issues":
string[]}` shape and never-fails contract as `validateEnvelopeOnly` above. The machine-readable shape
contract ships as [`docs/contracts/rule-pack.schema.json`](../contracts/rule-pack.schema.json)
(`zzop://contract/rule-pack-schema` over MCP); the human-readable field reference is
[rules/dsl-reference.md](../rules/dsl-reference.md).

## Endpoint queries: `queryIo`

`queryIo(analysisJson, queryJson)` answers "is io key X provided/consumed/joined?" DEFINITIVELY —
pure post-processing over an ALREADY-PRODUCED `analyzeTrees` output (no re-analysis, no cache
interaction). It is the one shared query core: the `check_endpoint`
tool and `zzop endpoint` CLI subcommand ([mcp.md](mcp.md)) both call this exact function, so
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

## Structural drift: `zzop manifest` / `zzop diff`

Two pure JSON transformations over an `analyzeTrees` run, in the same "post-processing, one shared
core" class as `queryIo` above — but a layer out: they live in the shared `zzop-summary` crate
(`crates/summary/src/manifest/`), not in `zzop-facade`, and they are **CLI-only** (no MCP tool twin —
the reasons are recorded in [`docs/contracts/surface-parity.json`](../contracts/surface-parity.json)'s
`_cliOnlyLanes`). zzop produces manifests; **keeping** one is yours (commit it next to the code, the
same model as `scripts/max-file-lines-baseline.txt`) — no snapshot is ever stored, named or cleaned up
by zzop.

**Why they exist.** "Just diff two runs yourself" holds only *below* the caps. What a host ships is
not raw output, it is a capped summary (`crossLayer.edges` ≤ 200, `bucketKeys` ≤ 20, findings ≤ 50,
`degraded` ≤ 50). Above a cap, two runs' texts still agree on the *counts* while saying nothing about
*which* route left the join. A manifest stays structurally readable there because it carries identity
and nothing else.

`zzop manifest <path>... | zzop manifest --config <zzop.config.jsonc>` — same two source modes and the
same analysis as `cross`, projected differently:

| Field | Shape | Why |
|---|---|---|
| `tool` | `version()`'s string (release version + every parser fingerprint) | Honesty gate 1's key — see `diff` below. |
| `sources[]` | `{sourceId, joinContributionZero, degraded}` | Honesty gate 2's key. Deliberately **no `root`**: an absolute path differs between a laptop and CI, which would make the two machines that most need to compare unable to. |
| `provides[]` | `{kind, key, source}` | The API surface each tree exposes (from `ir.io.provides`, the one place the full list lives). |
| `edges[]` | `{kind, key, from, to}` | Which tree calls which, by source id only. |
| `buckets[]` | `{bucket, kind, key, source}` | Membership in each of the five non-edge buckets. An unresolved consume has no key, so its `raw` expression is its identity (the same fallback `bucketKeys` uses) — never guessed, never silently dropped. |

Every array is sorted and deduped, so the same analysis produces byte-identical bytes, and a pure
refactor (files moved, lines shifted, a route declared in a second place) produces an **empty** diff.
Not carried, by design: file/line (one rename would drown the real signal), `findings` (finding
identity drifts with line numbers, and severity totals already ride uncapped counts — v1 is structural
contract state only), and no schema-version field (a schema change ships in a zzop release, which the
`tool` gate already refuses to compare across).

`zzop diff <a.json> <b.json> [--allow-tool-drift]` — two manifests in, one delta out. Read
`transitions` first: a `+` is common and usually harmless, but a key moving from `edges` to
`unprovidedConsumes` means the caller still calls it and the route is gone. (Reply keys are ordered
alphabetically, not by rank — every reply in that crate serializes through a `BTreeMap`, which is part
of what makes them byte-identical run over run.)

| Field | Meaning |
|---|---|
| `transitions` | Keys present in BOTH runs whose bucket placement changed: `{kind, key, from[], to[]}`. Placement is a *set* (a key can sit in two buckets at once — e.g. provided by two trees, consumed from one), so a transition is a set change. |
| `sources` | `{added, removed, coverageDropped}` — `coverageDropped` names each source whose `degraded` rose or that became `joinContributionZero`. |
| `provides` / `edges` / `buckets` | `{added, removed}` — the raw evidence under the ranking. A transition's own rows also appear here; the transition entry is the *reading* of them, not an extra fact. |
| `blindnessSuspect` | Present (`true`) on any removed row — or transition — attributable to a source that lost coverage or vanished from the second run. Absent otherwise; it is never a claim that other removals are trustworthy. |

Two honesty gates, because without them this feature manufactures exactly the silent wrongs zzop's
`disclosure` registry exists to name:

1. **Tool identity.** Two manifests from different zzop builds are not comparable — our own parser
   improvement can move keys between buckets with no change to the analyzed code, and would read as
   the other team breaking a contract. `diff` **refuses** by default (exit 1) and names the escape
   hatch; `--allow-tool-drift` compares anyway and the reply then carries a `toolDrift` block naming
   both builds. Refuse or disclose, never silently compare.
2. **Blindness vs deletion.** A tree that got *less visible* explains disappearances by itself, so
   `blindnessSuspect` + `sources.coverageDropped` keep "the route vanished" from being reported when
   "we stopped being able to see it" is the honest reading.

Exit codes follow every other subcommand: 0 on success, 2 for an argument-shape mistake, 1 for a
runtime failure — which includes the gate-1 refusal, and a file that is not a manifest at all (a named
error naming *which* argument, never two empty relation sets read as "nothing changed"). `diff` does
**not** exit non-zero on a detected transition: a CI gate reads the JSON (e.g.
`jq -e '.transitions | length == 0'`), so "the contract broke" can never be confused with "the diff
itself failed".

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
| `configWarnings` | `string[]` | Config-authoring problems computed at analysis time, kept OUT of `warnings`: a `disabledRules`/`severityOverrides` entry matching no known rule id (a typo, or a stale id from a different zzop version) did nothing, and is reported here instead — only analysis time has the known-rule-id set (native analysis ids + loaded DSL pack ids) a config parser never sees. Always present; `[]` means neither knob had a matching-nothing entry. A `suppressions` entry with the same problem is unaffected by this split and still reports on `warnings`. This host's own `zzop-config` crate (see [Config semantics](mcp.md#config-semantics)) attaches ITS OWN parse-time config problems (unknown config keys, a malformed overlay) to the same `configWarnings` name on its own reply; this facade-level field is the analysis-time half of that one channel, never a rename of `warnings`. |
| `cache` | `{ hits, misses } \| null` | Set only when `cacheDir` was given. |
| `ruleTimings` | `object[] \| null` | Per-rule id + elapsed time + finding count; set only when the caller requests profiling. |
| `coverage` | `object` | Per-tree coverage census — always present. See below. |

`coverage` fields (all plain counts over this tree, always present — a `0` means "counted and found
none", not "not run"):

| Field | Type | Meaning |
|---|---|---|
| `files` | `number` | Files walked (same as `fileCount`) — every file under the root, including docs, data and assets; the walk applies no extension filter. See `sourceFiles` for the code subset. |
| `sourceFiles` | `number` | The subset of `files` a native frontend dispatched on, or that an applied overlay covers. Dispatch is by extension, so a file that hit the size cap and fell back to a lexical count, or that failed to parse, still counts here — this is "analysis had a frontend for it", not "analysis extracted structure from it". `degraded` reports the parse failures separately. `files - sourceFiles` is the docs/data/asset remainder, which is why a repo can report thousands of walked files and far fewer analyzed ones. Caveat: Mode A/B envelope ingest sets this equal to `files`, so `files == sourceFiles` on an injected run is an identity of construction, not a coverage signal. |
| `symbols` | `number` | `SourceSymbol` entries extracted (`ir.symbols[]` length). |
| `importEdges` | `number` | Resolved import-graph edges — sum of `ir.dep` out-degrees (edge count, not source-file count). |
| `ioProvides` | `number` | `ir.io.provides` entries. |
| `ioConsumesKeyed` | `number` | `ir.io.consumes` entries whose key resolved statically. |
| `ioConsumesUnresolved` | `number` | `ir.io.consumes` entries whose key could not be statically determined. |
| `degraded` | `number` | Same count as `degraded.length`. |
| `joinContributionZero` | `boolean` | `true` when this tree analyzed files>0 but extracted zero JOINABLE io (0 `ioProvides` and 0 keyed consumes — unresolved consumes don't count, they cannot join) — the active-blindness fact: this tree is structurally invisible to `analyzeTrees`'s cross-layer join, so any join finding referencing it (`unconsumedProvides`/`unprovidedConsumes`/edges) is not meaningful for it. A framework/SDK client the extractor cannot see is a common cause; see `adapterOverlays` above (Mode B) to restore visibility. |

## `disclosure` — silent-failure-class registry (run-global)

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
- `unresolvedConsumes` — a consume whose target could not be statically determined: either no key was
  resolved at all (`key: null`, the source text in `raw`), or the resolved key names no route because
  every path segment is a `{}` placeholder (`GET /{}` — an unresolved `${BASE}` interpolation dropped the
  host; `key` is present for these, so they stay locatable in `bucketKeys`). Both are "the analysis is
  blind here", never "the route is missing", and both count toward
  `cross-layer/unresolved-consume-ratio`. Such a key only lands here on a MISS: if some tree really does
  provide a catch-all `GET /{}` route, the consume joins it as an ordinary edge.
- `externalConsumes` — a consume targeting an absolute external host URL (e.g.
  `GET https://vendor.com/api/users`): third-party egress, not joined, not treated as drift.
- `ambiguousConsumes` — a consume matching provides in 2+ distinct source trees: not
  auto-linked (no edge emitted), every candidate provider listed so the ambiguity can be resolved by hand.
- `edges[].lowConfidenceReason` (string, omitted when not set) — the edge's key matched a generic-path
  pattern (health checks, `/login`, etc.) that many unrelated services could share, so the match is lower
  confidence than a distinctively-named route; the edge is still emitted.

**The buckets are the raw join fact, not a findings list.** The only filters applied when building them
are ones readable from the key or the file itself — an unresolvable key, an absolute-URL key, a
test-classified file, provider absence or ambiguity. The linker is kind-agnostic and holds no rule
vocabulary, so no domain filter (static assets, health routes, ...) runs at this layer: a consume that a
rule vetoes as not-really-API still sits in `unprovidedConsumes`. Rules that report the same class apply
extra vetoes on top, so `crossLayerFindings` is a filtered *view* of these buckets and will legitimately
be smaller — the two disagreeing on one key is the contract working, not drift. Disclosed per run as the
`join-bucket-unfiltered` entry in the [`disclosure`](#disclosure--silent-failure-class-registry-run-global)
registry.

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
same value `zzop-mcp`'s `serverInfo.version` reports (see [MCP surface](mcp.md#mcp-surface)); CI's
release job verifies the release tag matches it, so a released build's reported version equals its tag
by construction.

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

## Error/panic discipline

`zzop-facade` (`crates/facade/src/lib.rs`) never panics by contract — every fallible path (bad JSON,
missing `root`, invalid envelope, a malformed query) returns `Result<String, String>`. The engine
itself already isolates a single bad file's parse/rule failure internally (see [degraded
files](../ARCHITECTURE.md#degraded-files)), so any caller — `zzop-mcp` or a direct `zzop-facade`
embedder — gets either a value or a `Result::Err`, never a process crash, with no extra
unwind-catching wrapper needed: an in-process Rust call has no FFI boundary to protect. `version` has
no `Result` (cannot fail).
