#!/usr/bin/env node
// line-census.mjs — how many lines of this workspace ship, and how many are test scaffolding.
//
// ## Why this is a script and not a number in a document (review ledger V73, V91)
//
// This count has been stated wrong three times, each time by someone reading a different population out
// of a similar-looking command:
//
//   - external review round 10: "shipped 161k / test 138k"     — tests undercounted
//   - review ledger V73 (my correction): "139-147k / 152-160k"  — a range, and still undercounted
//   - external review round 11: "shipped 117,815 / test 183,753"
//
// The disagreements were never about arithmetic. They were about what counts as a test line, and each
// answer was unfalsifiable because the command that produced it was not written down. So the number now
// lives here, next to its definition, and a document that quotes it quotes this script by name.
//
// ## The population, stated once
//
//   - Every `.rs` file under crates/, rules/, parser/, packages/. Nothing under target/, corpus/ or
//     examples/ — those are inputs and fixtures, not this workspace's source.
//   - A file is a TEST FILE, whole, when it sits under a `tests/` directory, is named `tests.rs`, or is
//     reached transitively from a `#[cfg(test)] mod <name>;` declaration. That last rule is the one every
//     earlier count missed: `mod tests;` in a shipped file makes `tests.rs` (or `tests/mod.rs`) a test
//     file, and modules IT declares are test files too, however deep.
//   - In a file that is not wholly a test file, a `#[cfg(test)]` block is removed by BRACE MATCHING, not
//     by a line heuristic, and those lines are counted as test.
//   - Blank lines and comment-only lines are reported separately, so "lines" can mean either and the
//     reader picks. The headline uses ALL lines, matching what `wc -l` would say.
//
// ## What it deliberately does not claim
//
// Nothing here says whether a line is worth having. A high test ratio is not a virtue on its own and
// this script takes no position on it — it exists so the RATIO stops being a matter of opinion.

import fs from "fs";
import path from "path";

const ROOTS = ["crates", "rules", "parser", "packages"];

function walk(dir, out) {
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const e of entries) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (e.name === "target" || e.name === "node_modules") continue;
      walk(p, out);
    } else if (e.name.endsWith(".rs")) {
      out.push(p.split(path.sep).join("/"));
    }
  }
  return out;
}

/** Every `#[cfg(test)] mod <name>;` in `src`, as declared module names. */
function cfgTestMods(src) {
  const out = [];
  const re = /#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/g;
  let m;
  while ((m = re.exec(src)) !== null) out.push(m[1]);
  return out;
}

/** Resolve a `mod name;` declared in `file` to the path it names, or null. */
function modPath(file, name) {
  const dir = path.posix.dirname(file);
  const stem = path.posix.basename(file, ".rs");
  // A file `a.rs` owns the directory `a/`; `mod.rs` and `lib.rs`/`main.rs` own their own directory.
  const owned = stem === "mod" || stem === "lib" || stem === "main" ? dir : `${dir}/${stem}`;
  for (const cand of [`${owned}/${name}.rs`, `${owned}/${name}/mod.rs`]) {
    if (fs.existsSync(cand)) return cand;
  }
  return null;
}

/** Every `mod <name>;` (test-gated or not) in `src`. */
function allMods(src) {
  const out = [];
  const re = /(?:^|\n)\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/g;
  let m;
  while ((m = re.exec(src)) !== null) out.push(m[1]);
  return out;
}

const files = ROOTS.flatMap((r) => walk(r, []));

// A floor, because this counter used to answer an empty read with a COMPLETE report: run from any
// other directory and it printed "0 .rs files ... shipped 0 lines (NaN%) ... test / shipped = NaNx"
// and exited 0 (review ledger V143). `walk()` swallows `readdirSync` errors by design -- a missing
// optional root is not an error -- and that turned "I read nothing" into a publishable number, in the
// one script three documents name as the OWNER of this repository's line counts.
//
// `check-max-file-lines.sh` has refused an empty population since it was written; this is the same
// idea, arriving late. A census with no subjects has no answer, not the answer zero.
if (files.length === 0) {
  console.error(`line-census: found 0 .rs files under ${ROOTS.join("/ ")}/ -- refusing to report.`);
  console.error("Run it from the repository root; an empty population is not a measurement of zero.");
  process.exit(1);
}
const source = new Map(files.map((f) => [f, fs.readFileSync(f, "utf8")]));

// --- classify whole test FILES, transitively -------------------------------------------------
const testFiles = new Set();
for (const f of files) {
  const base = path.posix.basename(f);
  if (base === "tests.rs" || f.includes("/tests/")) testFiles.add(f);
}
// seed from `#[cfg(test)] mod x;`, then close over the modules those files declare
const queue = [];
for (const [f, src] of source) {
  for (const name of cfgTestMods(src)) {
    const p = modPath(f, name);
    if (p) queue.push(p);
  }
}
while (queue.length) {
  const f = queue.pop();
  if (testFiles.has(f) || !source.has(f)) {
    if (!source.has(f)) continue;
  }
  if (testFiles.has(f)) continue;
  testFiles.add(f);
  for (const name of allMods(source.get(f))) {
    const p = modPath(f, name);
    if (p && source.has(p) && !testFiles.has(p)) queue.push(p);
  }
}

