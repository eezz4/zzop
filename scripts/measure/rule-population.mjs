#!/usr/bin/env node
/**
 * What does ONE DSL rule's population actually reach, and what does its file-level veto buy?
 *
 * ## Why this exists (2026-09-24, review ledger V323, and V314 before it asked the same thing)
 *
 * Two audit rounds in a row produced a finding of the same shape: a rule excludes some files BY NAME,
 * the same `file_pattern` admits more files of that kind, and the hand list is therefore "the range"
 * by accident. Both times the prescription was to widen the list, and both times the honest answer
 * needed three numbers nobody had: how many files each name contributes, how many matches the veto
 * actually removes, and what ELSE the widened shape would reach.
 *
 * On V323 those numbers inverted the prescription. `package-lock.json` was already vetoed and
 * contributed zero matches; the two names the reviewer wanted added contributed zero as well; and the
 * decisive boundary turned out not to be the veto list at all but the EXTENSION list, which silently
 * kept `.lock` files out of range entirely — including one file with 48 matching lines.
 *
 * ## What it prints, and what each column means
 *
 *   files     how many paths with this basename the walk saw
 *   in-pop    the `file_pattern` admits it (before any veto)
 *   vetoed    `file_exclude_pattern` then removes it
 *   matches   lines the rule's own `any`/`line_pattern` hits and `exclude_pattern` does NOT clear
 *
 * A row that is `in-pop` and not `vetoed` with `matches > 0` is a live finding source. A row that is
 * `vetoed` with `matches 0` is a veto paying nothing. A row that is neither is out of range for a
 * reason that has nothing to do with the veto, and that is the column the two audits kept missing.
 *
 * ## What it does NOT do
 *
 * It re-implements the matcher in JavaScript and therefore is NOT the engine. The divergences, stated
 * rather than found later — and this list was THREE until external review round 29 measured it and
 * found four more (review ledger V336), which is itself the point: a "known divergences" list is a
 * claim like any other and goes stale the moment the script grows:
 *   - `skip_comment_lines` is not applied, so a commented-out assignment counts here and not in a run.
 *   - Rust's `(?-i:...)` has no JavaScript equivalent. An `exclude_pattern` containing it is SPLIT at
 *     that arm and the arm is tested case-sensitively on its own; the guard below refuses any other
 *     inline-flag construct rather than silently mis-compiling it.
 *   - method-scan and symbol-scan rules have no single line pattern, so they are refused outright.
 *   - ONLY `exclude_pattern` is applied of the line-level vetoes. `prev_line_exclude_pattern`,
 *     `next_line_exclude_pattern`, `enclosing_call_exclude_pattern`, `call_window_exclude_pattern`,
 *     `trigger_call_exclude_pattern` and `line_exclude_pattern` need a window this script does not
 *     build, so `matches` is an OVER-count for any rule that carries one. That includes
 *     `security/config-file-secret`, the rule this script was written for — its `"examples"` veto is
 *     an `enclosing_call_exclude_pattern`, so the numbers it produced for V323 are upper bounds.
 *   - The walk's skip set is now taken from the engine's own `DEFAULT_SKIP_DIRS` rather than guessed.
 *     It used to skip `vendor`, which the engine does NOT skip, and to walk `.yarn`, which it does —
 *     wrong in both directions at once, and the `vendor` half mattered: V314 is a question about
 *     vendored paths.
 *   - A file that cannot be READ now fails the run. It used to count as `matches: 0`, which is the
 *     same picture as "in range and clean" — the silent-empty shape this repo names as A8-44.
 *   - `file_pattern` is tested against the path AS WALKED (`corpus/<tree>/...`), not against a
 *     tree-relative path. No shipped rule anchors a bare `^` today, so this changes no answer; it
 *     would the moment one did.
 * Use it to size a POPULATION question. Use the detection gate to decide whether a rule is right.
 *
 * Usage:  node scripts/measure/rule-population.mjs <pack>/<rule> [name-filter] [root]
 *   name-filter  case-insensitive substring of the BASENAME (default: every file)
 *   root         directory to walk (default: corpus)
 * Exit 2 when the rule cannot be read or the walk found nothing — an empty census is a broken
 * measurement, never a clean answer.
 */
