// Copies prebuilt `zzop` CLI binaries out of a flat CI-artifact directory into their npm/<platform>/
// sub-package, where bin/zzop.js's resolution cascade expects to find them at install time.
//
// Expects files named `zzop-cli-<platform>[.exe]` — the exact naming prebuild.yml's "Collect the zzop +
// zzop-mcp binaries" step already produces (e.g. `zzop-cli-win32-x64-msvc.exe`, `zzop-cli-linux-x64-gnu`).
// No target-triple -> platform translation is needed here: `<platform>` in the artifact name IS the
// npm/<platform> sub-package directory name already (the workflow's `matrix.platform`, the same
// napi-rs-style token used throughout this repo). If the
// artifacts were downloaded via `actions/download-artifact` per-job (one dir per `zzop-mcp-<target>`
// artifact name, which also contains the `zzop-mcp-*` sibling binaries), flatten them into one
// directory first — this script does not recurse, and simply ignores files that don't match the
// `zzop-cli-<platform>[.exe]` pattern (e.g. the `zzop-mcp-*` siblings living alongside them).
//
// GitHub artifact zips drop the unix executable bit, and npm pack/publish preserves whatever mode the
// tarball entry has — so this script chmods the placed binary to 0o755 itself, which is what makes
// the eventually-published binary executable on the consumer's machine.
//
// Usage: node scripts/place-artifacts.mjs <artifacts-dir>
//
// Plain Node, no dependencies.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const npmDir = path.join(__dirname, '..', 'npm');

// npm/<platform> sub-package directory name -> binary filename within it (mirrors bin/zzop.js's
// PLATFORM_PACKAGES map).
const PLATFORM_BINARY_NAMES = {
  'win32-x64-msvc': 'zzop.exe',
  'darwin-x64': 'zzop',
  'darwin-arm64': 'zzop',
  'linux-x64-gnu': 'zzop',
  'linux-arm64-gnu': 'zzop',
};

function main() {
  const artifactsDir = process.argv[2];
  if (!artifactsDir) {
    console.error('Usage: node scripts/place-artifacts.mjs <artifacts-dir>');
    process.exit(1);
  }
  if (!fs.existsSync(artifactsDir) || !fs.statSync(artifactsDir).isDirectory()) {
    console.error(`place-artifacts: not a directory: ${artifactsDir}`);
    process.exit(1);
  }

  const entries = fs.readdirSync(artifactsDir);
  let placed = 0;

  for (const entry of entries) {
    // Match ONLY the CLI binaries (`zzop-cli-<platform>[.exe]`), never the `zzop-mcp-<platform>`
    // siblings that share the same artifact set — the `-cli-` infix keeps the two apart with no
    // separate skip needed (an mcp binary simply doesn't match this pattern).
    const match = /^zzop-cli-(.+?)(\.exe)?$/.exec(entry);
    if (!match) continue;

    const platform = match[1];
    const binaryName = PLATFORM_BINARY_NAMES[platform];
    if (!binaryName) {
      console.warn(`place-artifacts: unrecognized platform "${platform}" (from ${entry}), skipping`);
      continue;
    }

    const destDir = path.join(npmDir, platform);
    if (!fs.existsSync(destDir)) {
      console.warn(`place-artifacts: missing sub-package dir ${destDir}, skipping ${entry}`);
      continue;
    }

    const dest = path.join(destDir, binaryName);
    fs.copyFileSync(path.join(artifactsDir, entry), dest);
    fs.chmodSync(dest, 0o755);
    console.log(`place-artifacts: ${entry} -> ${path.relative(process.cwd(), dest)}`);
    placed += 1;
  }

  if (placed === 0) {
    console.error(`place-artifacts: no recognized "zzop-cli-<platform>[.exe]" files found in ${artifactsDir}`);
    process.exit(1);
  }

  console.log(`place-artifacts: placed ${placed} artifact(s).`);
}

main();
