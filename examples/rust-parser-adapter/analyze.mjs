#!/usr/bin/env node
// Runs a NormalizedEnvelope (adapter.mjs's stdout) through zzop's Mode A entry point by spawning the
// Node-free `zzop` binary's `analyze-envelope` subcommand, and prints a compact summary — the
// smallest possible "external language, full analysis" round trip.
//
// Node-free rewrite (2026-07-20): this script used to `require('@zzop/native')` (falling back to the
// in-checkout `packages/native` addon) and call `analyzeEnvelope` in-process. The npm distribution
// (the `@zzop/cli` JS CLI + the `@zzop/native` napi binding) was removed that day — zzop now ships as
// a single Node-free binary, `zzop`, with no in-process JS embedding path at all. This script
// spawns that binary as a child process and parses its JSON stdout instead of `require()`-ing a
// native addon.
//
// USAGE
//   node adapter.mjs --root <workspaceRoot> --source <id> > envelope.json
//   node analyze.mjs envelope.json [--bin <path-to-zzop>]
//
// `--bin` defaults to `zzop` on PATH, falling back to an in-checkout `target/release/zzop` or
// `target/debug/zzop` build (`cargo build -p zzop-cli-bin [--release]`) so the example runs inside the
// zzop repo without a separate install.
import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const exeName = process.platform === 'win32' ? 'zzop.exe' : 'zzop';

const envelopePath = process.argv[2];
if (!envelopePath || envelopePath.startsWith('--')) {
  console.error('usage: node analyze.mjs <envelope.json> [--bin <path-to-zzop>]');
  process.exit(2);
}
const binArgIdx = process.argv.indexOf('--bin');
const binSpec = binArgIdx >= 0 ? process.argv[binArgIdx + 1] : null;

function resolveBinary() {
  if (binSpec) return path.resolve(binSpec);
  // In-checkout fallback so this example runs inside the zzop repo with no separate install —
  // mirrors the old script's fallback to the in-checkout `packages/native` addon.
  const checkoutCandidates = [
    path.join(here, '..', '..', 'target', 'release', exeName),
    path.join(here, '..', '..', 'target', 'debug', exeName),
  ];
  for (const candidate of checkoutCandidates) {
    if (existsSync(candidate)) return candidate;
  }
  return exeName; // rely on PATH
}

const bin = resolveBinary();
const result = spawnSync(bin, ['analyze-envelope', path.resolve(envelopePath)], { encoding: 'utf8' });

if (result.error) {
  console.error(`failed to run '${bin}': ${result.error.message}`);
  console.error(
    'build it with `cargo build -p zzop-cli-bin --release`, put zzop on your PATH, or pass --bin <path>'
  );
  process.exit(1);
}
if (result.status !== 0) {
  process.stderr.write(result.stderr);
  process.exit(result.status ?? 1);
}

// `zzop analyze-envelope` prints the same compact summary shape the `analyze_envelope` MCP tool
// returns (see docs/modules/mcp.md#output-contract): `findings` is `{total, bySeverity, byRule, shown,
// truncated?}`, not a flat array — `byRule` already carries the per-rule counts the old script had to
// tally itself from a raw `Finding[]`.
const out = JSON.parse(result.stdout);

const findings = out.findings || {};
const cov = out.coverage || {};
console.log(`files:        ${out.fileCount}`);
console.log(`symbols:      ${cov.symbols}`);
console.log(`import edges: ${cov.importEdges}`);
console.log(`findings:     ${findings.total ?? 0}`);
for (const [rule, n] of Object.entries(findings.byRule || {}).sort()) {
  console.log(`  - ${rule}: ${n}`);
}
console.log(`warnings:     ${(out.warnings || []).length}`);
for (const w of out.warnings || []) {
  console.log(`  - ${w.length > 140 ? `${w.slice(0, 140)}...` : w}`);
}
