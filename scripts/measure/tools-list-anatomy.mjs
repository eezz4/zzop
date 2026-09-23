#!/usr/bin/env node
// tools-list-anatomy.mjs — what the MCP session fixed cost is made of, with the counting rule
// written down beside every number.
//
// ## Why this file exists (review ledger V153)
// Three numbers went into an external review brief without a reproducible counting rule, and all
// three came back contested:
//
//   * "overlap 5.3%" — no tokenization rule was stated. The reviewer measured it three ways and got
//     3.8% / 5.5% / 6.5%. The CONCLUSION (overlap is low) survives all three; the number 5.3 comes
//     out of none of them. A number nobody can re-derive is not evidence, it is a rumour with a
//     decimal point.
//   * "*Meaning: 11 keys, 12,377 B" — no key pattern was stated. `/meaning$/i` gives 11 and
//     `/Meaning$/` gives 8, and those two count different things. Both are also CONDITIONAL: the
//     `architecture.*` legends are absent when git did not run, and the `truncated.*` ones when
//     nothing was capped. A number bound to a tree and an invocation was written as if it were a
//     property of the software.
//   * "refusal = 998 B" — structurally irreproducible. The refusal interpolates the path of the
//     missing config TWICE, so its size is a function of the directory name it was measured in. The
//     reviewer got 818 and 1,000 from the same build. That was never a measurement of zzop.
//
// So: every figure this script prints names its own rule, and the refusal figure is reported as a
// SLOPE (bytes per path character) plus a fixed baseline, because that is the shape of the truth.
//
// Usage:  node scripts/measure/tools-list-anatomy.mjs [--binary <path>]
//   Requires a built zzop-mcp. Prints to stdout; exits 2 if the binary is missing.

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

const argv = process.argv.slice(2);
const bi = argv.indexOf("--binary");
const BIN = bi >= 0 ? argv[bi + 1] : `./target/release/zzop-mcp${process.platform === "win32" ? ".exe" : ""}`;
if (!existsSync(BIN)) {
  console.error(`tools-list-anatomy: no binary at ${BIN} -- build it first (cargo build --release -p zzop-mcp)`);
  process.exit(2);
}

const init = {
  jsonrpc: "2.0", id: 1, method: "initialize",
  params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "tools-list-anatomy", version: "0" } },
};
const inited = { jsonrpc: "2.0", method: "notifications/initialized", params: {} };

function rpc(msgs) {
  return new Promise((resolve) => {
    const child = spawn(BIN, [], { stdio: ["pipe", "pipe", "ignore"] });
    let out = "";
    child.stdout.on("data", (d) => (out += d));
    for (const m of msgs) child.stdin.write(JSON.stringify(m) + "\n");
    child.stdin.end();
    child.on("close", () => {
      const byId = new Map();
      for (const line of out.split(/\r?\n/)) {
        if (!line.trim()) continue;
        try { const j = JSON.parse(line); if (j.id != null) byId.set(j.id, j); } catch { /* not a reply line */ }
      }
      resolve(byId);
    });
  });
}

// Wire bytes, always. `x.length` on a decoded string undercounts by the JSON escaping.
const B = (x) => JSON.stringify(x).length;

const byId = await rpc([init, inited, { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} }]);
const tools = byId.get(2).result.tools;

// ── 1 · anatomy ──────────────────────────────────────────────────────────────
// RULE: wire bytes of each field, summed over every tool in tools/list.
const descB = tools.reduce((n, t) => n + B(t.description || ""), 0);
const schemaB = tools.reduce((n, t) => n + B(t.inputSchema || {}), 0);
const totalB = B(byId.get(2));
console.log(`== tools/list anatomy ==`);
console.log(`  rule: JSON.stringify(field).length, summed over all ${tools.length} tools`);
console.log(`  whole reply  ${totalB.toLocaleString()} B`);
console.log(`  description  ${descB.toLocaleString()} B  (${((descB / totalB) * 100).toFixed(1)}%)`);
console.log(`  inputSchema  ${schemaB.toLocaleString()} B  (${((schemaB / totalB) * 100).toFixed(1)}%)`);

