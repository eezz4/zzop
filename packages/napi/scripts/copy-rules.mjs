// Copies the repo's DSL rule packs (<repo root>/rules/dsl/) into packages/napi/rules/, so they ship
// inside the published npm package and index.js's `defaultPacksDir()` can find them at
// `path.join(__dirname, 'rules')` without any repo-relative path assumptions at install time. Run
// automatically via the `prepack` script (see package.json) before `npm pack`/`npm publish`.
//
// Source layout: both flat (<repo root>/rules/dsl/<id>.json) and depth-1 nested
// (<repo root>/rules/dsl/<name>/<id>.json — this repo's own "pack folder" layout, see
// ../../../rules/README.md) pack files are discovered and copied, mirroring
// `zpz_core::pack_loader::load_dsl_packs`'s own two-shape scan (packages/core/src/pack_loader.rs) so the
// copied tree needs no special-casing by the loader. Nested structure is PRESERVED in destDir (not
// flattened) — same relative layout as the source. Only `*.json` is copied: a pack folder's co-located
// `<pack>.rs` (Rust source, e.g. `be-db/be-db.rs`) never ships in the npm package.
//
// Usage: node scripts/copy-rules.mjs

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const srcDir = path.join(__dirname, '..', '..', '..', 'rules', 'dsl');
const destDir = path.join(__dirname, '..', 'rules');

// Returns [{ src, rel }] for every pack `*.json` under srcDir — rel is srcDir-relative, either "<id>.json"
// (flat) or "<name>/<id>.json" (depth-1 nested). Only one level of subdirectory is scanned, matching
// load_dsl_packs's own depth.
function collectPackFiles(dir) {
  const files = [];
  for (const name of fs.readdirSync(dir)) {
    const full = path.join(dir, name);
    const stat = fs.statSync(full);
    if (stat.isFile() && name.endsWith('.json')) {
      files.push({ src: full, rel: name });
    } else if (stat.isDirectory()) {
      for (const nestedName of fs.readdirSync(full)) {
        const nestedFull = path.join(full, nestedName);
        if (fs.statSync(nestedFull).isFile() && nestedName.endsWith('.json')) {
          files.push({ src: nestedFull, rel: path.join(name, nestedName) });
        }
      }
    }
  }
  return files;
}

function main() {
  if (!fs.existsSync(srcDir) || !fs.statSync(srcDir).isDirectory()) {
    console.error(`copy-rules: source directory not found: ${srcDir}`);
    process.exit(1);
  }

  fs.rmSync(destDir, { recursive: true, force: true });
  fs.mkdirSync(destDir, { recursive: true });

  const files = collectPackFiles(srcDir);
  for (const { src, rel } of files) {
    const dest = path.join(destDir, rel);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.copyFileSync(src, dest);
  }

  console.log(`copy-rules: copied ${files.length} rule pack(s) from ${srcDir} to ${destDir}.`);
}

main();
