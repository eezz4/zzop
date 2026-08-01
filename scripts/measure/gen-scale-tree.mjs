#!/usr/bin/env node
/**
 * Generates a SYNTHETIC TypeScript tree of a requested size, for the one measurement the M-profile
 * question needs and no corpus we hold can answer.
 *
 * ## Why synthetic is legitimate here, and where it is not
 * The pass/fail criterion for the M profile is `peak RSS ~= f(thread count, per-file cap)` — a claim
 * about how memory scales with SIZE, not about what the analyzer FINDS. A generated tree answers it
 * exactly as well as a real one, because the resident set is driven by file count, per-file IR size and
 * concurrency, none of which care whether the code is meaningful.
 *
 * That reasoning does NOT transfer to rule validation. A synthetic tree cannot tell you whether the
 * `security` pack's Java rules fire on real code, because there the CONTENT is the subject. So this
 * generator is deliberately scoped to the scale axis and says so, rather than becoming "the corpus".
 *
 * ## What it emits
 * A tree with a realistic dependency SHAPE rather than a flat pile: files are laid out in modules,
 * each imports a bounded number of siblings and a few cross-module neighbours, and a fraction export
 * routes/consumes so the io channels and the cross-layer join are exercised too — a tree that resolved
 * zero imports would measure the walk and nothing else.
 *
 * Deterministic: same `--files`/`--seed` produces byte-identical output, so two runs are comparable.
 * There is no `Math.random()` — the PRNG below is seeded and explicit, for the same reason the
 * workflow scripts ban the global one.
 *
 * ## What the first run measured (2026-08-01, `zzop graph --domain dep`, cold)
 *
 *     files    1,000    2,000    4,000    8,000   10,000
 *     wall       29s      57s     135s        -     313s      -> ~linear in file count
 *     peak RSS     -   86.3MB        -  204.3MB        -      -> 4x files = 2.37x memory, SUBLINEAR
 *
 * The memory row is the result the M-profile question wanted: `peak RSS ~= f(thread count, per-file
 * cap)` holds in this range — what bounds the resident set is concurrency times the per-file cap, not
 * the size of the tree.
 *
 * ⚠ READ THE CURVE WITHIN ONE CORPUS. The first reading of these numbers was WRONG and the mistake is
 * worth more than the numbers: it compared this repo (1,173 files -> 7s) against a generated tree
 * (10,000 -> 313s) and called the ratio super-linear. Those are two different populations — this repo
 * is Rust-dominant so it runs a different parser, while every generated file here is TypeScript and
 * carries io facts. The 4.1x per-file gap at equal file counts is that difference, not scaling. A
 * curve is only a curve if every point on it comes from the same generator and the same settings.
 *
 * Usage:
 *   node scripts/measure/gen-scale-tree.mjs --files 10000 --out ./scratchpad/scale-10k [--seed 1]
 *   cargo run --release -p zzop-engine --example bench -- ./scratchpad/scale-10k
 */
import fs from 'fs';
import path from 'path';

const args = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
};

const FILES = Number(arg('--files', '10000'));
const OUT = arg('--out', '');
const SEED = Number(arg('--seed', '1'));
const FILES_PER_MODULE = 40;

if (!OUT || !Number.isFinite(FILES) || FILES < 1) {
  console.error('usage: gen-scale-tree.mjs --files <n> --out <dir> [--seed <n>]');
  process.exit(2);
}
if (fs.existsSync(OUT) && fs.readdirSync(OUT).length > 0) {
  console.error(`gen-scale-tree: ${OUT} exists and is not empty — refusing to write into it.`);
  console.error('  A half-overwritten tree measures neither the old size nor the new one.');
  process.exit(1);
}

/** Seeded PRNG (mulberry32). Explicit and reproducible — see the module doc. */
function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const rand = rng(SEED);
const pick = (n) => Math.floor(rand() * n);

