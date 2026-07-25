# Contributing to zzop

Thanks for your interest in contributing. This document covers prerequisites, the build/test
workflow, CI gates, and conventions for PRs.

## Prerequisites

- Rust (stable toolchain)
- Node 18+ — **only** for the local measurement harness under `scripts/measure/` (see
  [Re-measuring before you commit](#re-measuring-before-you-commit)). Nothing that ships needs it:
  both binaries are plain Rust.

## Build & test

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings   # kept at 0 warnings
cargo fmt --all
```

zzop ships two plain Rust binaries, each its own Cargo package over the shared `zzop-host` lib crate —
`zzop` (package `zzop-cli-bin`, the CLI) and `zzop-mcp` (package `zzop-mcp`, the MCP server) — no Node,
no native-addon toolchain needed to build them:

```sh
cargo build -p zzop-cli-bin -p zzop-mcp --release
```

See [`crates/host/README.md`](crates/host/README.md) for build/toolchain details. Pushing a workspace
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

A PR must pass every job in [`.github/workflows/ci.yml`](.github/workflows/ci.yml):

- **english-source-guard** — OSS-facing files (Rust sources, docs, manifests, rule packs) must be
  English-only, and must not reference internal (unpublished) paths.
- **swc-isolation-guard** — swc dependencies and `swc_core` usage must stay confined to
  `parser/parser-typescript`; no other crate may hold an swc AST.
- **ruff-isolation-guard** — the same discipline for the Python parser: `ruff_*` dependencies
  and AST usage stay confined to `parser/parser-python-3`.
- **rules-catalog-sync-guard** — `docs/rules/catalog.md` and `site/rules.html` must stay in sync
  (rule/analysis ids and source paths).
- **docs-rule-ids-guard** — every bare/`{pack}/{rule}` id used in a user-facing `rules:` config
  example (the getting-started doc, marketing site) must resolve against the rule catalog, so a
  stale example can't silently become a no-op.
- **docs-link-graph-guard** — every `docs/**/*.md` page must be referenced from the docs hub
  (`docs/README.md`), and every `examples/` entry from `examples/README.md`, so a new page cannot
  ship orphaned from the surfaces readers start at.
- **io-key-vocab-guard** — the io-key kind vocabulary ("http routes, env keys, DB tables,
  topics") stated in `crates/host/README.md`'s `check_endpoint` row must match its SSOT, the
  `check_endpoint` tool description in `packages/mcp/src/tools/definitions.rs`.
- **max-file-lines-guard** — Rust **source** files stay under 300 lines (oversized files are
  split into directory modules). Test files are exempt and may grow freely — keep unit tests
  out of the source file, paired beside it (`foo.rs` + `foo_test.rs`, or `foo/tests.rs`);
  `tests/` directories and `rules/dsl` pack tests are exempt by path. Pre-existing source
  violations are frozen in `scripts/max-file-lines-baseline.txt` and may only shrink (ratchet).
- **drift-guards** — a parser-fingerprint-bump guard (a parser crate's `src/**` changed without
  bumping its `PARSER_FINGERPRINT` const; a parser crate with a `src/` but no such const at all
  fails outright; a change to `crates/core`'s shared projected-type surface without a
  `CACHE_SCHEMA_VERSION` bump also fails — see the script's core section) and a policy-value
  census guard (a new policy-shaped constant must be triaged into `scripts/policy-census.txt`). The
  sibling axis — a new named `${...}` fragment in a DSL rule pack — is censused by `cargo test`
  instead, in `crates/core/src/dsl/tests_fragments/name_census.rs`, because reading it needs a real
  JSON parser rather than a line scan.
- **test** — `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.

The guard scripts live under `scripts/*.sh` and can be run locally with bash before pushing:

```sh
bash scripts/check-english-source.sh
bash scripts/check-swc-isolation.sh
bash scripts/check-ruff-isolation.sh
bash scripts/check-rules-catalog-sync.sh
bash scripts/check-docs-rule-ids.sh
bash scripts/check-docs-link-graph.sh
bash scripts/check-io-key-vocab.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-parser-fingerprint-bump.sh
bash scripts/check-policy-census.sh
```

To run the fast guards automatically before every commit, enable the committed git hooks once per
clone (plain git, no husky or npm dependency):

```sh
git config core.hooksPath .githooks
```

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

This is why the harness drives the `zzop-mcp` binary rather than the `zzop` CLI: the CLI has no
`--limit` flag, so it is pinned to the default findings cap and quietly clips any tree above it.

Two honest limitations:

- **CI does not run any of this**, and it is not wired to — every corpus this repo measures against
  is gitignored or lives outside the tree, so there would be nothing for CI to point at. These are
  developer tools, deliberately kept out of `scripts/check-*.sh` (where every file *is* CI-wired).
- **Bring your own corpus.** Any repository, or set of repositories, with a `zzop.config.jsonc`
  declaring 2+ trees works. A synthetic labeled corpus can additionally be scored for
  recall/precision against a ground-truth file with `scripts/measure/benchmark.mjs`.

## Conventions

- **English-only.** All source, comments, and docs are English (enforced by the english-source
  guard). Do not link to internal/unpublished paths from OSS-facing files.
- **Rule contributions.** Follow [`docs/rules/authoring-guide.md`](docs/rules/authoring-guide.md) for
  DSL rule packs. Keep `site/rules.html`'s rule listing in sync with
  [`docs/rules/catalog.md`](docs/rules/catalog.md) (CI-checked).
- **CLI docs.** Keep `crates/host/README.md` in sync with `zzop help`.

## PR process

- Fork the repository and work on a branch.
- Keep PRs focused on a single change; describe any behavior changes in the PR description.
- Do not bump version numbers in PRs — published versions come from release tags, not PR content.