// --- split the remaining files on brace-matched `#[cfg(test)]` blocks ------------------------
function splitCfgTest(src) {
  const lines = src.split(/\r?\n/);
  const isTest = new Array(lines.length).fill(false);
  for (let i = 0; i < lines.length; i++) {
    if (!/^\s*#\[cfg\(test\)\]/.test(lines[i])) continue;
    // find the opening brace, then match to its close
    let j = i;
    while (j < lines.length && !lines[j].includes("{")) {
      if (lines[j].trim().endsWith(";") && j > i) break; // `#[cfg(test)] mod x;` — no block here
      j++;
    }
    if (j >= lines.length || !lines[j].includes("{")) {
      continue;
    }
    let depth = 0;
    let k = j;
    let started = false;
    for (; k < lines.length; k++) {
      for (const ch of lines[k]) {
        if (ch === "{") {
          depth++;
          started = true;
        } else if (ch === "}") depth--;
      }
      if (started && depth <= 0) break;
    }
    for (let n = i; n <= Math.min(k, lines.length - 1); n++) isTest[n] = true;
    i = k;
  }
  return { lines, isTest };
}

const tally = {
  shipped: { all: 0, code: 0 },
  test: { all: 0, code: 0 },
  files: { shipped: 0, test: 0, mixed: 0 },
};
const isCode = (l) => {
  const t = l.trim();
  return t !== "" && !t.startsWith("//");
};

for (const [f, src] of source) {
  if (testFiles.has(f)) {
    const lines = src.split(/\r?\n/);
    tally.test.all += lines.length;
    tally.test.code += lines.filter(isCode).length;
    tally.files.test++;
    continue;
  }
  const { lines, isTest } = splitCfgTest(src);
  let any = false;
  for (let i = 0; i < lines.length; i++) {
    const bucket = isTest[i] ? tally.test : tally.shipped;
    if (isTest[i]) any = true;
    bucket.all++;
    if (isCode(lines[i])) bucket.code++;
  }
  if (any) tally.files.mixed++;
  else tally.files.shipped++;
}

const total = tally.shipped.all + tally.test.all;
const pct = (n) => ((n / total) * 100).toFixed(1);
console.log(`line-census — ${files.length} .rs files under ${ROOTS.join("/ ")}/`);
console.log(`  ${tally.files.shipped} shipped-only · ${tally.files.mixed} with an inline #[cfg(test)] block · ${tally.files.test} whole test files`);
console.log("");
console.log(`  shipped   ${String(tally.shipped.all).padStart(7)} lines (${pct(tally.shipped.all)}%)   ${String(tally.shipped.code).padStart(7)} non-blank non-comment`);
console.log(`  test      ${String(tally.test.all).padStart(7)} lines (${pct(tally.test.all)}%)   ${String(tally.test.code).padStart(7)} non-blank non-comment`);
console.log(`  total     ${String(total).padStart(7)} lines`);
console.log("");
console.log(`  test / shipped = ${(tally.test.all / tally.shipped.all).toFixed(2)}x  (code-only: ${(tally.test.code / tally.shipped.code).toFixed(2)}x)`);

// --- sensitivity: how much does the ANSWER depend on the RULE? -------------------------------
//
// This block is the point of the whole script. Three counts of this workspace disagreed and it was
// never arithmetic — it was the classifier. Printing the spread makes that unarguable: the TOTAL is a
// fact about the repository, the SPLIT is a fact about the definition you picked.
const alt = {
  "only a tests/ dir or a tests.rs file": (f) => f.includes("/tests/") || f.endsWith("/tests.rs"),
  "any path with 'test' in a segment": (f) => f.split("/").some((seg) => seg.includes("test")),
  "any file that mentions #[test]": (f) => source.get(f).includes("#[test]"),
  "any file that mentions #[test] or #[cfg(test)]": (f) =>
    source.get(f).includes("#[test]") || source.get(f).includes("#[cfg(test)]"),
};
const lineCount = (f) => source.get(f).split(/\r?\n/).length;
console.log("");
console.log("  the same repository under other classifiers (the total never moves):");
for (const [name, fn] of Object.entries(alt)) {
  let t = 0;
  for (const f of files) if (fn(f)) t += lineCount(f);
  console.log(
    "    shipped " + String(total - t).padStart(7) + "   test " + String(t).padStart(7) + "   " + name
  );
}
console.log("");
console.log("  ^ the split swings by ~116,000 lines with not one line of code changing. Quote this");
console.log("    script, never a bare pair of numbers — that is the whole reason it exists.");
