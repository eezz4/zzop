#!/usr/bin/env node
// selftest-benchmark-cases.mjs — build a KNOWN-GOOD scoring sandbox for `benchmark.mjs`, then damage
// it ONE WAY AT A TIME, so every abort branch in the scorer can be PROVEN to fire instead of quietly
// translating a malfunction into a number.
//
// This is `selftest-stub.rs`'s counterpart for the second half of the harness. The stub misbehaves at
// the BINARY, which is what snapshot.mjs validates; this misbehaves at the RUN DIRECTORY, the GROUND
// TRUTH and the COVERAGE FLOOR, which is all benchmark.mjs ever reads. Same rule, stated once in the
// stub and inherited here: a guard nobody has seen red is not known to work — and benchmark.mjs IS the
// detection gate's verdict, so a dead branch in it converts a broken measurement into a green CI job.
//
// NOT part of the cargo workspace and not a test runner — a fixture generator, driven by
// `scripts/measure/harness-selftest.sh`, which derives its subject list from `--list` below. A mode
// added here without an expectation row in that script fails loudly rather than going unexercised.
//
// ## Why the sandbox contains a COPY of benchmark.mjs
// The coverage floor's path is derived from the script's own location
// (`path.resolve(dirname(import.meta.url), "..", "detection-expected-baseline.txt")`) and no flag
// moves it. Damaging the floor in place would mean damaging a tracked file that the real gate reads,
// mid-run, on a developer's working tree — the exact class of accident this harness exists to stop. So
// the sandbox reproduces the script's DIRECTORY SHAPE (`<sandbox>/measure/benchmark.mjs` beside
// `<sandbox>/detection-expected-baseline.txt`) and copies the current bytes into it at run time. The
// subject is still the file in the tree; only its neighbourhood is disposable.
//
// ## The known-good fixture
// Two trees' worth of shape in one tree: one per-tree finding (axis 1) and one cross-layer finding
// (axis 2, attributed through `data.source`), against a ground truth that labels both plus one benign
// control, against a floor that records exactly that. It must score `TP 2  FN 0  FP 0`, exit 0. The
// runner asserts that BEFORE any damage: a sandbox that is already red would make every damage below
// "fire" for a reason that has nothing to do with the damage.
//
// usage:
//   node scripts/measure/selftest-benchmark-cases.mjs --list
//   node scripts/measure/selftest-benchmark-cases.mjs --mode <mode> --out <dir>
//
// `--mode` materializes the sandbox in <dir> and prints, one per line, the argv the runner must hand
// to <dir>/measure/benchmark.mjs.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REAL_BENCHMARK = path.join(HERE, "benchmark.mjs");

// ---- the known-good fixture ----------------------------------------------------------------------
const GOOD_META = {
  label: "selftest-good",
  axes: ["cross_repo", "analyze_repo"],
  binary: "/nonexistent/zzop-mcp",
  binarySha256: "0".repeat(64),
  limit: 1000,
  trees: [{ sourceId: "alpha", path: "/nonexistent/alpha", file: "tree-alpha.json", total: 1 }],
};

const GOOD_TREE = {
  fileCount: 1,
  findings: {
    total: 1,
    shown: [{ file: "src/a.ts", line: 3, ruleId: "selftest/rule-a", severity: "high" }],
  },
};

const GOOD_CROSS = {
  sources: [{ sourceId: "alpha", path: "/nonexistent/alpha", findingCount: 1 }],
  buckets: [],
  distinctBucketKeys: [],
  edges: [],
  crossLayerFindings: {
    total: 1,
    shown: [
      {
        ruleId: "cross-layer/selftest",
        file: "src/b.ts",
        line: 7,
        data: { source: "alpha" },
      },
    ],
  },
};

// The `gap` entry is deliberately part of the KNOWN-GOOD fixture rather than a damage mode. A gap that
// stays silent is the third disposition's healthy state — it must not touch TP/FN/FP and must not cost
// the run its exit 0 — and that is the one branch no damage mode can ever prove, because the runner's
// phase-2 loop requires a NONZERO exit from every mode it drives. Anchored at `alpha/src/c.ts:9`, which
// neither payload contains.
// Spelled once, because every damage mode that REWRITES the ground truth has to carry it along unless
// the gap itself is the damage. Dropping it as a side effect shrinks the floor's third column, and the
// mode then aborts for THAT instead of for the defect it exists to prove — the wrong-reason pass the
// runner's per-mode needles are there to catch. Observed 2026-07-29 while adding the column:
// `ratchet-grew` stopped exercising the GROWTH branch altogether and reported a shrink.
const GAP_ENTRY = '  "gap": { "alpha/src/c.ts:9": ["selftest/rule-c"] },\n';

