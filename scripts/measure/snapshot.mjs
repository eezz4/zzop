#!/usr/bin/env node
// snapshot.mjs — take ONE labeled measurement snapshot of a corpus, through the SHIPPED binary.
//
// NOT ITSELF A GUARD, but it now RUNS IN CI (2026-07-28) as the first half of
// `scripts/measure/detection-gate.sh` — the `detection-benchmark` job takes a snapshot with this script
// and hands it to benchmark.mjs, whose exit code is the verdict. Every `scripts/check-*.sh` guard is
// wired into both .githooks/pre-commit and ci.yml (check-guards-wired.sh enforces that); the gate is
// deliberately NOT named that way, because a `--release` build per commit is not a hook's budget.
//
// Until 2026-07-26 there was nothing for CI to point this at — every corpus was gitignored or lived
// outside the repo. Committing `cases/` removed that blocker; what remained was one fixture
// that CANNOT be committed (a contiguous vendor-token literal), and because the analyzer honors
// .gitignore, scoring the tree in place could never be green. The gate copies the corpus out of the
// repo and synthesizes that fixture there. The third-party dogfood checkouts (`corpus/oss/`) remain
// gitignored and bring-your-own — those are somebody else's code and measure a different question
// (real-world delta, not labeled recall).
//
// WHAT IT PRODUCES — <runs>/<label>/ :
//   cross.json            axis 2: cross_repo (buckets, bucketKeys, edges, crossLayerFindings, sources)
//   tree-<sourceId>.json  axis 1: analyze_repo, one file per tree (findings.byRule + FULL anchor list)
//   meta.json             what was measured with (binary identity, config, limit, axes, timings)
//
// WHY THE MCP SURFACE AND NOT `zzop analyze` — the original reason EXPIRED on 2026-07-27, recorded
// here rather than deleted so nobody re-derives it: the CLI had no `--limit` flag, so it was pinned to
// DEFAULT_FINDINGS_LIMIT (50 — crates/summary/src/output/mod.rs). Any tree over 50 findings came back
// silently clipped, and a clipped anchor list compares "identical" against another clipped one. The CLI
// now takes `--severity`/`--rule`/`--limit` through the same shared validator as the MCP tools (same
// MAX_LIMIT of 1000), so either transport can produce a complete anchor set. What still holds is this
// script's own contract: it REFUSES to write a snapshot whose anchor list was truncated. The transport
// is now incidental, not required — switching it is work nobody has needed, not a correctness fix.
//
// ## The five contracts this file enforces IN CODE (each one is a real accident, not a hypothesis)
//
//   1. LABELED, IMMUTABLE OUTPUT. Output goes to <runs>/<label>/ and an EXISTING label is REFUSED.
//      A fixed `after/` directory destroyed 22 baseline files mid-audit when the next round re-ran
//      the same script; every comparison after that was meaningless and nothing said so.
//   2. EVERY RESPONSE VALIDATED, LOUDLY. Exit code, kill signal, EMPTY stdout, per-line JSON parse,
//      the presence of the `tools/call` reply and not merely the `initialize` one, JSON-RPC `error`,
//      tool-level `isError`, payload parse, expected top-level keys. A harness once called a
//      subcommand that did not exist, discarded stderr, and wrote 22 zero-byte files — which the
//      comparison read as "460 -> 0, every finding eliminated", the best result of the batch.
//      A measuring tool that goes quiet errs in the direction that looks like success.
//   3. BOTH AXES, ALWAYS. Cross-layer findings exist ONLY in the multi-tree join response, so a
//      re-measurement done with analyze_repo alone cannot observe cross-layer rules at all — three
//      of them were structurally unmeasurable across a whole 22-tree round and the result table did
//      not show that. meta.axes records which axes ran, and diff.mjs refuses to compare two runs
//      whose axes differ, so a future single-axis mode has to announce itself.
//   4. SAME SCOPE ON BOTH AXES. Axis 1's tree list is taken from axis 2's own `sources[]` rather
//      than from a second reading of the config, so the two axes cannot silently disagree about
//      what was measured; and each tree's analyze_repo total is checked against that source's
//      `findingCount` from the join.
//   5. TRUNCATION IS NEVER "IDENTICAL". Any capped list (findings.truncated, shown.length != total,
//      edgesTruncated, crossLayerFindings.truncated) aborts the run. A capped anchor list silently
//      shrinks the set difference, and a shrunken set difference reads as "no change". Every cap left
//      on this list is one a --limit CAN raise; `bucketKeys` used to be the exception (capped by the
//      product, tolerated via --tolerate-bucket-key-cap) and that cap was deleted on 2026-07-29, so the
//      exception and its flag are both gone.
//
// PROVING IT: every abort branch above has a deliberately misbehaving counterpart in
// `selftest-stub.rs` (empty stdout, initialize-only, garbage, JSON-RPC error, isError, wrong
// payload, nonzero exit), and `scripts/measure/harness-selftest.sh` RUNS all seven — on every
// `detection-benchmark` CI job, as detection-gate.sh's preflight — requiring each to abort with ITS
// OWN message, because a mode that aborts for the wrong reason would let one over-eager branch stand
// in for six dead ones. A guard nobody has seen go red is not known to work; until 2026-07-29 nobody
// had seen these, because the stub sat in the tree with a manual build loop and zero callers.
// Still unproven anywhere: contract 1's other half — that an EXISTING label is refused. It needs no
// stub, and it is the branch that destroyed 22 baseline files mid-audit.
//
// usage:
//   node scripts/measure/snapshot.mjs --label <label> --bin <zzop-mcp[.exe]> --config <zzop.config.jsonc>
//                                     [--limit 1000] [--runs <dir>] [--allow-axis-scope-divergence]
//
// A good label says what was measured, not when: `<commit-or-branch>-<what-changed>`. It ends up in
// diff.mjs's header next to the binary's sha256, so two snapshots can never be confused.

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");

