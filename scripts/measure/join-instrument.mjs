#!/usr/bin/env node
/**
 * Rebuilds and runs the cross-repo JOIN instrument — the pair configs under `corpus/oss/pair-*.jsonc`.
 *
 * ## Why this file exists (2026-09-24, external review round 26, review ledger V298)
 *
 * The join is this project's headline capability and it had no regression measurement on this
 * machine. The maintainer's corpus notes name eight `corpus/oss/pair-*.jsonc` files as that
 * instrument — and `ls` found none. They live under `corpus/`, which is gitignored, so the 2026-09-10 machine move
 * took them and nothing in the repository could rebuild them: the reconstruction table was PROSE.
 *
 * Meanwhile the committed 19-tree config, the one thing that did survive, answers a different
 * question. Measured 2026-09-24: `zzop cross` over it yields 31 edges and exactly 2 cross-source
 * ones, both `table:users` joining immich to be-spring — an incidental collision on a common table
 * name, with HTTP cross-source at ZERO. Reading that as "the join is measured" is the mistake this
 * script exists to make impossible.
 *
 * ## What is committed and what is not
 *
 * The configs are not, and should not be: they point at gitignored trees. What is committed is the
 * GENERATOR and the EXPECTATIONS, so the instrument is one command away from existing again, and so
 * a number in prose can no longer be the only record of what the instrument answered.
 *
 * ## The vocabulary is copied wholesale, deliberately
 *
 * Each pair config carries the 19-tree config's `vocabulary` verbatim. The contract is "not declared
 * means not judged", so an undeclared vocabulary also switches off the `externallyFetchedPaths` veto
 * — measured, that makes `fe-vite+be-express` report one extra `unconsumed-endpoint` whose subject
 * is `GET /`, which was written into the prose table as a real drift and was not (review ledger
 * V83). Copying rather than re-typing is what keeps the pairs and the 19-tree run comparable.
 *
 * ## Usage
 *
 *   node scripts/measure/join-instrument.mjs            # rebuild the configs, run them, check
 *   node scripts/measure/join-instrument.mjs --gen-only # rebuild only
 *
 * Exit 1 when a pair's cross-source count disagrees with the table below, or when the corpus is not
 * present. A missing corpus is NOT a pass — re-clone it before reading a skipped run as green.
 */
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";

const repoRoot = execFileSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" }).trim();
const OSS = path.join(repoRoot, "corpus/oss");
const BASE = path.join(OSS, "zzop.config.jsonc");
const BIN = path.join(repoRoot, "target/release/zzop");

/**
 * The instrument, and the answer each pair gave on 2026-09-07 (the run that produced
 * the corpus notes' table) and again on 2026-09-24 (this script's first run).
 *
 * `decl` is the ONE declaration that unblocks the join, and which side it belongs on is not
 * interchangeable: angular/axum have the frontend calling `/articles` while the backend serves
 * `/api/articles`, so the declaration is the FE's `clientBase`; aspnet is the mirror, so it is the
 * BE's `mountedAt`. Put it on the wrong side and the join stays at 0.
 */
const PAIRS = [
  { fe: "fe-vite", be: "be-express", decl: null, expect: 19 },
  { fe: "fe-axios", be: "be-spring", decl: null, expect: 19 },
  { fe: "fe-angular", be: "be-nest", decl: { side: "fe", topology: { clientBase: "/api" } }, expect: 19 },
  { fe: "fe-axios", be: "be-axum", decl: { side: "fe", topology: { clientBase: "/api" } }, expect: 19 },
  { fe: "fe-vite", be: "be-aspnet", decl: { side: "be", topology: { mountedAt: "/api" } }, expect: 19 },
  // Two pairs answer 0 and that is the CORRECT answer, not a failure of the join: the frontend
  // produces no keyed consume at all. fe-redux wraps superagent; fe-svelte has no `.svelte` parser
  // in this build plus an `api.js` wrapper. The reply says so itself through
  // `coverage.joinContributionZero`. Pinning them at 0 is what makes a future 19 visible.
  { fe: "fe-redux", be: "be-fastapi", decl: null, expect: 0 },
  { fe: "fe-svelte", be: "be-gin", decl: null, expect: 0 },
  // NOT BUILT HERE: fe-vue + be-django. Its join needs the backend's 19 `routes` declared by hand,
  // read per view class out of `conduit/apps/*/views.py` — Django's `url()` names a view and the verb
  // lives on the DRF generic base or the view's own `def get/post`, which is a declaration this config
  // is meant to supply rather than something to read harder for. Those 19 lines are not in the prose
  // table, so generating them here would be inventing them. It stays a hole, and it is named.
];

