#!/usr/bin/env bash
# check-prose-rule-ids — a rule id written in shipped PROSE must name a rule a reader can actually run,
# or say where to get it, or be recorded as retired. Two checks, one subject family.
#
# ## The defect this exists for (2026-08-11, found by a domain review, not by a guard)
#
# The `typescript` pack moved out of the bundle into `examples/packs/typescript-lint.json`. Three
# surfaces kept naming its ids as if nothing had happened, and the worst was `site/usage.html`'s
# suppression-marker paragraph: it taught marker DERIVATION using `typescript/no-explicit-any` as the
# worked example, and the shipped binary answers that id with `unknown rule id`. A reader following the
# only instructions for deriving a marker had no rule to derive one from.
#
# `scripts/check-docs-rule-ids.sh` — the closest relative — did not see it and structurally could not:
# its subject is CONFIG EXAMPLE SHAPES (`"<id>": "off"`, `disabledRules: [...]`), and this id sat in an
# ordinary `<code>` span in a sentence. `VERSIONING.md`'s rename tables, which pointed at two of the
# same moved ids as the "new id" a reader should migrate TO, were read by nothing at all.
#
# ## CHECK 1 — qualified ids in prose (`<pack>/<rule>` inside backticks or <code>)
#
# The token shape is what makes this checkable without an allowlist: only ids carry a `<namespace>/`
# prefix in backticks, and a namespace is accepted only when it is a real pack id or a native-analysis
# namespace, so a path like `crates/config` or `docs/rules` is never a subject. An id passes when:
#
#   1. it is BUNDLED — a rule of a pack under `rules/dsl/`, or a native analysis id from
#      `docs/rules/catalog.md`'s own Native-analyses table; or
#   2. it lives in an `examples/packs/` pack AND THE SAME FILE NAMES THAT PACK'S FILE. An exported rule
#      is real but not loaded by default, so naming it is honest only alongside the way to load it.
#      This is the rule that failed `site/usage.html` in the incident above and that passes
#      `examples/packs/README.md`, and it is deliberately a property of the FILE rather than a list of
#      blessed files: the fix for a violation is to add the retrieval pointer, which is exactly what the
#      reader was missing; or
#   3. `VERSIONING.md`'s rename tables record it — that file IS this repo's register of ids that once
#      existed, so "is this a historical id?" has a derived answer instead of a marker-word heuristic.
#      A sentence explaining a rename must be able to name the old id (`docs/rules/catalog.md` and
#      `site/rules.html` both do, for `cross-layer/sdk-import-no-visible-consume`).
#
# Measured on the clean tree (2026-08-12): 257 qualified ids judged, 0 offenders. (Re-derive rather than
# trust that figure — the subject set is every tracked .md/.html and grows with the tree; the script
# prints its own count on every run.) Planting the historical `typescript/no-explicit-any` into
# `docs/getting-started.md` reports it with rule 2's reason — re-measured 2026-08-14. The recipe named
# `site/usage.html` until that day, when the page became a redirect stub: still technically in the subject
# set, but a stub nobody reads is a poor place to demonstrate a guard, and `docs/getting-started.md` is
# hand-written (a plant into the GENERATED site/index.html would be undone by the next regeneration) and
# does not name `examples/packs/typescript-lint.json`, which is what makes rule 2 the reason reported.
#
# ## CHECK 2 — the "new id" column of `VERSIONING.md`'s rename tables
#
# CHECK 1's rule 3 makes `VERSIONING.md` vouch for itself, so that file needs a check that does NOT
# consult the register. Every rename row's NEW id (the one a reader is told to migrate TO) must satisfy
# rule 1 or rule 2 — nothing else. Rows come from the table shape (3 cells = `pack | old | new`, 2
# cells = the native table's `old | new`), never from line numbers.
#
# ONE EXEMPTION, and it is derived rather than listed: an id whose own file names it on a line saying
# `REMOVED`. `http/get-route-no-cache-marker` is that case — renamed, then deleted outright with no
# replacement, which the ⚠ note above the table states. Checked BOTH WAYS (working-agreements §5.5's
# second rule): a `REMOVED` note naming an id that still resolves is a stale note and fails too, so the
# exemption cannot quietly outlive its subject.
#
# ## What this deliberately does NOT check
#
# A BARE id in prose (`dead-candidates`, `nplus1` — no namespace prefix). There is no token shape that
# separates one from an ordinary hyphenated English phrase, and `check-docs-rule-ids.sh` already covers
# the shape where a bare id is load-bearing: a config example key. Widening here would mean an
# allowlist, which is how this repo kills a guard.
set -euo pipefail
cd "$(dirname "$0")/.."

