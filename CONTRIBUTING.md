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

The N-API addon (the Node<->Rust boundary, `packages/napi`) is not built by default. On Windows it
requires the MSVC toolchain — the workspace's default toolchain is windows-gnu, which cannot build the
`addon` feature:

```sh
# Windows
cargo +stable-x86_64-pc-windows-msvc build -p zzop-napi --release --features addon

# macOS / Linux
cargo build -p zzop-napi --release --features addon
```

See [`packages/napi/README.md`](packages/napi/README.md) for platform/toolchain details.

## CI guards

A PR must pass every job in [`.github/workflows/ci.yml`](.github/workflows/ci.yml):

- **english-source-guard** — OSS-facing files (Rust sources, docs, manifests, rule packs) must be
  English-only, and must not reference internal (unpublished) paths.
- **swc-isolation-guard** — swc dependencies and `swc_core` usage must stay confined to
  `parser/parser-typescript`; no other crate may hold an swc AST.
- **rules-catalog-sync-guard** — `docs/rules/catalog.md` and `site/rules.html` must stay in sync
  (rule/analysis ids and source paths).
- **cli-readme-sync-guard** — `packages/cli/README.md` must stay in sync with the `--help` text
  embedded in `packages/cli/bin/zzop.js`.
- **test** — `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.
- **napi-addon-build** — builds the `zzop-napi` crate with the `addon` feature and runs its smoke
  test, proving the addon path compiles and loads.

The guard scripts live under `scripts/*.sh` and can be run locally with bash before pushing:

```sh
bash scripts/check-english-source.sh
bash scripts/check-swc-isolation.sh
bash scripts/check-rules-catalog-sync.sh
bash scripts/check-cli-readme-sync.sh
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
