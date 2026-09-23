//! `tools/list` result payload — the tool definitions (names, descriptions, input JSON Schemas),
//! split out of `tools.rs` unchanged. The strings here are the MCP contract; see the parent module
//! doc for the surface overview.

/// `tools/list` result: every tool this server exposes, with input JSON Schemas. Shared filter
/// arguments (`severity`/`rule`/`limit`) are the drill-down knobs the truncation hint points at.
pub fn list() -> serde_json::Value {
    let filter_props = serde_json::json!({
        "severity": { "type": "string", "enum": ["critical", "warning", "info"], "description": "Minimum severity to include in the findings list (counts always cover everything)." },
        "rule": { "type": "string", "description": "Exact rule id to include in the findings list. Valid ids are listed in the rule-catalog resource (zzop://contract/rule-catalog); a filter matching zero findings for an id absent from this run's own fired ids gets a disclosure note." },
        "limit": { "type": "integer", "minimum": 0, "maximum": 1000, "description": "Findings list cap (default 50, max 1000). 0 is legal — \"counts only, no findings listed\". Must be a JSON integer in range; an out-of-range or wrong-type value is a named error, not a silent no-op." }
    });
    serde_json::json!({
        "tools": [
            {
                "name": "analyze_repo",
                // Annotation honesty rule for this whole table (they are advisory UI hints, but a
                // false one lets a host skip a confirmation the user should have seen): the four
                // tree-analyzing tools WRITE — the product front end injects a `.zzop/cache` default
                // resolved beside the honored config file (`crates/config/src/mapper/options.rs`;
                // usually the tree root, and a config's own `cacheDir` moves or disables it), so they
                // must not claim `readOnlyHint: true`. They are `destructiveHint: false` — the cache
                // writes stay inside zzop's own `.zzop/cache/` (it also self-evicts and wipes its own
                // stale entries there, but never a file a user authored) — and `idempotentHint: true`
                // (deterministic analysis; a repeat call with the same arguments converges to the
                // same state). The three text-in/judgment-out tools (analyze_envelope,
                // validate_envelope, validate_rule_pack) touch no disk at all: the envelope lane's
                // request deliberately carries no `cacheDir` (`crates/facade/src/request.rs`) and the
                // MCP arm passes no envelope path to discover a config from
                // (`crates/summary/src/analyze/mod.rs`), so they claim `readOnlyHint: true`.
                // `openWorldHint: false` everywhere: zzop performs zero network calls (no HTTP client
                // crate exists in Cargo.lock), so no tool reaches outside the local filesystem.
                "title": "Analyze a repository",
                "annotations": {
                    "title": "Analyze a repository",
                    "readOnlyHint": false,
                    "destructiveHint": false,
                    "idempotentHint": true,
                    "openWorldHint": false
                },
                "description": "Run zzop's deterministic analysis on ONE repository/tree. Pass EITHER `path` (a tree root — auto-discovers <path>/zzop.config.jsonc: rules, packs, overlays, mounts, with the reply's `config` field saying whether one was honored) OR `configPath` (a zzop.config.jsonc at ANY location, for a config that does not sit at the tree root; the CLI twin spells this `zzop analyze --config <path>`). A config is REQUIRED: a tree with no zzop.config.jsonc is refused, and the error names the `config-template` contract document and, on this server, the resource URI that serves it (write those bytes to <tree>/zzop.config.jsonc and retry). Everything the config does not say still defaults (bundled rule packs + git signals included); the one thing with no default is its `vocabulary` block, the names zzop would otherwise guess about that project, where an undeclared key is a judgment zzop does not make. Returns a summary (full counts by severity/rule, engine warnings) plus a capped findings list — truncation is always disclosed. The reply's `coverage` field is the structural coverage census, the run's own blindness disclosure (its degraded count is the full, uncapped one the capped `degraded` list points at): read it before treating zero findings as a clean bill, because a run that could see little reports little. Beside it, `coverageGaps` — shape `{basis, extensions, meaning}` — names the PRINCIPAL filetypes whose code did not reach the DEPENDENCY GRAPH — because no structural parser read them, because a parser read every one of them and bailed (`coverage.degraded` tells those two apart, and they take opposite remedies), or because they parsed and none of their declared imports resolved — with a `basis` sentence so an empty list reads as measured rather than unasked, and a `meaning` that says which rules those gaps make untrustworthy. Each row is `{ext, files, kind, structural}`, and `kind` is the field to read FIRST because the population is no longer source languages alone: `\"source\"` is a language this tree is written in that no frontend read, while `\"data-config\"` is a structured data or configuration filetype (`.xml`, `.json`, `.yaml`, a lockfile) where whether anything was lost depends on what the files hold — a MyBatis mapper directory and an i18n bundle are the same row shape and take opposite remedies, and this build does not claim to tell them apart without reading them. It is always present, because \"the graph holds this tree's code\" and \"nobody looked\" are indistinguishable from the findings alone: measured on one 2.6k-file tree, 170 of its 173 `dead-candidates` sat on the single extension whose imports never resolved. Beside it, `configWarnings` is the config-honesty channel (unknown config keys, unknown rule ids in overrides — a typo'd override is reported there, never silently ignored); it is split from `warnings` on purpose: `configWarnings` says how the config was handled, `warnings` says what happened in this run. The reply's `packsLoaded` array is the pack-load confirmation, one `{id, rules, ruleIds, source, filesInScope?, filesInScopeIfEnabled?, zeroAdmissionRules?, didNotRun?}` entry per loaded rule pack. LOADING IS NOT RUNNING, and the row says which: `didNotRun` is present only on a pack that loaded and was never evaluated — `\"disabled\"` (its id is in `packs.disabled`) or `\"notAllowlisted\"` (a `packs.only` allowlist does not name it) — and such a row carries `filesInScopeIfEnabled` (what it WOULD have looked at) in place of `filesInScope` and no `zeroAdmissionRules` at all. So zero findings from a pack marked `didNotRun` mean NOT ANALYZED, never analyzed-and-clean. On a pack that ran, `filesInScope` counts the files whose PATHS the pack's rules would scan (eligibility, never a match or usage count) and its very presence is the claim that the pack ran; `ruleIds` is the LIST behind the `rules` COUNT — every rule id this run could have reported from that pack, so \"does this run carry a rule named X\" is answerable from the reply instead of guessed from the pack prefix — and `zeroAdmissionRules` (present only when non-empty) lists the rules whose own path gates admitted ZERO files of this tree, which could not have read a byte here, so their zero findings mean scope, NOT a clean bill. A config declaring multiple trees returns a guided error telling the caller to run the cross-layer join over that config instead; the shared sentence names no tool (it is built in a crate both products speak through), and on this server the reply then names `cross_repo` as the tool that answers — the same host half the missing-config refusal carries. Cross-layer (`cross-layer/*`) findings come from the multi-tree join and surface only in cross_repo/check_endpoint/check_file replies — this tool reports this tree's own per-tree findings only. Any honored config's rule overrides (disabled rules, remapped severities, and the `packs.only` pack allowlist) are positively acknowledged in the reply's `ruleOverridesApplied` field ({disabled, severityRemapped, only} id lists, omitted when none were requested), alongside the honored config file echoed in `config`. `only` and the `didNotRun` marker on each `packsLoaded` row answer different halves and you may need both: `only` reports which entries of YOUR OWN REQUEST took effect (a typo'd pack id appears nowhere), `didNotRun` reports what happened to each LOADED pack. An allowlist whose every entry is a typo admits no pack at all — `only` comes back `[]` while every `packsLoaded` row reads `didNotRun: \"notAllowlisted\"`. Beside the array, `packsLoadedMeaning` defines every one of its keys, including which of the two readings of `zeroAdmissionRules` is the true one (no analyzed file ever reached those rules — NOT \"files reached them and nothing fired\"); it is absent when no pack loaded. This reply, and the sibling `zzop analyze <path>` CLI form (same handler), are both a shaped summary that deliberately omits the raw `ir` block some engine disclosures point at — the full raw io facts (`ir.io`'s provide/consume lists) are only in the raw `zzop-facade` JSON output you get by embedding the engine directly, never in this Node-free binary's replies. When the underlying analysis ran git signals (the default, or a config's own `git` settings), the reply also carries a compact, capped `architecture` object summarizing the engine's health/recommendations/critical-file computation. Shape: `{pain, painMeaning, painByAxis, painMeasuredWeight, painTotalWeight, topRecommendation, topRecommendationMeaning, criticalTop, criticalTopMeaning}` — the three `*Meaning` strings ride IN the object because each of the three values beside them is routinely read as something it is not, and a legend that lives only in documentation is a legend no caller has read. `painMeaning` says `pain` is not a defect score; `criticalTopMeaning` says which ranking produced those paths; `topRecommendationMeaning` says that `topRecommendation.severity` is a PRIORITY BAND over structure, computed from no rule finding at all, so a `critical` band sitting next to a `findings.bySeverity` that holds no critical is the normal case rather than a contradiction. `topRecommendation` is `{id, severity, topItem}`, or null when nothing cleared threshold. Every other value in the object is defined by the three `*Meaning` strings that ride WITH it, and this description deliberately does not restate them — a legend duplicated here would drift from the value it explains, which is the very failure the in-object legends exist to prevent. The whole object is absent (not null) when git signals did not run. Full per-file scores/recommendations/critical-file detail is never in this summary — only the raw `zzop-facade` JSON output (direct engine embedding) carries the complete arrays.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Absolute path to the repo/tree to analyze." },
                        "configPath": { "type": "string", "description": "Path to a zzop.config.jsonc (or a directory containing one) naming the ONE tree to analyze — the config-first mode, for a config that does not sit at the tree root. A config declaring 2+ trees is refused here (that is cross_repo's question)." },
                        "severity": filter_props["severity"],
                        "rule": filter_props["rule"],
                        "limit": filter_props["limit"]
                    },
                    // Exactly ONE of `path` / `configPath`, the handler's real contract expressed in
                    // the schema (same shape cross_repo's `oneOf` already carries): both branches
                    // matching, or neither, fails — mirroring the "not both" / "pass one" errors.
                    "oneOf": [
                        { "required": ["path"] },
                        { "required": ["configPath"] }
                    ]
                }
            },
            {
                "name": "cross_repo",
                "title": "Join repositories across the layer boundary",
                "annotations": {
                    "title": "Join repositories across the layer boundary",
                    "readOnlyHint": false,
                    "destructiveHint": false,
                    "idempotentHint": true,
                    "openWorldHint": false
                },
                "description": "Analyze 2+ repos/trees and join them across the layer boundary — the cross-layer (kind,key) join (e.g. a React consume matching a Spring provide, a shared DB table, route drift). Pass EITHER `configPath` (a zzop.config.jsonc — its `trees`, including \"auto\", define the join; the config-first way) OR `paths` (explicit tree roots, each tagged by directory name — every root LOADS its own zzop.config.jsonc and must have one; the reply's `config` field stays null because no single config governs the run, and `configWarnings` names the ones that were honored). Returns per-tree summaries with engine warnings, the join buckets, matched edges, and cross-layer findings (capped lists disclose truncation). Each `sources[]` row carries `packsLoaded`, the same `{id, rules, ruleIds, source, filesInScope?, filesInScopeIfEnabled?, zeroAdmissionRules?, didNotRun?}` pack-load confirmation analyze_repo's reply carries for one tree: `filesInScope` is path ELIGIBILITY (never a match count) and its presence is the claim that the pack RAN, `ruleIds` is the list of rule ids behind the `rules` count, and `zeroAdmissionRules` (present only when non-empty) lists rules whose own path gates admitted zero files of that tree — their zero findings are scope, not a clean bill. A pack the run gated off carries `didNotRun` (`\"disabled\"` / `\"notAllowlisted\"`) plus `filesInScopeIfEnabled` instead: it never ran, so its silence is not cleanliness. ONE `packsLoadedMeaning` object at the reply root defines those keys for every source. Each row likewise carries `coverage`, the per-tree structural coverage census, including the engine's `joinContributionZero` assertion — the positive statement that a tree contributed NOTHING to the join — so a barely-seen source reads as silence, not cleanliness; check it before calling a quiet join clean. TWO VIEWS OF ONE BUCKET, and they are different numbers on purpose: `buckets` counts RAW ROWS (one per recorded call site) while `distinctBucketKeys` lists the DISTINCT keys those rows collapse into, so `buckets.X` is always >= the length of `distinctBucketKeys.X` — the reply states this relationship itself in `bucketMeaning`, so no reader has to infer it. The parallel `distinctBucketKeyFirstSites` gives ONE site per distinct key, the FIRST recorded `file:line`, so e.g. an unresolvedConsumes key is locatable without a further query. The honored config file, if any, is echoed at the top level (`config`), and each source's rule overrides, if any, are positively acknowledged per-tree (`ruleOverridesApplied`: {disabled, severityRemapped, only} id lists — `only` being the `packs.only` allowlist, outside which a pack did not run at all) rather than left implicit. Like analyze_repo, this reply (and the sibling `zzop cross` CLI form) is a shaped summary per source that omits the raw `ir` block — full raw io facts live only in the raw `zzop-facade` JSON output (direct engine embedding).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Absolute paths to the repos/trees to join (paths mode — each root loads its own zzop.config.jsonc).",
                            "minItems": 2
                        },
                        "configPath": { "type": "string", "description": "Path to a zzop.config.jsonc (or a directory containing one) whose trees define the join (config-first mode)." },
                        "severity": filter_props["severity"],
                        "rule": filter_props["rule"],
                        "limit": filter_props["limit"]
                    },
                    // The handler's real contract, expressed in the schema (not just the prose):
                    // exactly ONE of `paths` / `configPath` — both branches matching (or neither)
                    // fails `oneOf`, mirroring the "not both" / "pass one" handler errors.
                    "oneOf": [
                        { "required": ["paths"] },
                        { "required": ["configPath"] }
                    ]
                }
            },
            {
                "name": "check_file",
                "title": "Everything zzop knows about one file",
                "annotations": {
                    "title": "Everything zzop knows about one file",
                    "readOnlyHint": false,
                    "destructiveHint": false,
                    "idempotentHint": true,
                    "openWorldHint": false
                },
                "description": "DEFINITIVE answer to \"what does zzop know about THIS FILE?\" — the targeting twin of check_endpoint, with a file PATH as the target instead of an io key. Use it when you are working IN a file and want everything about that file rather than everything about the tree. Returns: which tree it belongs to (`sourceId`, plus `otherTrees` when the same relative path exists in more than one, never a silent pick), a `verdict`, its `loc`, its `symbols` (count + exported names), its `io` provides/consumes, its `dependencies` in BOTH directions (`imports` and `importedBy` — the second is the half you cannot read off the file itself), and every finding anchored in it, the tree's own and the cross-layer join's merged into one list with counts by severity and rule. NOTHING IS CAPPED: a single file's facts are bounded by the file, so this surface drops nothing and therefore never has to disclose a truncation. THE VERDICT ANSWERS WHETHER THE FILE WAS ANALYZED, NOT WHETHER IT IS HEALTHY, and that distinction is the point of the tool: an empty findings list means \"clean\" for an `analyzed` file and means \"nothing structural ever ran\" for a `lexical-only` or `degraded` one. Sealed four-token vocabulary — \"analyzed\", \"lexical-only\", \"degraded\", \"not-found\" — and the reply SPELLS OUT the returned token's meaning in its own `verdictMeaning` field, so this description is not a second owner of what a token means. `dependencies` carries the same kind of field, `dependenciesMeaning`: an EMPTY `imports` list is ambiguous on its own, so the reply says what it covers rather than leaving this description to define it. Three host-layer channels ride along beside the analysis, and they are named here because a CLOSED \"Returns:\" list reads as a denial of anything it omits: `config` (the config file this answer was computed under, or null), `warnings` (engine-side diagnostics — WHY a parse degraded, which packs did not load), and `configWarnings` (unknown keys, unknown rule ids). A `lexical-only` verdict without reading `warnings` tells you the file was not analyzed but not what stopped it. A `not-found` reply lists the nearest walked paths in `suggestions`. The target may be tree-relative or absolute (an absolute path is matched by its tail), and either separator style. Pass exactly ONE of `path` (one tree), `paths` (2+ tree roots, each loading its own config), or `configPath`.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "target": { "type": "string", "minLength": 1, "description": "The file to ask about — tree-relative (`src/api/users.ts`) or absolute; forward or back slashes both accepted. An absolute path matches by its tail, so you can pass what your editor gave you." },
                        "sourceId": { "type": "string", "description": "Optional. Pins the answer to ONE tree by its sourceId, for a relative path that exists in several. Omit it and every tree is searched, with `otherTrees` naming any additional match." },
                        "path": { "type": "string", "description": "Absolute path to ONE repo/tree (auto-discovers its zzop.config.jsonc, like analyze_repo)." },
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Absolute paths to 2+ repos/trees to join (paths mode, like cross_repo — each root loads its own zzop.config.jsonc).",
                            "minItems": 2
                        },
                        "configPath": { "type": "string", "description": "Path to a zzop.config.jsonc (or a directory containing one) whose trees define the analysis." }
                    },
                    "required": ["target"],
                    // Same "exactly ONE of path/paths/configPath" rule check_endpoint expresses, and for
                    // the same reason: two sources match two branches, zero match none, both fail oneOf.
                    "oneOf": [
                        { "required": ["target", "path"] },
                        { "required": ["target", "paths"] },
                        { "required": ["target", "configPath"] }
                    ]
                }
            },
            {
                "name": "check_endpoint",
                "title": "Check one io key across the join",
                "annotations": {
                    "title": "Check one io key across the join",
                    "readOnlyHint": false,
                    "destructiveHint": false,
                    "idempotentHint": true,
                    "openWorldHint": false
                },
                "description": "DEFINITIVE answer to \"is io key X provided/consumed/joined?\" — matches `pattern` against ANY cross-layer io key (http routes, DB tables, tRPC procedures) as a case-insensitive substring, over a fresh analysis of the given tree(s). Returns one `verdict` from a sealed eight-token vocabulary — \"linked\", \"provided-only\", \"consumed-unprovided\", \"external\", \"unresolved-only\", \"ambiguous\", \"mixed\", \"not-found\" — and the reply SPELLS OUT the returned token's meaning in its own `verdictMeaning` field, so this description is not a second owner of what a token means (the definitions live with the verdict computation, `zzop_facade`'s query core, and ride every reply on every host). An \"ambiguous\" verdict's candidate providers are listed per matched item, inside each `matches.ambiguousConsumes[]` entry's own `candidates` array — there is no top-level `candidates` field. Full per-bucket counts ride along uncapped; matched objects (file/line/source intact) and related findings are capped with disclosed truncation. `relatedFindings` is a TEXT match, never a computed link — a finding is listed when its rendered message contains the pattern or a matched key as a case-insensitive substring, so it both over- and under-matches, and an empty array is not evidence that no finding concerns the key. The reply states that itself in `relatedFindingsBasis`, so this description is not a second owner of it. The same three host-layer channels check_file names ride along here too — `config`, `warnings`, `configWarnings` — for the same reason: a verdict computed over a tree whose packs failed to load is a verdict about less than you asked for, and `warnings` is where that is said. Pass exactly ONE of `path` (one tree — the join still runs, intra-tree edges included), `paths` (2+ tree roots, each loading its own config), or `configPath`.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "minLength": 1, "description": "Non-empty, case-insensitive substring to match against every io key (and against the raw expression of unresolved consumes)." },
                        "path": { "type": "string", "description": "Absolute path to ONE repo/tree (auto-discovers its zzop.config.jsonc, like analyze_repo)." },
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Absolute paths to 2+ repos/trees to join (paths mode, like cross_repo — each root loads its own zzop.config.jsonc).",
                            "minItems": 2
                        },
                        "configPath": { "type": "string", "description": "Path to a zzop.config.jsonc (or a directory containing one) whose trees define the analysis." }
                    },
                    "required": ["pattern"],
                    // The "exactly ONE of path/paths/configPath" rule from the description,
                    // expressed in the schema: each branch requires `pattern` plus one source, so
                    // two sources (two branches match) or zero (none match) both fail `oneOf` —
                    // mirroring the handler's own "pass exactly ONE" error.
                    "oneOf": [
                        { "required": ["pattern", "path"] },
                        { "required": ["pattern", "paths"] },
                        { "required": ["pattern", "configPath"] }
                    ]
                }
            },
            {
                "name": "check_coverage",
                // A cache writer like the other three tree-analyzing tools: it runs the same
                // `analyzeTrees` path (`zzop_summary::coverage_summary`) before post-processing, so
                // the injected `.zzop/cache` default applies and `readOnlyHint: true` would be false.
                "title": "Check how much of a repo zzop can see",
                "annotations": {
                    "title": "Check how much of a repo zzop can see",
                    "readOnlyHint": false,
                    "destructiveHint": false,
                    "idempotentHint": true,
                    "openWorldHint": false
                },
                "description": "Answer \"how much of this stack does zzop actually SEE?\" for 1+ trees — the aggregate visibility view behind the `zzop coverage` CLI twin, and the one surface built to be read BEFORE trusting a zero. Every fact in the reply is exactly one of three kinds and may only say what its source can back: MEASURED (this run: the per-extension dispatch table — structural/lexicalOnly/degraded counts per filetype, plus `inDepGraph`, the files of that extension with at least one RESOLVED outgoing dep edge, so declared-but-unresolved import blindness reads off one row), CAPABILITY (this build, true before any tree is walked: `frameworkRecognizers`, every framework recognizer compiled in with the io channels it fills, uncrossed with any tree; and `nativeAnalyses`, the roster of native analyses this build registered — including `disabled` (this caller's config turned them off) and `shippedOff` (this BUILD ships them off by default, so their silence is a default, never a clean bill)), and UNMEASURED (`unmeasured`, a FIELD rather than a caveat sentence so it cannot be dropped in transit — today it names RECALL: how many of the findings that EXIST in your tree zzop reports has never been measured on your tree, and the committed benchmark scores zzop's own labeled corpus, not yours). THERE IS DELIBERATELY NO SINGLE COVERAGE SCORE and one must never be requested: folding the axes into one number would have to either include the unmeasured axis, manufacturing a claim, or exclude it — and the number then gets quoted without its exclusion list, reading as \"zzop sees N% of my repo\" with the missing axis being exactly the one that matters. The CAPABILITY×MEASURED crosses are the cells to read first. `blindSpots` crosses each rule's own compiled-in sightline declaration with this tree's structural extensions, naming rules whose finding count carries NO information about your code here; beside it `blindSpotBasis` says what the cross was computed from AND what it excluded, so an empty array cannot be read as \"no blind spots\". `unreadExtensions` names each filetype that is a principal share of this tree and that NO structural parser read — the population the per-rule cross structurally cannot contain, since a language with no parser never becomes a structural extension and so vanishes from the very list a reader checks for blindness (measured on one tree: `blindSpots: []` beside 587 unparsed `.vue` files). Its `kind` field decides the remedy and they are not interchangeable: `\"source\"` means bring a parser adapter, `\"data-config\"` means whether anything was lost DEPENDS ON WHAT THE FILES HOLD (a MyBatis `.xml` mapper directory is hundreds of SQL statements this run never saw; an i18n `locales/*.json` bundle is nothing at all — same row until you look). `ioChannels.extracted` carries one row per io kind PRESENT EVEN AT ZERO, so one filled channel cannot vouch for an empty one, and `ioChannels.zeroExtraction` names each (channel, extension) pair this build HAS a recognizer for and that still extracted 0 here. Every list is accompanied by its own `*Meaning` legend at the reply root, stated once. Pass EITHER `paths` (1+ tree roots — each loads its own zzop.config.jsonc and must have one) OR `configPath` (a zzop.config.jsonc whose trees define the run). Unlike cross_repo this takes ONE path happily: the question is about a tree before it is about a join. `vocabularyDeclared` answers the same question one axis over — for each tree, which convention-vocabulary keys the config DECLARED and which it left SILENT, plus `silentAuth`: a silent key means the judgement that reads it did not happen, and for the auth keys silence makes the reply LOUDER rather than quieter (a guard rule that cannot recognize your guard reports every mutating route as unguarded). Read `silent` as questions nobody asked, never as clean answers. Analysis findings are NOT in this reply — ask analyze_repo/cross_repo for those; this is the reply that says whether their zeros mean anything.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Absolute paths to the repos/trees to report visibility for — ONE is legal here (unlike cross_repo's 2+). Each root loads its own zzop.config.jsonc.",
                            "minItems": 1
                        },
                        "configPath": { "type": "string", "description": "Path to a zzop.config.jsonc (or a directory containing one) whose trees define the run (config-first mode)." }
                    },
                    // Exactly ONE source mode, the same `oneOf` the two other tree-resolving tools
                    // carry. No `severity`/`rule`/`limit`: this reply lists no findings to filter, and
                    // offering the knobs would imply it does.
                    "oneOf": [
                        { "required": ["paths"] },
                        { "required": ["configPath"] }
                    ]
                }
            },
            {
                "name": "module_map",
                // A cache writer like every other tree-analyzing tool here: it runs the same
                // `analyzeTrees` path before folding, so the injected `.zzop/cache` default applies
                // and `readOnlyHint: true` would be false.
                "title": "Map a codebase into modules and their imports",
                "annotations": {
                    "title": "Map a codebase into modules and their imports",
                    "readOnlyHint": false,
                    "destructiveHint": false,
                    "idempotentHint": true,
                    "openWorldHint": false
                },
                "description": "Answer \"what IS this codebase?\" — the import graph collapsed into MODULES and the imports between them, as data. Call it FIRST on a tree you have not seen: it is the orientation reply, and it is small on purpose. Measured over this wire on 2026-09-15, all three in one run, on this engine's own 1741-file tree: `fold` 1 returns 3032 bytes — 10 modules and 6 module edges — and `fold` 2 returns 10570 bytes and 39 modules, where analyze_repo over the same tree returns 30587 bytes and carries no module map at all, only per-extension import COUNTS under `coverage`. Those figures move with the repo and are illustrative of the RATIO, not constants.\n\nA MODULE is the first `fold` segments of a file path — nothing is inferred about packages, layers or ownership. `fold` is REQUIRED because an unfolded map is the file graph, which is the thing this collapses; raise it for a finer grain. That is the ONLY knob, and it is also the answer to \"this is too big to read\", because NOTHING HERE IS CAPPED: every module and every edge at the grain you asked for is present, no argument can drop one, and there is no truncation field because there is no truncation.\n\nReturns `{fold, modules[], edges[], census, meaning}`. The reply's own `meaning` field is the ONE owner of what those keys mean and it ships with every call, so this description deliberately does not define them a second time — two owners of one vocabulary is the drift this server is built to avoid. Read it before reading the rows; it is short, and the two things it exists to stop you misreading are that `edges[]` and `census.fileImports` count different populations, and that `modules[].lines` is a partial sum whenever `linesMeasuredOver` is below that row's `files`.\n\nTHIS IS A TOPOLOGY, NEVER A VERDICT. No row says a module is badly placed; there is no severity, no score, no ranking, and no finding of any kind in this reply — a large module and a thick edge are facts about shape, not judgements about it. For findings ask analyze_repo or cross_repo; for whether this tree's zeros mean anything ask check_coverage, which is also where you learn whether a filetype reached the dependency graph at all — a language no parser read contributes no imports here, so a thin map can be a thin reading rather than a thin codebase. The CLI twin is `zzop map <path>... [--fold <n>]`, which defaults `fold` to 1 because a terminal caller is looking at the answer.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Absolute paths to the repos/trees to map — ONE is legal (a map is a question about a tree before it is a question about a join). Each root loads its own zzop.config.jsonc.",
                            "minItems": 1
                        },
                        "configPath": { "type": "string", "description": "Path to a zzop.config.jsonc (or a directory containing one) whose trees define the map (config-first mode)." },
                        "fold": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "How many leading path segments make one module. 1 is the top-level map, 2 the next grain down. Required: an unfolded map is the file graph. Spelled the way the CLI picture lane spells the same collapse (`zzop graph --domain dep --fold <n>`), because one operation gets one word. There is no maximum — a value past the deepest path simply stops folding, and refusing a number that cannot mislead would be a cap on the one knob this reply has."
                        }
                    },
                    // Exactly ONE source mode, the same `oneOf` every other tree-resolving tool
                    // carries. No `severity`/`rule`/`limit`: this reply lists no findings.
                    "oneOf": [
                        { "required": ["paths"] },
                        { "required": ["configPath"] }
                    ],
                    "required": ["fold"]
                }
            },
            {
                "name": "analyze_envelope",
                "title": "Analyze a Normalized AST envelope (Mode A)",
                // Text in, judgment out — no tree, no cache, no disk (see the honesty rule above).
                "annotations": {
                    "title": "Analyze a Normalized AST envelope (Mode A)",
                    "readOnlyHint": true,
                    "idempotentHint": true,
                    "openWorldHint": false
                },
                "description": "Run Mode A full-envelope analysis: a complete Normalized AST envelope (a custom parser's output, already validated against its contract) REPLACES native parsing entirely for this run — contrast validate_envelope, which only checks the envelope's shape and runs no analysis at all, and Mode B overlay/mount requests, which merge external symbols ON TOP of a natively-parsed tree instead of replacing it. Only symbol-scan/io-scan DSL rules can fire (an envelope carries no source text, so text-scan/regex-body rules never match — the zzop://contract/rule-catalog resource says which rules are which kind); the native call-graph rules (mutating-route-no-auth, unsafe-read-endpoint, non-idempotent-write) additionally run when the envelope supplies its `files[].calls` channel (call edges — see the zzop://contract/envelope-schema resource; an envelope with http routes and no calls keeps them silent and the reply's warnings say so). The one tool that needs no config: bundled rule packs load the same way they do for every other zzop-mcp tool, and an envelope carries no filesystem location, so there is no `config` file to auto-discover (the reply has no `config`/`path` field at all) and none to require either. DECLARED LIMIT OF THIS LANE, needed to read the findings correctly: this tool receives envelope TEXT, so there is nothing adjacent to it on disk and the run judges by zzop's BUILT-IN convention vocabulary — it does not know what THIS project calls its own auth guards, its generated-file banners or its data-access receivers, so any finding whose text points at a `vocabulary` key is naming a declaration this lane had no way to read. The same limit reaches further than the vocabulary: every bundled rule's `file_pattern` targets zzop's OWN native filetypes, so an envelope describing a language zzop has no parser for — the case Mode A exists for — matches no bundled rule at all, and this lane has no way to add one (the reply's warnings say so rather than letting the zero read as clean). The CLI twin `zzop analyze-envelope <file>` names a file, so it reads the zzop.config.jsonc sitting next to that file and applies both that project's declared `vocabulary` and its rule-pack selection (`packs.extraDirs`, `packs.disabled`, `packs.only`), disclosing it in `configWarnings`; when a project's own naming conventions decide the verdict, or when the envelope's filetype needs a pack written for it, that twin is the lane that can honor them. Returns the SAME shaped summary analyze_repo/cross_repo return otherwise: full findings counts by severity/rule, engine warnings, `packsLoaded` confirmation (whose per-pack `zeroAdmissionRules` is mode-filtered here: rules of a kind this lane never evaluates — anything but symbol-scan/io-scan — are listed even when their path patterns match, because their green is vacuous without source text), and the structural coverage census — capped lists always disclose truncation. Never carries an `architecture` field, and `gitWindow` is always present but always `null`: git signals need a working tree to diff, which an envelope does not have. Pair with the zzop://contract/envelope-guide and example-envelope resources.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "envelopeJson": { "type": "string", "description": "The Normalized AST envelope JSON text to analyze (see the zzop://contract/envelope-schema resource)." },
                        "severity": filter_props["severity"],
                        "rule": filter_props["rule"],
                        "limit": filter_props["limit"]
                    },
                    "required": ["envelopeJson"]
                }
            },
            {
                "name": "validate_envelope",
                "title": "Validate a Normalized AST envelope",
                "annotations": {
                    "title": "Validate a Normalized AST envelope",
                    "readOnlyHint": true,
                    "idempotentHint": true,
                    "openWorldHint": false
                },
                "description": "Validate a Normalized AST envelope (a custom parser's output) against its contract WITHOUT running an analysis — the authoring feedback loop. Returns {valid, issues[], hints[]}; never fails on bad input. The two lists are DIFFERENT AXES: `issues` are why the envelope is REJECTED (they alone set `valid`), while `hints` are shapes that are accepted but almost certainly not what you meant. Their consequences are NOT uniform, so read the hint text instead of assuming one: some shapes make the cross-layer join find nothing at all (an `http` key that is not the normalized \"METHOD /path\" form the join keys on; a provide key carrying a host, which is consume-side external egress only), while others still join and instead change what the run produces (an absolute files[].path is added as a synthetic entry as a Mode B overlay rather than merging onto the file it names; a duplicate provide is joined once per copy). Every hint names its own concrete consequence and the fix, and the checks themselves — not this description — are the list: they live in `zzop_core::envelope_hints`. Treat a non-empty `hints` on a valid envelope as the more urgent signal. Hints are reported for an invalid envelope too (both axes in one round-trip), and are empty when the text did not parse at all. Pair with the zzop://contract/* resources (schema, guide, key-normalization fixture).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "envelopeJson": { "type": "string", "description": "The envelope JSON text to validate." }
                    },
                    "required": ["envelopeJson"]
                }
            },
            {
                "name": "validate_rule_pack",
                "title": "Validate a DSL rule pack",
                "annotations": {
                    "title": "Validate a DSL rule pack",
                    "readOnlyHint": true,
                    "idempotentHint": true,
                    "openWorldHint": false
                },
                "description": "Validate a DSL rule pack's STRUCTURE before loading it — the exact judgments the engine's pack loader makes at load time (bad JSON, missing field, wrong type, too-new schema_version) plus every rule that would load but could silently never fire — a matcher regex that fails to compile, a line-scan declaring neither `line_pattern` nor `any`, and a method-scan whose `trigger` names a label no `patterns` entry declares. This checks shape ONLY — it never judges rule quality or semantics (whether a pattern over-matches, whether a rule is useful). Validation is also PACK-LOCAL: it cannot see any other pack, so a pack `id` colliding with a bundled or another loaded pack (which replaces it WHOLE) is invisible here — that only surfaces at load time via `packsLoaded` (and its shadow warning, when one fires); check `packsLoaded` after loading a pack this tool passed. Returns {valid, issues[]}; never fails on bad input. Pair with the zzop://contract/rule-pack-schema resource (the machine-readable shape) and the dsl-reference/dsl-authoring-guide resources.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "packJson": { "type": "string", "description": "The rule-pack JSON text to validate (one pack file — rules/dsl/<pack>/<pack>.json in-repo or a packsDir file — or one packDefs entry)." }
                    },
                    "required": ["packJson"]
                }
            }
        ]
    })
}