// Set once the run's output directory exists; cleared once the run is complete. An ABORTED run
// removes its own (necessarily partial) directory: contract 1 forbids overwriting a baseline, and a
// half-written directory left behind would burn its label forever — which is precisely the pressure
// that makes someone reuse a label "just this once". A directory is only ever removed here if THIS
// process created it (a pre-existing label is refused before any mkdir).
let partialOutDir = null;

export function fail(msg) {
  let note = "";
  if (partialOutDir) {
    try {
      fs.rmSync(partialOutDir, { recursive: true, force: true });
      note = `\n  (removed the partial snapshot at ${partialOutDir} — a partial run must never be diffed,\n   and its label stays free for the retry)`;
    } catch (e) {
      note = `\n  (could NOT remove the partial snapshot at ${partialOutDir}: ${e.message} — delete it before retrying)`;
    }
  }
  console.error("\nHARNESS ABORT: " + msg + note + "\n");
  process.exit(1);
}

// ---- argv -------------------------------------------------------------------------------------
const argv = process.argv.slice(2);
function arg(name, dflt) {
  const i = argv.indexOf("--" + name);
  if (i === -1) {
    if (dflt === undefined) fail("missing --" + name);
    return dflt;
  }
  if (i + 1 >= argv.length || argv[i + 1].startsWith("--")) fail("--" + name + " needs a value");
  return argv[i + 1];
}

const label = arg("label");
// A label becomes a directory name. Rejecting separators here keeps `--label ../../etc` from
// writing outside the runs root, and keeps the label that diff.mjs prints equal to the directory
// it read — a label that normalizes away is a label that can collide with another run's.
if (!/^[A-Za-z0-9._@-]+$/.test(label)) {
  fail(`--label '${label}' — use only [A-Za-z0-9._@-]; a label is a directory name and must not normalize away.`);
}

const bin = path.resolve(arg("bin"));
const configPath = path.resolve(arg("config"));
const runsRoot = path.resolve(arg("runs", path.join(REPO, "scratchpad", "runs")));
const limit = Number(arg("limit", "1000"));
const allowAxisScopeDivergence = argv.includes("--allow-axis-scope-divergence");

if (!Number.isInteger(limit) || limit < 1) fail(`--limit must be a positive integer (got ${arg("limit", "1000")})`);
if (!fs.existsSync(bin)) fail("binary not found: " + bin);
if (!fs.existsSync(configPath)) fail("config not found: " + configPath);

// ---- CONTRACT 1: labeled, immutable output ------------------------------------------------------
const outDir = path.join(runsRoot, label);
if (fs.existsSync(outDir)) {
  fail(
    `run label already exists: ${outDir}\n` +
      "  Labels are IMMUTABLE. Overwriting silently destroys a baseline that another comparison may\n" +
      "  still be reading, and nothing downstream can tell that it happened. Pick a new label."
  );
}
fs.mkdirSync(outDir, { recursive: true });
partialOutDir = outDir;

const binBytes = fs.readFileSync(bin);
const binStat = fs.statSync(bin);
const binSha256 = crypto.createHash("sha256").update(binBytes).digest("hex");

