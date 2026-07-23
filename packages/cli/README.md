# @zzop/cli

An npm packaging of the native `zzop` CLI binary — identical to the binary [GitHub
Releases](https://github.com/eezz4/zzop/releases) ships, same subcommands (`zzop analyze`, `zzop cross`,
`zzop endpoint`, `zzop analyze-envelope`, `zzop validate-envelope`, `zzop validate-rule-pack`, `zzop
contract`, …). This package carries **no logic of its own** — `bin/zzop.js` is a thin launcher that
resolves the right platform binary and passes every argument straight through
(`spawnSync(binaryPath, process.argv.slice(2), { stdio: 'inherit' })`), exiting with the child's own
status code. For the full command/flag reference, config file format, and output contract, see the repo
docs: [docs/modules/mcp.md](../../docs/modules/mcp.md) (the CLI and `zzop-mcp` share one binary crate and
one analysis path) and [docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md).

Programmatic/JS use: run this CLI with its JSON output (`zzop analyze . | jq ...`, or `spawnSync`/`execFile`
it from Node) rather than importing an SDK — there is no separate Node binding to install; the JSON
contract itself is documented in [docs/modules/mcp.md](../../docs/modules/mcp.md).

## Install

```sh
npm i -g @zzop/cli
# or one-off, no install:
npx @zzop/cli analyze .
```

Requires Node.js >= 18 (Node is only used to run this launcher — the analysis itself is a native binary,
no Node runtime dependency beyond that).

## How it resolves the binary

1. `@zzop/cli-<platform>` — a prebuilt binary, installed automatically as an optional dependency matching
   your OS/CPU/libc.
2. `<repo root>/target/release/zzop[.exe]` — a dev fallback, so a source checkout works right after
   `cargo build -p zzop-cli-bin --release`, with no npm install at all.
3. Otherwise, a clear error listing the supported platforms and the build command above.

## Supported platforms

| npm sub-package               | OS      | CPU   | libc  |
| ------------------------------ | ------- | ----- | ----- |
| `@zzop/cli-win32-x64-msvc`     | Windows | x64   | MSVC  |
| `@zzop/cli-darwin-x64`         | macOS   | x64   | —     |
| `@zzop/cli-darwin-arm64`       | macOS   | arm64 | —     |
| `@zzop/cli-linux-x64-gnu`      | Linux   | x64   | glibc |
| `@zzop/cli-linux-arm64-gnu`    | Linux   | arm64 | glibc |

musl-based Linux (e.g. Alpine) and WASM are out of scope. On an unsupported platform, build from source
(see above) and run `target/release/zzop` directly, or place it where `bin/zzop.js`'s dev-fallback path
looks for it.

## Publishing

A release run of [`.github/workflows/prebuild.yml`](../../.github/workflows/prebuild.yml) — triggered by
a `v*` tag, or auto-tagged by the `meta` job when a version bump lands on `main` — is the only source of
the published version. Its `publish` job builds all 5 platform targets, overwrites every package's `0.0.0`
placeholder with the release version, and runs `npm publish --provenance` for each of the 6 packages
(`@zzop/cli` plus its 5 `npm/<platform>/` sub-packages) — this package's own 5 are placed by
`scripts/place-artifacts.mjs` from the workflow's `zzop-<platform>[.exe]` build artifacts before publish.

Publishing uses npm's OIDC **trusted publishing** — the workflow authenticates via GitHub Actions'
`id-token`, no `NPM_TOKEN` secret is stored anywhere. One-time setup required on npmjs.com before the first
release, for **each of this package's 5 platform sub-package names** (`@zzop/cli-win32-x64-msvc`,
`@zzop/cli-darwin-x64`, `@zzop/cli-darwin-arm64`, `@zzop/cli-linux-x64-gnu`,
`@zzop/cli-linux-arm64-gnu`) plus `@zzop/cli` itself: package Settings → Trusted Publisher → GitHub
Actions, repo `eezz4/zzop`, workflow file `.github/workflows/prebuild.yml`. If a package name has never
been published before, npm requires it to exist first — publish it once manually (`npm login && npm
publish --access public` from that package's directory) to claim the name, then attach the trusted
publisher for every release after that.

## License

MIT
