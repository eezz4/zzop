# Contributing to zzop

Thanks for your interest in contributing. This document covers prerequisites, the build/test
workflow, CI gates, and conventions for PRs.

## Prerequisites

- Rust (stable). [`rust-toolchain.toml`](rust-toolchain.toml) pins the channel and requests the
  `x86_64-pc-windows-msvc` target, so `rustup` applies both automatically on your first `cargo`
  invocation in this repo — you do not need to configure anything, and you should not override it with
  `rustup default`/`rustup override`. The pin exists because the release matrix in
  [`.github/workflows/prebuild.yml`](.github/workflows/prebuild.yml) ships that MSVC target for
  Windows: a Windows box building with the `-gnu` toolchain builds against the mingw CRT, so
  `cargo test --workspace` would validate a different C runtime than the artifact users receive.
  **On Windows this target needs the MSVC linker** — install the Visual Studio Build Tools with the
  "Desktop development with C++" workload. On macOS and Linux the only effect is a one-time rust-std
  download for a target you never link against — your builds keep using your own host triple.
  - ⚠ **Windows contributors, one limit worth knowing.** A bare `stable` channel resolves against your
    rustup **host** triple, which is a different setting from `rustup default`. If your rustup was
    installed with the standard MSVC host, the pin flips you to MSVC and `cargo test --workspace`
    validates the shipped CRT. If your rustup host is `-gnu`, the pin only guarantees the MSVC target
    is *installed* — your default `cargo test` still builds mingw, and you need an explicit
    `cargo test --workspace --target x86_64-pc-windows-msvc` to test what ships. The file cannot force
    this for everyone without naming a full Windows triple, which would break macOS and Linux
    contributors outright.
