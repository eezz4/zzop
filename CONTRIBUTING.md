# Contributing to zzop

Thanks for your interest in contributing. This document covers prerequisites, the build/test
workflow, CI gates, and conventions for PRs.

## Prerequisites

- Rust (stable toolchain)
- Node.js >= 18

## Build & test

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings   # kept at 0 warnings
cargo fmt --all
```

The N-API addon (the Node<->Rust boundary, `packages/native`) is not built by default. On Windows it
requires the MSVC toolchain — the workspace's default toolchain is windows-gnu, which cannot build the
`addon` feature:

```sh
# Windows
cargo +stable-x86_64-pc-windows-msvc build -p zzop-napi --release --features addon

# macOS / Linux
cargo build -p zzop-napi --release --features addon
```

See [`packages/native/README.md`](packages/native/README.md) for platform/toolchain details.

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
- **cli-readme-sync-guard** — `packages/cli/README.md` must stay in sync with the `--help` text
  embedded in `packages/cli/bin/zzop.js`.
- **docs-rule-ids-guard** — every bare/`{pack}/{rule}` id used in a user-facing `rules:` config
  example (README, init template, getting-started doc, marketing site) must resolve against the rule
  catalog, so a stale example can't silently become a no-op.
- **docs-link-graph-guard** — every `docs/**/*.md` page must be referenced from the docs hub
  (`docs/README.md`), and every `examples/` entry from `examples/README.md`, so a new page cannot
  ship orphaned from the surfaces readers start at.
- **site-sdk-tokens-guard** — `site/sdk.html` must stay in sync with the real `@zzop/native`
  surface (`packages/native/index.d.ts`): every exported function appears as a `<code>` token,
  the page's stated function count matches the real export count, and the function table lists
  only real exports.
- **io-key-vocab-guard** — the io-key kind vocabulary ("http routes, env keys, DB tables,
  topics") stated in `packages/cli/README.md`'s `zzop endpoint` row and `packages/mcp/README.md`'s
  `check_endpoint` row must match its SSOT, the `check_endpoint` tool description in
  `packages/mcp/src/tools/definitions.rs`.
- **napi-request-fields-guard** — `docs/modules/napi.md`'s `AnalyzeRequest` field table must
  list exactly the pub fields of `crates/facade/src/request.rs`'s `AnalyzeRequest` struct
  (camelCase wire names, bidirectional set comparison — a rename fails as one missing plus one
  extra).
- **max-file-lines-guard** — Rust **source** files stay under 300 lines (oversized files are
  split into directory modules). Test files are exempt and may grow freely — keep unit tests
  out of the source file, paired beside it (`foo.rs` + `foo_test.rs`, or `foo/tests.rs`);
  `tests/` directories and `rules/dsl` pack tests are exempt by path. Pre-existing source
  violations are frozen in `scripts/max-file-lines-baseline.txt` and may only shrink (ratchet).
- **drift-guards** — a parser-fingerprint-bump guard (a parser crate's `src/**` changed without
  bumping its `PARSER_FINGERPRINT` const; a parser crate with a `src/` but no such const at all
  fails outright; a change to `crates/core`'s shared projected-type surface without a
  `CACHE_SCHEMA_VERSION` bump also fails — see the script's core section) and a policy-value
  census guard (a new policy-shaped constant must be triaged into `scripts/policy-census.txt`).
- **test** — `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.
- **napi-addon-build** — builds the `zzop-napi` crate with the `addon` feature and runs its smoke
  test, proving the addon path compiles and loads.

The guard scripts live under `scripts/*.sh` and can be run locally with bash before pushing:

```sh
bash scripts/check-english-source.sh
bash scripts/check-swc-isolation.sh
bash scripts/check-ruff-isolation.sh
bash scripts/check-rules-catalog-sync.sh
bash scripts/check-cli-readme-sync.sh
bash scripts/check-docs-rule-ids.sh
bash scripts/check-docs-link-graph.sh
bash scripts/check-site-sdk-tokens.sh
bash scripts/check-io-key-vocab.sh
bash scripts/check-napi-request-fields.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-parser-fingerprint-bump.sh
bash scripts/check-policy-census.sh
```

To run the fast guards automatically before every commit, enable the committed git hooks once per
clone (plain git, no husky or npm dependency):

```sh
git config core.hooksPath .githooks
```

## Conventions

- **English-only.** All source, comments, and docs are English (enforced by the english-source
  guard). Do not link to internal/unpublished paths from OSS-facing files.
- **Rule contributions.** Follow [`docs/rules/authoring-guide.md`](docs/rules/authoring-guide.md) for
  DSL rule packs. Keep `site/rules.html`'s rule listing in sync with
  [`docs/rules/catalog.md`](docs/rules/catalog.md) (CI-checked).
- **CLI docs.** Keep `packages/cli/README.md` in sync with `zzop --help` (CI-checked).

## PR process

- Fork the repository and work on a branch.
- Keep PRs focused on a single change; describe any behavior changes in the PR description.
- Do not bump version numbers in PRs — published versions come from release tags, not PR content.
