#!/usr/bin/env node
'use strict';

// `zzop` — npm packaging of the native `zzop` CLI binary (Option A, 2026-07-23: the JS
// reimplementation that used to live in lib/ duplicated crates/config + crates/summary + the
// native CLI's own arg dialect and inevitably drifted from it — see 1.decisions/. This shim carries
// NO logic of its own: it resolves a platform-specific binary and hands off every argument
// unmodified.
//
// Resolution cascade, in order:
//   1. `@zzop/cli-<platform>` — the prebuilt binary, installed as an optionalDependency matching
//      the current OS/CPU/libc (see the PLATFORM_PACKAGES map below).
//   2. `<repo root>/target/release/zzop[.exe]` — a repo-local dev build (`cargo build
//      -p zzop-cli-bin --release`), so a source checkout works with no npm install at all.
//   3. Otherwise, throw with the list of supported platforms and the local-build command.
//
// musl (Alpine) and WASM targets are out of scope.

const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const PLATFORM_PACKAGES = {
  'win32-x64': { pkg: '@zzop/cli-win32-x64-msvc', bin: 'zzop.exe' },
  'darwin-x64': { pkg: '@zzop/cli-darwin-x64', bin: 'zzop' },
  'darwin-arm64': { pkg: '@zzop/cli-darwin-arm64', bin: 'zzop' },
  'linux-x64': { pkg: '@zzop/cli-linux-x64-gnu', bin: 'zzop' },
  'linux-arm64': { pkg: '@zzop/cli-linux-arm64-gnu', bin: 'zzop' },
};

const platformKey = `${process.platform}-${process.arch}`;
const entry = PLATFORM_PACKAGES[platformKey];

const attempts = [];

function resolvePlatformPackageBinary() {
  if (!entry) {
    attempts.push(`  - (no prebuilt package registered for platform "${platformKey}")`);
    return null;
  }
  try {
    const pkgJsonPath = require.resolve(`${entry.pkg}/package.json`);
    return path.join(path.dirname(pkgJsonPath), entry.bin);
  } catch (err) {
    attempts.push(`  - ${entry.pkg}: ${err && err.message}`);
    return null;
  }
}

function resolveDevFallbackBinary() {
  const devBin = process.platform === 'win32' ? 'zzop.exe' : 'zzop';
  const devPath = path.join(__dirname, '..', '..', '..', 'target', 'release', devBin);
  if (fs.existsSync(devPath)) {
    return devPath;
  }
  attempts.push(`  - ${devPath} (repo-local dev build): not found`);
  return null;
}

function resolveBinaryPath() {
  return resolvePlatformPackageBinary() || resolveDevFallbackBinary();
}

// Is this a musl-based Linux (Alpine)? Only consulted on the failure path below.
//
// WHY: on Alpine the failure above reads as a self-contradiction. `process.platform` is `linux` and
// `process.arch` is `x64`, so `platformKey` is `linux-x64` — and the "Supported prebuilt platforms"
// line then prints `linux-x64` back at a user who is standing on exactly that. The list is not wrong:
// the `@zzop/cli-linux-*-gnu` packages declare `libc: glibc`, so npm SKIPS the optionalDependency on
// musl and there is genuinely nothing installed to resolve. But `attempts` can only report a bare
// "Cannot find module", which is indistinguishable from a broken install, and the summary line cannot
// say why. Naming musl is the missing half; musl is out of scope (see the header), not unnoticed.
//
// DETECTION: Node records the runtime glibc version in its diagnostic report, and a musl build has no
// such field. `process.report` can be missing or restricted in an embedder, so a throw here means
// "cannot tell" and returns false — the hint is purely additive, and a miss just restores the message
// this shim printed before. It is never allowed to turn a resolution failure into a crash.
function isMuslLinux() {
  if (process.platform !== 'linux') return false;
  try {
    return !process.report.getReport().header.glibcVersionRuntime;
  } catch {
    return false;
  }
}

const binaryPath = resolveBinaryPath();

if (!binaryPath) {
  const supported = Object.keys(PLATFORM_PACKAGES)
    .map((key) => `${key} (${PLATFORM_PACKAGES[key].pkg})`)
    .join(', ');
  const muslHint = isMuslLinux()
    ? 'Note: this looks like a musl-based Linux (Alpine). The linux prebuilds are glibc-only — ' +
      'they declare `libc: glibc`, so npm skipped the optional dependency for your platform, which ' +
      'is why the line above lists a platform you appear to be on. musl is not a supported prebuild ' +
      'target; build from source as below.\n'
    : '';
  process.stderr.write(
    `zzop: failed to resolve the native binary for "${platformKey}".\n` +
      `Tried:\n${attempts.join('\n')}\n` +
      `Supported prebuilt platforms: ${supported}.\n` +
      muslHint +
      'For unsupported platforms (or local development), build from source: ' +
      '`cargo build -p zzop-cli-bin --release`, then re-run — this shim also checks ' +
      '<repo root>/target/release/zzop[.exe]. See packages/cli/README.md for details.\n'
  );
  process.exit(1);
}

const result = spawnSync(binaryPath, process.argv.slice(2), { stdio: 'inherit' });

if (result.error) {
  process.stderr.write(`zzop: failed to launch "${binaryPath}": ${result.error.message}\n`);
  process.exit(1);
}

// A null status means the child was killed by a signal (no exit code to propagate) — exit 1 rather
// than propagate `null`/undefined straight into process.exit.
process.exit(result.status === null ? 1 : result.status);