// ── 2 · overlap, under THREE stated tokenizations ────────────────────────────
// The point is not which number is right. It is that the conclusion must survive all three, and
// that any single figure quoted without its rule is unreproducible.
const TOKENIZERS = {
  "punct->space": (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, " ").split(/\s+/).filter(Boolean),
  "punct-dropped": (s) => s.toLowerCase().replace(/[^a-z0-9\s]+/g, "").split(/\s+/).filter(Boolean),
  "whitespace-only": (s) => s.toLowerCase().split(/\s+/).filter(Boolean),
};
const N = 8;
console.log(`\n== description overlap, ${N}-gram ==`);
console.log(`  rule: a gram shared by 2+ tools; percentage is of all gram positions`);
for (const [name, tok] of Object.entries(TOKENIZERS)) {
  const owners = new Map();
  const perTool = tools.map((t) => tok(t.description || ""));
  perTool.forEach((words, ti) => {
    for (let i = 0; i + N <= words.length; i++) {
      const g = words.slice(i, i + N).join(" ");
      if (!owners.has(g)) owners.set(g, new Set());
      owners.get(g).add(ti);
    }
  });
  let shared = 0, total = 0;
  perTool.forEach((words, ti) => {
    for (let i = 0; i + N <= words.length; i++) {
      total++;
      if (owners.get(words.slice(i, i + N).join(" ")).size > 1) shared++;
    }
  });
  console.log(`  ${name.padEnd(16)} ${((shared / total) * 100).toFixed(1)}%  (${shared} of ${total} positions)`);
}
console.log(`  => the CONCLUSION (overlap is low) is what survives; no single figure is the measurement`);

// ── 3 · *Meaning, under BOTH key patterns, and named as conditional ──────────
const TREE = "cases/trees/api-be";
const call = await rpc([
  init, inited,
  { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "analyze_repo", arguments: { path: TREE } } },
]);
const text = call.get(3)?.result?.content?.[0]?.text;
console.log(`\n== reply legends (*Meaning) on ${TREE} ==`);
if (!text) {
  console.log(`  MEASUREMENT FAILURE: analyze_repo returned no text -- not "zero legends"`);
} else {
  const reply = JSON.parse(text);
  for (const [label, re] of [["/meaning$/i", /meaning$/i], ["/Meaning$/", /Meaning$/]]) {
    let count = 0, bytes = 0;
    (function walk(v) {
      if (!v || typeof v !== "object") return;
      for (const k of Object.keys(v)) {
        if (!Array.isArray(v) && re.test(k)) { count++; bytes += B(v[k]); }
        walk(v[k]);
      }
    })(reply);
    console.log(`  ${label.padEnd(12)} ${String(count).padStart(3)} keys  ${bytes.toLocaleString().padStart(8)} B`);
  }
  console.log(`  => both are CONDITIONAL on this tree and this call:`);
  console.log(`     architecture.* legends are absent when git signals did not run`);
  console.log(`     truncated.*    legends are absent when nothing was capped`);
  console.log(`     so quote the tree and the invocation, or quote neither number`);
}

// ── 4 · refusal size is a SLOPE, not a constant ──────────────────────────────
// The refusal interpolates the missing config's path twice, so its length is a function of where
// you ran it. Measured at two path lengths; the slope is the reproducible fact.
console.log(`\n== config refusal ==`);
console.log(`  rule: same refusal measured at two path lengths; the SIZE IS NOT A CONSTANT`);
const points = [];
for (const pad of ["a", "a".repeat(41)]) {
  const dir = mkdtempSync(path.join(tmpdir(), `zzop-refusal-${pad}-`));
  try {
    const r = await rpc([
      init, inited,
      { jsonrpc: "2.0", id: 4, method: "tools/call", params: { name: "analyze_repo", arguments: { path: dir } } },
    ]);
    const t = r.get(4)?.result?.content?.[0]?.text ?? "";
    points.push({ len: dir.length, bytes: Buffer.byteLength(t, "utf8") });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
if (points.length === 2 && points[1].len !== points[0].len) {
  const slope = (points[1].bytes - points[0].bytes) / (points[1].len - points[0].len);
  const base = points[0].bytes - slope * points[0].len;
  for (const p of points) console.log(`  path ${String(p.len).padStart(3)} chars -> ${p.bytes} B`);
  console.log(`  slope ${slope.toFixed(2)} B per path character, baseline ${Math.round(base)} B`);
  console.log(`  => a bare "the refusal is N bytes" is a measurement of the reporter's tmpdir name`);
} else {
  console.log(`  MEASUREMENT FAILURE: could not vary the path length`);
}
