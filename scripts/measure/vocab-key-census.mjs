#!/usr/bin/env node
// vocab-key-census.mjs — which LANES read each `vocabulary` key, so a key whose NAME promises more
// than its readers deliver cannot hide.
//
// ## The failure this exists to end
// `vocabulary.secretParamNames` reads like "the names I call secrets". Its template comment scopes it
// to "Cross-repo joins: which query parameters carry secrets", and the DSL rule `hardcoded-secret`
// never reads it -- so a user who declares `jwt`, `auth`, `signature` and `key` gets silence from the
// literal scan and an EMPTY `configWarnings`, which reads as "it worked". Measured 2026-09-11: of
// seven declared names, three fired, and two of those three fired only because they contain `secret`
// or `token` as substrings -- the declaration had nothing to do with it.
//
// That is one key. The question this script answers is how many more look like it.
//
// ## What it measures, and what it deliberately does NOT
// For every key in the config surface's `vocabulary` list, it greps the workspace for readers and
// buckets each reading file by LANE (path-based):
//
//   cross-layer : rules/native/rules-cross-layer/**  (exists only when 2+ trees are joined)
//   native      : the rest of rules/native/**        (one tree)
//   dsl-engine  : crates/core/src/dsl/**             (where DSL matchers read vocabulary)
//   dsl-pack    : rules/dsl/**
//   engine      : the rest of crates/**
//   config      : crates/config/**                   -- DROPPED: declaring is not consuming
//
// A key read from two lanes is a CANDIDATE for the failure above, not proof of it: the lane split is
// path-based, so it cannot tell PLUMBING (the engine threading a value through) from CONSUMPTION (a
// rule judging with it). Narrowing that is real work and is filed as such -- this script's job is to
// hand a human the population, not the verdict. Test files are excluded so a key mentioned only by
// its own tests reads as what it is: unread.
//
// Usage:  node scripts/measure/vocab-key-census.mjs

import { execFileSync, spawnSync } from "child_process";
import fs from "fs";

const surface = JSON.parse(fs.readFileSync("crates/config/config-surface.json", "utf8"));

function findVocabList(node) {
  let found = null;
  const walk = (x) => {
    if (!x || typeof x !== "object" || found) return;
    for (const k of Object.keys(x)) {
      if (k === "vocabulary" && Array.isArray(x[k])) {
        found = x[k];
        return;
      }
      walk(x[k]);
    }
  };
  walk(node);
  return found;
}

const keys = findVocabList(surface);
if (!keys) {
  console.error("vocab-key-census: no `vocabulary` array in crates/config/config-surface.json");
  process.exit(2);
}

function laneOf(p) {
  if (p.startsWith("rules/native/rules-cross-layer/")) return "cross-layer";
  if (p.startsWith("rules/native/")) return "native";
  if (p.startsWith("crates/core/src/dsl/")) return "dsl-engine";
  if (p.startsWith("rules/dsl/")) return "dsl-pack";
  if (p.startsWith("crates/config/")) return "config";
  if (p.startsWith("crates/")) return "engine";
  return "other";
}

const IS_TEST = /(^|\/)tests?\//.test.bind(/(^|\/)tests?\//);
/// camelCase -> snake_case. `VocabularyConfig` carries `#[serde(rename_all = "camelCase")]`, so the
/// WIRE spelling is camelCase and every Rust reader spells the SAME key snake_case. Searching only
/// the wire spelling finds the declaration and misses the readers.
const snake = (key) => key.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);

/// One `git grep -l` over both spellings of one key.
///
/// 🔴 TWO DEFECTS LIVED HERE UNTIL 2026-09-26, and they produced a FALSE RED rather than a false
/// green — rarer and, because nobody distrusts a report of work to do, slower to catch.
///
/// (1) IT SEARCHED THE camelCase SPELLING ONLY. Every Rust reader spells these snake_case under
///     `#[serde(rename_all = "camelCase")]`, so a key whose only consumers are Rust looked unread.
///     Measured: this printed `middlewareGuardCallees` and `schemaUsageSkipFields` as "declared and
///     read by NOTHING" while both have complete production reader chains — `pipeline/fresh.rs` via
///     `router_mounts()` and `analyze/assemble/rules.rs` via `schema_usage_findings`. A whole
///     external-review question ("why does this repo ship two dead config keys?") rested on it.
///
/// (2) `catch { return [] }` TREATED TWO DIFFERENT EVENTS AS ONE. `git grep` exits 1 when nothing
///     matches (legal) and >1 when git itself fails — a bad pathspec, a missing directory, a broken
///     repo. Both arrived here as "no readers". That is this repo's named class: an instrument that
///     cannot fail loudly reports a defect it never looked for.
function readersOf(key) {
  const spellings = [...new Set([key, snake(key)])];
  const files = new Set();
  for (const needle of spellings) {
    const r = spawnSync(
      "git",
      ["grep", "-l", "--", needle, "crates", "rules", "packages", "parser"],
      { encoding: "utf8" },
    );
    // rc 0 = matches, rc 1 = none (legal). Anything else is the tool breaking, and it must not be
    // laundered into an empty result.
    if (r.status !== 0 && r.status !== 1) {
      console.error(
        `vocab-key-census: \`git grep\` failed for "${needle}" (status ${r.status}). ` +
          `This is a broken search, not a key nobody reads.`,
      );
      console.error((r.stderr || "").trim());
      process.exit(1);
    }
    for (const line of (r.stdout || "").split("\n")) {
      if (line && !IS_TEST(line) && !/tests?\.rs$|_tests?\.rs$/.test(line)) files.add(line);
    }
  }
  return [...files];
}

const rows = keys.map((k) => {
  const files = readersOf(k);
  const lanes = new Set(files.map(laneOf));
  lanes.delete("config"); // the declaration site is not a consumer
  return { key: k, lanes: [...lanes].sort(), files: files.length };
});

const multi = rows.filter((r) => r.lanes.length >= 2);
const single = rows.filter((r) => r.lanes.length === 1);
const none = rows.filter((r) => r.lanes.length === 0);

console.log(
  `vocabulary keys ${rows.length} | read from 2+ lanes ${multi.length} | from 1 lane ${single.length} | from none ${none.length}`
);

console.log("\n== read from 2+ lanes (a name that means one lane is the failure above) ==");
for (const r of multi.sort((a, b) => b.lanes.length - a.lanes.length || a.key.localeCompare(b.key))) {
  console.log(`  ${r.key.padEnd(38)} ${r.lanes.join(" + ").padEnd(36)} (${r.files} file(s))`);
}

console.log("\n== declared and read by NOTHING ==");
if (none.length === 0) console.log("  (none)");
for (const r of none) console.log(`  ${r.key}`);

console.log("\n== read from exactly one lane ==");
for (const r of single.sort((a, b) => a.lanes[0].localeCompare(b.lanes[0]) || a.key.localeCompare(b.key))) {
  console.log(`  ${r.key.padEnd(38)} ${r.lanes[0]}`);
}