import fs from "node:fs";
import path from "node:path";

const [ruleArg, nameFilter = "", root = "corpus"] = process.argv.slice(2);
if (!ruleArg || !ruleArg.includes("/")) {
  console.error("usage: node scripts/measure/rule-population.mjs <pack>/<rule> [name-filter] [root]");
  process.exit(2);
}

const [pack, ruleId] = ruleArg.split("/");
const packFile = `rules/dsl/${pack}/${pack}.json`;
if (!fs.existsSync(packFile)) {
  console.error(`rule-population: no pack at ${packFile}`);
  process.exit(2);
}
const rule = (JSON.parse(fs.readFileSync(packFile, "utf8")).rules ?? []).find((r) => r.id === ruleId);
if (!rule) {
  console.error(`rule-population: ${packFile} has no rule ${ruleId}`);
  process.exit(2);
}
const m = rule.matcher ?? {};
if (m.type !== "line-scan") {
  console.error(`rule-population: ${ruleArg} is a ${m.type}, and only line-scan has one line pattern.`);
  // The reason has to name the kind the caller ACTUALLY asked about. This second line was hard-coded
  // to method-scan and printed for call-scan and io-scan too, handing back a reason about a different
  // matcher -- and a reason that does not describe your rule reads as a bug in the tool, which then
  // gets worked around rather than answered.
  const WHY = {
    "method-scan": "A method-scan's verdict spans a function body",
    "call-scan": "A call-scan's verdict spans a call and the lines its arguments occupy",
    "io-scan": "An io-scan's verdict is a whole-tree join, not a line",
    "literal-scan": "A literal-scan's verdict is a string's contents plus its surroundings",
  };
  const why = WHY[m.type] ?? `A ${m.type}'s verdict is not one line`;
  console.error(`  ${why}; sizing it this way would be a different claim.`);
  // 📏 Population, so the size of what this refuses is legible rather than discovered: of 118 DSL
  // rules, 57 are line-scan and 61 are not (method-scan 56, io-scan 2, call-scan 2, literal-scan 1),
  // measured 2026-09-30. So this refusal is the answer for slightly over half the packs, and the
  // `.githooks/commit-msg` gate that names this script also names `zzop analyze --config` as the
  // other way to produce the line -- that one works for every rule.
  console.error("  For a rule this cannot size: zzop analyze --config corpus/audit/<tree>/zzop.config.jsonc");
  process.exit(2);
}

