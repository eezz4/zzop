#!/usr/bin/env node
// Runs a NormalizedEnvelope (adapter.mjs's stdout) through zzop's Mode A entry point
// (`analyzeEnvelope`) and prints a compact summary — the smallest possible "external language,
// full analysis" round trip.
//
// USAGE
//   node adapter.mjs --root <workspaceRoot> --source <id> > envelope.json
//   node analyze.mjs envelope.json [--native <path-to-@zzop/native-or-checkout-packages/native>]
//
// `--native` defaults to `@zzop/native` (npm install) and falls back to the in-checkout addon at
// `../../packages/native/index.js` so the example runs inside the zzop repo without an install.
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const here = path.dirname(fileURLToPath(import.meta.url));

const envelopePath = process.argv[2];
if (!envelopePath || envelopePath.startsWith('--')) {
  console.error('usage: node analyze.mjs <envelope.json> [--native <module-or-path>]');
  process.exit(2);
}
const nativeArgIdx = process.argv.indexOf('--native');
const nativeSpec = nativeArgIdx >= 0 ? process.argv[nativeArgIdx + 1] : null;

let native;
if (nativeSpec) {
  native = require(path.resolve(nativeSpec));
} else {
  try {
    native = require('@zzop/native');
  } catch {
    native = require(path.join(here, '..', '..', 'packages', 'napi', 'index.js'));
  }
}

const envelopeJson = readFileSync(envelopePath, 'utf8');
const out = JSON.parse(native.analyzeEnvelope(envelopeJson, '{}'));

const byRule = new Map();
for (const f of out.findings || []) {
  byRule.set(f.ruleId, (byRule.get(f.ruleId) || 0) + 1);
}
const cov = out.coverage || {};
console.log(`files:        ${out.fileCount}`);
console.log(`symbols:      ${cov.symbols}`);
console.log(`import edges: ${cov.importEdges}`);
console.log(`findings:     ${(out.findings || []).length}`);
for (const [rule, n] of [...byRule.entries()].sort()) {
  console.log(`  - ${rule}: ${n}`);
}
console.log(`warnings:     ${(out.warnings || []).length}`);
for (const w of out.warnings || []) {
  console.log(`  - ${w.length > 140 ? `${w.slice(0, 140)}...` : w}`);
}
