# @zpz/native

N-API binding for the zpz engine (single Node<->Rust boundary — see `src/lib.rs`). This package is a thin
loader: the actual analysis runs in the `zpz-napi` Rust crate, exposed as a `.node` addon.

## Loading

`index.js` implements the standard napi-rs `optionalDependencies` cascade:

1. `require("@zpz/native-<platform>")` — a prebuilt binary, installed automatically as an optional
   dependency matching the current OS/CPU/libc.
2. `./zpz-napi.node` — a local build next to this file (development, and what `smoke.mjs` exercises).
3. Otherwise, throw with the list of supported platforms and the local-build command.

## Supported (prebuilt) platforms

| npm sub-package               | OS      | CPU   | libc  |
| ----------------------------- | ------- | ----- | ----- |
| `@zpz/native-win32-x64-msvc`  | Windows | x64   | MSVC  |
| `@zpz/native-darwin-x64`      | macOS   | x64   | —     |
| `@zpz/native-darwin-arm64`    | macOS   | arm64 | —     |
| `@zpz/native-linux-x64-gnu`   | Linux   | x64   | glibc |
| `@zpz/native-linux-arm64-gnu` | Linux   | arm64 | glibc |

These sub-packages live under `npm/<platform>/` and ship only a `package.json`, `README.md`, and (at
release time, from CI) the `zpz-napi.node` binary itself — the binary is not committed to this repo (see
`.gitignore`); it is placed there by `scripts/place-artifacts.mjs` from `prebuild.yml`'s build artifacts.

## Unsupported platform

If your platform/CPU/libc combination isn't one of the five above (notably: **musl-based Linux, e.g.
Alpine**, and **WASM** — both explicitly out of scope for now), `index.js`'s loader falls through to
`./zpz-napi.node` and, failing that, throws with build instructions. To build from source:

```sh
# Windows (MSVC toolchain required for the addon; the default toolchain in this workspace is
# windows-gnu, which cannot build the addon feature — see src/lib.rs's "Feature gating" module doc):
cargo +stable-x86_64-pc-windows-msvc build -p zpz-napi --release --features addon

# macOS / Linux:
cargo build -p zpz-napi --release --features addon
```

Then copy the produced binary (`zpz_napi.dll` / `libzpz_napi.dylib` / `libzpz_napi.so`) to
`packages/napi/zpz-napi.node`. There is currently no automated musl or WASM build path — contributions
targeting those would need their own prebuild-matrix entry (see `.github/workflows/prebuild.yml`).

## Scripts

- `node scripts/sync-versions.mjs` — copies this package's `version` into every `npm/<platform>/package.json`
  and this package's own `optionalDependencies` pins. Only used by CI at publish time (see below) — the
  `0.0.0` committed in every one of these `package.json` files is an inert placeholder, never itself
  published; don't hand-edit it.
- `node scripts/place-artifacts.mjs <artifacts-dir>` — copies CI-built `zpz-napi.<target>.node` files (named
  per `prebuild.yml`'s "Collect artifact" step) into the matching `npm/<platform>/zpz-napi.node`.
- `node smoke.mjs` (or `npm run smoke`) — end-to-end sanity check against whatever binary the loader
  resolves (prebuilt sub-package or local `./zpz-napi.node`).

## Publishing

A `v*` git tag is the only trigger and the only source of the published version — pushing e.g. `v0.1.0`
runs `.github/workflows/prebuild.yml`'s `publish` job, which builds all 5 platform targets, overwrites
every package's `0.0.0` placeholder with `0.1.0`, and runs `npm publish --provenance` for each of the 6
packages (`@zpz/native` plus the 5 `npm/<platform>/` sub-packages).

Publishing uses npm's OIDC **trusted publishing** — the workflow authenticates via GitHub Actions'
`id-token`, no `NPM_TOKEN` secret is stored anywhere. One-time setup required on npmjs.com before the
first tag, for **each of the 6 package names**: package Settings → Trusted Publisher → GitHub Actions,
repo `eezz4/zpz`, workflow file `.github/workflows/prebuild.yml`. If a package name has never been
published before, npm may require it to exist first — publish it once manually (`npm login && npm
publish --access public` from that package's directory) to claim the name, then attach the trusted
publisher for every release after that.