. ./scripts/lib/tracked-grep.sh

if ! command -v node >/dev/null 2>&1; then
  echo "check-prose-rule-ids: \`node\` not found. This guard parses pack JSON and Markdown tables" >&2
  echo "  rather than scanning text, so it cannot degrade to grep. Install node; skipping would" >&2
  echo "  report clean while checking nothing." >&2
  exit 1
fi

files="$(tracked_and_untracked_files_matching '.' '*.md' '*.html')"
if [ -z "$files" ]; then
  echo "check-prose-rule-ids: enumerated ZERO .md/.html files — the enumeration broke. An empty" >&2
  echo "  subject set is a broken guard, never a clean tree." >&2
  exit 1
fi

printf '%s\n' "$files" | node --input-type=module -e '
import fs from "fs";
import path from "path";
import { nativeAnalysisIds } from "./scripts/lib/catalog-native-ids.mjs";

// --- the id universe, all three sources DERIVED ------------------------------------------------
const packsUnder = (dir) => {
  const out = new Map();
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) { walk(p); continue; }
      if (!e.name.endsWith(".json")) continue;
      const pack = JSON.parse(fs.readFileSync(p, "utf8"));
      if (!pack.id || !Array.isArray(pack.rules)) continue;
      out.set(pack.id, { base: e.name, rules: new Set(pack.rules.map((r) => r.id)) });
    }
  };
  walk(dir);
  return out;
};
const bundled = packsUnder("rules/dsl");
const examples = packsUnder("examples/packs");

// The native half of rule 1. Shared with check-runtime-text-rule-ids.sh, which read the same table
// with the same two widening bugs until 2026-08-13 — scripts/lib/catalog-native-ids.mjs carries the
// measurement and names who owns the reasoning.
const natives = nativeAnalysisIds("check-prose-rule-ids", "every native id would read as unknown.");

if (bundled.size < 8) {
  console.error("check-prose-rule-ids: derived " + bundled.size + " bundled pack(s) — far under the");
  console.error("  live count. The extraction broke; every prose id would be judged against an empty");
  console.error("  universe and this guard would fail loudly OR pass vacuously.");
  process.exit(1);
}

const versioning = fs.readFileSync("VERSIONING.md", "utf8");
const versioningLines = versioning.split(/\r?\n/);

// A rename-table row: every cell is a lone backticked id (a trailing footnote dagger is allowed).
const idCell = (c) => {
  const m = /^`([a-z][a-z0-9-\/]+)`\s*†?$/.exec(c.trim());
  return m ? m[1] : null;
};
const renameRows = [];
for (const [i, line] of versioningLines.entries()) {
  if (!line.startsWith("|")) continue;
  const cells = line.split("|").slice(1, -1).map(idCell);
  if (cells.length < 2 || cells.includes(null)) continue;
  if (cells.length === 3) renameRows.push({ line: i + 1, old: `${cells[0]}/${cells[1]}`, neu: `${cells[0]}/${cells[2]}` });
  else if (cells.length === 2) renameRows.push({ line: i + 1, old: cells[0], neu: cells[1] });
}
if (renameRows.length < 20) {
  console.error("check-prose-rule-ids: parsed " + renameRows.length + " rename-table row(s) from " +
    "VERSIONING.md (expected at least 20; 31 on 2026-08-12). The table shape changed — CHECK 2 would");
  console.error("  pass over a table it can no longer read.");
  process.exit(1);
}

// CHECK 1 rule 3'"'"'s register: every id VERSIONING.md names at all (table cells and the prose notes
// around them). The file is the record of ids that once existed.
const retired = new Set();
for (const r of renameRows) { retired.add(r.old); retired.add(r.neu); }
for (const m of versioning.matchAll(/`([a-z][a-z0-9-]*\/[a-z][a-z0-9-]*)`/g)) retired.add(m[1]);