const GOOD_EXPECTED =
  "{\n" +
  "  // Detection-benchmark ground truth — SELFTEST FIXTURE, not the corpus.\n" +
  '  "alpha/src/a.ts:3": ["selftest/rule-a"],\n' +
  '  "alpha/src/b.ts:7": ["cross-layer/selftest"],\n' +
  '  "benign": ["alpha/src/quiet.ts"],\n' +
  GAP_ENTRY +
  '  "untracked": []\n' +
  "}\n";

// Derived from GOOD_EXPECTED by the same rule countsOf() applies: 2 key x ruleId pairs, 1 control and
// 1 open gap, all anchored in `alpha`. Written out rather than computed so a bug in countsOf cannot
// also write the floor it is checked against.
const GOOD_BASELINE = "# selftest coverage floor\n# <sourceId> <expectations> <benign> <gap>\nalpha 2 1 1\n";

// ---- sandbox construction -------------------------------------------------------------------------
function sandbox(out) {
  const dirs = {
    root: out,
    measure: path.join(out, "measure"),
    runs: path.join(out, "runs"),
    run: path.join(out, "runs", "good"),
  };
  for (const d of [dirs.root, dirs.measure, dirs.runs, dirs.run]) fs.mkdirSync(d, { recursive: true });

  const p = {
    script: path.join(dirs.measure, "benchmark.mjs"),
    baseline: path.join(dirs.root, "detection-expected-baseline.txt"),
    expected: path.join(dirs.root, "EXPECTED.jsonc"),
    run: dirs.run,
    meta: path.join(dirs.run, "meta.json"),
    cross: path.join(dirs.run, "cross.json"),
    tree: path.join(dirs.run, "tree-alpha.json"),
  };

  fs.copyFileSync(REAL_BENCHMARK, p.script);
  fs.writeFileSync(p.baseline, GOOD_BASELINE);
  fs.writeFileSync(p.expected, GOOD_EXPECTED);
  fs.writeFileSync(p.meta, JSON.stringify(GOOD_META, null, 2));
  fs.writeFileSync(p.cross, JSON.stringify(GOOD_CROSS, null, 2));
  fs.writeFileSync(p.tree, JSON.stringify(GOOD_TREE, null, 2));
  return p;
}

const writeJson = (f, o) => fs.writeFileSync(f, JSON.stringify(o, null, 2));
const clone = (o) => JSON.parse(JSON.stringify(o));

/** Default argv: score the good run against the good ground truth. */
const score = (p) => ["--run", p.run, "--expected", p.expected];

