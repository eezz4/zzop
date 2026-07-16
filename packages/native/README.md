# @zzop/native

N-API binding for the zzop engine (single Node<->Rust boundary — see `src/lib.rs`). This package is a thin
loader: the actual analysis runs in the `zzop-napi` Rust crate (a thin `#[napi]` shim, feature-gated as
`addon`, over the shared napi-free `zzop-facade` crate — see [docs/modules/napi.md](../../docs/modules/napi.md)),
exposed as a `.node` addon.

## Loading

`index.js` implements the standard napi-rs `optionalDependencies` cascade:

1. `require("@zzop/native-<platform>")` — a prebuilt binary, installed automatically as an optional
   dependency matching the current OS/CPU/libc.
2. `./zzop-napi.node` — a local build next to this file (development, and what `smoke.mjs` exercises).
3. Otherwise, throw with the list of supported platforms and the local-build command.

## Supported (prebuilt) platforms

| npm sub-package               | OS      | CPU   | libc  |
| ----------------------------- | ------- | ----- | ----- |
| `@zzop/native-win32-x64-msvc`  | Windows | x64   | MSVC  |
| `@zzop/native-darwin-x64`      | macOS   | x64   | —     |
| `@zzop/native-darwin-arm64`    | macOS   | arm64 | —     |
| `@zzop/native-linux-x64-gnu`   | Linux   | x64   | glibc |
| `@zzop/native-linux-arm64-gnu` | Linux   | arm64 | glibc |

These sub-packages live under `npm/<platform>/` and ship only a `package.json`, `README.md`, and (at
release time, from CI) the `zzop-napi.node` binary itself — the binary is not committed to this repo (see
`.gitignore`); it is placed there by `scripts/place-artifacts.mjs` from `prebuild.yml`'s build artifacts.

## Unsupported platform

If your platform/CPU/libc combination isn't one of the five above (notably: **musl-based Linux, e.g.
Alpine**, and **WASM** — both explicitly out of scope for now), `index.js`'s loader falls through to
`./zzop-napi.node` and, failing that, throws with build instructions. To build from source:

```sh
# Windows (MSVC toolchain required for the addon; the default toolchain in this workspace is
# windows-gnu, which cannot build the addon feature — see src/lib.rs's "Feature gating" module doc):
cargo +stable-x86_64-pc-windows-msvc build -p zzop-napi --release --features addon

# macOS / Linux:
cargo build -p zzop-napi --release --features addon
```

Then copy the produced binary (`zzop_napi.dll` / `libzzop_napi.dylib` / `libzzop_napi.so`) to
`packages/native/zzop-napi.node`. There is currently no automated musl or WASM build path — contributions
targeting those would need their own prebuild-matrix entry (see `.github/workflows/prebuild.yml`).

## Scripts

- `node scripts/sync-versions.mjs` (or `npm run sync-versions`) — copies this package's `version` into every
  `npm/<platform>/package.json` and this package's own `optionalDependencies` pins. Only used by CI at
  publish time (see below) — the `0.0.0` committed in every one of these `package.json` files is an inert
  placeholder, never itself published; don't hand-edit it.
- `node scripts/place-artifacts.mjs <artifacts-dir>` — copies CI-built `zzop-napi.<target>.node` files (named
  per `prebuild.yml`'s "Collect artifact" step) into the matching `npm/<platform>/zzop-napi.node`.
- `node scripts/copy-rules.mjs` — runs automatically as this package's `prepack` script (`npm pack`/`npm
  publish`, never invoked by hand in normal use): copies the repo's DSL rule packs
  (`<repo root>/rules/dsl/`) into `packages/native/rules/`, so `index.js`'s bundled-packs default
  (see [docs/modules/napi.md](../../docs/modules/napi.md#defaults-zero-config--full-analysis)) finds them
  at `path.join(__dirname, 'rules')` in an installed package with no repo-relative path assumptions.
- `node smoke.mjs` (or `npm run smoke`) — end-to-end sanity check against whatever binary the loader
  resolves (prebuilt sub-package or local `./zzop-napi.node`).

## Publishing

A `v*` git tag is the only trigger and the only source of the published version — pushing e.g. `v0.1.0`
runs `.github/workflows/prebuild.yml`'s `publish` job, which builds all 5 platform targets, overwrites
every package's `0.0.0` placeholder with `0.1.0`, and runs `npm publish --provenance` for each of the 7
packages (`@zzop/native`, the 5 `npm/<platform>/` sub-packages, plus `@zzop/cli`).

Publishing uses npm's OIDC **trusted publishing** — the workflow authenticates via GitHub Actions'
`id-token`, no `NPM_TOKEN` secret is stored anywhere. One-time setup required on npmjs.com before the
first tag, for **each of the 7 package names**: package Settings → Trusted Publisher → GitHub Actions,
repo `eezz4/zzop`, workflow file `.github/workflows/prebuild.yml`. If a package name has never been
published before, npm may require it to exist first — publish it once manually (`npm login && npm
publish --access public` from that package's directory) to claim the name, then attach the trusted
publisher for every release after that.
