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

// The set of platforms is READ FROM DISK — every directory under npm/ is a sub-package that will be
// published — rather than hand-listed here. A hand list is a copy of the release matrix, and the sixth
// platform's day is exactly when a copy is wrong: an artifact for an unlisted platform used to be warned
// about and skipped, which is how a sub-package could publish with no binary inside it.
function publishedPlatforms() {
  return fs
    .readdirSync(npmDir, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => e.name)
    .sort();
}

// The binary filename inside a sub-package is DERIVED, not mapped: Windows carries the `.exe` suffix and
// nothing else does. That is a property of the platform token (`win32-…`), so it needs no table to drift.
function binaryNameFor(platform) {
  return platform.startsWith('win32-') ? 'zzop.exe' : 'zzop';
}

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
  const expected = publishedPlatforms();
  if (expected.length === 0) {
    console.error(`place-artifacts: no sub-package directories under ${npmDir} — nothing to place into.`);
    process.exit(1);
  }
  const filled = new Set();

  for (const entry of entries) {
    // Match ONLY the CLI binaries (`zzop-cli-<platform>[.exe]`), never the `zzop-mcp-<platform>`
    // siblings that share the same artifact set — the `-cli-` infix keeps the two apart with no
    // separate skip needed (an mcp binary simply doesn't match this pattern).
    const match = /^zzop-cli-(.+?)(\.exe)?$/.exec(entry);
    if (!match) continue;

    const platform = match[1];
    const destDir = path.join(npmDir, platform);
    // FATAL, not a skip. A built artifact this script cannot place is a platform whose sub-package is
    // about to publish empty — "we produced a binary and then dropped it on the floor" must never be a
    // warning, because the step's exit code is what the release lane trusts.
    if (!expected.includes(platform)) {
      console.error(
        `place-artifacts: artifact "${entry}" names platform "${platform}", which has no sub-package ` +
          `directory under npm/. Known: ${expected.join(', ')}.\n` +
          `  Either add npm/${platform}/ (with its package.json, and list it in the root package.json's ` +
          `optionalDependencies) or stop building that target.`
      );
      process.exit(1);
    }

    const dest = path.join(destDir, binaryNameFor(platform));
    fs.copyFileSync(path.join(artifactsDir, entry), dest);
    fs.chmodSync(dest, 0o755);
    console.log(`place-artifacts: ${entry} -> ${path.relative(process.cwd(), dest)}`);
    filled.add(platform);
  }

  // `placed > 0` was the old success condition, and it is not one: with five sub-packages and four
  // artifacts it is true while one package publishes with no binary. EVERY sub-package must have been
  // filled — that is the property "no empty package is published" actually rests on.
  const missing = expected.filter((p) => !filled.has(p));
  if (missing.length) {
    console.error(
      `place-artifacts: no "zzop-cli-<platform>[.exe]" artifact arrived for: ${missing.join(', ')}.\n` +
        `  Every npm/ sub-package publishes, so a missing artifact means an EMPTY package on the registry.\n` +
        `  Check the prebuild build matrix produced all ${expected.length} CLI binaries.`
    );
    process.exit(1);
  }

  console.log(`place-artifacts: placed ${filled.size} artifact(s), one per sub-package.`);
}

main();