// ---- CONTRACT 2: one tools/call, fresh process, every response validated -------------------------
function callTool(name, args, tag) {
  const reqs = [
    {
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2024-11-05",
        capabilities: {},
        clientInfo: { name: "zzop-measure-harness", version: "1" },
      },
    },
    { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: args } },
  ];
  const input = reqs.map((r) => JSON.stringify(r)).join("\n") + "\n";
  const started = Date.now();
  const res = spawnSync(bin, ["mcp"], {
    input,
    encoding: "utf8",
    maxBuffer: 512 * 1024 * 1024,
    timeout: 30 * 60 * 1000,
  });
  const elapsedMs = Date.now() - started;

  // stderr is NEVER discarded: the 22-zero-byte-files accident was a wrong invocation whose whole
  // explanation was sitting in a stderr stream nobody read.
  const err = (res.stderr || "").trim();
  const withErr = (m) => `[${tag}] ${m}` + (err ? `\n  --- STDERR ---\n${err}` : "\n  (stderr was empty)");

  if (res.error) fail(withErr(`spawn failed: ${res.error.message}`));
  if (res.signal) fail(withErr(`killed by signal ${res.signal} (timeout?)`));
  if (res.status !== 0) fail(withErr(`exited ${res.status} — the binary REFUSED this invocation.`));
  if (!res.stdout || !res.stdout.trim()) fail(withErr("EMPTY stdout — nothing was measured."));

  const lines = res.stdout.split(/\r?\n/).filter((l) => l.trim());
  const frames = lines.map((l, i) => {
    try {
      return JSON.parse(l);
    } catch {
      return fail(withErr(`stdout line ${i} is not JSON: ${l.slice(0, 200)}`));
    }
  });

  const init = frames.find((f) => f.id === 1);
  const call = frames.find((f) => f.id === 2);
  if (!init) fail(withErr("no reply to `initialize` — this is not an MCP stdio server."));
  // The exact accident: an initialize-only stdout read as a successful measurement.
  if (!call) fail(withErr("got ONLY the `initialize` reply — no `tools/call` reply. NOTHING was measured."));
  if (call.error) fail(withErr(`JSON-RPC error: ${JSON.stringify(call.error)}`));
  if (!call.result) fail(withErr("`tools/call` reply carries no `result`."));
  if (call.result.isError) fail(withErr(`tool reported isError: ${JSON.stringify(call.result.content)}`));
  const text = call.result.content?.[0]?.text;
  if (!text) fail(withErr("tool result carries no text content."));
  let payload;
  try {
    payload = JSON.parse(text);
  } catch {
    fail(withErr(`tool payload is not JSON: ${text.slice(0, 300)}`));
  }
  return { payload, elapsedMs, serverInfo: init.result?.serverInfo };
}

const meta = {
  label,
  // CONTRACT 3: which axes ran is DATA, not a comment. diff.mjs reads it and refuses mismatched runs.
  axes: ["cross_repo", "analyze_repo"],
  binary: bin,
  binarySha256: binSha256,
  binaryMtime: binStat.mtime.toISOString(),
  binarySize: binStat.size,
  configPath,
  limit,
  toolSurface: "zzop-mcp stdio JSON-RPC — cross_repo(configPath) + analyze_repo(path) per tree",
  startedAt: new Date().toISOString(),
  calls: [],
};

// ---- axis 2: cross_repo (runs FIRST — it defines the scope axis 1 then uses) ---------------------
console.error(`[${label}] axis 2  cross_repo ...`);
const cross = callTool("cross_repo", { configPath, limit }, "cross_repo");
const c = cross.payload;
for (const k of ["sources", "buckets", "bucketKeys", "edges", "crossLayerFindings"]) {
  if (c[k] === undefined) fail(`cross_repo payload is missing key '${k}' — wrong tool surface or a changed contract.`);
}
if (!Array.isArray(c.sources) || c.sources.length === 0) fail("cross_repo returned 0 sources — nothing was measured.");

// CONTRACT 5: truncation is never "identical".
if (c.edgesTruncated) fail("cross_repo `edges` TRUNCATED — the edge set is incomplete: " + JSON.stringify(c.edgesTruncated));
if (c.crossLayerFindings.truncated) {
  fail(
    "`crossLayerFindings` TRUNCATED — cross-layer anchors are incomplete: " +
      JSON.stringify(c.crossLayerFindings.truncated) +
      `\n  Raise --limit (currently ${limit}).`
  );
}
// `bucketKeys` had a branch here until 2026-07-29: the product capped it at 20 distinct keys, this
// script aborted on the resulting `bucketKeysTruncated`, and `--tolerate-bucket-key-cap` was the opt-in
// that recorded the cap instead. The product cap is GONE (crates/summary/src/output/bucket_keys.rs) —
// `bucketKeys` is now complete by construction, so there is no truncation to detect, tolerate or record.
// This corpus is what removed it: it sat at exactly 20 keys and a batch adding trees scored zero lines.

