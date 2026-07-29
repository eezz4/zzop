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
- Node 18+ — **only** for the local measurement harness under `scripts/measure/` (see
  [Re-measuring before you commit](#re-measuring-before-you-commit)). Nothing that ships needs it:
  both binaries are plain Rust.

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

The hook's `GUARDS` array mirrors the CI job's step order element-for-element, so a green pre-commit
means the guard half of CI is already satisfied.

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

- **CI does not run any of this**, and it is not wired to. Until 2026-07-26 the reason was that
  there was nothing to point CI at — every corpus was gitignored or lived outside the tree. That is
  no longer true: the labeled benchmark is committed at `cases/`, so a recall/precision
  gate is now *possible*. It is still not wired, and the remaining objection is cost and scope
  (a `--release` build plus a full two-axis snapshot per run), not availability — plus the one
  ungittable fixture described in the next bullet, which any such gate has to synthesize or exclude
  before it can ever go green. These stay
  developer tools, deliberately kept out of `scripts/check-*.sh` (where every file *is* CI-wired).
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
  DSL rule packs. Keep `site/rules.html`'s rule listing in sync with
  [`docs/rules/catalog.md`](docs/rules/catalog.md) (CI-checked).
- **CLI docs.** Keep `packages/README.md` in sync with `zzop help`.

## PR process

- Fork the repository and work on a branch.
- Keep PRs focused on a single change; describe any behavior changes in the PR description.
- Do not bump version numbers in PRs — published versions come from release tags, not PR content.
