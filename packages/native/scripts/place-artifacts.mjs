// Copies prebuilt addon binaries out of a flat CI-artifact directory into their npm/<platform>/
// sub-package, where index.js's loader cascade expects to find them at install time.
//
// Expects files named `zzop-napi.<rust-target-triple>.node` — the exact naming
// .github/workflows/prebuild.yml's "Collect artifact" step already produces (e.g.
// `zzop-napi.x86_64-pc-windows-msvc.node`), so no change to that workflow's naming was needed. If the
// artifacts were downloaded via `actions/download-artifact` per-job (one dir per `zzop-napi-<target>`
// artifact name), flatten them into one directory first — this script does not recurse.
//
// Usage: node scripts/place-artifacts.mjs <artifacts-dir>
//
// Plain Node, no dependencies.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const npmDir = path.join(__dirname, '..', 'npm');

// Rust target triple (as built by prebuild.yml) -> npm/<platform> sub-package directory name
// (napi-rs/swc/Prisma convention).
const TARGET_TO_PLATFORM = {
  'x86_64-pc-windows-msvc': 'win32-x64-msvc',
  'x86_64-apple-darwin': 'darwin-x64',
  'aarch64-apple-darwin': 'darwin-arm64',
  'x86_64-unknown-linux-gnu': 'linux-x64-gnu',
  'aarch64-unknown-linux-gnu': 'linux-arm64-gnu',
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
    const match = /^zzop-napi\.(.+)\.node$/.exec(entry);
    if (!match) continue;

    const target = match[1];
    const platformDir = TARGET_TO_PLATFORM[target];
    if (!platformDir) {
      console.warn(`place-artifacts: unrecognized target "${target}" (from ${entry}), skipping`);
      continue;
    }

    const destDir = path.join(npmDir, platformDir);
    if (!fs.existsSync(destDir)) {
      console.warn(`place-artifacts: missing sub-package dir ${destDir}, skipping ${entry}`);
      continue;
    }

    const dest = path.join(destDir, 'zzop-napi.node');
    fs.copyFileSync(path.join(artifactsDir, entry), dest);
    console.log(`place-artifacts: ${entry} -> ${path.relative(process.cwd(), dest)}`);
    placed += 1;
  }

  if (placed === 0) {
    console.error(`place-artifacts: no recognized "zzop-napi.<target>.node" files found in ${artifactsDir}`);
    process.exit(1);
  }

  console.log(`place-artifacts: placed ${placed} artifact(s).`);
}

main();