const strip = (s) => s.replace(/^[ \t]*\/\/.*$/gm, "").replace(/\/\*[\s\S]*?\*\//g, "");

function die(msg) {
  console.error(`join-instrument: ${msg}`);
  process.exit(1);
}

if (!fs.existsSync(BASE)) {
  die(
    `no ${path.relative(repoRoot, BASE)} — the dogfood corpus is not on this machine.\n` +
      "  That is not a pass. Re-clone the corpus before reading a skipped run as green.",
  );
}
const base = JSON.parse(strip(fs.readFileSync(BASE, "utf8")));
if (!base.vocabulary || Object.keys(base.vocabulary).length === 0) {
  die("the 19-tree config declares no vocabulary — copying it would switch off the vetoes (ledger V83).");
}

const written = [];
for (const p of PAIRS) {
  const name = `${p.fe}-${p.be}`;
  const tree = (id, on) => {
    const t = { root: `./${id}`, sourceId: id };
    if (on) t.topology = p.decl.topology;
    return t;
  };
  const cfg = {
    trees: [tree(p.fe, p.decl?.side === "fe"), tree(p.be, p.decl?.side === "be")],
    packs: base.packs,
    rules: base.rules,
    git: base.git,
    exclude: base.exclude,
    parsers: base.parsers,
    vocabulary: base.vocabulary,
  };
  const file = path.join(OSS, `pair-${name}.jsonc`);
  const header =
    `// GENERATED by scripts/measure/join-instrument.mjs — edit that file, not this one.\n` +
    `// Join instrument: ${p.fe} + ${p.be}. Expected cross-source edges: ${p.expect}.\n` +
    `// Vocabulary is copied verbatim from the 19-tree config (ledger V83: an undeclared vocabulary\n` +
    `// also switches off the externallyFetchedPaths veto, which once turned a non-finding into one).\n\n`;
  fs.writeFileSync(file, header + JSON.stringify(cfg, null, 2) + "\n");
  written.push({ ...p, name, file });
}
console.log(`join-instrument: wrote ${written.length} pair config(s) to ${path.relative(repoRoot, OSS)}/`);

if (process.argv.includes("--gen-only")) process.exit(0);

if (!fs.existsSync(BIN)) {
  die(`no ${path.relative(repoRoot, BIN)} — build it first: cargo build --release -p zzop-cli-bin`);
}

/**
 * Cross-source edges only. Reading the `edges` total instead is the mistake the corpus notes warn
 * about in the same breath: a tree's own `db-table` edges land in that number, which is how
 * the 19-tree run reports 31 edges while joining two repositories exactly twice, on a table name.
 */
function crossSource(reply) {
  const edges = reply?.crossLayer?.edges ?? reply?.edges ?? [];
  return edges.filter((e) => e?.from?.source !== undefined && e.from.source !== e?.to?.source).length;
}

let bad = 0;
const rows = [];
for (const p of written) {
  let out;
  try {
    out = execFileSync(BIN, ["cross", "--config", path.basename(p.file), "--limit", "0"], {
      cwd: OSS,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
    });
  } catch (e) {
    rows.push([p.name, p.expect, "ERROR", "x"]);
    console.error(`  ${p.name}: ${String(e.stderr || e.message).split("\n")[0]}`);
    bad++;
    continue;
  }
  const got = crossSource(JSON.parse(out));
  const ok = got === p.expect;
  if (!ok) bad++;
  rows.push([p.name, p.expect, got, ok ? "ok" : "DRIFT"]);
}

const w = Math.max(...rows.map((r) => r[0].length));
for (const [name, expect, got, verdict] of rows) {
  console.log(`  ${name.padEnd(w)}  expected ${String(expect).padStart(3)}  got ${String(got).padStart(3)}  ${verdict}`);
}

if (bad > 0) {
  console.error(
    `join-instrument: ${bad} pair(s) disagree with the recorded table.\n` +
      "  A change here is a change to the headline capability. Either the join moved, or a corpus tree\n" +
      "  is at a different commit than the one the table was measured on — check the second before the\n" +
      "  first (the corpus notes pin each tree's sha).",
  );
  process.exit(1);
}
console.log(
  `join-instrument: OK (${rows.length} pairs match; fe-vue+be-django is NOT covered — see this file's PAIRS note).`,
);