// --- resolution ---------------------------------------------------------------------------------
// "bundled" / "example" / null, plus whether a given file names the example pack'"'"'s own file.
const resolve = (id, text) => {
  if (natives.has(id)) return "bundled";
  if (!id.includes("/")) return null;
  const [ns, rule] = id.split("/");
  if (bundled.has(ns) && bundled.get(ns).rules.has(rule)) return "bundled";
  if (examples.has(ns) && examples.get(ns).rules.has(rule)) {
    return text.includes(examples.get(ns).base) ? "example-named" : "example-unnamed";
  }
  return null;
};

const namespaces = new Set([
  ...bundled.keys(),
  ...examples.keys(),
  ...[...natives].filter((n) => n.includes("/")).map((n) => n.split("/")[0]),
]);

// --- CHECK 1 -------------------------------------------------------------------------------------
const docs = fs.readFileSync(0, "utf8").split("\n").map((s) => s.trim()).filter(Boolean);
const tokenRe = /(?:`([a-z][a-z0-9-]*\/[a-z][a-z0-9-]*)`|<code>([a-z][a-z0-9-]*\/[a-z][a-z0-9-]*)<\/code>)/g;
const offenders = [];
let judged = 0;

for (const file of docs) {
  let text;
  try { text = fs.readFileSync(file, "utf8"); } catch { continue; }
  const lines = text.split(/\r?\n/);
  for (const [i, line] of lines.entries()) {
    tokenRe.lastIndex = 0;
    let m;
    while ((m = tokenRe.exec(line))) {
      const id = m[1] || m[2];
      if (!namespaces.has(id.split("/")[0])) continue;
      judged++;
      const state = resolve(id, text);
      if (state === "bundled" || state === "example-named") continue;
      if (retired.has(id)) continue;
      offenders.push(state === "example-unnamed"
        ? `${file}:${i + 1}  ${id}  — lives in examples/packs/${examples.get(id.split("/")[0]).base}, which is NOT loaded by default, and this file never names that file. Either use a bundled id or tell the reader where the pack is.`
        : `${file}:${i + 1}  ${id}  — names no rule this repo ships, and VERSIONING.md does not record it as retired.`);
    }
  }
}

if (judged < 100) {
  console.error("check-prose-rule-ids: only " + judged + " qualified id(s) were judged (expected at " +
    "least 100; 257 on 2026-08-12). The token extraction stopped matching — a broken guard reporting");
  console.error("  clean, not a repo that stopped naming its rules.");
  process.exit(1);
}

// --- CHECK 2 -------------------------------------------------------------------------------------
// The derived exemption: an id VERSIONING.md itself names on a `REMOVED` line.
const removedNoted = new Set();
for (const line of versioningLines) {
  if (!line.includes("REMOVED")) continue;
  for (const m of line.matchAll(/`([a-z][a-z0-9-]*\/[a-z][a-z0-9-]*)`/g)) removedNoted.add(m[1]);
}

for (const row of renameRows) {
  const state = resolve(row.neu, versioning);
  if (state === "bundled" || state === "example-named") continue;
  if (removedNoted.has(row.neu)) continue;
  offenders.push(state === "example-unnamed"
    ? `VERSIONING.md:${row.line}  ${row.neu}  — the migration target moved to examples/packs/${examples.get(row.neu.split("/")[0]).base} and VERSIONING.md never names that file, so a reader who migrates gets \`unknown rule id\`.`
    : `VERSIONING.md:${row.line}  ${row.neu}  — the migration target resolves to no shipped rule, and no \`REMOVED\` note in this file names it.`);
}

// The exemption checked the OTHER way: a `REMOVED` note whose id still resolves is stale, and would
// silently license a real regression the day that id is reused.
for (const id of removedNoted) {
  const state = resolve(id, versioning);
  if (state === "bundled" || state === "example-named") {
    offenders.push(`VERSIONING.md  ${id}  — a \`REMOVED\` note names it, but it RESOLVES today (${state}). ` +
      "The note is stale; drop it rather than leaving an exemption that vouches for a live id.");
  }
}

if (offenders.length) {
  console.error("check-prose-rule-ids: shipped prose names a rule id a reader cannot run:");
  for (const o of offenders) console.error("  " + o);
  process.exit(1);
}

console.log("check-prose-rule-ids: OK (" + judged + " qualified id(s) in prose, " + renameRows.length +
  " rename-table row(s), across " + docs.length + " docs)");
'
