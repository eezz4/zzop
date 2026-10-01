#!/usr/bin/env node
// WHICH TOOL DESCRIPTIONS HAND VOCABULARY BACK TO THE REPLY, AND WHICH KEEP A SECOND COPY.
//
// The rule is already shipped, in packages/cli-bin/src/cli/help.rs: a reply field named `*Meaning`
// "spells out THAT token's meaning in the reply itself, so no help text here (OR IN ANY OTHER HOST)
// is a second owner of the vocabulary". "Any other host" is the MCP tool table, and that table is
// the largest fixed cost an agent pays: every session buys `tools/list` before its first question.
//
// 🔴 WHY THIS FILE EXISTS: the backlog row that reported "2 of 8 tools delegate" wrote its recount
// command as `node <the script above>` — a placeholder naming no file. No such script was ever
// written, so the number
// could not be re-run, and by 2026-09-27 the row had drifted (8 tools -> 9, 32,082 B -> 36,454 B)
// with no way to tell whether its headline had drifted too.
//
// ## What it measures, stated narrowly so the number is not read wider than it is
// For each tool: the `*Meaning` field names that appear in its description. Naming the owner field
// IS the delegation — it is how a description points at the reply instead of restating it.
//
// ## What it CANNOT see, stated rather than left to be discovered
// * It cannot tell a POINTER from a DEFINITION. A description that names `painMeaning` and then also
//   defines pain is counted here as delegating. This instrument answers "does it name the owner?",
//   never "did it stop explaining?" — the second question needs a reader.
// * A reply may own vocabulary through a field named plainly `meaning` rather than `<x>Meaning`.
//   The first draft missed exactly that and produced a FALSE NEGATIVE on the table's best delegator:
//   module_map's description says "The reply's own `meaning` field is the ONE owner of what those
//   keys mean ... so this description deliberately does not", and the census scored it as naming
//   nobody. Matching the bare word `meaning` in prose would be far too loose, so what is matched is
//   the BACKTICKED field reference — a spelling that means the field, not the English word.
// * Still outside it: a reply owning vocabulary through a field with neither spelling (`verdict`,
//   `disclosure`). Those need a reader.
// * A tool whose reply carries no `*Meaning` field at all cannot delegate and is reported as N/A
//   rather than as a failure — counting it as non-delegating would invent a defect.
//
// ## Sibling, and why its byte count differs
// scripts/measure/tools-list-anatomy.mjs measures the SAME table for a different question: what the
// fixed cost is made of (description vs inputSchema share, 8-gram overlap between descriptions, the
// reply's own legend bytes). It does not attribute owners to tools, which is this file's question.
// Their totals differ ON PURPOSE and each states its rule: anatomy sums JSON.stringify(field).length
// per field (36,311 B today), this file measures the wire object {tools:[...]} (36,454 B today), and
// only the second is comparable to SESSION_FIXED_COST_CEILING in packages/mcp/src/tools/tests.rs,
// which serializes the same object. Quote the one whose rule matches your question.
//
// Usage:  node scripts/measure/mcp-meaning-delegation.mjs [path/to/zzop-mcp]
// Default binary: ./target/release/zzop-mcp

import { execFileSync } from "node:child_process";
import fs from "node:fs";

const BIN = process.argv[2] ?? "./target/release/zzop-mcp";
if (!fs.existsSync(BIN)) {
  console.error(`mcp-meaning-delegation: no binary at ${BIN}`);
  console.error("  Build it first: cargo build --release -p zzop-mcp");
  console.error("  Refusing to report a delegation count measured against nothing.");
  process.exit(1);
}

// The vocabulary owners, DERIVED from the source rather than listed here: any `"<name>Meaning"`
// string literal the reply layer emits. A hand list would drift the moment a field is added, which
// is the failure this whole file is about.
// 🔴 THREE roots, not two. The first draft of this file read summary + mcp only, reported "2 of 9"
// that happened to match the backlog row's "2 of 8" — and named DIFFERENT tools. That mismatch was
// the tell: `verdictMeaning`, the field the shipped rule quotes by name, is emitted from
// crates/facade (query.rs, query_file.rs) and was outside the derivation. A count can agree with
// the old one and still be measuring something else.
const SRC_ROOTS = ["crates/summary/src", "packages/mcp/src", "crates/facade/src"];
const meaningFields = new Set();
const walk = (d) => {
  for (const e of fs.readdirSync(d, { withFileTypes: true })) {
    const f = `${d}/${e.name}`;
    if (e.isDirectory()) walk(f);
    else if (e.name.endsWith(".rs")) {
      for (const m of fs.readFileSync(f, "utf8").matchAll(/"([a-zA-Z]+Meaning)"/g)) meaningFields.add(m[1]);
    }
  }
};
for (const r of SRC_ROOTS) {
  if (!fs.existsSync(r)) {
    console.error(`mcp-meaning-delegation: source root ${r} is missing — the derivation is reading a`);
    console.error("  tree it does not understand. Fix the roots here rather than reporting a count.");
    process.exit(1);
  }
  walk(r);
}
// FLOOR: this repo cannot be in a state with no `*Meaning` fields — the reply contract is built on
// them. Zero here is a broken needle, never a tree that stopped delegating.
if (meaningFields.size === 0) {
  console.error("mcp-meaning-delegation: found ZERO *Meaning field names in the reply layer.");
  console.error("  That is an extraction failure, not a repo without meaning fields.");
  process.exit(1);
}

const req = [
  JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "m", version: "0" } } }),
  JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} }),
].join("\n") + "\n";

const out = execFileSync(BIN, { input: req, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
let tools = null;
for (const line of out.split("\n")) {
  if (!line.trim()) continue;
  let d;
  try { d = JSON.parse(line); } catch { continue; }
  if (d.id === 2 && d.result?.tools) tools = d.result.tools;
}
if (!tools || tools.length === 0) {
  console.error("mcp-meaning-delegation: the server returned no tools — the probe broke, the table did not empty.");
  process.exit(1);
}

// Wire bytes the way the ceiling test measures them: compact JSON, UTF-8, NOT \u-escaped.
// (Measuring with JS's default JSON.stringify is already UTF-8; Python's json.dumps is not, and
// that difference cost 268 bytes of a wrong reading on 2026-09-27.)
const wire = Buffer.byteLength(JSON.stringify({ tools }), "utf8");

console.log(`tools ${tools.length} · tools/list wire ${wire.toLocaleString()} B · known *Meaning fields ${meaningFields.size}`);
console.log();

let delegating = 0;
const rows = [];
for (const t of tools) {
  const d = t.description ?? "";
  const named = [...meaningFields].filter((f) => d.includes(f));
  // The plain `meaning` field, matched only as a backticked field reference (see header).
  if (/`meaning`/.test(d)) named.push("meaning (plain)");
  if (named.length) delegating++;
  rows.push({ name: t.name, bytes: Buffer.byteLength(JSON.stringify(t), "utf8"), desc: Buffer.byteLength(d, "utf8"), named });
}
rows.sort((a, b) => b.desc - a.desc);
for (const r of rows) {
  const mark = r.named.length ? "names" : "     ";
  console.log(`  ${r.name.padEnd(19)} desc ${String(r.desc).padStart(5)} B  of ${String(r.bytes).padStart(5)} B  ${mark}  ${r.named.join(", ")}`);
}
console.log();
console.log(`⇒ ${delegating} of ${tools.length} descriptions NAME at least one *Meaning field.`);
console.log("   That is 'points at the owner', not 'stopped explaining' — see this file's header for");
console.log("   what it cannot see. A reader still decides whether a named field is also re-defined.");
