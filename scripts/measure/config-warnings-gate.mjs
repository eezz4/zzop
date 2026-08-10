#!/usr/bin/env node
// config-warnings-gate.mjs — assert that a measurement config's `configWarnings` is EMPTY, so a
// measurement can never again be taken through a config the engine is partly ignoring.
//
// ## Why this exists
// `corpus/oss/zzop.config.jsonc` declared `vocabulary.fsd` for two days after that key was renamed to
// `vocabulary.featureSlicedDesign` (2026-08-07). Under the vocabulary contract an UNDECLARED key is not
// judged, so every corpus measurement in that window ran the FSD/hierarchy axis on the undeclared
// partition while the file read as if it configured one. The engine was not silent — it emitted
// `unknown config key "vocabulary.fsd" (ignored)` on all 17 trees, on every run. Nobody read it, because
// nothing FAILED.
//
// That is the whole class, and the instance is the boring part: a knob whose declaration changes nothing
// is worse than no knob, because the config file then advertises control it does not have. A measurement
// harness is exactly where that has to be fatal — a rule-precision number taken through a half-honored
// config is a number about a strawman, and the 2026-07-29 external review that read corpus noise as
// "real-code precision 1/3" is this repo's standing proof of how expensive that mistake is.
//
// ## Why EMPTY and not "no unknown keys"
// Narrowing the assertion to the unknown-key spelling would re-open the class one warning at a time:
// `vocabulary.<k> entry "..." can never match` (an accepted key whose VALUE is inert) is a different
// sentence and the same defect, and the next such sentence has not been written yet. So the subject is
// the whole array, and anything that must survive is named EXPLICITLY below with its reason.
//
// ## The allowlist, and the three ways it is kept honest
// `ALLOWED` is not an escape hatch:
//   1. Every entry is SCOPED TO ONE CONFIG by its `config` regex. An exemption is a fact about a
//      particular measurement corpus, never about zzop configs in general, so an entry cannot leak onto
//      the next config someone points this gate at. (Found by running the gate against a single-tree
//      config while the list was still global: it failed with a STALE verdict on an exemption that was
//      simply irrelevant there.)
//   2. An entry that matches a KNOB-CLASS warning is itself a FAILURE. The allowlist can never be used
//      to silence the thing this gate exists for, however it is spelled.
//   3. An IN-SCOPE entry that matches NOTHING is a FAILURE. A stale allowlist is how a guard rots into a
//      rubber stamp, and this one says so on the run that makes it stale.
// Allowlisted warnings are PRINTED, never hidden — the gate is green with them on screen.
//
// usage:
//   node scripts/measure/config-warnings-gate.mjs --bin <zzop[.exe]> --config <zzop.config.jsonc>
//
// Lane is chosen by the config itself: a config declaring 2+ trees is the cross-layer join's question
// (`zzop cross`), and a 1-tree config is `zzop analyze` — `analyze` REFUSES a multi-tree config, so
// guessing wrong here would read as a config defect rather than a harness one.
//
// Exit 0 = clean (or only allowlisted warnings). Exit 1 = a declared knob is not being read.

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

function fail(msg) {
  console.error("\nCONFIG-WARNINGS GATE FAILED: " + msg + "\n");
  process.exit(1);
}

// ---- argv ---------------------------------------------------------------------------------------
const argv = process.argv.slice(2);
function arg(name) {
  const i = argv.indexOf("--" + name);
  if (i === -1) fail("missing --" + name + "\n  usage: --bin <zzop[.exe]> --config <zzop.config.jsonc>");
  if (i + 1 >= argv.length || argv[i + 1].startsWith("--")) fail("--" + name + " needs a value");
  return argv[i + 1];
}
const bin = path.resolve(arg("bin"));
const configPath = path.resolve(arg("config"));
if (!fs.existsSync(bin)) fail("binary not found: " + bin);
if (!fs.existsSync(configPath)) fail("config not found: " + configPath);

// ---- warnings that are NOT about a knob ----------------------------------------------------------
// Each entry: WHICH config it speaks for, the warning regex, and the sentence a reader needs for why it
// is not a defect. Adding one is a deliberate act — see the three honesty rules in this file's header.
const ALLOWED = [
  {
    config: /corpus[\\/]oss[\\/]zzop\.config\.jsonc$/,
    re: /^\d+ sibling director(?:y|ies) under .* (?:is|are) not part of this join:/,
    why:
      "corpus/oss holds one checkout (the-algorithm-ml) deliberately OUTSIDE the 17-tree join: it is an " +
      "unrelated ML repo, not another layer of the same system, and the join's question is cross-layer " +
      "contracts. The engine is right to say so, and the right answer is to leave it out — this is an " +
      "advisory about CORPUS SCOPE, not about a config key being ignored.",
  },
];