const moduleCount = Math.max(1, Math.ceil(FILES / FILES_PER_MODULE));
const relOf = (i) => `src/mod${Math.floor(i / FILES_PER_MODULE)}/f${i}.ts`;

/** One file's body: imports (mostly intra-module, some cross), a symbol or two, sometimes io. */
function body(i) {
  const mod = Math.floor(i / FILES_PER_MODULE);
  const lines = [];
  const localBase = mod * FILES_PER_MODULE;
  const localCount = Math.min(FILES_PER_MODULE, FILES - localBase);

  // 1-3 sibling imports: the bulk of a real tree's edges are intra-module.
  for (let k = 0; k < 1 + pick(3); k++) {
    const target = localBase + pick(localCount);
    if (target !== i) lines.push(`import { s${target} } from "./f${target}";`);
  }
  // ~1 in 4 files reaches into a neighbouring module — enough to make the graph one component
  // rather than N islands, which is what a real repo looks like.
  if (moduleCount > 1 && pick(4) === 0) {
    const otherMod = (mod + 1 + pick(moduleCount - 1)) % moduleCount;
    const target = otherMod * FILES_PER_MODULE + pick(Math.min(FILES_PER_MODULE, FILES - otherMod * FILES_PER_MODULE));
    if (target !== i && target < FILES) {
      lines.push(`import { s${target} } from "../mod${otherMod}/f${target}";`);
    }
  }

  lines.push('');
  // ~1 in 12 files serves a route, ~1 in 12 calls one — both channels get exercised, and the keys
  // line up so a real fraction of them JOIN instead of all landing in the unresolved bucket.
  if (i % 12 === 0) {
    lines.push(`export async function handler${i}(request: Request) {`);
    lines.push(`  const url = new URL(request.url);`);
    lines.push(`  if (url.pathname === "/api/r${i}" && request.method === "GET") {`);
    lines.push(`    return new Response("ok");`);
    lines.push('  }');
    lines.push('}');
  } else if (i % 12 === 6) {
    const target = (i - 6) % FILES;
    lines.push(`export async function call${i}() {`);
    lines.push(`  return fetch("/api/r${target}");`);
    lines.push('}');
  }
  lines.push(`export const s${i} = ${i};`);
  // A little filler so per-file LOC is not degenerate — the per-file cap is one of the axes.
  for (let k = 0; k < 8; k++) lines.push(`const pad${i}_${k} = ${i * 31 + k};`);
  lines.push(`export function use${i}() { return [${Array.from({ length: 8 }, (_, k) => `pad${i}_${k}`).join(', ')}]; }`);
  return lines.join('\n') + '\n';
}

fs.mkdirSync(OUT, { recursive: true });
let written = 0;
for (let i = 0; i < FILES; i++) {
  const rel = relOf(i);
  const abs = path.join(OUT, rel);
  fs.mkdirSync(path.dirname(abs), { recursive: true });
  fs.writeFileSync(abs, body(i));
  written++;
}
fs.writeFileSync(
  path.join(OUT, 'package.json'),
  JSON.stringify({ name: 'zzop-scale-fixture', private: true, version: '0.0.0' }, null, 2) + '\n'
);
// The tree says what it is, so nobody mistakes a generated fixture for measured corpus evidence.
fs.writeFileSync(
  path.join(OUT, 'README.md'),
  [
    '# Generated scale fixture — NOT a corpus',
    '',
    `Produced by \`scripts/measure/gen-scale-tree.mjs --files ${FILES} --seed ${SEED}\`.`,
    '',
    'Valid for ONE question: how peak RSS and wall time scale with file count, thread count and the',
    'per-file cap. Invalid for anything about what the analyzer FINDS — the content is generated, so a',
    'finding count here measures the generator, not a rule.',
    '',
  ].join('\n')
);

console.log(
  `gen-scale-tree: wrote ${written} .ts file(s) across ${moduleCount} module(s) into ${OUT} (seed ${SEED}).`
);
console.log(`  measure: cargo run --release -p zzop-engine --example bench -- ${OUT}`);
