#!/usr/bin/env node
/**
 * Which MCP-registry versions advertise download URLs that do not exist.
 *
 * ## Why this exists (2026-09-24, review ledger V328)
 *
 * The repository was recreated on 2026-09-23 to remove a committed identity from its public history,
 * and the release assets of every earlier tag went with it. `VERSIONING.md` was narrowed the same day
 * to promise assets only from v0.35.0 onward (review ledger V304) — but the MCP registry keeps its own
 * copy of that promise, one record per published version, each carrying five `.mcpb` download URLs,
 * and every one of those records still says `status: active`.
 *
 * A bare 404 is an honest answer: it says *this does not exist*, which is true. `status: active` is
 * not an answer, it is a CLAIM — the registry is telling every MCP client that this version is
 * installable, in our name. That is the shape this repository already judged worse than failing (the
 * same call V304 made when it refused to back-fill old tags with current binaries, and V312 before
 * it: a citation that opens and shows nothing is worse than one that fails).
 *
 * ## What it measures, and why not by version number
 *
 * The obvious subject is "every version below the promise line". This script does not use that,
 * because the promise line is a statement about intent and the defect is a statement about fact: a
 * record is broken when the bytes it points at are gone. So it HEADs every advertised URL and
 * classifies each record by what came back. That stays correct if assets are ever restored, if the
 * promise line moves, and if a version above the line loses its release.
 *
 *   DEAD   status=active and every advertised URL is unreachable  -> the defect; exit 1
 *   MIXED  some URLs reachable, some not                          -> reported, never auto-acted on
 *   LIVE   every advertised URL reachable                         -> nothing to do
 *
 * MIXED is deliberately not folded into DEAD. A half-published version is a different fault with a
 * different cause, and deprecating it would hide the half that works; this script names it and stops.
 *
 * ## What it does NOT check
 *
 * Reachability, not correctness: a URL that returns 200 might serve the wrong bytes, and the registry
 * records a `fileSha256` this script never verifies. It also reads only the records the registry
 * returns for this one server name — a different namespace is a different subject.
 *
 * ## Both verdicts are observed, not just one
 *
 * Its sibling `release-surfaces.mjs` records that its green path had never been seen. This one saw
 * both on its first run: 14 records DEAD and v0.36.0 LIVE, in the same output. The reachable half of
 * the classification is therefore demonstrated rather than assumed.
 *
 * Usage:  node scripts/measure/registry-dead-versions.mjs [--json] [--json-out=PATH]
 * `--json` prints the machine-readable form instead of the table; `--json-out=PATH` writes it to a
 * file while the table still prints, so a caller gets both from ONE measurement — two passes could
 * disagree with each other, and then the list acted on would not be the list reported.
 * Exit 1 when at least one active record advertises only dead URLs; exit 2 when the registry could
 * not be read, or read back an empty or shapeless list (which is not the same as clean and must
 * never be reported as green).
 */

import { writeFileSync } from "node:fs";

const SERVER_NAME = "io.github.eezz4/zzop";
const REGISTRY = "https://registry.modelcontextprotocol.io";
const META_KEY = "io.modelcontextprotocol.registry/official";

const asJson = process.argv.includes("--json");
const jsonOutArg = process.argv.find((a) => a.startsWith("--json-out="));
const jsonOut = jsonOutArg ? jsonOutArg.slice("--json-out=".length) : null;
const log = (s) => {
  if (!asJson) console.log(s);
};

function die(code, ...lines) {
  for (const l of lines) console.error(l);
  process.exit(code);
}

async function readRegistry() {
  const url = `${REGISTRY}/v0/servers?search=${encodeURIComponent(SERVER_NAME)}&limit=100`;
  let res;
  try {
    res = await fetch(url, { headers: { accept: "application/json" } });
  } catch (e) {
    die(2, `registry-dead-versions: could not reach the registry: ${e.message}`);
  }
  if (!res.ok) die(2, `registry-dead-versions: registry answered ${res.status} for the server list`);
  const body = await res.json();
  const rows = Array.isArray(body?.servers) ? body.servers : null;
  // FLOOR. An empty or shapeless list is a broken read, never a clean registry: this server has been
  // published for many versions, so zero records means the query shape changed under us.
  if (!rows || rows.length === 0) {
    die(2, "registry-dead-versions: the registry returned no records for " + SERVER_NAME + ".");
  }
  return rows.filter((r) => r?.server?.name === SERVER_NAME);
}