// ---- the damages -----------------------------------------------------------------------------------
// Each entry damages the sandbox EXACTLY ONE WAY and returns the argv to run with. Ordered roughly as
// benchmark.mjs encounters them, so a reader can walk the two files side by side.
//
// Every one of these is chosen because its damaged form COULD FLOW THROUGH AS AN EMPTY OR ABSENT
// VALUE — a missing field, an unparsed file, an empty result set. Branches that die on absent argv
// (`missing --expected`, `--run needs a value`) are deliberately NOT here: they cannot be reached with
// a value in hand, they have no silent form, and covering them would raise a count without raising
// confidence. See the runner's header for the full list of what was left out and why.
const CASES = {
  // --- the run directory ---------------------------------------------------------------------------
  /** The directory exists but is not a snapshot at all. */
  "no-meta": (p) => {
    fs.rmSync(p.meta);
    return score(p);
  },
  /** meta.json present but unreadable — a truncated write, an interrupted snapshot. */
  "meta-corrupt": (p) => {
    fs.writeFileSync(p.meta, '{"label":"selftest-good","axes":["cross_repo",');
    return score(p);
  },
  /** `meta.axes` absent entirely: `meta.axes || []` is exactly the shape that flows through as empty. */
  "axes-absent": (p) => {
    const m = clone(GOOD_META);
    delete m.axes;
    writeJson(p.meta, m);
    return score(p);
  },
  /** `meta.trees` absent: the per-tree axis would contribute nothing and nothing would say so. */
  "meta-no-trees": (p) => {
    const m = clone(GOOD_META);
    delete m.trees;
    writeJson(p.meta, m);
    return score(p);
  },
  /** One axis only — every cross-layer expectation would become a false negative blamed on the engine. */
  "axes-partial": (p) => {
    const m = clone(GOOD_META);
    m.axes = ["analyze_repo"];
    writeJson(p.meta, m);
    return score(p);
  },
  /** The join half of the snapshot is gone. */
  "cross-missing": (p) => {
    fs.rmSync(p.cross);
    return score(p);
  },
  /** meta.trees names a per-tree file that is not on disk. */
  "tree-missing": (p) => {
    fs.rmSync(p.tree);
    return score(p);
  },
  /** The per-tree payload parses, but its anchor list is GONE (`a.findings.shown || []`). */
  "tree-findings-empty": (p) => {
    writeJson(p.tree, { fileCount: 1, findings: { total: 1 } });
    return score(p);
  },
  /** The join payload parses, but carries no crossLayerFindings (`(cross.x || {}).shown || []`). */
  "cross-findings-empty": (p) => {
    const c = clone(GOOD_CROSS);
    delete c.crossLayerFindings;
    writeJson(p.cross, c);
    return score(p);
  },

  // --- the ground truth ------------------------------------------------------------------------------
  /** No answer key at all. */
  "expected-missing": (p) => {
    fs.rmSync(p.expected);
    return score(p);
  },
  /** The answer key exists but does not parse — the branch with no try/catch around it. */
  "expected-corrupt": (p) => {
    fs.writeFileSync(p.expected, '{\n  // truncated mid-write\n  "alpha/src/a.ts:3": ["selftest/rule-a",\n');
    return score(p);
  },
  /** Parses to an object with no expectations in it. */
  "expected-empty": (p) => {
    fs.writeFileSync(p.expected, '{\n  "benign": ["alpha/src/quiet.ts"],\n  "untracked": []\n}\n');
    return score(p);
  },
  /** The legacy tree-relative key format, in which locations in different trees collapse onto one key. */
  "legacy-keys": (p) => {
    fs.writeFileSync(
      p.expected,
      '{\n  "src/a.ts:3": ["selftest/rule-a"],\n  "src/b.ts:7": ["cross-layer/selftest"],\n  "benign": [],\n  "untracked": []\n}\n'
    );
    // Floor re-derived for the legacy shape, so the ratchet cannot fire first and stand in for the
    // format check: under legacy keys `countsOf` reads the leading path segment as the sourceId.
    fs.writeFileSync(p.baseline, "# selftest coverage floor\nsrc 2 0 0\n");
    return score(p);
  },

  // --- the coverage floor ------------------------------------------------------------------------------
  /** The floor is not on disk — nothing stops the ground truth from shrinking. */
  "baseline-missing": (p) => {
    fs.rmSync(p.baseline);
    return score(p);
  },
  /** A row the parser cannot read. Skipping it would mean a floor of ZERO for that tree. */
  "baseline-badrow": (p) => {
    fs.writeFileSync(p.baseline, "# selftest coverage floor\nalpha 2\n");
    return score(p);
  },
  /** The laundering scenario: an expectation quietly dropped from the answer key. */
  "ratchet-shrank": (p) => {
    fs.writeFileSync(
      p.expected,
      '{\n  "alpha/src/a.ts:3": ["selftest/rule-a"],\n  "benign": ["alpha/src/quiet.ts"],\n' +
        GAP_ENTRY +
        '  "untracked": []\n}\n'
    );
    return score(p);
  },
  /**
   * Coverage added but never recorded as the new floor — it could be lost silently later. The added
   * expectation is `alpha/src/d.ts:11` and NOT the gap's own `c.ts:9`: reusing that anchor would make
   * this mode a gap PROMOTION (expectations +1, gap -1) rather than plain growth, and it would abort on
   * the shrunk gap column instead of the growth branch it exists to prove.
   */
  "ratchet-grew": (p) => {
    fs.writeFileSync(
      p.expected,
      '{\n  "alpha/src/a.ts:3": ["selftest/rule-a"],\n  "alpha/src/b.ts:7": ["cross-layer/selftest"],\n' +
        '  "alpha/src/d.ts:11": ["selftest/rule-d"],\n  "benign": ["alpha/src/quiet.ts"],\n' +
        GAP_ENTRY +
        '  "untracked": []\n}\n'
    );
    return score(p);
  },

  // --- the third disposition: `gap` ---------------------------------------------------------------------
  /**
   * A `gap` key FIRED. This is not a corruption — it is a valid score carrying good news, and it is the
   * one outcome the disposition exists to refuse to swallow: the capability landed, so the entry must be
   * re-adjudicated by hand into a real expectation. Absorbed silently, the corpus would keep reporting
   * `GAP 0/N` forever while the gap was closed, and a benchmark that cannot see a capability GAIN has
   * stopped measuring. The score still prints in full — nothing here is unreadable — and then the run
   * exits nonzero naming what to promote.
   *
   * The floor moves with the ground truth ON PURPOSE, the same way `legacy-keys` re-derives it: left at
   * `alpha 2 1 1` the ratchet would abort first and this mode would "pass" while proving nothing about
   * gap promotion — the wrong-reason pass the runner's per-mode needles exist to catch.
   */
  "gap-fired": (p) => {
    fs.writeFileSync(
      p.expected,
      '{\n  "alpha/src/a.ts:3": ["selftest/rule-a"],\n' +
        '  "benign": ["alpha/src/quiet.ts"],\n' +
        '  "gap": { "alpha/src/b.ts:7": ["cross-layer/selftest"] },\n' +
        '  "untracked": []\n}\n'
    );
    fs.writeFileSync(p.baseline, "# selftest coverage floor\nalpha 1 1 1\n");
    return score(p);
  },
  /**
   * The same fired gap, reached through `--write-expected` instead of the score. Regeneration would
   * emit the fired finding as an ordinary expectation while the carried `gap` still claims it is open —
   * the same key x ruleId in two dispositions, promoted by nobody. The score path would abort on the
   * NEXT run, but that is exactly the "checking afterwards leaves it on disk for the next command, and
   * the next command is usually `git add`" argument the ratchet header already settled for itself.
   */
  "gap-fired-write": (p) => {
    fs.writeFileSync(
      p.expected,
      '{\n  "alpha/src/a.ts:3": ["selftest/rule-a"],\n' +
        '  "benign": ["alpha/src/quiet.ts"],\n' +
        '  "gap": { "alpha/src/b.ts:7": ["cross-layer/selftest"] },\n' +
        '  "untracked": []\n}\n'
    );
    return [...score(p), "--write-expected"];
  },
  /**
   * A `gap` entry deleted from the ground truth without lowering the floor. This is the whole reason the
   * baseline grew a third column: without it a gap is the one labeled shape that costs nothing to hold
   * and nothing to delete — it is exit-zero while open, so its disappearance would be invisible in the
   * score, and the corpus would quietly stop claiming a hole it had adjudicated.
   */
  "gap-ratchet-shrank": (p) => {
    fs.writeFileSync(
      p.expected,
      '{\n  "alpha/src/a.ts:3": ["selftest/rule-a"],\n  "alpha/src/b.ts:7": ["cross-layer/selftest"],\n' +
        '  "benign": ["alpha/src/quiet.ts"],\n  "untracked": []\n}\n'
    );
    return score(p);
  },

  // --- regeneration and floor maintenance ---------------------------------------------------------------
  /**
   * `--write-expected` over an unparseable answer key. The hand-curated `benign` and `untracked` lists
   * cannot be re-derived from any run, so a regeneration that proceeds here deletes the corpus's entire
   * precision axis at the moment nobody is reading the diff.
   */
  "write-over-corrupt": (p) => {
    fs.writeFileSync(p.expected, "{ this is not json\n");
    return [...score(p), "--write-expected"];
  },
  /**
   * `--write-expected` from a run that measured nothing. An empty derivation is a broken read, never a
   * corpus with no expectations — and it is the one input that would write a floor of nothing.
   */
  "write-derives-zero": (p) => {
    // meta's recorded total moves with the emptied anchor list ON PURPOSE. Left at 1, the snapshot
    // self-consistency check aborts first and this mode would "pass" while proving nothing about the
    // ratchet — the same wrong-reason pass the runner's per-mode needles exist to catch. Observed.
    const m = clone(GOOD_META);
    m.trees[0].total = 0;
    writeJson(p.meta, m);
    writeJson(p.tree, { fileCount: 1, findings: { total: 0, shown: [] } });
    const c = clone(GOOD_CROSS);
    c.crossLayerFindings = { total: 0, shown: [] };
    writeJson(p.cross, c);
    fs.writeFileSync(p.expected, '{\n  "alpha/src/a.ts:3": ["selftest/rule-a"],\n  "benign": [],\n  "untracked": []\n}\n');
    return [...score(p), "--write-expected"];
  },
  /** `--update-baseline` must not be the command that lowers the floor. This is the anti-laundering core. */
  "update-lowers": (p) => {
    fs.writeFileSync(
      p.expected,
      '{\n  "alpha/src/a.ts:3": ["selftest/rule-a"],\n  "benign": ["alpha/src/quiet.ts"],\n' +
        GAP_ENTRY +
        '  "untracked": []\n}\n'
    );
    return ["--expected", p.expected, "--update-baseline"];
  },
};

// ---- driver ------------------------------------------------------------------------------------------
const argv = process.argv.slice(2);
function opt(name) {
  const i = argv.indexOf("--" + name);
  if (i === -1 || i + 1 >= argv.length) return null;
  return argv[i + 1];
}

if (argv.includes("--list")) {
  for (const m of Object.keys(CASES)) console.log(m);
  process.exit(0);
}

const mode = opt("mode");
const out = opt("out");
if (!mode || !out) {
  console.error("usage: selftest-benchmark-cases.mjs (--list | --mode <mode> --out <dir>)");
  process.exit(2);
}
if (mode !== "good" && !CASES[mode]) {
  console.error(`selftest-benchmark-cases: unknown mode '${mode}'`);
  process.exit(2);
}

const paths = sandbox(out);
const runArgv = mode === "good" ? score(paths) : CASES[mode](paths);
for (const a of runArgv) console.log(a);
