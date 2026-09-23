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
| `validateEnvelopeOnly` | `(envelopeJson: string) -> string` | envelope JSON → `{valid: boolean, issues: string[], hints: string[]}` — see [below](#validation-only-validateenvelopeonly). |
| `validateRulePackOnly` | `(packJson: string) -> string` | rule-pack JSON → `{valid: boolean, issues: string[]}` — see [below](#validation-only-validaterulepackonly). |
| `queryIo` | `(analysisJson: string, queryJson: string) -> string` | an `analyzeTrees` OUTPUT + `{pattern}` → the definitive endpoint-query result — see [below](#endpoint-queries-queryio). |
| `queryFile` | `(analysisJson: string, queryJson: string) -> string` | an `analyzeTrees` OUTPUT + `{path, sourceId?}` → everything this run knows about ONE file — see [below](#file-queries-queryfile). |
| `queryCoverage` | `(analysisJson: string, queryJson: string) -> string` | an `analyzeTrees` OUTPUT + `{}` → the AGGREGATE visibility view: the per-extension dispatch table (each row carrying `declaredImports` — the pre-resolution declared-specifier sum from the census's `declaredImportsByExt`, `null` = never measured — beside `inDepGraph`, so declared-but-unresolved import blindness reads directly off one row), per-tree blind spots crossed against each declared rule sightline — beside them `unreadExtensions`, the principal filetypes (10%+ of the tree, and of a kind where a dispatch-`None` CAN cost facts by `zzop_engine::extraction_can_lose_facts` — source languages plus data/config formats, each row labelled with which by its `kind`) that NO structural parser read, which the per-rule cross deliberately skips, and a `blindSpotBasis` that now names that exclusion so an empty `blindSpots` cannot be read as "no blind spots" (measured on directus: `blindSpots: []` next to 587 unparsed `.vue` files) — the top-level `frameworkRecognizers` capability table (every framework recognizer compiled into this build with the channels it fills — verbatim from `zzop_engine::framework_recognizers`, uncrossed with any tree, so "does this build know my stack?" has an answer before the first run), the per-tree `ioChannels` cell (`extracted` — one row per io kind the rules read, present even at zero, so a full channel can no longer vouch for an empty one the way the kind-agnostic `joinContributionZero` does; and `zeroExtraction` — the CAPABILITY×MEASURED cross naming each (channel, extension) this build has a recognizer for that contributed 0 **and that is a principal share of what this run read structurally** — a filetype under that floor is absent from the list rather than cleared by it — a COVERAGE FACT keyed on the tree rather than on recognizing a framework by name), and the axes this run did NOT measure. The one surface that answers whether a 0-finding reply is clean or blind. Backs BOTH `zzop coverage` and the `check_coverage` MCP tool (2026-09-04 — until then this was a CLI-only lane, which meant the agent persona could not reach the visibility surface at all and had to trust a zero it could not qualify). |
| `version` | `() -> string` | none (cannot fail, no `Result`) |
| `versionString` | `() -> string` | none — the same string `zzop version --verbose` and `zzop-mcp version --verbose` print: this release plus, per bundled parser, `<PARSER_FINGERPRINT>/<derived source hash>` and a trailing `zzop-engine=<hash>`. The fingerprint half names the pinned parser FRONTEND (swc / ruff / tree-sitter version + projection-shape generation); the derived-hash half is the same value the cache key uses, so it moves whenever extraction code changes. Two builds that analyze a tree differently therefore print different strings — this value answers "are these two analyses comparable" (rule-pack-only differences excepted; those live in `ruleset_fingerprint`, not here). |
| `explain` / `explain_with_config` | `(query: &str) -> Result<String, String>` · `(config_path: &str, query: &str) -> Result<String, String>` | one rule id → that DSL rule's data as human-readable lines. The pair differs ONLY in which packs are searched: `explain` reads the packs compiled into the binary, `explain_with_config` reads the packs that config's trees load (compiled-in ones plus every `zzop/rules/`/`packs.extraDirs` directory), so a rule recovered out of the bundled set is explainable through the config that runs it. `Err` names what the id actually is (native analysis id, pack id, output field id, ambiguous bare id, unknown). **Every field of the rule's matcher is reported**, in three blocks after the `matcher:` line: SCOPE (what it scans — `file_pattern`, the `require_file`/`require_file_all` pre-skips, `line_pattern`/`any`/`patterns`/`trigger`, `key_pattern`/`name_pattern`/`symbol_pattern`, and the per-line text switches), EXCLUSIONS (`require_file_absent`, `absent`, the attribute gates, and every `*_exclude_pattern` that matcher kind carries — `exclude_pattern`, `file_exclude_pattern` and `anchor_exclude_pattern` alongside the neighbouring-text vetoes that read a line above, a line below, an enclosing call or the trigger call's own parentheses), then the single field that gates nothing (`snippet_max`, printed with a label saying so). A field the rule left unset still prints its NAME, with `no` for an unset pattern and `any` for an unset filter, so the reader learns which knobs that matcher kind even offers; only fields that matcher kind actually carries appear at all. This is what makes `zzop explain <rule-id>` the canonical answer to "would this rule look at my file?", so no other document has to copy a rule's regexes — and `every_matcher_field_is_reachable_from_explain` (`crates/facade/src/explain/field_coverage_tests.rs`) reads the field list out of the matcher structs' own source, so a new DSL field cannot ship un-explained. Native analysis ids are compiled Rust with no matcher at all and stay in the `Err` lane, which names no scope field. |

`AnalyzeRequest` (`#[serde(rename_all="camelCase", default)]`, unknown fields ignored):

| Field | Type | Notes |
|---|---|---|
| `root` | `String` (required — empty → `Err`) | Tree root to walk. |
| `sourceId` | `String` (default `""`) | Free-form label carried through into cross-tree output. |
| `packsDir` | `Option<String \| String[]>` | Directory (or directories) of `*.json` DSL rule packs to load — see [rules/authoring-guide.md](../rules/authoring-guide.md). Multiple directories are loaded and MERGED (see [Defaults](#defaults-a-config-is-required-what-it-does-not-have-to-say) below for the collision rule). A bad/missing directory is a non-fatal `warnings` entry, not a failure — other directories in the list still load. |
| `packDefs` | `RulePackDef[]` (default `[]`) | Inline rule-pack definitions handed to the engine as data instead of a filesystem directory — the self-contained-binary alternative to `packsDir` (`zzop-mcp`'s bundled packs, embedded at compile time). Loaded BEFORE `packsDir` directories, so a directory pack with the same id wins the collision. A same-id collision among `packDefs` entries themselves: the later array entry wins whole. Also accepted on `analyzeEnvelope`'s config — `EnvelopeAnalyzeRequest` carries the same field with the identical contract. |
| `cacheDir` | `Option<String>` | See [Caching](../ARCHITECTURE.md#caching). Omit to run uncached — **this is the facade-wire answer**, and it is the one place the two dialects differ: the facade injects nothing, so an embedder that names no directory gets no cache and no directory created in its tree. A `zzop`/`zzop-mcp`/`zzop.config.jsonc` run reaches this field through `zzop-config`, which defaults it to `.zzop/cache` (see [Defaults](#defaults-a-config-is-required-what-it-does-not-have-to-say) below). |
| `git` | `Option<{ since: Option<String>, recentDays: Option<u32>, commitTypePatterns: Option<Array<{ pattern: String, tag: String }>>, commitSubjectPatterns: Option<Array<{ pattern: String, label: String }>> }>` | Enables git-derived scores/health/recommendations/criticality/seams. `recentDays` default is 30. `commitTypePatterns` is an ARRAY of `{ pattern, tag }` objects (NOT a map) — e.g. `[{ "pattern": "^hotfix:", "tag": "FIX" }]` — and, when present and non-empty, REPLACES the default FIX/FEAT/REVERT/... classifier table entirely (match order = array order, mirroring the default table's REVERT-first rationale); an entry whose `pattern` fails to compile as a regex is skipped (matches nothing) and reported as a `warnings` entry, never a failure. `commitSubjectPatterns` is the DECLARED subject-label axis and differs from its sibling in three deliberate ways: (1) it has NO default table — absent or empty labels nothing at all, because what a "revert"/"ticket"/"hotfix" subject looks like is a per-project convention the engine would otherwise have to guess; (2) it is NOT first-match-wins — every declared pattern that matches contributes its `label`, in declaration order, with a repeated label kept once at its first declared position; (3) the `pattern` is compiled EXACTLY as written, with no implicit `(?i)`, and is matched against the raw subject (no leading-`[scope]` stripping). Two self-reports ride the `warnings` channel: an entry whose `pattern` fails to compile (skipped, matches nothing, never a failure), and a declared table that matched ZERO collected commits (a declared-but-dead knob is otherwise indistinguishable from declaring nothing). **Today this key's only observable effect is those warnings** — the preserved subject and its labels live on the engine-internal per-commit record and are not yet carried on any output channel. Known limit: subjects are decoded from git's output with `String::from_utf8_lossy`, so a legacy-encoded subject (a commit object with no `encoding` header, e.g. latin-1 / Shift-JIS history) reaches matching with each non-UTF-8 byte already replaced by U+FFFD — a pattern spelling those original characters can never match it, and the zero-match warning says so whenever a U+FFFD is actually observed. |
| `vocabulary` | An object of optional convention-vocabulary keys — the authoritative list is `zzop contract config-surface`, and the engine type is `zzop_engine::VocabularyConfig` (default `{}`) | CONVENTION VOCABULARY — the names a PROJECT picks, declared instead of guessed. A name a framework fixed (`@GetMapping`, `router.post`) stays built in because nobody can rename it; a name the project chooses (what it calls its auth guards, which URL segments mark its API, where its Java sources live, which directories hold build output) is declarable here, because holding it as a built-in literal means the engine guesses and silently misclassifies every project that names it differently. PER KEY, WHOLE REPLACEMENT: a key you name replaces its built-in list or pattern outright — never an element-wise merge, the same one-origin rule `packs.extraDirs` and `git.commitTypePatterns` state. A key you do not name IS NOT JUDGED — there is no built-in fallback, and a declared-but-empty value (`null`, `""`, `[]`) means the same thing. (`crates/engine/src/vocabulary/resolved.rs` states in its own module doc that no built-in fallback arm is left in the file.) The built-in values still exist, but only as the defaults `zzop init` writes into the starter config — so they reach a run because the author's config SAYS them, never behind the author's back. Note what this direction costs: an undeclared exemption vocabulary grants no exemption and an undeclared guard vocabulary proves no guard, so both make a rule fire MORE. Measured 2026-07-29 across a 17-repo corpus, declaring the shipped template vocabulary took `mutating-route-no-auth` from 58 findings to 6. An unconfigured project is a noisy project, on purpose: this is the under-clear-rather-than-over-clear direction, and it is why "declare nothing" is not spellable as "treat everything as a guard". Disable a judgment with `rules: { "<id>": "off" }`, not by declaring an empty vocabulary. A declared pattern that does not compile as a regex matches nothing, never a panic. `skipDirs` lands on the walker's own skip list rather than staying on this struct, so one list has one owner. NOT the same roof as `git.commitTypePatterns`/`git.commitSubjectPatterns`, deliberately: those configure the git collector and match commit MESSAGES (prose a human wrote), while every key here names something the analyzed code itself spells. `zzop init` writes every key with its built-in value, so the starter file documents these assumptions instead of hiding them. CACHE: these names decide what gets EXTRACTED, not only what gets reported, so the whole object is hashed into both halves of every per-file cache key — changing any key re-analyzes the affected files instead of serving entries written under the previous vocabulary. |
| `parsers` | `{ globOverrides: Vec<{ glob: String, language: String }> }` (default `{}` — `zzop_facade::ParsersRequest`) | PARSER ROUTING — force-routes paths matching `glob` to a named language, applied in order (first match wins) AHEAD of the extension map. For the files whose extension lies about what they contain. An entry naming a language this build does not have is SKIPPED WITH A WARNING rather than failing the run: an unknown language is a config-authoring mistake, and the run's other trees still have honest answers to give (`crates/facade/src/config/declared.rs`). A SEPARATE roof from `vocabulary` on purpose: every key under that one names something the project CALLS its own (a guard, a segment, a directory), and this one names a path→parser MAPPING. Folding a mapping in among names would repeat the mistake `git.commitTypePatterns` is explicitly kept out of `vocabulary` to avoid — same "user-declared table" feel, different subject matter. |
| `scores` | `{ excludeTestFilesFromFileMetrics: bool }` (default `{}` — `zzop_facade::ScoresRequest`) | STRUCTURAL-SCORE POLICY — the config's `scores` object, the wire exposure of `zzop_engine::EngineConfig::scores_exclude_test_files`. Its one key decides whether TEST FILES count toward the population every structural score averages over. `false` (the default, and what an absent object means) counts them, which is the population every score has always used, so an existing caller's numbers do not move. `true` removes every path the shared test-path vocabulary matches (`zzop_core::is_test_file`), widened by this tree's own `vocabulary.extraTestPathPatterns`. SCORES ONLY: no rule reads it, so no finding appears or disappears (measured on `corpus/frameworks/express`: `findings.total` 58 -> 58, `bySeverity`/`byRule` byte-identical, `architecture.pain` 30.0 -> 11.5; re-measured 2026-09-23 — the INVARIANT is the claim, the absolute count drifted from 56, and the recount command lives with the field itself in `crates/facade/src/request/scores.rs`) — an excluded file is still parsed, still a node with all its edges, and still reportable; what it loses is standing as the SUBJECT of a score. And only of SOME scores: it reaches the eleven metrics keyed on a FILE plus the `health.pain` rollup, never the four keyed on a directory rollup (`cohesion`, `sdp`, `mainSequence`, `modularity`) and never `critical`/`recommendations`, which are computed outside the scores subsystem. Because it re-bases `pain`, the shaped reply's `architecture.painMeaning` states the live population on every run, so two replies can be compared. A third roof beside `vocabulary` and `parsers` for the reason that one gives: this is neither a name the project picked nor a path-to-parser mapping, it is a policy about which files a score may judge. |
| `sizeCap` | `Option<usize>` | Default 1,500,000 bytes (~1.5MB) — see [degraded files](../ARCHITECTURE.md#degraded-files). |
| `disabledRules` | `Vec<String>` | Rule/analysis ids to turn off — see [rules/catalog.md](../rules/catalog.md) for the id list. |
| `packsOnly` | `Vec<String>` (default `[]`) | DSL pack ALLOWLIST — when non-empty, a pack whose id is absent does not run. The opt-in twin of `disabledRules`, which can only say "everything except". Empty means NO allowlist (every loaded pack runs), never allow-nothing. Scoped to packs: native analyses keep running and stay `disabledRules`' business. Composes with `disabledRules` (the allowlist selects, `disabledRules` still subtracts). Config-file dialect: `packs.only`. |
| `severityOverrides` | `BTreeMap<String, "critical" \| "warning" \| "info">` (default `{}`) | Per-rule severity remap, keyed by rule id (same id space as `disabledRules`). Promotes/demotes a rule's findings without editing the pack — applied post-merge, so it also re-sorts the finding into its new severity band. |
| `suppressions` | `Vec<{ rule: String, path?, glob? }>` (default `[]`) | Finding-level accept-list. Each entry drops findings for `rule` either everywhere (no filter), only in files whose path CONTAINS `path` as a plain substring (case-sensitive), or only in files matching `glob` (full-path shell glob; `glob` takes precedence over `path`). Multiple entries for one rule are OR-ed. |
| `globalExcludes` | `Vec<{ path?, glob? }>` (default `[]`) | Config-wide, rule-agnostic REPORT-level filter — the top-level `"exclude"` config key. Same `path`/`glob` matching as `suppressions`, but drops matching paths from EVERY rule at once (rather than one named `rule`) and from **every other reporting channel**: `recommendations`, `crossLayerFindings`, `critical` (the summary's `architecture.criticalTop`), and every per-metric violation list under `scores.*`. **The graph is not filtered; the SUBJECT SET is** (changed 2026-07-30 — this entry previously said "nothing is filtered at computation time", which is no longer true). An excluded file is still parsed, still appears in `nodes` and the dep graph, and is still a real import target, so every *other* file's coupling, fan-out and blast radius are exactly what they were — excluding a vendored SDK does not make the files that import it look cleaner. What the exclusion removes is that file's own standing as a JUDGED SUBJECT: it leaves the violation list AND the denominator behind each per-file score, so `health.pain` moves with it. Both halves matter. Dropping the file from the graph instead would not filter the report, it would state that a real dependency does not exist; leaving it in the denominator while dropping it from the violations would silently inflate the compliant ratio. The old behaviour failed the other way — measured on zzop's own tree, excluding three whole top-level directories replaced every `criticalTop` slot and left `pain` identical to one decimal, so the headline number kept a value its own report no longer explained, and no exclusion a user could write could move it. **Note the direction is not predictable**: excluding code that is CLEANER than average raises `pain` (measured: 62.5 to 64.3 on this repo), because the figure describes the judged population. A run whose `exclude` removed at least one scanned file says so in `warnings`, since the number is only comparable against runs using the same `exclude`. **Two channels take no filter at all**: `warnings`, the config-diagnostics channel that reports an `exclude` so broad the problem only *looks* absent (filtering it would let the filter erase its own warning), and anything keyed by a slice or module rather than a file path (`cohesion.slices`, `sdp.violations`, `mainSequence.modules`, and the `modularity` rollup) — their subject is a directory, not a file, so "this file is not judged" has no referent, and those four metrics receive no subject gate either. **Edge-shaped rows** (an import violation, a diamond) are judged per SUBJECT — the importer or root, which is by definition not excluded — so the row is counted and kept, with any excluded path on its far side replaced by `<excluded>`, the same treatment finding evidence gets below. Counted and printed stay the same set. **TWO ROLES, ONE KEY**: a path is either a finding's ANCHOR (`file` — the subject) or its EVIDENCE (`evidencePaths` — a path it merely names). Excluding an anchor drops the finding whole; excluding an evidence path keeps the finding and replaces that path with `<excluded>` in `message` and everywhere inside `data`. *"My folder is the subject of the problem, I don't look at it. My folder is evidence in someone else's problem, I see the problem but my paths are not named."* The role decides the treatment, so there is no second `partialExclude` knob to choose between. **Cross-layer precision note**: the join is run-level, and the filter matches on tree-relative paths — so two trees that share a relative path share an exclude. That imprecision is deliberate and recorded: recovering the owner would require a hand-maintained per-rule key table, which is the staleness shape this repo removes rather than adds. |
| `profileRules` | `bool` (default `false`) | RULE TIMING instrumentation — the ESLint `TIMING=1` / oxlint rule-timing equivalent, and the wire exposure of `EngineConfig::profile_rules`. `true` times each DSL rule and each whole-graph native analysis that runs, populating the output's `ruleTimings`; `false` leaves it `null` at zero added cost. Never changes `findings`/`ir`, and deliberately takes NO part in the cache key — a profiled and an unprofiled run of the same tree are the same analysis and reuse each other's cache entries. The one request field here with NO `zzop.config.jsonc` key: a config declares what is true about the PROJECT and gets committed, while a timing report is a question about ONE INVOCATION on one machine. CLI dialect: `--profile-rules` on `analyze`/`analyze-envelope`/`cross`. `EnvelopeAnalyzeRequest` carries the same field with the identical contract (added when Mode A's pack evaluation was routed through the engine's timing accumulator — before that the field did not exist there and the CLI refused the flag on that lane by name, so a knob nothing reads was never accepted). ⚠ A file served whole from cache never re-runs its per-file rules and contributes no timing, so a WARM run reports only the whole-graph native analyses — the emitted report discloses this and carries the cache counts that prove it. |
| `adapterOverlays` | `Vec<NormalizedEnvelope>` (default `[]`) | Mode-B adapter overlays: partial Normalized-AST envelopes merged ON TOP of native analysis (each re-validated, soft-skipped with a warning if invalid). How a framework/SDK adapter adds IoFacts the engine does not parse natively without reimplementing the parser — contrast `analyzeEnvelope`, where a full envelope REPLACES native analysis. Post-cache, so it does not affect the cache key. See [../NORMALIZED_AST.md](../NORMALIZED_AST.md). |
| `mountedAt` | `Option<String>` | Deployment-topology whole-tree gateway/ingress mount prefix — shorthand for a `mounts` entry with `dir: ""`, folded in LAST (after every `mounts` entry) so an explicit equal-length `mounts` entry wins a tie. `None` (default) adds no implicit mount. Applied to `kind=http` provides only, stacking on top of any code-extracted prefix. See [../ARCHITECTURE.md](../ARCHITECTURE.md#cross-layer-join). |
| `clientBase` | `Option<String>` | The CALLING side's mirror of `mountedAt` — the path prefix this tree's own outbound http calls carry, for when the base is assigned from a cross-file constant (`axios.defaults.baseURL = settings.baseApiUrl`) and the never-guess extractor therefore reads nothing. Prepended to every keyed RELATIVE `kind=http` CONSUME of the tree, unscoped by client (a declaration speaks for the whole tree, as `mountedAt` does on the serving side); an unresolved consume and an absolute-URL key are never touched. Shape is validated fail-fast by the CLI mapper (`ConfigError`); the engine defensively skips+warns on a malformed value. Unlike `mountedAt`, stacking is **warned, not silent**: if a readable literal base was already applied from the code, the declaration still wins but a `warnings` entry names both prefixes, because on the calling side a second prefix is usually a duplicate rather than a second real layer. A declaration that rewrites nothing warns too. See [../ARCHITECTURE.md](../ARCHITECTURE.md#cross-layer-join). |
| `mounts` | `Vec<{ dir: String, at: String }>` (default `[]`) | Deployment-topology per-directory mounts: prepends `at` to a `kind=http` provide's key when its file path falls under `dir` (longest matching `dir` wins per provide). Shape is validated fail-fast by the CLI mapper (`ConfigError`); the engine itself defensively skips+warns on a malformed value as a backstop. |
| `hosts` | `Vec<String>` (default `[]`) | Hosts this tree owns. An absolute-URL consume from any tree targeting one of these hosts (`http`/`https` only) is re-keyed to an internal joinable key at cross-layer link time instead of falling into `externalConsumes` — see `hostRekeyCounts` below. |
| `routes` | `Vec<{ key: String, role?: "provide" \| "consume" }>` (default `[]`) | Lightweight route-fact injection — the ergonomic counterpart of `adapterOverlays` for the common "inject one route zzop could not resolve from source" case (a non-literal path, a dynamic verb, a computed URL). `key` is a `"METHOD PATH"` interface key (`"GET /api/users"`), normalized through the same transform the extractors use for that side (`http_interface_key` for a provide; the query/fragment-dropping `http_consume_interface_key` for a consume, so `"GET /articles?limit=10"` joins a native `GET /articles`); `role` picks whether the route is SERVED here (`provide`, default) or CALLED from here (`consume`). The whole array expands into ONE synthetic adapter overlay of `http` provides/consumes, so it composes through the identical cross-layer join path as a hand-authored overlay. A `key` that is not a `METHOD`+`PATH` pair is soft-skipped with a warning (never a hard error). See [../ARCHITECTURE.md](../ARCHITECTURE.md#cross-layer-join). |

## Defaults (a config is required; what it does not have to say)

**A config file is mandatory for every analysis lane** (2026-07-27, reversing the earlier zero-config
default). Both binaries refuse a tree that has no `zzop.config.jsonc`, identically, and point at the
`config-template` document; `zzop init` writes it. The reason is the convention vocabulary below: an
undeclared name vocabulary makes no judgment at all, so a run with no config would analyze less while
reporting itself complete. What a config still does not have to say is everything else on this page —
the defaults below fill in, so a starter file that only declares `roots` gets the full analysis.

The `analyze`/`analyzeTrees` facade functions inject no defaults themselves — default-injection is
each host's own config front end's job, applied before the request ever reaches `zzop-facade`.
`zzop-mcp` does this through the shared `zzop-config` crate (`crates/config`), which embeds the bundled
rule packs at compile time (`build.rs`, `BUNDLED_PACK_SOURCES`) and injects them as inline `packDefs` —
carrying over the two defaults that make a bare `{ root }` request run the full analysis instead of
silently degrading to native-analyses-only:

- **Bundled DSL packs.** A single-binary host has no sidecar `rules/` directory to point a `packsDir`
  string at, so `zzop-config` embeds and injects the bundled packs as inline `packDefs` directly. A
  caller-supplied pack directory (config `packs.extraDirs`, or an embedder's own `packsDir`) is loaded
  alongside the bundled inline `packDefs`. A config that declares no `packs.extraDirs` at all gets ONE
  default directory instead: the user-authored `zzop/rules/` under the resolution base, and only when it
  exists on disk (see [ARCHITECTURE.md's On-disk layout](../ARCHITECTURE.md#on-disk-layout)). That is a
  fallback, never a merge — a declared `extraDirs` replaces it outright, `[]` included, so pack
  directories always have exactly one origin; a base with no `zzop/rules/` warns about nothing. Either
  way: a pack id present in both directory and bundled sources is taken WHOLE from the directory
  pack — a caller's pack always wins a collision against a shipped pack with the same id, while every
  distinctly-id'd pack from either source stays loaded. A bad/unreadable directory is a non-fatal
  `warnings` entry; every other directory still loads. An explicit `packsDir: null` disables
  directory-based pack loading — `null` means "no DSL packs from a directory", not "no defaults".
- `git` — when the key is absent, defaults to `git: {}` (the engine applies its own `recentDays: 30`
  default). An explicit value wins; `git: null` disables git collection. If `root` is not a git
  repository, the engine degrades gracefully with a "git collection skipped" warning.
- `vocabulary` — **the one key with no default at all.** It is forwarded exactly as declared, and an
  absent key means the judgments it governs are NOT MADE: no name is an auth guard, no banner marks a
  file generated, no receiver name is a write site. Those judgments are load-bearing — an undeclared
  vocabulary is a materially different analysis, not the same one minus a nicety — which is why it is
  not something a run can be silent about, and why `zzop init`
  writes zzop's own values INTO your file rather than assuming them behind it. Editing a key replaces
  that whole list or pattern; a key you leave alone keeps whatever your file says, not what zzop thinks.
  `workspaceSkipDirs` is the one member the front end consumes itself (`trees: "auto"` workspace
  discovery) and never forwards.
- `cacheDir` — when the key is absent, defaults to `.zzop/cache`, resolved against the config file's
  directory (there is always one — a config is required). This is the one default that WRITES: a first
  run creates that directory inside the analyzed tree, which is why the fact is spelled out for users in
  [ARCHITECTURE.md](../ARCHITECTURE.md#caching) (including the anchored `**/.zzop/` gitignore line, and
  why it must not be a `zzop*` glob — see [On-disk layout](../ARCHITECTURE.md#on-disk-layout)) rather
  than only here. A JSON-falsy value (`null` canonically) turns caching off and emits no `cacheDir` at
  all — byte-identical to the request an omitted key produced before this default existed. Note this
  bullet describes the front end only: the `AnalyzeRequest` field itself still means "omit to run
  uncached", per its row above.

`analyzeEnvelope`'s config gets only the pack default — envelope mode has no `root`/git — and gets it
at a single layer: the engine facade itself (`zzop_facade::analyze_envelope_json`) seeds the bundled
packs as inline `packDefs` on EVERY envelope analysis, whatever the host — the envelope path has no
per-host config front-end on the Rust side (an envelope carries no filesystem root for one to attach
to), so its full-analysis default lives at this shared chokepoint instead. Envelope mode is therefore
also the one lane the config requirement above does NOT apply to: there is no tree to look in, and
the convention vocabulary follows the same chokepoint rule as the packs — a `vocabulary` declared on
the `analyzeEnvelope` config is applied whole (per key, the tree lane's replacement contract), and a
request that declares none gets the PRODUCT default (`VocabularyConfig::built_in()`) assigned
explicitly by the facade, which is what Mode A's call-graph pass then consults. The shipped hosts
(`zzop analyze-envelope`, MCP `analyze_envelope`) expose no vocabulary knob and so always run the
product default; declaring one is a facade-request capability.
The seed
order keeps the same collision rule as above: bundled inline defs load first, a caller `packDefs`
entry with a bundled id wins whole (later inline def wins), and any `packsDir` directory pack wins
whole over both — so a raw facade/binary caller with no explicit `packsDir` sees `packsLoaded`
`source: "inline"`. An explicit `packsDir: null` disables the bundled seed and all pack directories —
caller-supplied `packDefs` are still honored, per the standing "packDefs always load" contract — the
facade distinguishes an absent key from an explicit `null` for exactly this opt-out. Note only
`symbol-scan`/`io-scan` DSL rules can fire in envelope mode (no source text; the NATIVE call-graph-BFS
rules — `mutating-route-no-auth`, `unsafe-read-endpoint`, `non-idempotent-write` — additionally run
when the envelope supplies its `calls` channel, see `../NORMALIZED_AST.md`'s `calls` section; an
envelope without it gets a `warnings` disclosure naming the silent rules instead) — and the bundled packs do
ship two `io-scan` rules (`http/protected-path-no-auth-evidence`, `http/dev-path-no-guard-hint`), so the
bundled default changes
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
- ``N loaded pack(s) had 0 files in scope and still ran. No file in this tree matches any of their rules' `file_pattern`, a path check made before any file content is read — so those packs can only ever report zero here, which is scope, not a clean bill of health, and it changes the moment a matching file is added. If this tree will never carry those stacks, dropping them buys back their rule-evaluation time: `packs: { disabled: ["<id>", ...] }` in zzop.config.jsonc (embedders: `disabledRules`). Packs you already disabled are not listed here.`` ONE aggregated line per run, pack ids sorted — never one line per pack (a single-language repo can have most bundled packs out of scope, and a line each would be noise). It is the actionable half of `packsLoaded[].filesInScope` below: the count says which packs matched nothing, this says what to do about it. Three silences, each deliberate: a pack you already disabled is never named (whole-pack or every rule of it individually — as of 2026-08-26 the whole-pack half IS readable off `packsLoaded[].didNotRun`, which is what this warning consults; the per-rule half, every rule of a pack disabled one by one, still has no field of its own); a pack with no rules is never named (there is no evaluation time to buy back); and a tree that analyzed zero files is silent entirely (every pack is trivially zero-scope there, and the "root produced 0 analyzable files" note owns that case). Advice only — the engine never disables a pack on its own.
- ``N file(s) with extension .<ext> have no native parser — no io/symbol facts were extracted from them: <up to 3 sample paths, +N more>. …`` One line per distinct unparsed extension, pointing at the `overlays: [...]` Mode B escape hatch. Excludes non-source extensions and any extension an adapter overlay already covers (the overlay IS the parser for those); extension-less files (README, Dockerfile) are deliberately never named. **Ordered by unread file count, largest first**, with the extension name breaking ties — so the biggest thing this run could not read is the first of these lines you meet, and the closing `No native parser exists for N extension(s) …` summary names the same leaders rather than the alphabetically-first ones. Nothing is dropped or capped: the order is the whole of what this channel does about length. It is a reading order, not a severity — a large count means less of the tree was read, never that the tree is worse.

These are capability notes, not errors — the analysis still completes normally. The zero-packs note
can reach `analyzeEnvelope` only via the explicit `packsDir: null` opt-out now (the facade's bundled
default otherwise guarantees a non-empty pack set); the dead-rule and no-applicable-rule notes reach
it normally; the git and unparsed-extension notes never do (envelope mode has no git and no
filesystem walk by design).

`EnvelopeAnalyzeRequest { sourceId: String, packsDir: Option<String | Vec<String>> (absent ≠ null), packDefs: Vec<RulePackDef>, disabledRules: Vec<String>, packsOnly: Vec<String>, severityOverrides: BTreeMap<String, Severity>, suppressions: Vec<{ rule, path?, glob? }>, globalExcludes: Vec<{ path?, glob? }>, mountedAt: Option<String>, mounts: Vec<{ dir, at }>, clientBase: Option<String>, profileRules: bool, vocabulary: Option<VocabularyConfig> }` —
deliberately no `root`/`cacheDir`/`git`/`sizeCap` (envelope mode has no filesystem root or git repo).
`vocabulary` is the one field whose undeclared default differs from `AnalyzeRequest`'s: a declared
object is applied whole per key (identical to the tree lane), while an ABSENT key means the facade
assigns the product default (`VocabularyConfig::built_in()`) — the envelope lane has no config file
for `zzop init` to have written the built-ins into, so they live at this chokepoint instead (see the
Defaults section above).
`packDefs`/`severityOverrides`/`suppressions`/`globalExcludes`/`mountedAt`/`mounts`/`clientBase`/`profileRules` behave
identically to their `AnalyzeRequest` counterparts above (for `packDefs` that includes the seed order: inline defs
load BEFORE `packsDir` directories, so a directory pack with the same id wins the collision whole; for
`mountedAt`/`mounts` that includes the fold order — every `mounts[]` entry first, `mountedAt` as the
implicit whole-tree `dir: ""` entry last — with the engine applying them uniformly to Mode A envelopes,
per `../NORMALIZED_AST.md`'s deployment-topology note). `clientBase` is there for the same
origin-agnostic reason and is if anything more load-bearing here: Mode A runs no code-extracted base
pass at all, so a declaration is the only base an envelope's consumes can carry.
Unlike `AnalyzeRequest`, `packsDir` here distinguishes an ABSENT key from an explicit `null`: absent
(or a directory value) keeps the facade's bundled-pack default (see [Defaults](#defaults-a-config-is-required-what-it-does-not-have-to-say)
above); `null` opts out of the bundled seed and all pack directories (caller `packDefs` are still
honored). `NormalizedEnvelope` shape: see `../NORMALIZED_AST.md`.

## Validation-only: `validateEnvelopeOnly`

`validateEnvelopeOnly(envelopeJson)` runs the same structural/semantic checks `analyzeEnvelope` applies
to its envelope argument (`zzop_core::validate_envelope`) but stops there — no `configJson`, no pack
loading, no engine run — so an external adapter author gets fast, offline "is my envelope well-formed"
feedback without a full analysis. It returns `{"valid": boolean, "issues": string[], "hints": string[]}`
and, unlike every other function on this page, **never fails**: an unparseable or semantically invalid
envelope still produces an ordinary `valid: false` result rather than a rejected `Result`/thrown
`Error` — a validity check cannot itself be "wrong" the way a malformed request can.

`issues` and `hints` are **different axes**, and that split is the contract. `issues` are why the
envelope is REJECTED and are the only input to `valid` — so **nothing about `hints` moves `valid`, and
the `zzop validate-envelope` exit code is unchanged**: a valid envelope carrying hints still reports
`valid: true` and still exits `0`. `hints` are shapes zzop ACCEPTS but that are almost certainly not
what the producer meant, each one a way the cross-layer join silently finds nothing while the envelope
reads fine — so a non-empty `hints` on a valid envelope is usually the more urgent of the two lists.
The field is **always present**, empty array included: omitting it when nothing was found would be
indistinguishable from a build that has no hint pass at all. Hints are reported for an invalid envelope
too (both axes in one round-trip), and are empty when the text did not parse. The canonical list of what
is hinted lives with the code (`zzop_core::envelope_hints`, surfaced by `ValidateEnvelopeReport` in
`crates/facade/src/envelope.rs`) rather than being recited here, where it would rot as the pass grows.

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
structurally sound pack with a useless rule is `valid: true`. Same never-fails contract as
`validateEnvelopeOnly` above, and the same `{"valid": boolean, "issues": string[]}` core — but
deliberately **no `hints` field**: there is no hint pass for rule packs, and shipping an always-empty
list here would claim "we looked and found nothing" about a search that never ran. The two reports
differing is the honest state, not drift. The machine-readable shape
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
  substring against every cross-layer io key (http routes, DB tables, tRPC procedures — every
  bucket plus `edges`), and against the `raw` expression of an unresolved consume (`key: null`);
  an unresolved consume with no `raw` recorded is unmatched, never guessed. An unknown query key
  (a typo like `"patern"`) is a named error, not a silent `not-found`.

The result (camelCase):

| Field | Meaning |
|---|---|
| `pattern` | Echo of the query pattern. |
| `verdict` | ONE token from the sealed vocabulary below. |
| `verdictMeaning` | One sentence saying what THAT returned token means. Ships in every reply so the vocabulary is self-describing on the wire — the definitions live next to the computation that assigns the token (`crates/facade/src/query.rs`), which is why neither host's help text nor the MCP tool description is a second owner of them. |
| `counts` | FULL match counts per bucket (`{edges, unconsumedProvides, unprovidedConsumes, unresolvedConsumes, externalConsumes, ambiguousConsumes}`) — never capped. |
| `matches` | The same six keys, each an array of the ORIGINAL matched objects (`file`/`line`/`source` intact), capped at 20 per bucket. |
| `truncated` | `{bucket: remainingCount}` — present only when a bucket's `matches` list was capped. |
| `relatedFindings` | Findings (from every tree's `findings` AND `crossLayerFindings`) selected by a TEXT match, not by a computed link: a finding is listed when its rendered message contains the pattern or any matched key as a case-insensitive substring. A `Finding` carries no io key to join on, so this over-matches (a message quoting a longer path that contains the pattern) and under-matches (a finding about this very key whose message spells it differently, or names only the file) — **an empty array is not evidence that no finding concerns this key**. Capped at 20, with a sibling `truncatedFindings: N` only when capped. |
| `relatedFindingsBasis` | The sentence above, on the wire, in every reply — `relatedFindings` is the one list here whose selection rule changes what an empty result means, so the caveat rides beside it rather than living only in a tool description (same one-owner convention as `verdictMeaning`; the text lives at `query.rs`'s `RELATED_FINDINGS_BASIS`). |
| `suggestions` | Up to 10 candidate keys, present ONLY on a `not-found` verdict (see `suggestionsTruncated` below when the cap bit): keys whose last path segment equals the pattern's (case-insensitively), falling back to keys containing any single `/`-segment of the pattern. |
| `suggestionsTruncated` | How many further candidates the `suggestions` cap left out — present ONLY when it left some out, the same shape `truncated`/`truncatedFindings` take. Both suggestion lanes disclose here: the substring pass above, and the nearest-key fallback `zzop endpoint` / `check_endpoint` run when the substring pass came back empty. |
| `disclosure` | Forwarded verbatim from the analysis output (the run-global registry below). |

`verdict` is a **sealed wire vocabulary** (`crates/facade/src/query.rs`), derived deterministically
from which join buckets contain a match: `edges` → `"linked"`, `unconsumedProvides` →
`"provided-only"`, `unprovidedConsumes` → `"consumed-unprovided"`, `unresolvedConsumes` →
`"unresolved-only"`, `externalConsumes` → `"external"`, `ambiguousConsumes` → `"ambiguous"`.
Exactly one class matching yields its token; two or more yield `"mixed"` (the `counts`
disambiguate); zero yield `"not-found"`. Each token's own one-sentence definition is not repeated here:
the reply carries it as `verdictMeaning`, from the one owner beside the computation.

## File queries: `queryFile`

`queryFile(analysisJson, queryJson)` answers "what does zzop know about THIS FILE?" — the second
targeting axis beside `queryIo`, and the same class of function: pure post-processing over an
ALREADY-PRODUCED `analyzeTrees` output, no re-analysis, no cache interaction, no filesystem access at
all. It is the one shared core behind the `check_file` tool and the `zzop file` CLI subcommand
([mcp.md](mcp.md)), so both hosts answer identically for the same analysis.

The axis is a file PATH because that is the target a caller already has — it just opened, wrote, or was
asked about a file — where `queryIo`'s target is an io key. **This surface drops nothing**: a single
file's symbols, io facts, edges and findings are bounded by the file itself, so there is no cap here and
therefore no truncation to disclose. The one capped list is a `not-found` reply's `suggestions`, which
ranks over every walked path rather than describing the target.

- `analysisJson` — the string `analyzeTrees` returned. A single-tree `analyze` output is a named error:
  the reply names the TREE a file was found in, and a single-tree output has no tree identity to report.
- `queryJson` — `{"path": "<target>", "sourceId": "<tree>"?}`. The target is matched against each tree's
  own relative paths: an exact tree-relative path, or an absolute path matched by its TAIL (longest
  match first, so `src/api/users.ts` never loses to `users.ts`); backslashes and a leading `./` are
  normalized away. This is a textual match, deliberately not canonicalization — the core never touches
  disk, so it resolves no symlinks and no `..`, rather than pretending to against a tree the analysis no
  longer has. Without `sourceId` every tree is searched.

The result (camelCase):

| Field | Meaning |
|---|---|
| `target` | The tree-relative path that matched (the resolved one, not the argument verbatim) — or, on `not-found`, the target as given. |
| `sourceId` | The tree the file was found in. |
| `otherTrees` | Present ONLY when the same relative path exists in more than one tree: the other trees' source ids. The answer comes from the first by tree order, and this field is what keeps that from being a silent pick — pass `sourceId` to choose. |
| `verdict` | ONE token from the sealed vocabulary `analyzed` / `lexical-only` / `degraded` / `not-found` (`FILE_VERDICTS`, `crates/facade/src/query_file.rs`). |
| `verdictMeaning` | One sentence saying what THAT returned token means, from the one owner beside the computation that assigns it — the same self-describing discipline `queryIo` uses, and the reason no host's help text or tool description defines these tokens. |
| `loc` | The file's line count, as the IR recorded it. |
| `symbols` | `{count, exported[]}` — how many symbols this file contributed and the names of the exported ones. |
| `io` | `{provides[], consumes[]}` — this file's own io facts, the original objects verbatim. |
| `dependencies` | `{imports[], importedBy[]}` — its position in the dependency graph, both directions. `importedBy` is the half a caller cannot read off the file's own text. The key names are unchanged; what they mean rides beside them in `dependenciesMeaning`. |
| `dependenciesMeaning` | One sentence saying what `dependencies` covers, shipped in the reply for the same reason `verdictMeaning` is: an EMPTY `imports` list has two readings and only one is right. The dep graph is [resolved-in-tree-only](#the-dep-graph-is-resolved-in-tree-only), so an empty list means "no in-tree import of this file resolved" — never "this file imports nothing". A file whose every import is a package or an unresolvable specifier has no edges here at all. The rule's own sentence has one owner and is not repeated in this document's `queryFile` section. |
| `findings` | `{total, bySeverity, byRule, list}` — every finding anchored in this file, the tree's own and the cross-layer join's merged into one uncapped list, with counts over that same list. |
| `suggestions` | Present ONLY on `not-found`: up to 10 walked paths, ranked by how each relates to the target's own basename (equal first, then containing-or-contained, then any path containing the whole target string) with length as the tiebreak — a deterministic ordering, never a fuzzy score. A target that relates to nothing walked gets an EMPTY list rather than a guess. |
| `suggestionsTruncated` | How many further candidates that cap left out; present only when it left some out, the same shape `queryIo`'s truncation fields take. |

**The verdict answers whether the file was ANALYZED, not whether it is healthy**, and that is the point
of the surface. An empty findings list means *clean* for an `analyzed` file and means *nothing
structural ever ran* for a `lexical-only` or `degraded` one — a caller asking about one file will
otherwise read silence as an all-clear. `analyzed` is assigned from the presence of a structural
projection (symbols and/or dependency-graph membership) and deliberately does not distinguish native
parsing from a Mode-B adapter overlay: for the question "does a projection exist", an overlay IS its
parser. Each token's own definition is not repeated here — the reply carries it as `verdictMeaning`.

## Structural drift: `zzop manifest` / `zzop diff`

Two pure JSON transformations over an `analyzeTrees` run, in the same "post-processing, one shared
core" class as `queryIo` above — but a layer out: they live in the shared `zzop-summary` crate
(`crates/summary/src/manifest/`), not in `zzop-facade`, and they are **CLI-only** (no MCP tool twin —
the reasons are recorded in [`docs/contracts/surface-parity.json`](../contracts/surface-parity.json)'s
`_cliOnlyLanes`). zzop produces manifests; **keeping** one is yours (commit it next to the code, the
same model as `scripts/max-file-lines-baseline.txt`) — no snapshot is ever stored, named or cleaned up
by zzop.

**Why they exist.** "Just diff two runs yourself" holds only *below* the caps. What a host ships is
not raw output, it is a capped summary (`crossLayer.edges` ≤ 200, findings ≤ 50,
`degraded` ≤ 50). Above a cap, two runs' texts still agree on the *counts* while saying nothing about
*which* route left the join. A manifest stays structurally readable there because it carries identity
and nothing else.

`zzop manifest <path> <path>... (2+ paths) | zzop manifest --config <zzop.config.jsonc>` — same two source modes and the
same analysis as `cross`, projected differently:

| Field | Shape | Why |
|---|---|---|
| `tool` | `version()`'s string (release version + every parser fingerprint) | Honesty gate 1's key — see `diff` below. |
| `sources[]` | `{sourceId, joinContributionZero, degraded}` | Honesty gate 2's key. Deliberately **no `root`**: an absolute path differs between a laptop and CI, which would make the two machines that most need to compare unable to. |
| `provides[]` | `{kind, key, source}` | The API surface each tree exposes (from `ir.io.provides`, the one place the full list lives). |
| `edges[]` | `{kind, key, from, to}` | Which tree calls which, by source id only. |
| `buckets[]` | `{bucket, kind, key, source}` | Membership in each of the five non-edge buckets. An unresolved consume has no key, so its `raw` expression is its identity (the same fallback `distinctBucketKeys` uses) — never guessed, never silently dropped. |

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
   both builds. Refuse or disclose, never silently compare. **The consequence, which belongs next to the gate rather than left to be discovered:** `tool` is `zzop_facade::version_string()` — the release number plus all eight parser fingerprints plus the engine fingerprint — so it moves on EVERY release, including one that changed nothing about extraction. A manifest committed as a baseline therefore has a lifetime of one release, and re-baselining is part of upgrading. README teaches the commit-and-diff workflow and now says this there too. ⚠ And `--allow-tool-drift` gives up more than noise tolerance: `parse_manifest` checks only that the top-level `sources`/`provides`/`edges`/`buckets` arrays exist, so nothing else compares the two files' ROW shape. The tool-identity refusal is the only guard against diffing across a manifest format change, which is why it must not become a habitual flag (review ledger V88).
2. **Blindness vs deletion.** A tree that got *less visible* explains disappearances by itself, so
   `blindnessSuspect` + `sources.coverageDropped` keep "the route vanished" from being reported when
   "we stopped being able to see it" is the honest reading.

Exit codes follow every other subcommand: 0 on success, 2 for an argument-shape mistake, 1 for a
runtime failure — which includes the gate-1 refusal, and a file that is not a manifest at all (a named
error naming *which* argument, never two empty relation sets read as "nothing changed"). `diff` does
**not** exit non-zero on a detected transition: a CI gate reads the JSON (e.g.
`jq -e '.transitions | length == 0'`), so "the contract broke" can never be confused with "the diff
itself failed".

## Custom rules, consumer side: `zzop facts`

zzop's *producer* extension point has been frozen for a while — an external parser emits a
Normalized-AST envelope ([NORMALIZED_AST.md](../NORMALIZED_AST.md), Mode A/B) and zzop ingests it. The
symmetric *consumer* side had nothing: your only custom-rule path was a DSL JSON pack, and past its
expressiveness you had to contribute to `rules/native/` and build zzop from source. `zzop facts` is
that missing half, built as the smallest mechanism that can work:

**zzop emits what it knows; your program decides what is a problem.**

```
zzop facts ./api ./web > facts.json && ./my-rule facts.json
```

zzop **executes nothing** — it never spawns your program, so scanning a repo can never mean running
code the repo's own config named. It **ingests nothing** — there is no channel for your findings to
come back in, so `disabledRules` does not (yet) reach them. Both are deliberate first-step boundaries,
not gaps waiting on a bug fix; the ingest half starts when someone who actually uses this asks for it.

`zzop facts <path>... | zzop facts --config <zzop.config.jsonc>` — the same three source modes as
`endpoint` (one path, 2+ paths, or a config), because a rule author with one repo should not have to
invent a second tree: the cross-layer join runs fine over a single source, intra-tree edges included.
Like `manifest`/`diff`, it lives in `zzop-summary` (`crates/summary/src/facts.rs`) and is **CLI-only** —
the no-MCP-twin reasoning is recorded in
[`docs/contracts/surface-parity.json`](../contracts/surface-parity.json)'s `_cliOnlyLanes`.

**Stage: post-assembly.** These are the tree-wide facts *after* assembly and the cross-layer join —
router mounts, controller prefixes and tRPC composition are already applied. That is not a taste call:
per-file results participate in the engine's cache fingerprint, and there is no honest fingerprint for
*your* program (its mtime? its bytes? its transitive deps?), so a per-file hook would be a stale-result
generator. Post-assembly needs no fingerprint at all.

| Field | Shape | Notes |
|---|---|---|
| `tool` | `version()`'s string | Release version + every parser fingerprint. A rule keyed on a fact shape needs to know which build produced it — an extraction improvement on our side can move keys with no change to your code. |
| `config` | `string \| null` | The `zzop.config.jsonc` actually honored, or `null`. |
| `configWarnings` | `string[]` | The config-honesty channel — loader warnings first, then each tree's analysis-time entries. |
| `trees[].sourceId` | `string` | Request order, never re-sorted (see *Determinism* below). |
| `trees[].coverage` | `object` | The per-tree census, including `joinContributionZero` — read it **before** trusting a zero. |
| `trees[].warnings` | `string[]` | That tree's engine self-reports (framework silence, an ineffective topology host, the tRPC mount-route suppression note, the wildcard-route partition note). |
| `trees[].commonIr` | `CommonIr` | The whole IR: `source`, `parser`, `dep`, `symbols`, `loc`, `io` — with file and line intact, which is exactly what `manifest` strips and what a rule program needs to report a location. |
| `crossLayer` | `CrossLayerResult` | All eight buckets, verbatim and **uncapped**: `edges`, `unconsumedProvides`, `unprovidedConsumes`, `unresolvedConsumes`, `externalConsumes`, `ambiguousConsumes`, `hostRekeyCounts`, `wildcardRoutePartitions`. |
| `warnings` | `string[]` | Run-level self-reports belonging to the join itself, not any one tree. |
| `disclosure` | `object[]` | The run-global blindness-class registry. Carried here even though it is a build-time constant: this is the one surface where the reader writes their own verdicts, so what zzop is structurally blind to belongs next to the facts. |

**Every key is always present**, including empty ones. A capability that can silently produce nothing
must positively confirm it ran — the same rule `packsLoaded` follows (`[]` is the honest "zero packs"
signal) and the `capability-absent-vs-empty` disclosure class states ("a present output field means the
capability ran"). Concretely: `commonIr.io` is materialized to `{provides: [], consumes: []}` where the
engine omits the optional field, and every `crossLayer` bucket is materialized to `[]`. You never have
to read an absent key as either "zero" or "did not run".

**Determinism.** Byte-stable for the same input. Everything set-shaped is already ordered upstream
(`dep`/`loc` serialize through a sorted map, `symbols` follows the file pass's sorted-by-path
invariant, `io` is `(kind, key, file, line)`-sorted, every join bucket is sorted by the linker).
`trees` deliberately keeps **request order** rather than being re-sorted by `sourceId`: the
`crossLayer` buckets are themselves accumulated in tree order, so re-sorting only the tree array would
publish two contradictory orders inside one document.

**Not carried, on purpose:**

- **`findings` / `crossLayerFindings`** — those are zzop's *verdicts*, not facts. You compute your own;
  carrying ours would put the same data on two surfaces under two different caps, which is the exact
  drift class the surface-parity registry exists to prevent. Every input our own cross-layer rules read
  *is* here, so they can be re-implemented rather than only inspected: `cross-layer/unconsumed-endpoint`,
  for instance, needs `crossLayer.unconsumedProvides` (kind/key/source/file/line), the
  `unresolvedConsumes` count behind its blindness caveat, and `edges` (from which its tRPC-participation
  exclusion is derived) — all three are emitted.
- **`AttributeStore`** — the one post-assembly fact whose *container* is not already a serialized wire
  shape (its element `Attribute` is, and is already an envelope input channel). Emitting it would freeze
  a **new** shape, and a new shape ships with a consuming rule or not at all. Its absence is a decision,
  not an oversight.

The per-tree IR rides under `commonIr` rather than the engine's own `ir` field name — a deliberate
choice, recorded so it does not look accidental. `commonIr` camel-cases the exact type you need to look
up to read the block (`CommonIr`, whose field list is the row above), and it stays greppable in your own
codebase, where the two letters `ir` are a substring of `circular`, `directory` and `require`. The cost,
accepted: embedding `zzop-facade` directly gives you the same block under the key `ir`, so moving
between the two surfaces means carrying one mapping.

`AnalyzeOutputView` (`camelCase`, a zero-copy borrowing view) is the shape every successful `analyze`/
`analyzeEnvelope` call returns:

| Field | Type | Meaning |
|---|---|---|
| `ir` | `CommonIr` | The language-neutral IR — see [Output data shapes](#output-data-shapes) below. |
| `findings` | `Finding[]` (merged, sorted) | See [Output data shapes](#output-data-shapes) for the `Finding` shape and sort order. |
| `degraded` | `string[]` (sorted) | Paths that hit the size cap or failed to parse — see [ARCHITECTURE.md](../ARCHITECTURE.md#degraded-files). |
| `buildScriptPaths` | `string[]` (sorted) | Files this tree's own `package.json` manifests name from a NON-run `scripts` command, resolved against the walked file set — the tree declaring, in its own manifest, "this is how I am built, not what I ship". A token named by an npm RUN-lifecycle key (`prestart`/`start`/`poststart`, `prerestart`/`restart`/`postrestart`) is deliberately NOT here: that is the package's run entry, and it joins the `main`/`bin`/`exports` side instead (`crates/engine/src/pipeline/manifest.rs::is_run_lifecycle_script_key` owns the key set and the measurement behind it). **ALWAYS serialized, `[]` included** — the same "an empty array is the honest signal" convention `packsLoaded`/`warnings`/`configWarnings` use: `[]` means the manifest walk ran and no manifest declared a resolvable build-script path (the state of every tree outside the npm ecosystem, by construction), and hiding it would make that indistinguishable from an engine build that does not report the field. A plain fact, not a verdict: the one consumer is `zzop-summary`'s finding ORDER, whose deployment-role tier sorts findings on these files after production and test-path findings of the SAME severity and discloses the demotion as `findings.buildPaths` (see [mcp.md](mcp.md#output-contract)). Nothing is dropped and no count moves. Not carried on the shaped reply — the path list would be a second, larger copy of a fact the ordering already applied; `findings.buildPaths` carries the count and the sentence. |
| `fileCount` | `number` | Files walked. |
| `nodes` | `FileNode[]` | Per-file git/graph metrics (churn, fan-in/out, risk score, ...) — populated fully only when `git` is set. Its `fanIn`/`fanOut`/`totalConnections` are measured over a graph that is [resolved-in-tree-only](#the-dep-graph-is-resolved-in-tree-only). `riskScore`/`hotspotScore` are always `0` for non-source files (data/config/assets — anything outside the "Language support" table in [ARCHITECTURE.md](../ARCHITECTURE.md#language-support)); `churn`/`loc`/`changeCount` stay real for them, so a large data file's edit history is still visible without it dominating a risk-sorted view. |
| `scores` | `object \| null` | 15 structural health sub-scores, 0–100; `null` unless `git` is set. **Every score carries the POPULATION it scored over, in the same object** (2026-08-08 ruling) — `featureSlicedDesign.layerClassifiedImports` (imports touching a DECLARED FSD layer — its ratio denominator `totalImports` counts every import and so cannot say whether the tree adopted the convention at all; zzop's own tree scores 100 on 2,795 imports with 0 of them FSD-classified), `cohesion.sliceCount`, `coupling.importerCount`, `sdp.totalCrossSliceEdges`, `hierarchy.totalIntraModuleEdges`, `publicApi.totalCrossModuleImports`, `fileSizeCompliance.total`, `mainSequence.classifiedFiles`, `modularity.edgeCount`, `godFile.total`, `siblingCross.totalIntraModuleEdges`, `diamond.rootsExamined`, `renameInstability.total`, `busFactor.total`, `fixRatio.taggedFileTouches`. **A population of `0` IS the never-measured signal**: every per-metric formula returns 100 on an empty population, so without the denominator "judged 4,000 subjects and all passed" and "found nothing it could judge" were the same number. No separate flag rides beside it — a flag would be a second owner of the same fact, free to disagree with the count it describes. The denominator is a FIELD rather than a caveat sentence for the reason `queryCoverage`'s `unmeasured` array is one: prose gets dropped in transit, a field does not. Each is a JUDGED-POPULATION total, not a tree total, and two gates narrow it: the config's top-level `exclude` removes a path from both the violation list and the denominator (see `globalExcludes` above; the 4 slice/module-keyed metrics take no such gate), and `fileSizeCompliance`/`godFile` additionally judge source files only. **`mainSequence.classifiedFiles` is `0` on every current build** — nothing in zzop classifies a file as abstract or concrete, so read its `score`/`avgDistance` and each row's `abstractness`/`distance` as unmeasured, and read only `instability`/`fileCount`. **Removed 2026-08-08: `typeSafety` and `lod`** — neither input channel had a producer anywhere in the build, so both published `score: 100` (and `typeSafety.totalAsCast: 0`) on every run ever made, TypeScript repos included; building the producer is the precondition for their return. **What each score key MEANS rides beside it in `scoreMeanings`** — read that rather than inferring from the key, especially for `sdp`. |
| `scoreMeanings` | `object` | One sentence per `scores` key, keyed identically. Present exactly when `scores` is — a legend for numbers that did not run explains nothing — and absent (not `null`) otherwise. Added 2026-08-04, after a name survey measured that four score keys were bare acronyms whose expansion existed only in Rust doc-comments. Two of the four were then renamed on the wire — `sfc` → `fileSizeCompliance` and `fsd` → `featureSlicedDesign` — because their letters did not name what the metric measures; `sfc` in particular read as Vue's Single-File Component, which this repo's other docs legitimately use `SFC` for, while the score is a LOC-cap compliance ratio. Of the two that abbreviated PROPER NAMES, `lod` (Law of Demeter) left with its score on 2026-08-08, so `sdp` (Stable Dependencies Principle) is the last key the legend expands rather than renames. A sentence still ships for every key, and a sentence may never outlive its score — both directions are pinned against serde's own key set, so a removed metric cannot leave its legend behind. Every sentence answers what a LOW number means, because every score is 0–100 with higher being healthier and that direction is the first thing a reader gets wrong. Same self-describing-reply device as `verdictMeaning`, chosen over a table here for the same reason: a table would give one fact a second owner and nothing would make it move when a score does. |
| `health` | `object \| null` | One composite index rolled up from `scores`: `{pain, axisPain[], measuredWeight, totalWeight, contributors[]}`. **`pain` contains no rule findings and is mostly not a defect claim at all** — `axisPain[]` (added 2026-08-12) splits it by `HealthAxis` into `defect` (import cycles, the only entry), `opinion` (barrel discipline, FSD layering, SDP/Main Sequence, Newman modularity, LOC ceilings) and `history` (rename churn, bus factor), each on `pain`'s own scale and summing to it; recount the split with `axis_weight` rather than quoting a percentage. The axis lives in the weight table itself (`HEALTH_METRIC_WEIGHTS`'s third column), so a new metric cannot ship without declaring which kind of claim it makes, and every `contributors[]` row carries its own `axis`. **`pain` renormalizes over the MEASURED metrics only** — `raw x totalWeight / measuredWeight`, where a metric enters only if its population is non-zero — so it stays on one 0–186 scale whether a tree could measure fourteen axes or three. Before 2026-08-08 it summed all fourteen unconditionally, and because an unmeasurable metric scores 100 it contributed 0 pain and was then dropped from `contributors`: **an axis zzop could not measure made the repo look HEALTHIER**, byte-identically to a clean one (measured on a Go tree, where `featureSlicedDesign` — weight 2.5, the second highest — is defined over a directory convention Go does not use). **`pain` is `null`, never `0`, when `measuredWeight` is 0**: absence of data, the same `null`-vs-`[]` convention `coChange` uses. `measuredWeight`/`totalWeight` ship as fields so the scalar always travels with the denominator saying how much of the structure it describes — `queryCoverage` forbids a bare folded score for exactly this reason, and `pain` is that shape. `contributors[]` rows are `{metric, weight, population, gap, contribution}` ranked by contribution, and now KEEP every unmeasured metric (`population: 0`, `gap`/`contribution` `null`) instead of filtering it out; measured-and-clean stays dropped, since it explains nothing. `contribution` is RAW points before renormalization, so the rows sum to `pain` only when every metric was measured. |
| `recommendations` | `object[]` | ROI-ranked improvement suggestions. An item whose file carries a rule-confirmed critical finding is moved (never copied) into a synthetic `urgent-bug-risk` group that sorts first, and gains a `bugEvidence: string[]` explaining why — this never changes the item's `roi` number, which always stays a pure reduction/cost ratio. That ratio is an ORDERING WEIGHT, not an estimate in any unit: `estimatedReduction` is `base_risk × reductionRatio × severityMultiplier` with both multipliers hardcoded per rule id and per severity and no calibration behind them, `base_risk` itself is an unnormalised sum of a commit count, a line count and an edge count (lines dominate on any real file), and `estimatedCost` is `max(10, loc + fanIn × 3)` — a size proxy, never measured against how long a fix took. So `roi` is dimensionless and comparable only within one reply's item list; ranking by it ranks small files up, which is the intended cheap-wins-first tilt and a preference rather than a finding. **Each entry carries `itemsTruncated`** — how many rows that rule's own cap dropped, `0` on a complete list and always present. Its scope is the cap and ONLY the cap: `items.length + itemsTruncated` is what the rule PRODUCED, not what the tree holds, because config excludes are applied after the cap (so an excluded path can still consume a cap slot) and the `urgent-bug-risk` move relocates rows rather than dropping them. That synthetic group reports `0` because it has no cap of its own. |
| `critical` | `object[]` | Files ranked by **size-weighted blast radius** — `blastRadius * ln(loc + 2)`, with `blastRadius` as the tie-break (`crates/metrics/src/criticality.rs`), because a 5-line re-export barrel and a 400-line core of equal blast are not equal danger. `blastRadius` itself is the transitive-dependent count, over a graph that is [resolved-in-tree-only](#the-dep-graph-is-resolved-in-tree-only), so it counts in-tree dependents. **Re-sorting this array by `blastRadius` alone yields a different order** and does not reproduce the summary's `architecture.criticalTop`, which is simply this array's first 3 paths. **The array is capped at 20, and `criticalTruncated` beside it counts what that cap dropped** (`0` = complete, always present). That cap was silent until 2026-09-04 — and since `criticalTopMeaning` publishes only the THREE paths it lifts, a reader who drilled into this array met a 20-row wall no sentence in the reply named. |
| `seams` | `object[]` | Folders that are good first-extraction candidates (low boundary-crossing coupling). `files` is the count of that folder's **dep-graph keys**, not of files walked or of files on disk: the computation's only input is the dep graph, so a walked file no import edge names is in no seam count, and noise folders (`tests`, `dist`, `docs`, ... — `crates/metrics/src/seams.rs`) are skipped whole. `temporalBoundary` is likewise a filtered figure — its substrate keeps only commits touching 2–25 files and only each file's top 10 co-change partners, so it is "the strongest measured cross-folder co-change", not the folder's total temporal coupling, and `0` on a git-less run means nothing was measured. |
| `folders` | `object \| null` | Folder-granularity rollup of `nodes`/the dep graph, **at a fixed depth of 2 leading path segments** (`features/alpha`, `src/main`) — the granularity is not configurable and is not echoed in the payload, so read every row under it. This depth suits layouts that put module names in the first two segments and **collapses the ones that nest their source root deeper**: a Maven/Gradle tree (`src/main/java/<group>/<artifact>/...`) or a .NET `src/<Project>/...` tree lands its whole source root in the single row `src/main`/`src/<Project>`, and its cross-package import edges roll up to self-edges that are dropped — so a near-empty `edges` there means "no boundary is visible at this depth", never "no module depends on another". For a granularity you choose, use `zzop graph --domain dep --fold <n>`. Nothing in `scores`/`health` reads this rollup; it is a summary view only. Not git-gated — `nodes`/dep graph are built unconditionally, so this is always non-null (an empty tree still gets an object with empty arrays, never `null`). Each row's count is `nodeCount`, **not** `fileCount`, and the difference is the point: the top-level `fileCount` counts files WALKED, while a `FileNode` exists only for a dep-graph key or a git-touched path, so with `git` off every lexical-only file has no node and belongs to no folder row. **Summing `nodeCount` over the rows does not reproduce `fileCount`, on a perfectly healthy run.** (Renamed from `fileCount` 2026-07-31 — one reply carrying that word at two levels over two universes read as a rollup that should add up.) |
| `layerCoChurn` | `object[] \| null` | Cross-layer commit co-churn pairs (files in different architectural layers that change together). `null` unless `git` is set and collection succeeded — same git-gating as `scores`/`health`; `[]` (not `null`) when git is active but no pair meets the co-change threshold. **Every path here is relative to the ANALYZED TREE, the same as `nodes`/`dep`/`folders` — including when the tree is a subdirectory of its git repository.** History is collected for the whole repository (one collection is shared by every tree of a run), then rebased onto each tree root, and a commit that touched nothing inside the tree is dropped rather than counted; so the co-change window ("2–25 files") counts a commit's files IN THIS TREE, and a monorepo package never inherits its sibling's history. Before 2026-08-13 it did: a subdirectory tree received the enclosing repository's pair list verbatim. **`coChanges` is a SUBSET total, not a repository total**: commits touching fewer than 2 or more than 25 files are skipped as noise (a 1-file commit couples nothing; a mass rename would couple everything with everything), so the honest reading of `coChanges: 40` is "40 filtered co-changes", never "these two layers changed together 40 times". Pairs below 2 co-changes are then dropped and only the top 20 rows are returned. |
| `gitWindow` | `{ recentDays: number, since: string \| null } \| null` | Echoes the resolved git-history collection window — ALWAYS serialized (unlike `ruleOverridesApplied`'s omit-when-untouched convention); `null` on the wire IS the "git didn't run" signal (`git` not set, or collection failed), same gating as `scores`/`health`. When non-null: `recentDays` is always a resolved number (the caller's value, or the engine's `30` default when omitted); `since` is the caller's raw filter string (e.g. `"1.year"`, an ISO date) verbatim, or `null` when omitted (full history). |
| `packsLoaded` | `{ id, rules, ruleIds, source, filesInScope?, filesInScopeIfEnabled?, zeroAdmissionRules?, didNotRun? }[]` | Positive pack-load confirmation: one entry per loaded DSL pack (sorted by `id`), with its rule count as loaded and its provenance — `source` is `"dir"` (read from a `packsDir` directory) or `"inline"` (`packDefs` — how `zzop-config`'s bundled defaults arrive for `zzop-mcp`). `filesInScope` counts the files this tree has that a pack's rules WOULD scan by `file_pattern` candidacy alone (see [rules/dsl-reference.md](../rules/dsl-reference.md)), computed before any content/pattern check runs — it is never a "matched" or "found N usages" count, and it deliberately does NOT subtract per-rule `file_exclude_pattern` vetoes (a pack-level number cannot honestly fold in which rule vetoed what, so it is an upper bound on candidacy; the rule-level field below does consult them). A large `filesInScope` (e.g. every `.java` file in an all-Java tree) means "eligible", nothing more; pair it with zero findings to read "this pack ran, found no evidence" (`filesInScope > 0`, zero findings) versus "this pack has nothing to say about this tree" (`filesInScope: 0`, e.g. a redis pack over a tree with no redis-shaped file paths at all). `zeroAdmissionRules` is the same census one level down — the sorted ids of THIS pack's rules whose own path gates (`file_pattern` AND that rule's `file_exclude_pattern`) admit zero analyzed files. A rule listed there could not have read a single byte of this tree, so its zero findings mean scope, never "checked and clean" — the distinction `filesInScope` cannot make within an in-scope pack (a pack with 100 eligible files can still carry a rule scoped to `(^|/)migrations/` on a tree with no such directory). Content gates (`require_file*`) are deliberately NOT part of admission: a rule whose content probe rejected N files did judge those N files. In envelope mode (`analyzeEnvelope`) the census is additionally MODE-filtered: a rule whose matcher kind that mode never evaluates (anything but symbol-scan/io-scan — an envelope carries no source text) is listed regardless of what its path gates match, because it read nothing there and its green is vacuous — so in that lane a listed id is "this rule could not judge this input", not necessarily a path-scope fact. Present only when non-empty; omitted on a `filesInScope: 0` pack too (every rule of it is trivially zero — the pack-level zero already says "all of them"). `ruleIds` is the LIST behind the `rules` COUNT — every id this pack loaded with, SORTED (the same ordering its neighbour `zeroAdmissionRules` uses, so the two can be subtracted without re-sorting either — it shipped in pack-declaration order for one review cycle and this sentence was the half that did not follow the change), ALWAYS present (an omitted list would read as "this build declines to say", which is the one state a `--rule`/`rule` filter validator must be able to tell apart from "no such rule"). It exists because the reply's only statement about what a run could report was a number: `zzop analyze <tree> --rule security/no-such-rule` exited 0 with an empty stderr and `shown: 0` — a typo inside a LOADED pack, byte-identical in a CI log to a clean rule — while `--rule zzz/qqq` (an unloaded pack, answerable from the `id` alone) correctly exited 2. Validating against the catalog compiled into the binary instead would falsely refuse a rule from a user pack loaded out of `<tree>/zzop/rules/`, so only the run's own ids are right for both pack sources. Per-rule lever: `disabledRules: ["<pack>/<rule>"]`. Always present (the array); `[]` is the honest "zero DSL packs loaded" state (the same condition the `warnings` self-report names). **`didNotRun` is where loading stops being the whole story (2026-08-26).** A pack the run gated off still appears here — it did load — but it is now marked: `"disabled"` (its id is in `disabledRules`; config `packs.disabled`, or a `rules` entry set to off) or `"notAllowlisted"` (a `packsOnly` allowlist is in force and does not name it). Absent on a pack that ran, so an ungated run is byte-identical to what it was before the key existed. The same row then publishes `filesInScopeIfEnabled` INSTEAD of `filesInScope` and drops `zeroAdmissionRules`, which is the point: a positive scanned-file count and a 51-id rule list on a pack that read nothing was measured being read as "this pack ran over 9 files and found nothing — clean" for a `security` pack that never ran once. The presence of `filesInScope` is the claim that the pack ran; `filesInScopeIfEnabled` states the same census as a counterfactual (what re-enabling would buy). Both this field and `ruleOverridesApplied` are needed and neither replaces the other — that one reports which entries of your REQUEST took effect (a typo’d id appears nowhere), this one reports what happened to each LOADED pack (which is why an all-typo `packsOnly`, the shape where every DSL rule goes silent at once, shows up here as every row marked `notAllowlisted` while `ruleOverridesApplied.only` is `[]`). A `filesInScope: 0` pack is the one you can act on: `packs: { disabled: ["<id>"] }` drops it, and the run already names every such pack in one `warnings` line (above) so you do not have to scan this array yourself. |
| `packsLoadedMeaning` | `object | absent` | The legend for the array above, added 2026-08-26 when an audit found it was this reply’s one numeric channel shipping with no statement of what it measures. Build-constant sentences keyed `row` (a row is a LOADED pack, and loading is not running), `filesInScope` (path candidacy, never a match count, and its presence is the claim that the pack ran) and `zeroAdmissionRules` — that last one because “admission 0” has two readings whose implications are OPPOSITE: “no analyzed file ever reached this rule” (its zero is scope) versus “files reached it and nothing fired” (a clean bill). It has always meant the first, and now says so. A fourth key, `didNotRun`, appears only when a pack really was gated off — a legend entry for a state this run is not in is noise, and noise is what teaches readers to skip disclosures. OMITTED entirely (never an empty object) when no pack loaded: an empty roster has no entries to explain, and `packsLoaded: []` already carries that fact. A sibling key rather than a `meaning` inside the object because `packsLoaded` is an ARRAY — the only “inside” it has is a row, and the legend would then repeat once per loaded pack; same shape as `scoreMeanings`. |
| `ruleOverridesApplied` | `{ disabled: string[], severityRemapped: string[], only: string[] }` | Positive confirmation that `disabledRules`/`severityOverrides`/`packsOnly` were applied: `disabled` lists the affected rule ids, `severityRemapped` likewise for the severity remap, and `only` the pack ids of an honored allowlist (config dialect `packs.only`). Omitted (or empty) when none of the three was requested — a consumer must treat an absent key the same as "no overrides," never as `null`. `only` is the one to read before calling a rule family clean: packs outside it never ran, and `packsLoaded` does not disclose that (it is a path-match census, identical under every one of these knobs). A present-but-empty `only` means an allowlist was set and named no loaded pack — the shape in which every DSL finding disappears at once. |
| `warnings` | `string[]` | Non-fatal issues (e.g. a bad `packsDir`) plus the capability self-report notes — see [Defaults](#defaults-a-config-is-required-what-it-does-not-have-to-say). |
| `configWarnings` | `string[]` | Config-authoring problems computed at analysis time, kept OUT of `warnings`: a `disabledRules`/`severityOverrides` entry matching no known rule id did nothing, and is reported here instead — only analysis time has the known-rule-id set (native analysis ids + loaded DSL pack ids) a config parser never sees. **An unmatched id is not necessarily a typo**, and this row said it was until 2026-08-13: besides a misspelling or a stale id from a different zzop version, the entry may name a REAL id belonging to a pack this build SHIPS BUT DOES NOT LOAD — the shape a v0.29.0 config takes unchanged into v0.30.0, which exported four packs. The message on the wire spells the readings out and prescribes the retrieval that fixes that one; `crates/metrics/src/diagnostics/config_reports.rs` owns that wording for all four unknown-id reports, so read the string you received rather than a second copy of it here. Always present; `[]` means neither knob had a matching-nothing entry. A `suppressions` entry with the same problem is unaffected by this split and still reports on `warnings`. This host's own `zzop-config` crate (see [Config semantics](mcp.md#config-semantics)) attaches ITS OWN parse-time config problems (unknown config keys, a malformed overlay) to the same `configWarnings` name on its own reply; this facade-level field is the analysis-time half of that one channel, never a rename of `warnings`. |
| `cache` | `{ hits, misses } \| null` | Set only when `cacheDir` was given. |
| `ruleTimings` | `object[] \| null` | Per-rule id + elapsed time + finding count; set only when the caller requests profiling. |
| `coverage` | `object` | Per-tree coverage census — always present. See below. |

`coverage` fields (all plain counts over this tree, always present — a `0` means "counted and found
none", not "not run"):

| Field | Type | Meaning |
|---|---|---|
| `files` | `number` | Files walked (same as `fileCount`) — every file under the root, including docs, data and assets; the walk applies no extension filter. See `parserDispatched` for the code subset. |
| `parserDispatched` | `number` | (RENAMED from `sourceFiles`, 2026-07-31 — the old name read as "is source code", which `.sql` also is; this name states the membership rule.) The subset of `files` a native frontend dispatched on, or that an applied overlay covers. Dispatch is by extension, so a file that hit the size cap and fell back to a lexical count, or that failed to parse, still counts here — this is "analysis had a frontend for it", not "analysis extracted structure from it". `degraded` reports the parse failures separately. `files - parserDispatched` is the docs/data/asset remainder, which is why a repo can report thousands of walked files and far fewer analyzed ones. Caveat: Mode A/B envelope ingest sets this equal to `files`, so `files == parserDispatched` on an injected run is an identity of construction, not a coverage signal. |
| `symbols` | `number` | `SourceSymbol` entries extracted (`ir.symbols[]` length). |
| `resolvedImportEdges` | `number` | (RENAMED from `importEdges`, 2026-07-31 — same standard as `parserDispatched` above: the name must state the membership RULE. The old one claimed the tree's imports; the number is the tree's *resolved* imports, and nothing on the wire said so.) Sum of `ir.dep` out-degrees: one count per `(importing file, imported file)` pair the resolver mapped to a file this walk visited — an edge count, not an importing-file count. **Package imports and unresolvable specifiers are excluded**, because they never enter `ir.dep` (see [the dep graph is resolved-in-tree-only](#the-dep-graph-is-resolved-in-tree-only)). A 91-file Python tree reporting `3` is saying "3 of its imports landed on files in this tree", not "this repo barely imports anything". |
| `declaredImportsByExt` | `object` (extension → `number`) | The declared-side **denominator** for `resolvedImportEdges`, per extension (lowercased tail after the last `.` — the same grain `queryCoverage`'s table groups by): the sum over that extension's parsed files of each file's *distinct* declared import specifiers (import/use/using bindings, re-exports, dynamic `import()`), counted **before** resolution — so package imports and specifiers no resolver could map are still in it. Deliberately **not 1:1** with `resolvedImportEdges` in either direction: a declaration is a specifier, an edge is a resolved `(importer, file)` pair — several specifiers can land on one file, and one glob import (`import com.foo.*`) can fan out to several edges. **An ABSENT extension key means "never measured" — never 0**: the parser projects no import channel at all (prisma/sql, `.vue`/`.svelte`, lexical-only files), or the tree was ingested as a Mode A envelope (`{}` — that path measures nothing here). A parsed file with zero imports contributes a real measured `0` to its extension's sum. This is the field that makes the 91-file/3-edge shape legible without an adapter: `"py": 120` next to `resolvedImportEdges: 3` says the imports were *declared* and did not resolve. `zzop coverage` reads it into its per-extension table as the `declaredImports` cell (`null` = unmeasured). |
| `ioProvides` | `number` | `ir.io.provides` entries. |
| `ioConsumesKeyed` | `number` | `ir.io.consumes` entries whose key resolved statically. |
| `ioConsumesUnresolved` | `number` | `ir.io.consumes` entries whose key could not be statically determined — **`key: null` only**. This is NOT the size of the join's `crossLayer.unresolvedConsumes` bucket, which carries the same word and a wider membership: that bucket is a **superset**, adding every keyed but placeholder-only route (`GET /{}`, the head-drop artifact of an unresolved `${BASE}` interpolation — `zzop_core::CrossLayerResult::unresolved_consumes`). The two agree exactly on a tree with no head-dropped interpolations, which is why they read equal today; the gap that opens when one appears is the second shape being counted where it is known, never a join defect. |
| `degraded` | `number` | Same count as `degraded.length`. |
| `joinContributionZero` | `boolean` | `true` when this tree analyzed files>0 but extracted zero JOINABLE io (0 `ioProvides` and 0 keyed consumes — unresolved consumes don't count, they cannot join) — the active-blindness fact: this tree is structurally invisible to `analyzeTrees`'s cross-layer join, so any join finding referencing it (`unconsumedProvides`/`unprovidedConsumes`/edges) is not meaningful for it. A framework/SDK client the extractor cannot see is a common cause; see `adapterOverlays` above (Mode B) to restore visibility. |

### `coverageGaps` — the census's per-extension companion, on the SHAPED summary reply

The census above is tree-wide, and that is exactly what it cannot answer: **which** of a tree's
principal filetypes the resolved dependency graph does not contain. `zzop analyze <path>` (and the
`analyze_repo`/`analyze_envelope` MCP replies, which share one shaper) therefore carry a
`coverageGaps` object beside `coverage`. It is a summary-layer field, like `architecture` — the raw
`zzop-facade` output has no such key, because embedders receive `ir` and can compute it.

| Field | Type | Meaning |
|---|---|---|
| `extensions` | `{ext, files, structural, kind}[]`, extension-ascending | One row per extension contributing **zero** resolved import edges while being a principal filetype: at least `MIN_UNCOVERED_EXTENSION_SHARE_PCT` (10%) of the tree's walked FILES. (A LINE share stood beside that leg until 2026-09-01, and a largest-file exclusion was tried on the line share itself on 2026-08-20 and reverted the same day. Both removals are the same policy: how a filetype spreads its lines, and how many lines the tree's OTHER filetypes carry, are not evidence about whether anything read it — a `package-lock.json`-shaped row is disclosed by its `kind` instead of erased. See the two sections below for what the line leg cost, measured over nine public trees.) Or — for an extension that parsed — at least the same share of the files with a structural projection, plus a non-zero `coverage.declaredImportsByExt` entry. The first leg — the NO-PARSER kind, `structural: 0` — carries a third condition the second does not: the filetype must be one where a dispatch-`None` CAN have cost something (the engine's own `extraction_can_lose_facts` test, borrowed through `zzop_facade`), so prose, stylesheets, images, fonts, media and archives are excluded outright while SOURCE languages and DATA/CONFIG formats both qualify. That test asks about a STRUCTURAL projection, not about every fact a file can hold: since 2026-08-20 a `.md` page's `<script setup>` block DOES contribute `import` lines to the dep graph (VitePress/Nuxt Content compile each page as a Vue SFC), and since 2026-08-21 a `.mdx` page's BARE top-level `import` statements do the same (measured: 175 of the dogfood corpus's 218 `.mdx` files carry one — `find corpus ( -name .git -o -name node_modules -o -name .zzop ) -prune -o -type f -iname '*.mdx' -print0 | xargs -0 grep -l '^import ' | wc -l`), as in-edges only in both cases — their symbols and io stay unprojected, which is why prose remains on the excluded side of this gate rather than moving and are told apart by the row's `kind` (`"source"` | `"data-config"` | `"unclassified"` — the third was added 2026-09-06: the residual used to be spelled `"source"`, so a filetype this build simply has no classification for, a `.gitignore` among them, was published as a missing parser). The parsed leg does not ask that question, because a parser did claim those files and there is no dispatch-`None` to price. The gate was `is_non_source_extension` for one review cycle, which answers the DIFFERENT question the "bring a parser adapter" warning asks, and it suppressed the largest extraction gap in the dogfood corpus — macrozheng/mall's 114 `.xml` MyBatis mappers (15.8% of files, 11.5% of lines, 906 SQL statements) produced an empty list on this surface AND on `unreadExtensions`. A `data-config` row is a place to LOOK, not a verdict: an i18n `locales/*.json` directory and a mapper directory are the same row until the reader opens them, and this build does not read them to find out. `structural: 0` covers TWO causes with opposite remedies — no parser claimed the extension, or one claimed every file and bailed; `coverage.degraded` tells them apart (directus: `{"ext": "vue", "files": 587, "structural": 0}` beside 306 `unimported-export` and 126 `dead-candidates` findings); `structural > 0` is the parsed-but-unresolved kind (gogs: `{"ext": "js", "files": 173, "structural": 172}` beside 173 `dead-candidates`, one per file). |
| `basis` | `string` | What the cross was computed from — the extension count and how many files contribute at least one resolved edge. It exists so an EMPTY `extensions` cannot be read as "never measured", the same job `blindSpotBasis` does for `blindSpots` in `queryCoverage`. |
| `meaning` | `string` | What a row IS, what its two `kind` values mean, and what a `structural: 0` can and cannot mean — riding inside the object it describes (same device as `ruleTimings`/`architecture.painMeaning`), because `kind` is a token in the rows above and this is where a reader meets it. FOLDED since 2026-09-01: the clauses a reader does not need AT the row (why the measuring stick is the file count and only the file count, which filetypes are excluded from the list outright, how this cell relates to `queryCoverage`'s `unreadExtensions`) are byte-identical on every run and for every repository, so they ship once from the `reply-legends` contract document — `zzop contract reply-legends`, MCP resource `zzop://contract/reply-legends` — which this sentence names in both dialects. |

**Always present** on every shaped analyze reply, empty list included — `queryCoverage`'s
always-present norm, not `architecture`'s absent-when-it-did-not-run one, because the absence of a gap
statement is precisely the ambiguity this field exists to remove.

**One share, and it is the FILE share** (since 2026-09-01). A LINE share stood beside it until then,
and the pair was argued in both directions at once on gogs: `.png` at 33.9% of the files and 6.6% of
the lines said "not the file axis alone", `.ini` at 12.7% of the lines and 1.3% of the files said "not
the line axis alone". Only the second defends a leg that is still here. The first stopped being the
line leg's work on 2026-08-20, when `extraction_can_lose_facts` was added underneath the share test
and excluded images, fonts, prose, media and archives outright.

What the line leg cost was then measured over nine public trees: it withheld the two largest unread
SOURCE populations in that set — immich `.svelte` (415 files, 12.1% of files) and nocodb `.vue` (962
files, 21.2%), both `structural: 0`, both named by this reply's sibling `unreadExtensions` — because
those trees' LINE totals are led by things nobody wrote (immich: `.ttf` 22.57% across 19 files, `.png`
17.68%; nocodb: a vendored `.sql` dump at 56.35% across 27 files). The premise the second leg rested
on is inverted by that: the noise this cell fears (lock files, vendored dumps, generated bundles,
fonts, `.pdb`) concentrates in LINES, not in FILES. Removing the leg added exactly those two rows
across the nine trees and no third row; `total`, `bySeverity` and `byRule` were unchanged on all nine.
Two alternatives were measured and rejected — making the legs OR adds four trees' worth of `.json`
plus two one-file populations, and narrowing the line denominator is a no-op (the eligibility test is
the complement of a closed list, so an unlisted `.sql` or `.pdb` stays in the denominator; all nine
replies came back byte-identical).

**The accepted cost, disclosed rather than erased.** One outsized member — a lock file, a generated
bundle — can carry its filetype into this list, and so can a lone build manifest in a tree small
enough that one file clears a tenth of the file count. A largest-file exclusion was tried against the
first of those on 2026-08-20 and reverted the same day, because it also erased a MyBatis mapper
directory whose XML happened to sit in one file rather than ten — an ERASING change resting on an
INFERENCE, which leaves the reader no trace. Both shapes are labelled `kind: "data-config"` instead,
and the field's `meaning` says such a row is a place to look rather than a verdict. The percentage
lives with the predicate — `is_principal` in
`crates/summary/src/analyze/coverage_gaps/census.rs`, reading the engine's own
`MIN_UNCOVERED_EXTENSION_SHARE_PCT` — rather than in a second copy here, and the `meaning` string
interpolates that same constant rather than spelling a number beside it.

**`queryCoverage` asks a narrower version of the same question, and the two now agree on the whole of
the overlap.** Its `unreadExtensions` cell covers the PARSER half, and with the line leg gone the two
apply the same floor to the same eligibility test over the same denominator (walked files), so the
no-parser rows here and there are one answer rather than two that differ at the margin — which is
what they were until 2026-09-01, when this list required a line share that one never asked for. Both
read `zzop_engine::extraction_can_lose_facts` and `zzop_engine::MIN_UNCOVERED_EXTENSION_SHARE_PCT`
through `zzop-facade`'s re-export (that crate is below this reply's shaping layer, which ships nothing
under `zzop-facade`), so both questions keep one owner. What remains is a difference in SUBJECT rather
than in threshold, and the field's `meaning` states it: this list additionally carries extensions that
PARSED and still resolved nothing, which a parser-capability list has no way to hold.

**Nothing here is a second copy.** The declared-import side stays in `coverage.declaredImportsByExt`,
the join side stays in `coverage.ioProvides`/`ioConsumesKeyed`/`ioConsumesUnresolved`/
`joinContributionZero`, and the per-extension sample paths and adapter on-ramp stay in `warnings` —
`meaning` points at each rather than restating a number that would then be free to go stale.

### `ioChannels` — the per-CHANNEL companion, on the `queryCoverage` reply

`joinContributionZero` and `joinVisibility` both ask ONE question of the whole io contribution, so a
filled channel vouches for an empty one. Measured on gogs (`d460e50fc82978640cfaa202ce5fbbb6fe0f7dc7`):
12 GORM `db-table` provides made `joinContributionZero` read `false` and `joinVisibility` read "this
tree contributed joinable io", while the http provide channel sat at **0** across 301 structural `.go`
files carrying 235+ route registrations. `ioChannels` carries two arrays instead:

| field | kind | meaning |
| --- | --- | --- |
| `extracted[]` | MEASURED | `{kind, provides, consumesKeyed, consumesUnresolved}`, one row for **every** io kind the rules read (`zzop_core::RULE_READ_IO_KINDS`), present even at zero — so "this channel was empty" and "this channel does not exist" stop sharing a shape. |
| `zeroExtraction[]` | CAPABILITY × MEASURED | `{channel, ext, structuralFiles, extracted: 0, recognizers[]}` — one row per (recognizer channel, extension) where a **single** `frameworkRecognizers` row names both, the tree has structural files of that extension, and extraction returned 0. Sorted on `(channel, ext)`. `recognizers[]` is the complete set for that exact pair, so the 0 is relative to those alone — an idiom none of them reads yields the same 0 as an absence. A pair with no recognizer at all produces **no row**, not an empty one; that absence is `frameworkRecognizers`' and `unreadExtensions`' answer. |

The channel vocabulary a row's `channel` (and a `frameworkRecognizers` row's `emits`) can carry is
closed and named in `zzop_core::recognizer::channel`: `io.provides` (route/handler declarations),
`io.consumes` (outbound calls), `io.provides:db-table` (table/model DECLARATIONS),
`io.consumes:db-table` (queries against a table), and `evidence.auth-guarded` (guard evidence feeding
route-auth exemptions, not an io side at all). **The db kind was one spelling for both sides until
2026-08-26**, and the coverage cross — which counts the two sides separately — therefore listed
consume-only recognizers behind a provide-side zero: on immich's `server/src/schema`, 77 `CREATE
TABLE`s across 24 `.ts` files extracted 0, and the row named `raw sql` as a recognizer this build has
for that channel. `raw sql` on `.ts` reads queries; the table-declaration reader there is `typeorm`
alone. Only `io.provides`, `io.consumes` and `io.provides:db-table` are MEASURED as `zeroExtraction`
rows — `io.consumes:db-table` is declarable but has no row population, deliberately.

`zeroExtraction` is keyed on **the tree and the build**, never on recognizing a framework by name.
That is the point: the older zero-route disclosures are conditioned on recognition — the S2 tripwire
matches a hand-typed server-framework package list, and the S8 call-graph gap iterates the routes that
were *extracted* — so both go silent exactly when the gap is largest (gogs' `gopkg.in/macaron.v1` is in
neither, so nothing was left to condition on). Each row is a **coverage fact**, not a defect claim: a
CLI, a library or a frontend that serves no HTTP legitimately reads 0. It ships here rather than in
`warnings` for that reason — as an alarm it would fire on every route-free tree and train readers to
skip the channel.

### The dep graph is resolved-in-tree-only

`ir.dep` is built by **resolution**, not transcription: an edge survives only when the import specifier
resolved to a file this walk visited. An import of a published package (`import requests`,
`import React from "react"`) and a specifier no resolver could map are **both dropped** — package
imports survive only as `AnalyzeOutput::package_imports`, a per-package importing-file set, and never as
edges. Every number derived from that graph therefore counts in-tree edges only: the census's
`resolvedImportEdges`, a node's `fanIn`/`fanOut`/`totalConnections`, a critical file's `blastRadius`,
`queryFile`'s `dependencies`, and the `degree` column of `zzop graph --format cosmograph-nodes`.

**A low number can mean unresolved imports rather than few imports.** That misread is why the census
field was renamed: a 91-file Python tree reporting `3` was read as "this repo barely imports anything".
The census's `declaredImportsByExt` (above) now carries the declared side, counted before resolution,
so the two readings are distinguishable from the output itself instead of requiring that rename's
history lesson.
The graph-theoretic names were **not** renamed alongside it — `fanIn`, `fanOut`, `totalConnections`,
`degree` and `blastRadius` are correct *about* the graph they measure, and the missing fact was never
the term but which graph it describes. So that fact is disclosed once instead: this section, and on the
wire the single sentence `zzop_core::DEP_GRAPH_RESOLVED_ONLY`, which rides the `cosmograph` stderr
census and is the first half of `queryFile`'s `dependenciesMeaning`. One owner — nothing restates it.

## The join's picture: `zzop graph`

The cross-layer join is what zzop exists to compute, and until now it had no picture. `zzop graph` is
that picture — and it is the **one serialization layer** zzop owns for it:

```
zzop graph ./api ./web > join.mmd          # then render it anywhere mermaid renders
zzop graph --config ./zzop.config.jsonc --scope src/billing --top 10
zzop graph --config ./zzop.config.jsonc --domain posture > posture.mmd
```

**One flag, one picture each.** `--domain` picks WHICH picture this lane draws, and they are different
pictures rather than recolourings of one — each has its own node kind. The accepted set is owned by
`GraphDomain::WIRE_NAMES`, which every usage line and rejection message derives from; ask the binary for
the current list with `zzop graph --domain zzz` rather than trusting this table's row count:

| `--domain` | A node is | What it answers |
|---|---|---|
| `join` (the default) | a cross-layer io key | which provides and consumes joined up, and which sit in a bucket instead. The rest of this section. |
| `dep` | a file | what imports what. Files in a cycle and their edges are drawn distinctly, because a cycle is the structural finding hardest to read as text — cycle membership is read off the engine's own `circular` findings rather than re-derived, so there is never a second answer. |
| `risk` | a critical file (hub) or a candidate folder (seam) | where the blast radius is, and which folder boundaries edges cross. An edge here means CONTAINMENT — folder to hub inside it — never an import: that meaning belongs to `dep`, and one arrow style cannot carry both. |
| `posture` | a mutating http route | how much of the write surface this run reported `mutating-route-no-auth` on. Guard status is that rule's verdict, never re-derived here. |
| `cochange` | a file | which files keep being committed TOGETHER — the only domain read from git history instead of imports, so it sees coupling no dep graph can. Edges are undirected and labelled with the shared-commit count. A SAMPLE, never a total: the map behind it keeps only commits touching 2..=25 files and only each file's top 10 partners, and both filters are stated in the document. |

**`--fold <n>` is the second axis, and it is deliberately not more domains.** `--domain` answers *which
relation connects two nodes*; `--fold` answers *what a node is*, by drawing each path's first `n`
segments as one box. The two vary independently, so spelling their product as domain names
(`folderdep`, `layerdep`, `foldercochange`, …) would multiply the vocabulary without adding a fact.
`--domain dep --fold 2` is the same import graph with coarser endpoints — the module map the file-level
picture was always a picture of — and `--fold 1` is the top-level one.

It applies to the **relation** domains (`dep`, `cochange`) and is REFUSED by the **judgment** domains
(`risk`, `posture`) and by `join`, whose nodes are engine verdicts and io keys rather than paths and so
have no granularity to coarsen. The accepted set has one owner, `GraphDomain::accepts_fold`, which the
refusal message and the help line both read. A fold of the *join* — collapsing the SITES on either side
of each contract, so a monolith reads as "which FE area talks to which BE area" instead of a flat route
list — is a real picture and a different design; it is not this flag doing nothing.

What a fold costs is disclosed in the document rather than avoided, because none of it is fixable by
picking a better depth: a folded edge is labelled with the **number of file-level edges it collapsed**
and nothing else (summing `cochange`'s commit counts across pairs would exceed the repository's commit
count — a plausible-looking lie); the depth is a **convention, not a fact about the tree**, so the census
prints the node count either side of the collapse and a one-box picture is legible as one; and a path
shorter than the depth stands for itself, counted separately. `--scope` runs BEFORE the fold and `--top`
AFTER, so "which files are in this picture" and "how many boxes are drawn" each keep their own meaning.

Each domain names its own omissions in its own document, the same way the join map does below: `risk`
states that the structural health scores are NOT drawn (a table of numbers, and a flowchart of them is
strictly worse than the table — `zzop analyze`'s `architecture.pain` carries the composite, and the full
per-metric table rides the direct `zzop-facade` output, **not** `zzop facts`, which carries no scores at
all; the emitted line derives its count from `SCORE_MEANINGS` rather than naming one here), and `posture`
states that read routes and non-http io are not drawn, and
that a route with no finding is `guarded-or-exempt` rather than guarded — the rule is also silent on
routes it cannot judge, so absence of a finding is not proof of a guard. `cochange` states both of its
upstream filters and, per tree, whether git ran at all: a tree that measured nothing and a tree that
measured and found no pair are reported as different sentences, never as one empty diagram. What every domain shares is the
`--scope`/`--top` scoping below, the two-channel truncation disclosure, and mermaid.

**zzop renders no pixels.** The engine stays pure, Node-free and IO-free; the output is a standard
mermaid `flowchart LR` document and an *external* renderer (mermaid.js, a chat client that renders
mermaid inline, `mmdc`) draws it. That split is deliberate and load-bearing: a viz stack inside the
analyzer would be a permanent maintenance surface with no analysis value.

**DOT was rejected rather than deferred.** Not because `--format` would be a new flag — that flag
exists and this lane already carries three values (`mermaid` plus the two `cosmograph-*` NDJSON
tables, see above) — but because DOT buys nothing the mermaid text cannot already say, while adding a
second *drawing* emitter's tests and a second row in every document that names this lane. The two
cosmograph values earned their place by serving a different consumer (an interactive viewer that needs
uncapped tables, not a diagram); a DOT emitter would serve the same consumer as mermaid, twice. If you
want graphviz, `zzop facts` emits the whole join uncapped and a short script converts it.

**What a node is.** A `(sourceId, side, kind, key)` tuple, where side is *provide* or *consume* — not a
call site. Twelve `fetch` calls to the same route in one tree collapse into **one** consume node. That
is the point of a picture, and it means **file and line are not in this output at all**; `zzop facts`
(per-site, uncapped) and `zzop cross`'s `distinctBucketKeyFirstSites` (one site per distinct key — the
first) are where those live.

| Drawn as | Means |
|---|---|
| One `subgraph` per analyzed tree | Every source appears, including one that contributed nothing — with a note node saying *which* zero it is (blindness vs. an empty contract). |
| Rectangle node | A **provide** — something this tree serves. |
| Stadium node | A **consume** — a call site (aggregated). |
| Node label `role · kind key` + a `classDef` class | The bucket the row came from: `linked`, `candidate`, `unconsumed`, `unprovided`, `unresolved`, `external`, `ambiguous`. The role rides the **label** as well as the colour, so a viewer that drops styling still reads the verdict. |
| Solid arrow | A resolved edge: consumer → provider. |
| Dotted arrow | A relation zzop does **not** assert: an ambiguous consume to each candidate provider, or an edge the linker flagged `lowConfidenceReason` (labelled with the reason). A guess is never drawn like a resolved join. |

**Scoped by construction, with the truncation disclosed twice.** A large join makes an unreadable
diagram, so `--top` caps **drawn relations** per bucket (per-bucket rather than
per-document precisely so a big `edges` list cannot push a whole bucket out of the picture) and
`--scope <prefix>` keeps only rows whose source id *or* one of whose site paths starts with the prefix.
Both are announced in the `%%` header as a per-bucket `drawn/inScope/total` census — always, capped or
not — **and** as a visible note node on the canvas, because a mermaid comment does not survive
rendering and a picture that silently omits rows is the failure mode this project forbids. `--top` has
no upper bound: this is a file/pipe surface like `facts`, not the cap-governed MCP wire.

`--top`'s DEFAULT differs per domain, and is deliberately not restated here: a join has tens of
relations where an import graph has thousands, so one shared number would either black out the second or
starve the first. `GraphDomain::default_top()` (`crates/summary/src/graph/mod.rs`) is the single owner
and `zzop graph --help` prints each domain's value from it, so the number a caller is told is the number
they get. What `--top` COUNTS differs with the node kind too — `join` caps relations per bucket, `dep`
caps nodes by fan-in + fan-out, `risk` caps per kind, `posture` caps routes per tree — and each
document's own `%%` census names which.

The cap counts **relations, not rows**, and the census publishes both scales (`edges: 4/4/4 from 60
site(s)`). Measured on the OSS corpus's express/axios pair, 60 `edges` rows collapse into 4 distinct
`(source, key)` relations — so a row-based `--top 5` drew exactly *one* arrow while the disclosure said
five. Deduping before capping is what keeps the census a description of the picture rather than of the
input.

**What this format cannot carry**, printed in the document's own header so completeness is never
inferred from a picture:

- **`crossLayerFindings`** — the drift/near-miss **verdicts** (route shadowing, body-field drift, ...).
  They are findings *about* the join, not members of it; a finding has no node identity here, and
  inventing one would mean inventing facts the IR does not have. `zzop cross` is where they live.
- **`hostRekeyCounts`** — a per-host counter, not an edge.
- **`wildcardRoutePartitions`** — a route lifted *out* of the join for being an ANT pattern rather than
  a key. It is in no bucket, so it is in no node: the picture cannot show it, and would otherwise omit a
  live route with nothing on the canvas saying so. `zzop cross`'s `sources[].warnings` names each one.
- **`warnings` / `configWarnings` / `disclosure`** — prose channels; `cross` and `facts` carry them.
- **an item with neither `key` nor `raw`** — nothing to label a node with, so it is counted as an
  unlabelable remainder in the census and never guessed at.

**Determinism.** Byte-stable for the same input and options: nodes live in a sorted map, edges in a
sorted set, and the `n0`/`n1`/... ids are assigned *after* that sort (which is also what keeps arbitrary
key text out of mermaid identifier position — it only ever reaches a quoted label, where whitespace is
collapsed and `#`/`"`/`<`/`>` become mermaid entity codes). Tree **request** order does not change the
document: subgraphs sort by source id.

Like `manifest`/`diff`/`facts`, `graph` lives in `zzop-summary` (`crates/summary/src/graph/`) and is
**CLI-only** — the no-MCP-twin reasoning is recorded in
[`docs/contracts/surface-parity.json`](../contracts/surface-parity.json)'s `_cliOnlyLanes`.

## `disclosure` — silent-failure-class registry (run-global)

`analyze`, `analyzeEnvelope` and `analyzeTrees` all emit a top-level `disclosure` array: zzop's pinned,
honest list of the ways its own output can be silently misread. It is **run-global** (identical every
run, emitted once — on the multi-tree output it sits beside `trees`/`crossLayer`, never repeated per
tree) and static, so a consumer learns not just what zzop found but which *classes* of blindness zzop
does and does not yet actively detect.

> ⚠ **This section describes the FACADE output, which is also the derivation source.** The shaped
> product replies — `zzop analyze`/`cross`/`endpoint` and their `analyze_repo`/`cross_repo`/
> `check_endpoint` MCP twins — carry a **fold** of this array since 2026-07-29: the counts
> (`classes`/`asserted`/`partial`/`notYetDetected`) plus a `note`, a `resource`
> (`zzop://contract/disclosure-classes`) and a `command` (`zzop contract disclosure-classes`). The
> prose is run-invariant, so it ships once through the contract lane instead of on every call; the
> facade keeps emitting the full array because everything else derives from it, and the CLI-only
> `zzop facts` lane carries it verbatim too. The counts and the served document come from this one
> registry and tests on both sides enforce that, so growing the registry without moving the counts
> fails the build.

Each entry:

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
`docs/NORMALIZED_AST.md`'s external-parser envelope input contract
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
  host; `key` is present for these, so they stay locatable in `distinctBucketKeys`). Both are "the analysis is
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

`crossLayer` also carries `wildcardRoutePartitions`, present only when some tree declares a route whose
path is an ANT **pattern** rather than a key (`GET /api/files/**`). Such a route is lifted OUT of the
exact join entirely — the join is an exact `(kind, key)` match and never prefix-guesses, so comparing a
pattern literally made a live catch-all read as a dead route (`unconsumedProvides`) *and* made every call
beneath it read as a missing one (`unprovidedConsumes`): three wrong answers from one cause. Each entry
is `{sourceId, key, file, line, coveredConsumes}`, where `coveredConsumes` counts the consume call sites
that pattern took out of `unprovidedConsumes` (`0` is meaningful — the route is still partitioned, it
just swallowed nothing this run). **The partition emits no edge**, by construction: it removes a provide
that could never join and the consumes that provide really serves, so `edges` is unchanged and the win
reads only in the two residue buckets. The verb must match, so a `POST` beneath a `@GetMapping("/x/**")`
still reports as unprovided. The same rows drive the declaring tree's own `warnings` self-report, which
is how `zzop cross` tells a reader what the silence cost. The field is omitted entirely when no tree
declares such a route.

`crossLayerFindings` is the output of the `cross-layer/*` native rules run over `crossLayer` (see the
"Native analyses" table in [docs/rules/catalog.md](../rules/catalog.md) for the full id list) — sorted the
same `(severity, file, line, ruleId)` way as every per-tree `findings` array, and gated by the UNION of
every tree's `disabledRules` (any one tree disabling a cross-layer rule id drops it from this array
entirely, since it is a joint-analysis output no single tree fully owns).

`version()` returns
`"zzop/{version} zzop-parser-typescript={FP}/{hash} zzop-parser-prisma={FP}/{hash} zzop-parser-python-3={FP}/{hash} zzop-parser-java-21={FP}/{hash} zzop-parser-rust={FP}/{hash} zzop-parser-go={FP}/{hash} zzop-parser-sql={FP}/{hash} zzop-parser-csharp={FP}/{hash} zzop-engine={hash}"`
— every native parser's `PARSER_FINGERPRINT` plus its derived source-closure hash (the same value the
cache key uses), in that order, with the engine's own hash last. `{version}` is `CARGO_PKG_VERSION` — the
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
| `SourceSymbol` | `id, file, name, kind, line, exported, isDefault, bodyStart?, bodyEnd?, writeSites?` | `kind` is one of `function\|class\|const\|type\|interface`; `bodyStart`/`bodyEnd` (1-based, inclusive) delimit the scannable region, and `crates/core/src/ir.rs`'s "Body span contract" section is their single owner — this row is a pointer, not a second copy. Two parts of it are load-bearing here because this table is a wire-shape reference: `bodyStart` is the line the DECLARATION begins on, leading decorators/annotations/attributes INCLUDED, **not** the line the body block opens on; and their ABSENCE is a POSITIVE claim that the declaration encloses nothing scannable (a type/interface, a field, a trait), never "the producer could not compute one". This row said "set only for functions/classes with a recoverable body span" until 2026-08-11, which got both halves backwards — it made omission sound like a measurement failure and named a `kind` whitelist the contract does not have; `writeSites` (skipped when empty; camelCase-only, no snake_case alias — see [NORMALIZED_AST.md](../NORMALIZED_AST.md)) lists pre-computed store-write call sites within the symbol's body span (TS only; feeds the `unsafe-read-endpoint`/`non-idempotent-write` call-graph scanners). camelCase on output like every other type here. On the way IN, `SourceSymbol` is also reused verbatim as the deserialize target for [NORMALIZED_AST.md](../NORMALIZED_AST.md)'s external-parser envelope input contract (`FileProjection.symbols`), so it additionally *accepts* that contract's snake_case names (`is_default`, `body_start`, `body_end`) via `#[serde(alias = ...)]` — a conforming envelope producer's JSON keeps working unchanged. |

Every finding — from a DSL rule pack or a native analysis alike — has this shape:

| Field | Value |
|---|---|
| `ruleId` | `"{pack}/{rule}"` for a DSL rule (e.g. `"sql/nplus1"`), or a plain id for a native analysis (e.g. `"circular"`) — see [rules/catalog.md](../rules/catalog.md) for the full id list. |
| `severity` | `"critical" \| "warning" \| "info"` — the rule's default severity (see [rules/catalog.md](../rules/catalog.md)). |
| `file` | The finding's file, relative to `root`. |
| `line` | 1-based line number. |
| `message` | Human-facing cause/fix-hint. **Usually NOT the rule definition's text any more** — measured on `cases/trees/api-be` at the default window, 46 of 50 shown findings carry a pointer instead. Three things can stand here: the rule's own text verbatim; a pointer into this reply (resolved through `messageRef` or `templateParts`); or a pointer OUT of it (resolved through the finding's own `ruleId`, signalled by `messageBy`). **Which one it is, is told by a sibling FIELD, never by the sentence** — a finding with none of the three fields carries prose. Read the sections below before treating this field as prose. |
| `messageRef` | `string`, **present only on a folded finding**. The key into `findings.ruleMessages` that carries this finding's full text. When present, `message` is a placeholder that names its own replacement (`"[folded] this rule's full text is carried once in this reply at ruleMessages[…]"`), so a consumer that ignores this field prints a pointer instead of a message rather than printing nothing. |
| `messageBy` | `string`, **present only on a finding whose text was left to its rule's own entry**. Its value names the resolver; `"ruleId"` is the only value today and means "look this rule up by the `ruleId` on this same finding". Its presence IS the signal, the same contract the two fields around it keep, and no finding ever carries more than one of the three. |
| `templateParts` | `string[]`, **present only on a template-folded finding**. The interpolated pieces for this finding, to be substituted into the template at `findings.ruleMessageTemplates[ruleId]`. Whole-message folding cannot key a rule whose text interpolates a per-finding identifier; this is that rule's lane. |
| `evidencePaths` | `string[]`, **omitted when empty** (the common case). Every OTHER `root`-relative path this finding names in `message`/`data` besides `file` — populated by the relational rules that necessarily point at two places (a consume site and the provide it mismatched, an N-source collision's sibling sites). It exists so `exclude` can mean "do not name this path to me" and not merely "do not anchor here": an excluded path in this list is replaced by `<excluded>` throughout `message` and `data` while the finding itself survives. See `globalExcludes` above for the two roles. |
| `data` | Matcher-specific JSON payload (e.g. `{snippet, label}` for a line-scan hit). **The field is on the compatibility surface; the keys inside it are not.** `data` is always present and always a JSON object — that much [VERSIONING.md](../../VERSIONING.md)'s table covers, like any other field name and type. What a given rule puts IN it is documented per matcher in [dsl-reference.md](../rules/dsl-reference.md) as a description of what that matcher emits TODAY, not as a frozen shape: DSL packs author their own keys ad hoc (mostly camelCase already, e.g. `handlerSymbol`), so no uniform casing rule applies inside `data` either. The reason is not reluctance — it is that no machine could hold the promise. `Finding::data` is a free-form `serde_json::Value` at the engine boundary, `Finding::data`'s own doc refuses a per-rule table of shapes (a thirteenth rule leaves such a table silently short), and the guard that does exist (`crates/engine/tests/rule_contracts/finding_data_keys.rs`) checks ONE direction — every key a shipped consumer READS is spelled by some producer — which cannot see a rule quietly changing the shape it writes. So: read `data` defensively, treat a missing key as "this rule does not carry that", and pin nothing inside it that a run would not survive losing. ⚠ This row said only "opaque, rule-specific" until 2026-09-07 (review ledger V89), which read as "do not depend on any of it" while dsl-reference documented exact keys — an adapter author had no way to tell which document to believe. |

### Message folding — why `message` is not always the message

Rule prose is long and repeats: one rule firing forty times used to put forty byte-identical copies of
its text on the wire. When it pays for itself in SERIALIZED bytes, the reply carries the text once and
points the findings at it. Two lanes, both under `findings`:

| Field | Value |
|---|---|
| `ruleMessages` | `{ [key]: string }` — one entry per folded whole message. A finding joins it through its own `messageRef`. |
| `ruleMessagesMeaning` | The sentence that says what the map above is, shipped in the reply so a host needs no copy of this page. |
| `ruleMessageTemplates` | `{ [ruleId]: string }` — one entry per folded TEMPLATE, for rules whose text interpolates a per-finding value. A finding supplies its own pieces in `templateParts`. |
| `ruleMessageTemplatesMeaning` | The same, for the template map. |

Three properties a consumer can rely on. **Folding is byte-driven, not shape-driven**: a reply folds a
message only when doing so makes the serialized reply smaller, so the same rule on the same tree may be
folded at one `limit` and inline at another, and a small reply is typically not folded at all. **A
folded `message` still says something true about itself** rather than being empty or absent, and it
names the exact key holding its text. And **the reconstruction is exact** — resolving every pointer
reproduces the unfolded reply byte for byte.

### The third lane — `messageBy`, the one that points OUT of the reply

The two lanes above deduplicate: the text is still in your hands, one copy instead of forty. A third
mechanism does something different, and the difference is the only thing about it worth memorising.

A finding carrying **`messageBy`** does not carry its text at all. Its rule declared that text word for
word, so it is rebuildable from the finding's own `ruleId`:

```sh
zzop explain <ruleId>          # CLI
```

```text
zzop://rule/{id}               # MCP resource template, same bytes, same function
```

Both return the message, the suppress marker and the disable knob that used to ride on the finding.
There is no table to join — the address is a field you already hold.

🔴 **Detect this lane by the `messageBy` FIELD, never by the sentence in `message`.** The field is the
contract; the sentence is wording, and [VERSIONING.md](../../VERSIONING.md) keeps exact message wording
explicitly outside the compatibility surface. A consumer that pattern-matches the sentence is built on
the half that is free to move — which is the same rule `scripts/measure/resolve-folded-message.mjs`
already states for `messageRef`.

| Field | Value |
|---|---|
| `messageByIdMeaning` | The sentence that says all of the above, shipped in the reply so a host needs no copy of this page. Present exactly when some shown finding carries `messageBy`, absent otherwise. |

**Four classes never carry it** and keep every byte inline:

- a **native analysis** — it has no declared message to restore from;
- a finding whose text was **rewritten to name a suppression comment** the rule does not honour — that
  token came from your source and is in no other field, so no rule id can rebuild it;
- a rule **this binary does not carry**. Both resolvers above answer only for rules compiled in, so a
  pack loaded from `zzop/rules/` or `packs.extraDirs` keeps its prose. Pointing at it would name a door
  that answers `unknown rule id` — measured, and the reason the population is "what `zzop explain`
  can answer for" rather than "where did this pack come from";
- a rule whose **whole message is cheaper** than the pointer and its field — the same net-byte gate the
  folds use, so a message left whole is a size decision and never a sign that anything is missing.

**What it does not touch**: which findings you were shown, or what they counted toward. `total`,
`bySeverity` and `byRule` never read `message`, and `truncated` is still the only key that means rows
were left out.

**If you were reading prose out of `message`, this is a break** — the field is still a string and still
present, so a type or presence check does not move, but a grep for a sentence now finds the pointer.
`CHANGELOG.md` carries it. Measured on that fixture (`zzop analyze cases/trees/api-be | wc -c`):
127,937 → 62,110 bytes, -51.5%, re-measured 2026-09-13 with
`zzop analyze cases/trees/api-be | wc -c`. Quote that command, not the number: the reply carries
run-invariant legends, so ANY change to one moves this figure without touching this lane at all.

**Where it runs.** The SHAPER writes it (`crates/summary/src/output/rule_prose/by_id.rs`), not the
facade and not the engine. That matters twice: the three entry points `VERSIONING.md` freezes as one row
all funnel through the shaper and therefore agree by construction, and `zzop_facade::analyze_trees_json`
— a data producer with nine consumers inside `zzop-summary` — hands every one of them the full prose. A
Rust embedder calling `zzop_engine::analyze_tree` directly is likewise unaffected.

These keys are part of the CLI JSON output surface and are versioned as such; they are equally
part of the MCP reply, and `docs/contracts/surface-parity.json` is the registry that binds the two.
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