/** Rust's inline-flag syntax that JavaScript does not have. Split at it rather than mis-compile it. */
function compile(src, label) {
  if (!src) return null;
  const body = src.replace(/^\(\?i\)/, "");
  const parts = body.split(/\(\?-i:/);
  if (parts.length === 1) return [{ re: new RegExp(body, "i"), ci: true }];
  // `a|b(?-i:[A-Z])|c` -> test the whole thing case-SENSITIVELY as one arm. Widening it any further
  // would change the answer, and guessing is worse than refusing.
  if (/\(\?[a-zA-Z-]*[:)]/.test(body.replace(/\(\?-i:/g, "").replace(/\(\?:/g, ""))) {
    console.error(`rule-population: ${label} carries an inline flag group this script cannot map.`);
    process.exit(2);
  }
  return [{ re: new RegExp(body.replace(/\(\?-i:/g, "("), ""), ci: false }];
}

const filePat = new RegExp(m.file_pattern.replace(/^\(\?i\)/, ""), "i");
const fileEx = m.file_exclude_pattern ? new RegExp(m.file_exclude_pattern.replace(/^\(\?i\)/, ""), "i") : null;
const hits = (m.any ?? [{ pattern: m.line_pattern }]).map((a) => new RegExp(a.pattern.replace(/^\(\?i\)/, ""), "i"));
const excl = compile(m.exclude_pattern, "exclude_pattern");

// The ENGINE's list, not a guess: `crates/engine/src/dispatch.rs` `DEFAULT_SKIP_DIRS`. A population
// instrument that skips what the engine walks (and walks what it skips) answers a different question
// than the one asked. `.zzop` is that file's `zzop_cache::TOOL_DIR`.
const SKIP = new Set([
  "node_modules",
  "dist",
  "build",
  ".next",
  ".git",
  "target",
  ".yarn",
  ".zzop",
  "zzop-reports",
  ".zzop-cache",
]);
const files = [];
(function walk(dir) {
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const e of entries) {
    if (e.isSymbolicLink()) continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (!SKIP.has(e.name)) walk(p);
    } else if (e.isFile()) {
      if (!nameFilter || e.name.toLowerCase().includes(nameFilter.toLowerCase())) files.push(p);
    }
  }
})(root);

if (files.length === 0) {
  console.error(`rule-population: the walk of ${root} matched no file (name filter ${JSON.stringify(nameFilter)}).`);
  console.error("  An empty census is a broken measurement, not a clean answer.");
  process.exit(2);
}

const rows = new Map();
for (const f of files) {
  const base = path.basename(f);
  const inPop = filePat.test(f);
  const vetoed = inPop && fileEx ? fileEx.test(f) : false;
  let matches = 0;
  let sample = null;
  if (inPop) {
    let txt;
    try {
      txt = fs.readFileSync(f, "utf8");
    } catch (e) {
      // NOT `txt = null`. An unreadable file counted as `matches: 0`, which on this report is the
      // same picture as "in range and clean" — a silent empty, and the one thing a census must never
      // produce. Dying is the honest answer; the caller can narrow the walk.
      console.error(`rule-population: could not read ${f}: ${e.message}`);
      console.error("  Refusing to report it as zero matches — that reads identically to clean.");
      process.exit(2);
    }
    for (const line of txt.split("\n")) {
      if (!hits.some((h) => h.test(line))) continue;
      if (excl && excl.some((e) => e.re.test(line))) continue;
      matches++;
      if (!sample) sample = line.trim().slice(0, 96);
    }
  }
  const key = `${base}|${inPop}|${vetoed}`;
  const row = rows.get(key) ?? { base, inPop, vetoed, files: 0, matches: 0, sample: null };
  row.files++;
  row.matches += matches;
  if (sample && !row.sample) row.sample = sample;
  rows.set(key, row);
}

const shown = [...rows.values()].filter((r) => r.inPop || r.matches > 0).sort((a, b) => b.matches - a.matches || a.base.localeCompare(b.base));
const outOfPop = [...rows.values()].filter((r) => !r.inPop);

console.log(`rule-population: ${ruleArg} over ${root}${nameFilter ? ` (basename contains ${JSON.stringify(nameFilter)})` : ""}`);
console.log(`  ${files.length} file(s) walked; ${shown.length} basename group(s) in population, ${outOfPop.length} out of it.`);
console.log("");
console.log(`  ${"basename".padEnd(30)} ${"files".padStart(5)} ${"match".padStart(6)}  status`);
for (const r of shown) {
  const status = r.vetoed ? "vetoed" : "IN RANGE";
  console.log(`  ${r.base.padEnd(30)} ${String(r.files).padStart(5)} ${String(r.matches).padStart(6)}  ${status}${r.sample ? "   e.g. " + r.sample : ""}`);
}
if (outOfPop.length) {
  console.log("");
  console.log(`  OUT OF POPULATION — the file_pattern never admits these, veto or no veto (${outOfPop.length} basename group(s)):`);
  const names = outOfPop.map((r) => r.base).sort();
  console.log("    " + names.slice(0, 30).join(", ") + (names.length > 30 ? `, … ${names.length - 30} more` : ""));
  console.log("  ^ This column is the one two audit rounds missed. A name absent from the veto list is not");
  console.log("    necessarily reported; the EXTENSION list decides first, and it decides silently.");
}