// HEAD, following redirects, because a live release asset answers 302 to the object store. A HEAD
// that throws is counted as unreachable rather than crashing the run: the verdict this script exists
// for is "these URLs do not resolve", and a DNS or TLS failure is that same answer with a different
// spelling. It is reported so a network-wide outage cannot read as "every version is dead" silently.
async function reachable(url) {
  try {
    const res = await fetch(url, { method: "HEAD", redirect: "follow" });
    return { ok: res.ok, code: res.status };
  } catch (e) {
    return { ok: false, code: `ERR ${e.message.slice(0, 40)}` };
  }
}

const records = await readRegistry();
const out = [];
let urlsSeen = 0;

for (const r of records) {
  const version = r.server?.version ?? "(no version)";
  const status = r._meta?.[META_KEY]?.status ?? "(no status)";
  const urls = (r.server?.packages ?? [])
    .map((p) => p?.identifier)
    .filter((u) => typeof u === "string" && u.startsWith("https://"));
  urlsSeen += urls.length;

  const checked = await Promise.all(urls.map(async (u) => ({ url: u, ...(await reachable(u)) })));
  const live = checked.filter((c) => c.ok).length;
  const verdict = urls.length === 0 ? "NO-URLS" : live === 0 ? "DEAD" : live === urls.length ? "LIVE" : "MIXED";
  out.push({ version, status, urls: urls.length, live, verdict, checked });
}

// FLOOR, second half: records with no advertised URL at all would make every verdict vacuous.
if (urlsSeen === 0) {
  die(2, `registry-dead-versions: ${records.length} record(s) but not one advertises a download URL.`);
}

const deadActive = out.filter((r) => r.verdict === "DEAD" && r.status === "active");
const mixed = out.filter((r) => r.verdict === "MIXED");

const machine = { serverName: SERVER_NAME, records: out, deadActive: deadActive.map((r) => r.version) };
if (jsonOut) writeFileSync(jsonOut, JSON.stringify(machine, null, 2) + "\n");

if (asJson) {
  console.log(JSON.stringify(machine, null, 2));
} else {
  log(`registry-dead-versions: ${SERVER_NAME} — ${out.length} record(s), ${urlsSeen} advertised URL(s)`);
  log("");
  const w = Math.max(...out.map((r) => r.version.length));
  for (const r of out) {
    log(`  ${r.version.padEnd(w)}  ${r.status.padEnd(10)} ${String(r.live).padStart(2)}/${r.urls} live  ${r.verdict}`);
  }
  log("");
}

if (mixed.length) {
  console.error(`registry-dead-versions: ${mixed.length} record(s) are MIXED — some URLs resolve and some do not.`);
  console.error("  Not folded into the count below and not safe to deprecate: a half-published version");
  console.error("  is a different fault, and deprecating it would hide the half that works.");
  for (const r of mixed) console.error(`    ${r.version}: ${r.live}/${r.urls} live`);
}

if (deadActive.length === 0) {
  log("registry-dead-versions: clean — every active record's download URLs resolve.");
  process.exit(mixed.length ? 1 : 0);
}

console.error(
  `registry-dead-versions: ${deadActive.length} of ${out.length} record(s) say status=active while ` +
    `NONE of their download URLs resolve.`,
);
console.error("");
for (const r of deadActive) console.error(`    ${r.version}  (0/${r.urls} live)`);
console.error("");
console.error("  These claim, in this project's name, that a version is installable when its bytes are");
console.error("  gone. A 404 alone would be honest; `status: active` in front of one is not.");
console.error("");
console.error("  Fix: run the `registry-status` workflow from the Actions tab with `apply` checked. It");
console.error("  re-measures first, then sets each of these to `deprecated` with a message pointing at");
console.error("  the versions that do exist. Reversible — the same tool sets `active` back.");
process.exit(1);