// A warning matching any of these is about a DECLARED KNOB THAT IS NOT BEING READ — the class this gate
// exists for. It may never be allowlisted, whatever an ALLOWED regex happens to match.
const KNOB_CLASS = [
  /unknown config key/i,
  /can never match/i,
  /REMOVED from zzop's recognized config keys/i,
];
const isKnobClass = (w) => KNOB_CLASS.some((re) => re.test(w));

// ---- run ------------------------------------------------------------------------------------------
const raw = fs.readFileSync(configPath, "utf8");
// jsonc: this repo's configs put `//` comments on their own lines only, so a line filter is enough and
// cannot eat a `//` inside a regex value (e.g. authFamilyPathPattern).
const declared = JSON.parse(raw.split("\n").filter((l) => !/^\s*\/\//.test(l)).join("\n"));
const treeCount = Array.isArray(declared.trees) ? declared.trees.length : 0;
if (treeCount === 0) fail(`${configPath} declares no trees — nothing to measure through.`);
const subcommand = treeCount >= 2 ? "cross" : "analyze";

console.error(`[config-warnings-gate] ${subcommand} --config ${configPath} (${treeCount} tree(s)) ...`);
const run = spawnSync(bin, [subcommand, "--config", configPath, "--limit", "0"], {
  encoding: "utf8",
  maxBuffer: 256 * 1024 * 1024,
});
if (run.error) fail(`could not run ${bin}: ${run.error.message}`);
if (run.status !== 0) fail(`${bin} ${subcommand} exited ${run.status}\n--- STDERR ---\n${run.stderr}`);
if (!run.stdout.trim()) fail(`${bin} ${subcommand} wrote NOTHING to stdout.\n--- STDERR ---\n${run.stderr}`);

let payload;
try {
  payload = JSON.parse(run.stdout);
} catch (e) {
  fail(`${bin} ${subcommand} stdout is not JSON (${e.message}) — wrong binary or a changed surface.`);
}
// Non-vacuity floor: `configWarnings` is always present on both lanes, empty included. If it is missing,
// this gate read nothing and would pass forever.
if (!Array.isArray(payload.configWarnings)) {
  fail(
    "the reply has no `configWarnings` ARRAY — this gate would be vacuously green.\n" +
      `  top-level keys seen: ${Object.keys(payload).join(", ")}`,
  );
}

// ---- verdict ----------------------------------------------------------------------------------------
// Honesty rule 1: only entries written FOR this config are in play. An out-of-scope entry exempts
// nothing and is not held to the stale check either — it simply is not this config's business.
const active = ALLOWED.filter((a) => a.config.test(configPath));

const warnings = payload.configWarnings.map(String);
const matchedBy = new Map(active.map((a) => [a, []]));
const fatal = [];
for (const w of warnings) {
  const hit = active.find((a) => a.re.test(w));
  if (hit && !isKnobClass(w)) {
    matchedBy.get(hit).push(w);
  } else {
    fatal.push(w);
  }
}

// Honesty rule 2: an ALLOWED entry that covers a knob-class warning is a defect in the allowlist.
const swallowed = warnings.filter((w) => isKnobClass(w) && active.some((a) => a.re.test(w)));
if (swallowed.length) {
  fail(
    "an ALLOWED pattern matches a KNOB-CLASS warning — the allowlist is being used to hide the exact\n" +
      "class this gate exists for. Narrow the pattern; do not widen the exemption.\n" +
      swallowed.map((w) => "  " + w).join("\n"),
  );
}

// Honesty rule 3: a stale IN-SCOPE ALLOWED entry.
const stale = active.filter((a) => matchedBy.get(a).length === 0);
if (stale.length) {
  fail(
    "an ALLOWED entry written for THIS config matched NOTHING — the warning it exempts is gone, so the\n" +
      "exemption is stale and this gate is quietly weaker than it reads. Delete the entry.\n" +
      stale.map((a) => "  " + a.re).join("\n"),
  );
}

for (const a of active) {
  for (const w of matchedBy.get(a)) {
    console.error(`[config-warnings-gate] ALLOWED: ${w}`);
    console.error(`[config-warnings-gate]   why: ${a.why}`);
  }
}

if (fatal.length) {
  fail(
    `${fatal.length} config warning(s) — this config declares something the engine is NOT reading, so\n` +
      "every measurement taken through it is about a different configuration than the file describes:\n" +
      fatal.map((w) => "  " + w).join("\n") +
      "\n\nFix the config (or, if the warning is genuinely not about a knob, add it to ALLOWED in this\n" +
      "script WITH its reason — read the two honesty rules in the header first).",
  );
}

// Reaching here means `fatal` is empty, so every warning seen was an in-scope allowlisted one.
console.error(
  `[config-warnings-gate] clean — 0 fatal config warnings ` +
    `(${warnings.length} seen, all allowlisted and printed above).`,
);