fs.writeFileSync(path.join(outDir, "cross.json"), JSON.stringify(c, null, 2));
meta.serverInfo = cross.serverInfo;
meta.calls.push({ tool: "cross_repo", ms: cross.elapsedMs, sources: c.sources.length });
console.error(`[${label}]   ok — ${c.sources.length} sources, ${c.crossLayerFindings.total} cross-layer findings, ${cross.elapsedMs} ms`);

// ---- axis 1: analyze_repo per tree (CONTRACT 4: scope comes from axis 2's own sources[]) ---------
const trees = [];
const scopeDivergence = [];
for (const s of c.sources) {
  const sid = s.sourceId;
  const p = s.path;
  if (!sid || !p) fail("a cross_repo source has no sourceId/path: " + JSON.stringify(s));
  console.error(`[${label}] axis 1  analyze_repo ${sid} ...`);
  const r = callTool("analyze_repo", { path: p, limit }, "analyze_repo:" + sid);
  const a = r.payload;
  for (const k of ["findings", "fileCount"]) {
    if (a[k] === undefined) fail(`analyze_repo(${sid}) payload is missing key '${k}'.`);
  }
  if (a.findings.truncated) {
    fail(
      `analyze_repo(${sid}) findings TRUNCATED — anchors incomplete: ${JSON.stringify(a.findings.truncated)}\n` +
        `  Raise --limit (currently ${limit}). A clipped anchor list compares "identical" against another clipped one.`
    );
  }
  if (a.findings.shown.length !== a.findings.total) {
    fail(
      `analyze_repo(${sid}) shown(${a.findings.shown.length}) != total(${a.findings.total}) — the anchor set is\n` +
        "  incomplete even though no truncation was disclosed. Refusing to snapshot a partial set."
    );
  }
  // CONTRACT 4 (second half): the two axes must agree on how much they saw of each tree. They can
  // legitimately differ — analyze_repo auto-discovers a config INSIDE the tree, while cross_repo
  // applies the join config's per-tree overrides — but that difference must be declared, not found
  // out later while reading a table.
  if (typeof s.findingCount === "number" && s.findingCount !== a.findings.total) {
    scopeDivergence.push({ sourceId: sid, crossFindingCount: s.findingCount, analyzeTotal: a.findings.total });
  }
  const fname = "tree-" + sid.replace(/[^A-Za-z0-9._-]/g, "_") + ".json";
  fs.writeFileSync(path.join(outDir, fname), JSON.stringify(a, null, 2));
  trees.push({ sourceId: sid, path: p, file: fname, total: a.findings.total, crossFindingCount: s.findingCount });
  meta.calls.push({ tool: "analyze_repo", sourceId: sid, ms: r.elapsedMs, total: a.findings.total });
  console.error(`[${label}]   ok — ${a.findings.total} findings, ${r.elapsedMs} ms`);
}

if (scopeDivergence.length) {
  const detail = scopeDivergence
    .map((d) => `    ${d.sourceId}: cross_repo says ${d.crossFindingCount}, analyze_repo says ${d.analyzeTotal}`)
    .join("\n");
  if (!allowAxisScopeDivergence) {
    fail(
      "THE TWO AXES DISAGREE ON SCOPE — the same trees produced different finding counts:\n" +
        detail +
        "\n  Usually this means the join config applies rule overrides that a per-tree analyze_repo does not\n" +
        "  see (analyze_repo only auto-discovers a zzop.config.jsonc INSIDE the tree). Comparing two runs\n" +
        "  across this difference reads a SCOPE change as a RULE change.\n" +
        "  If the divergence is expected for this corpus, re-run with --allow-axis-scope-divergence — it is\n" +
        "  then recorded in meta.json and printed by diff.mjs on the same screen as the deltas."
    );
  }
  meta.axisScopeDivergence = scopeDivergence;
  console.error(`[${label}] !! AXIS SCOPE DIVERGENCE recorded for ${scopeDivergence.length} tree(s):\n${detail}`);
}

meta.trees = trees;
meta.finishedAt = new Date().toISOString();
// meta.json is written LAST, so its presence is what makes a snapshot readable: diff.mjs and
// benchmark.mjs both open it first, and a directory without it cannot be mistaken for a run.
fs.writeFileSync(path.join(outDir, "meta.json"), JSON.stringify(meta, null, 2));
partialOutDir = null;
console.error(`[${label}] DONE -> ${outDir}`);
