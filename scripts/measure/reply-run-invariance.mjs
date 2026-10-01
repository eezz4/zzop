#!/usr/bin/env node
/**
 * How much of a reply is RUN-INVARIANT prose — the same sentence in two different trees once the
 * numbers are masked. A sentence like that is a LEGEND: it says the same thing wherever it is
 * printed, so the reader pays for it on every call and learns nothing new from the repeat.
 *
 * ## Why this exists (2026-09-24, review ledger V302)
 *
 * `measure-reply-bytes.mjs` prints the size of each part of one reply and says, in its own output,
 * that it CANNOT tell legend from measurement with one tree. So every time this question came up the
 * answer was a throwaway script in a scratchpad, and by the next round the number could not be
 * re-derived. Third time; it lives here now.
 *
 * ## The unit is a SENTENCE, and that is the whole finding
 *
 * MEASURED both ways on the same pair of trees. Comparing WHOLE STRINGS says `warnings` is 0.0%
 * run-invariant; comparing SENTENCES says 83.0%. Both are arithmetically correct and only one
 * answers the question, because a warning is one long string with a measurement and a legend welded
 * together — mask the digits and the whole string still differs, while most of its paragraph does not.
 * The sentence is also the unit a reader skips or reads. Anyone quoting a figure from this script
 * has to say which unit it is in, and this file only produces the sentence one.
 *
 * ## What it does NOT claim
 *
 * The denominator is sentences of 40+ characters inside string VALUES. Keys, punctuation, numbers and
 * short strings are outside it, so this is a floor on how much legend a reply carries and never a
 * statement about total reply size — `measure-reply-bytes.mjs` owns that. It also says nothing about
 * whether a legend SHOULD be moved: a sentence repeated on every call may be the one that stops a
 * number being misread, which is exactly the trade the ledger row is open on.
 *
 * Usage:  node scripts/measure/reply-run-invariance.mjs <treeA> <treeB> [treeC ...]
 * The first tree is the subject; a sentence counts as invariant only if EVERY other tree has it, so
 * adding trees can only lower the number. Exit 2 on fewer than two trees — one tree cannot answer it.
 */
import { execFileSync } from "node:child_process";

const TREES = process.argv.slice(2);
if (TREES.length < 2) {
  console.error("usage: node run-invariant.mjs <treeA> <treeB> [treeC ...]");
  process.exit(2);
}

const mask = (s) => String(s).replace(/\d[\d,._%]*/g, "#");

// SENTENCES, not whole strings. A warning is ONE long string carrying a measurement and a legend
// welded together, so comparing whole strings reports 0% invariant while most of the paragraph is
// the same in every tree. Measured both ways on the same pair: whole-string says `warnings` is 0.0%
// run-invariant, sentence-level says otherwise, and the sentence is the unit a reader skips or reads.
const SENT = /[^.!?\n]+[.!?]+|\n+|[^.!?\n]+$/g;
const sentences = (s) => (String(s).match(SENT) || []).map((x) => x.trim()).filter((x) => x.length >= 40);

function reply(tree) {
  const out = execFileSync("./target/release/zzop", ["analyze", tree, "--limit", "1000", "--severity", "info"], {
    encoding: "utf8",
    maxBuffer: 1 << 28,
  });
  return JSON.parse(out);
}

// Every string value in the object, with the path that reaches it. A top-level key's whole subtree is
// attributed to that key, which is how "legend" and "measurement" separate in the report below.
function strings(node, topKey, acc) {
  if (typeof node === "string") {
    for (const sen of sentences(node)) acc.push({ topKey, s: sen });
    return acc;
  }
  if (Array.isArray(node)) {
    for (const v of node) strings(v, topKey, acc);
    return acc;
  }
  if (node && typeof node === "object") {
    for (const [k, v] of Object.entries(node)) strings(v, topKey ?? k, acc);
  }
  return acc;
}

const per = new Map();
for (const t of TREES) {
  const j = reply(t);
  const acc = [];
  for (const [k, v] of Object.entries(j)) strings(v, k, acc);
  per.set(t, acc);
  console.log(`  ${t}: envelope ${JSON.stringify(j).length} B · ${acc.length} sentence(s) of 40+ chars`);
}

const [A, ...rest] = TREES;
const a = per.get(A);
// A masked string that appears in EVERY other tree is run-invariant: it says the same thing wherever
// it is printed, so it is a legend and not a measurement.
const othersets = rest.map((t) => new Set(per.get(t).map((x) => mask(x.s))));

const byKey = new Map();
for (const { topKey, s } of a) {
  const m = mask(s);
  const invariant = othersets.every((set) => set.has(m));
  const r = byKey.get(topKey) ?? { total: 0, inv: 0, invStrings: [] };
  r.total += s.length;
  if (invariant) {
    r.inv += s.length;
    if (r.invStrings.length < 2) r.invStrings.push(s.slice(0, 90));
  }
  byKey.set(topKey, r);
}

const rows = [...byKey.entries()].sort((x, y) => y[1].inv - x[1].inv);
const totB = rows.reduce((n, [, r]) => n + r.total, 0);
const invB = rows.reduce((n, [, r]) => n + r.inv, 0);

console.log("");
console.log(`RUN-INVARIANT vs ${rest.join(" + ")} (digits masked), by top-level key of ${A}:`);
console.log(`  ${"key".padEnd(22)} ${"bytes".padStart(8)} ${"invariant".padStart(10)}   share`);
for (const [k, r] of rows) {
  if (r.total === 0) continue;
  console.log(`  ${k.padEnd(22)} ${String(r.total).padStart(8)} ${String(r.inv).padStart(10)}   ${((r.inv / r.total) * 100).toFixed(1)}%`);
}
console.log("");
console.log(`  TOTAL string bytes ${totB} · run-invariant ${invB} (${((invB / totB) * 100).toFixed(1)}%)`);
console.log("");
console.log("  ⚠ The unit is a SENTENCE of 40+ characters, not the whole envelope: short strings, keys,");
console.log("    punctuation and numbers are outside it. A floor on the legend, never a claim about size.");
for (const [k, r] of rows) {
  if (!r.invStrings.length) continue;
  console.log("");
  console.log(`  ${k} — sentences that are the same in every tree:`);
  for (const s of r.invStrings) console.log("    " + s);
}