- Node 18+ — **required to commit**, not optional. Two guards in the pre-commit set shell out to
  `node` and, rather than skipping a check they cannot run, hard-fail without it:
  `check-rules-catalog-sync` (verifies `site/rules.html`'s generated rule rows against
  `docs/rules/catalog.md`) and `check-license-shipping` (verifies `THIRD-PARTY-NOTICES.md` and
  unit-tests the harvest logic). Neither needs the network or an `npm install` — only `node` and
  `git`. Node is also what the local measurement harness under `scripts/measure/` runs on (see
  [Re-measuring before you commit](#re-measuring-before-you-commit)). Nothing that *ships* needs
  it: both binaries are plain Rust.

## Repository layout

- `crates/core` — engine library: Common IR, cross-layer linker, graph analyses, call graph, rule
  DSL interpreter (line/method/symbol/io matchers), unified rule registry + gating
- `crates/metrics` — score channels consumed by `engine`: roi/health/criticality/coupling/
  seams/recommendations/diagnostics
- `crates/engine` — fused execution pipeline: language dispatch (TS/Prisma/Python/Rust/Go/Java/C#/SQL) → rayon
  per-file parse + per-file rules → AST drop → whole-graph passes; graceful degrade, cache
  consumption, git/scores integration, multi-tree cross-layer join, rule profiling
- `crates/git` — git history collection (single `git log --numstat` pass → per-file stats +
  per-commit sets)
- `crates/cache` — per-file IR/findings cache (content hash + parser fingerprint + ruleset
  fingerprint)
- `crates/facade` — the analysis-meaning contract layer. Its JSON-string-in/JSON-string-out entry points
  are `analyze`/`analyzeTrees`/`analyzeEnvelope` (the analyses) plus `validateEnvelopeOnly`/
  `validateRulePackOnly`/`queryIo` (read-only lookups over engine and rule data — the verdict
  vocabulary, envelope and rule-pack validation). Two more reads answer with no JSON at all: `explain`
  returns one bundled rule's compiled-in data as human-readable lines, and `version` returns the bare
  release number. Called by `crates/summary`, no Node and no native addon in between
- `crates/config` — shared Rust config front end (`zzop.config.jsonc` discovery → JSONC strip →
  config→facade-request mapper → `trees: "auto"` workspace expansion) plus request assembly (which
  trees a call is about) and host-boundary path absolutization, used by `crates/summary`
- `crates/summary` — the host-shared answer layer BOTH products call: reply shaping, caps, filters and
  warning merging, the `facts`/`graph`/`manifest` projections, and the embedded authoring-contract
  table. It is the only zzop crate either product package ships against: `packages/cli-bin` (→ `zzop`)
  and `packages/mcp` (→ `zzop-mcp`) each declare exactly `zzop-summary` + `serde_json` as
  `[dependencies]` and nothing below it (`zzop-config`/`zzop-core` appear only as `[dev-dependencies]`,
  for test-only pins against those crates' own embeds) — which is what keeps a CLI query and an MCP tool
  call on one code path
- `packages/` — the two shipped binaries plus the npm shim and Desktop manifest
  ([packages/README.md](packages/README.md))
- `parser/` — parser frontends: source → Common IR, including HTTP route/consume extraction across
  languages and frameworks ([parser/README.md](parser/README.md))
- `rules/native/` — whole-graph native rules (`rules-graph`, `rules-http`, `rules-cross-layer`, `rules-schema`) plus `rules/dsl/`
  declarative JSON rule packs ([rules/README.md](rules/README.md))


Moved here from `README.md` on 2026-08-08: that page is read to DECIDE whether to adopt zzop, and a
per-crate dependency map answers a question nobody asks before installing. Nothing was dropped in
the move.

### Distribution surfaces — per-file responsibilities

| Location | Responsibility |
|---|---|
| `packages/cli-bin/src/main.rs` | The `zzop` CLI entry — thin argument dispatch: `analyze` / `analyze-envelope` / `cross` / `file` / `endpoint` / `manifest` / `diff` / `facts` / `graph` / `init` / `contract` / `explain` / `validate-*` / `version` / `help` subcommands calling `zzop-summary` directly (the `USAGE` const in that file is the canonical list — every subcommand but `help` is spelled there). Its findings filters are built through the WIRE-NEUTRAL `FindingFilters::new`, never an MCP-shaped JSON object. |
| `packages/cli-bin/src/cli/` | The CLI's own argv helpers, split by responsibility: `args.rs` (argv shape + the findings knobs), `help.rs` (the per-subcommand elaboration table both `zzop help` and `zzop <sub> --help` read), `analysis.rs` (the four lanes that run an analysis), `run.rs` (the remaining diverging runners), `mod.rs` (the two terminal print/read steps). |
| `packages/mcp/src/bin/zzop-mcp.rs` | The `zzop-mcp` server entry — thin: bare / `mcp` serve stdio; `version` / `help` / unknown-arg lanes. |
| `packages/mcp/src/server.rs` | The stdio JSON-RPC 2.0 loop (`initialize`, `tools/*`, `resources/*`); re-exports `version()` from `zzop-summary`. Split into seams so the protocol is testable in-process: `run_stdio` (binds real stdin/stdout), `serve` (the transport loop over any reader/writer — this is what makes *several messages on one connection* reachable from a test), `handle_line` (the trim/blank/parse-error lane, and the object-vs-array fork), `handle_batch` (maps a JSON-RPC batch through the same dispatch a lone request takes), and `handle_message(&Value) -> Option<Value>` (the dispatch, a pure function of the parsed message; `Option` is what carries "a notification gets no reply", and it is also what makes a notification-only batch produce nothing). |
| `packages/mcp/src/server/tests.rs` | The protocol table driving `handle_message`, plus the loop tests that need a real connection (sequential requests, an interleaved notification, a malformed line not ending the session, a final line with no trailing newline). Added 2026-07-29: the dispatch previously had **no** Rust test entering it, while a test file asserted it was "covered by the protocol unit tests in `server.rs`" — a claim of coverage that did not exist, which is why a family of loop defects survived several releases. |
| `packages/mcp/build.rs` + `packages/mcp/src/staleness.rs` | The "this build is old" self-report. `build.rs` bakes ONE constant — the committer date of the source `HEAD` this binary was built from (`SOURCE_DATE_EPOCH` if set and no older than the project's first commit — a pre-history stamp such as nix stdenv's 1980 placeholder is rejected; else `git log -1 --format=%ct`, else `None`); `staleness.rs` compares it against the system clock and, past a 90-day threshold, produces the notice that rides `initialize`'s `instructions` and the serve-time stderr banner. Zero network calls, and it never claims a newer release EXISTS — see [MCP surface](docs/modules/mcp.md#mcp-surface). |
| `packages/mcp/src/tools.rs` | Pure dispatch: match tool name → extract arguments → call the shared `zzop-summary` function → wrap the MCP result. No shaping logic lives here. |
| `packages/mcp/src/tools/definitions.rs` | MCP tool descriptions + input schemas (`tools/list`). |
| `packages/mcp/src/resources.rs` | MCP resource handlers (`resources/list`, `resources/read`) over the embedded authoring contracts (from `zzop_summary::contracts`). |
| `crates/summary/src/contracts.rs` | The embedded contract documents themselves — compiled into both binaries via `include_str!`, the ONE table both surfaces resolve `<name>` through (the `config-surface` row points at `zzop_config`'s embed rather than re-embedding those bytes). |
| `crates/summary/src/manifest/` | `zzop manifest` / `zzop diff` — pure functions, called straight from the CLI. CLI-only, and unlike `explain` it is recorded as a CONTRACT in [contracts/surface-parity.json](docs/contracts/surface-parity.json)'s `_cliOnlyLanes` so a later batch cannot "restore parity" without re-reading why there is none. |
| `crates/summary/src/facts.rs` | `zzop facts` — the CONSUMER half of the custom-rule extension point, emitting the uncapped post-assembly fact substrate for a user's own rule program. zzop executes nothing and ingests nothing. CLI-only, recorded in the same `_cliOnlyLanes` contract. |
| `crates/summary/src/graph/` | `zzop graph` — a graph serialized for an EXTERNAL renderer (zzop renders no pixels): a mermaid `flowchart LR` by default, or `--format cosmograph-nodes\|cosmograph-links` NDJSON tables over `--domain dep` for an interactive viewer. The one lane whose product is not a JSON document; scoped by construction (`--top` per bucket, `--scope` prefix) with every cap and filter disclosed in the document itself. CLI-only, recorded in the same `_cliOnlyLanes` contract. |
| `crates/facade/src/explain.rs` (+ `explain/render.rs`, `explain/output_ids.rs`) | `zzop explain <rule-id>`'s read-only lookup over the DSL rule data compiled into the binary (`zzop_config::BUNDLED_PACK_SOURCES`, parsed via `zzop_core::parse_dsl_pack`) plus the live native-analysis registry, and the rendering half. Lives in the facade because it is the same KIND of work as `queryIo`'s verdict vocabulary — a pure read whose answer is a meaning. CLI-only — MCP already reaches the same data through the `rule-catalog` embedded contract resource, so it has no `tools/call` twin. |
| `crates/facade/src/version.rs` | `version()` and `version_string()` — one owner, two forms over `CARGO_PKG_VERSION`, so the CLI's `version`, `zzop-mcp version` and MCP `initialize` can never disagree. |
| `crates/config/src/trees.rs` (+ `paths.rs`) | Request assembly: which trees a call is about (`path` XOR `paths` XOR `configPath`, single-tree-vs-join judgment, per-path config loading in paths mode) and host-boundary path absolutization — next to the config vocabulary that decides what a valid tree declaration is. |

Everything functional — output shaping (full counts, capped lists, explicit truncation disclosure;
see [Output contract](docs/modules/mcp.md#output-contract)), finding filters, bucket keys/sites, typo
suggestions, the architecture summary, config-warnings merging, sibling-scope
disclosure — is **not** in either product crate: it lives in the shared
`zzop-summary` crate (`crates/summary`), whose crate doc states the rule: hosts are thin protocol
facades, ALL summary logic is shared so it cannot drift per-host (the surface-parity contract at
[contracts/surface-parity.json](docs/contracts/surface-parity.json) machine-checks the complement:
every engine output field is either carried or documented-omitted per surface). The config
front-end (`zzop.config.jsonc` discovery, JSONC parsing, config→request mapping, `trees: "auto"`
workspace expansion, and the request assembly above) likewise lives in the shared `zzop-config` crate
(`crates/config`).
Dependency-wise each product package declares exactly `zzop-summary` + `serde_json` under
`[dependencies]` and nothing below it. Crates lower down appear only under `[dev-dependencies]`, for
test-only pins against those crates' own embeds — `zzop-config` in both packages (the `config-surface`
contract bytes, and a real bundled pack as `validate_rule_pack`'s happy-path fixture) and `zzop-core` in
`packages/cli-bin` (loading every bundled DSL pack the way `explain` does, so no hand-copied rule-id
list can drift). Nothing that SHIPS reaches below `zzop-summary`, because `zzop-summary` re-exports the
handful of `zzop-facade` entry points a product uses verbatim (`explain`, `version`, the two offline
validators) rather than wrapping them, so "a host needs only `zzop-summary`" holds without a
pass-through layer that could drift.



Moved here from `docs/modules/mcp.md` on 2026-08-08: it named which source file holds which test,
which is a fact about working ON zzop rather than about using it, and that page is a user-facing
contract document.

## Build & test

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings   # kept at 0 warnings
cargo fmt --all
```

zzop ships two plain Rust binaries, each its own Cargo package over the shared `zzop-summary` lib crate —
`zzop` (package `zzop-cli-bin`, the CLI) and `zzop-mcp` (package `zzop-mcp`, the MCP server) — no Node,
no native-addon toolchain needed to build them:

```sh
cargo build -p zzop-cli-bin -p zzop-mcp --release
```

See [`packages/README.md`](packages/README.md) for build/toolchain details. Pushing a workspace
version bump to `main` auto-tags and releases both binaries (the `meta` job in
[`.github/workflows/prebuild.yml`](.github/workflows/prebuild.yml)) — see [VERSIONING.md](VERSIONING.md)
for details.

### Pointing an MCP client at your local build (dogfooding)

The published install paths (the Claude Code plugin, the `.mcpb` Desktop bundle) all resolve a
RELEASED binary — none of them will ever pick up the one you just built. To test your working tree,
point the client straight at the build output with an absolute path. In the repo you want to analyze,
create `.mcp.json`:

```jsonc
{
  "mcpServers": {
    "zzop": {
      "command": "/absolute/path/to/zzop/target/release/zzop-mcp",  // .exe on Windows
      "args": ["mcp"]
    }
  }
}
```

Then restart the client (MCP servers are spawned once, at session start). On startup `zzop-mcp` writes
its own version to stderr, which the client shows in its server log — check that line first whenever a
result looks like it came from a different build than you expect.

Note the plugin route (`/plugin marketplace add eezz4/zzop` then `/plugin install`) installs a
RELEASED binary into the plugin's data directory via its `SessionStart` hook — it will never pick up
your working tree, and it will not overwrite a binary that is already there. Use the `.mcp.json`
above for local work; the two live side by side.

## CI guards

A PR must pass every job in [`.github/workflows/ci.yml`](.github/workflows/ci.yml). **That workflow is
the canonical list — this page deliberately does not restate the job names.** The mechanical guards used
to be one job each and are now consolidated into a single job running them as named steps, so a
hand-copied job list here would go stale invisibly; ci.yml's own maintainer note spells out the sharper
version of the same hazard (a branch-protection rule on `main` still listing the old per-guard job names
**silently voids the gate**). Read ci.yml for what must pass; read the step names in it for which guard
failed.

The guards themselves are `scripts/check-*.sh`, one script per step, and can all be run locally with
bash before pushing — `scripts/check-guards-wired.sh` is the meta-guard that fails when a
`scripts/check-*.sh` exists but is not wired into **both** the pre-commit hook and the CI job, so the
scripts directory and the workflow cannot drift apart in either direction. A handful are worth knowing
about before you write code, because
they gate how a change must be SHAPED rather than merely whether it compiles:

- **English-only source** (`check-english-source.sh`) — OSS-facing files (Rust sources, docs, manifests,
  rule packs) must be English-only, and must not reference internal (unpublished) paths.
- **Parser-dependency isolation** (`check-swc-isolation.sh`, `check-ruff-isolation.sh`,
  `check-syn-isolation.sh`, `check-tree-sitter-isolation.sh`) — each parser backend's dependencies and
  AST types stay confined to that parser crate; no other crate may hold one. An upgrade's
  re-verification scope is then one crate, not the workspace.
- **Max file lines** (`check-max-file-lines.sh`) — Rust **source** files stay under 300 lines (oversized
  files are split into directory modules). Test files are exempt and may grow freely — keep unit tests
  out of the source file, paired beside it (`foo.rs` + `foo_test.rs`, or `foo/tests.rs`); `tests/`
  directories and `rules/dsl` pack tests are exempt by path. Pre-existing source violations are frozen
  in `scripts/max-file-lines-baseline.txt` and may only shrink (ratchet).
- **Doc/catalog sync** (`check-rules-catalog-sync.sh`, `check-docs-rule-ids.sh`,
  `check-docs-link-graph.sh`, `check-io-key-vocab.sh`) — `docs/rules/catalog.md` and `site/rules.html`
  must agree; every rule id in a user-facing `rules:` config example must resolve against the catalog, so
  a stale example cannot silently become a no-op; every `docs/**/*.md` page must be reachable from the
  docs hub (`docs/README.md`) and every entry under an examples hub from that hub's `README.md`
  (`examples/README.md`, `examples/adapters/README.md`); and a vocabulary
  restated in prose must match its SSOT in code.
- **Census guard** (`check-policy-census.sh`) — a new policy-shaped
  constant must be triaged into `scripts/policy-census.txt`, and the triage VERDICT is part of the
  snapshot — every line carries an axis (`fact` / `convention` / `cap` / `internal` / `test`), and
  `--update` writes `?` for a name it has not seen before, so regenerating cannot make the guard green.
  The script header states what each axis means and the one question that separates them. That script
  also asserts its own blind spots: every `const NAME: <TYPE>` in the scanned directories must have its
  TYPE either read by the census or explicitly waived with a written reason, so a vocabulary spelled in
  an unread shape can no longer be invisible and silent at the same time. The sibling axis — a new named `${...}`
  fragment in a DSL rule pack — is censused by `cargo test` instead, in
  `crates/core/src/dsl/tests_fragments/name_census.rs`, because reading it needs a real JSON parser
  rather than a line scan.
- **Release-version propagation** (`check-release-version-propagation.sh`) — `Cargo.toml`'s
  `[workspace.package] version` is the version SSOT, and every committed version on the release surface
  must equal it in the SAME commit: `.claude-plugin/plugin.json`, `server.json`'s top-level `version`
  and each `packages[].version`, and the `releases/download/v<version>/` URL in each
  `packages[].identifier`. The subject set is derived from every semver string in tracked `*.json`
  rather than hand-listed, minus a few exemptions — chiefly the `0.0.0` placeholder that the release
  workflow rewrites at publish time (`packages/cli/**`, `packages/mcpb/manifest.json`). The current
  exemption list, and the reason beside each one, lives in `scripts/check-release-version-propagation.sh`'s
  own header; it is not restated here, because a second copy of it is what goes stale. This runs on every commit rather than only on release
  commits, because a manifest that disagrees with the SSOT is wrong whatever the commit is about — and
  because the workflow jobs that ask the same question run *after* the push that creates the tag, which
  is too late for a tag that must never move.
- **Convention-vocabulary declarability** (`check-convention-vocab-declarable.sh`) — a name vocabulary the
  census marks `convention` is a name the *project* picks (guard functions, secret parameters, money
  fields, ignored directories), so the engine must not hold it as a built-in guess: it needs a key in
  `crates/config/config-surface.json` that the starter template (`crates/config/src/template.rs`) names,
  recorded as `-> <configPath>` after the axis on its census line. Vocabulary that predates the rule is
  frozen in `scripts/convention-vocab-baseline.txt` (ratchet — shrink only, expected to reach zero); that
  file is debt, not an exemption list, and `--update-baseline` refuses to add to it.

Beyond the guards, CI builds and tests the workspace (`cargo fmt --all --check`, `cargo clippy
--workspace --all-targets -- -D warnings`, `cargo test --workspace`) and separately proves the
`@zzop/cli` npm shim works against a real natively-built `zzop` binary.

To run the fast guards automatically before every commit, enable the committed git hooks once per
clone (plain git, no husky or npm dependency):

```sh
git config core.hooksPath .githooks
```

The hook's `GUARDS` array mirrors the CI `guards` job's step order element-for-element (bound by
`scripts/check-guards-wired.sh`), so a green pre-commit means that job is satisfied — but `guards` is
one of CI's five jobs, not its guard *half*. The other four (`test`, `cli-shim-test`,
`detection-benchmark`, `site-render-check`) had no local counterpart at all until `scripts/ci-local.sh`,
which runs them in CI's order; that gap is how v0.28.0's tag went red on an immutable tag. Run
`bash scripts/ci-local.sh` (or `--fast`, which skips the two jobs needing a release build — the two
that caught v0.28.0) before pushing anything you care about.

## Re-measuring before you commit

`cargo test` proves you did what you intended. It does not prove the change did what you expected to
**real code** — fixtures are the shape you imagined, a corpus is the shape that exists. So if your
change can move which findings fire, run the analyzer over a real multi-tree corpus **before and
after**, and read the difference. The harness for that lives in `scripts/measure/`:

```sh
cargo build -p zzop-mcp --release

# 1. snapshot the baseline (e.g. a worktree at the commit you branched from)
node scripts/measure/snapshot.mjs --label base-<commit> \
     --bin /path/to/baseline/target/release/zzop-mcp --config /path/to/corpus/zzop.config.jsonc

# 2. snapshot your working tree
node scripts/measure/snapshot.mjs --label mine-<what-changed> \
     --bin ./target/release/zzop-mcp --config /path/to/corpus/zzop.config.jsonc

# 3. read the difference
node scripts/measure/diff.mjs base-<commit> mine-<what-changed>
```

Before pushing a change that can move findings, also score the labeled benchmark — the same thing
CI's `detection-benchmark` job runs:

```sh
bash scripts/measure/detection-gate.sh   # takes NO arguments; it builds the binary it scores
```

It builds its own binary on purpose. It used to accept a path, and on 2026-07-29 a leftover 0.24.0
binary scored a green `TP 143 FN 0 FP 0` against a 0.25.0 tree — the gate certified a build nobody
had made. CI runs this only **after** a push, so this local run is the only place a detection
regression is visible beforehand, which is exactly why it must not be able to score the wrong bytes.

**Read the anchor difference, not the count table.** `diff.mjs` prints, as its primary output, which
`(tree, rule, file, line)` anchors disappeared and which appeared. A per-rule count delta cannot tell
you which of those happened: a rule going 6 → 2 has already been measured here to mean "three old
findings gone and one new true positive found", which is neither "4 fixed" nor a regression. If your
count moved, say which anchors moved, and check the ones that appeared as carefully as the ones that
left.

**Both axes always run.** `snapshot.mjs` calls `analyze_repo` once per tree *and* `cross_repo` once
over the whole config, because `cross-layer/*` findings exist only in the multi-tree join reply — a
re-measurement done with single-tree analysis alone cannot observe those rules at all, and will
report "no change" for a rule it never ran.

**The harness aborts instead of reporting a zero.** It fails loudly on a non-zero exit, an empty
stdout, a reply that contains only the MCP `initialize` frame, a JSON-RPC or tool-level error, a
missing payload key, and on *any* truncated list — a capped findings list silently shrinks the set
difference, and a shrunken set difference reads as "identical". It also refuses to reuse an existing
run label, so a re-run can never overwrite a baseline someone else is still comparing against. If it
aborts, nothing was measured; do not read the previous snapshot as the answer.

The harness drives the `zzop-mcp` binary rather than the `zzop` CLI, and the reason it was written
that way is gone: the CLI had no `--limit` flag, so it was pinned to the default findings cap and
quietly clipped any tree above it. Both surfaces now take the same `severity`/`rule`/`limit` filters
through the same shared validator, so either transport could carry this today. Nobody has needed the
switch, so it has not been made — which is a different sentence from "the CLI cannot do this."

Two honest limitations:

- **CI runs the recall/precision axis, and only that one.** Since the labeled benchmark was committed
  at `cases/` (2026-07-26), a gate became possible and is now wired: ci.yml's `detection-benchmark`
  job scores `cases/` against its adjudicated `EXPECTED.jsonc` on every run, kept a separate job so a
  red score reads as "detection regressed" rather than "the test job failed". What CI still does **not**
  run is everything else on this page — the drift check and the full two-axis snapshot — because those
  need the ungittable corpus described in the next bullet, which any such gate would have to synthesize
  or exclude before it could ever go green. Those stay developer tools, deliberately kept out of
  `scripts/check-*.sh` (where every file *is* CI-wired).
- **Bring your own corpus for the drift check.** Any repository, or set of repositories, with a
  `zzop.config.jsonc` declaring 2+ trees works — that axis needs real code, which this repo cannot
  ship. The recall/precision axis does not: score `cases/` (in-tree, labeled) against its
  `EXPECTED.jsonc` with `scripts/measure/benchmark.mjs`. **Expect a nonzero exit on a fresh clone,
  and read it before you read the engine:** one fixture in that corpus is named individually in
  `.gitignore`, so even a perfect run reports false negatives on that fixture's line and exits 1.
  Recreating the file does not help — the analyzer honors `.gitignore` and cannot see the path either
  way. `benchmark.mjs` names the file and the reason next to those FN (it reads `EXPECTED.jsonc`'s
  `untracked` list); `.gitignore` carries the full why — a live vendor-token literal is exactly what
  push protection and our own commit-time guard reject — and the two ways out.

## Conventions

- **English-only.** All source, comments, and docs are English (enforced by the english-source
  guard). Do not link to internal/unpublished paths from OSS-facing files.
- **Rule contributions.** Follow [`docs/rules/authoring-guide.md`](docs/rules/authoring-guide.md) for
  DSL rule packs. Write the rule's row in [`docs/rules/catalog.md`](docs/rules/catalog.md) — that file
  is the SSOT — then run `node scripts/gen-site-rules.mjs`, which rewrites `site/rules.html`'s rule
  rows from it. Do not hand-edit those rows; a guard rejects any that the catalog does not generate.
- **CLI docs.** Keep `packages/README.md` in sync with `zzop help`.

## PR process

- Fork the repository and work on a branch.
- Keep PRs focused on a single change; describe any behavior changes in the PR description.
- Do not bump version numbers in PRs — published versions come from release tags, not PR content.


