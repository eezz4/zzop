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

const binaryPath = resolveBinaryPath();

if (!binaryPath) {
  const supported = Object.keys(PLATFORM_PACKAGES)
    .map((key) => `${key} (${PLATFORM_PACKAGES[key].pkg})`)
    .join(', ');
  process.stderr.write(
    `zzop: failed to resolve the native binary for "${platformKey}".\n` +
      `Tried:\n${attempts.join('\n')}\n` +
      `Supported prebuilt platforms: ${supported}.\n` +
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
