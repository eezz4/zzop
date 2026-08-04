#!/usr/bin/env node
// plant-revert-selftest.mjs — damage `plant-revert.mjs` ON PURPOSE, one way at a time, and require
// every one of its refusals to fire, naming ITS OWN defect. Preflight for `self-analysis-gate.sh`.
//
// ## Why this file has to exist
// `plant-revert.mjs` is the thing that makes "I verified this guard by breaking something" a true
// sentence. Its entire value is that it refuses to report success when the mutation never landed, when
// the mutated file stopped parsing, or when the revert did not put the file back. A refusal nobody has
// seen fire is not known to fire — and here that is worse than usual, because every one of these
// branches fails in the SAME direction when it rots: quietly, into a green result. The four scratchpad
// versions this module replaces all "worked" right up until the day one of them silently did nothing.
//
// So: a known-good case first (an anchor that is not green makes every refusal below fire for a reason
// unrelated to its damage), then one case per refusal, each pinned by needles that name that specific
// defect. A case that refuses for the WRONG reason is a failure here, exactly as in
// `harness-selftest.sh`, whose shape this follows.
//
// ## The fixtures are CRLF, because that is the trap
// This repo's working tree is CRLF and the measured accident is a mutation written with LF endings
// against it: nothing matches, nothing changes, the guard runs against an unmodified file and passes.
// Two cases below are that accident in its two spellings (an anchor carrying `\n`, and a `line`
// carrying one), and one more proves the happy path INSERTS CRLF into a CRLF file and LF into an LF
// one — line endings read from the file, never assumed.
//
// ## The abort case is the only one that needs a second process
// `process.abort()` skips `finally` AND every signal handler, so it is the one failure mode the module
// cannot cover from inside itself; the sidecar pair plus `plant-revert-recover.sh` is what covers it.
// This proves that pair end to end: a child plants, aborts mid-probe, and the parent verifies the
// target really is left MUTATED (a recovery that had nothing to recover would prove nothing), then runs
// the recovery script and requires byte identity.
//
//   node scripts/measure/plant-revert-selftest.mjs
//
// It touches nothing outside a temp directory: every fixture is created under `mkdtemp`, and the only
// tracked file it reads is the recovery script it invokes.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import crypto from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import { PlantError, assertRestored, plant, revert, withPlanted } from "./plant-revert.mjs";

const RECOVER_SH = fileURLToPath(new URL("./plant-revert-recover.sh", import.meta.url));
const SELF = fileURLToPath(import.meta.url);
const sha256 = (buf) => crypto.createHash("sha256").update(buf).digest("hex");

// --- fixtures -------------------------------------------------------------------------------------
const FIXTURE_LINES = [
  "// selftest fixture — small, and shaped like the files this module really plants in.",
  "export function loadThing(label) {",
  "  return { label };",
  "}",
  "",
  "export function otherThing() {",
  "  return 1;",
  "}",
  "",
];
const ANCHOR = "export function loadThing(label) {";
const MARKER = "_selftest_planted_marker";
const LINE = `  const ${MARKER} = 1;`;

function writeFixture(file, eol) {
  fs.writeFileSync(file, FIXTURE_LINES.join(eol));
}

/** A fresh directory per case: a case that leaves damage must not be able to reach the next one. */
function newCaseDir(root, name) {
  const dir = path.join(root, name);
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

function baseSpec(target, over = {}) {
  return { target, anchors: [ANCHOR], line: LINE, marker: MARKER, ...over };
}

// --- the child half of the abort case ---------------------------------------------------------
// Runs before anything else, because it IS a whole process: `node plant-revert-selftest.mjs
// --child-abort <dir> <backupPrefix>` plants two targets and aborts on the first one, leaving the
// damage the parent then requires the recovery script to undo.
const argv = process.argv.slice(2);
if (argv[0] === "--child-abort") {
  const dir = argv[1];
  const backup = argv[2];
  withPlanted([baseSpec(path.join(dir, "a.js")), baseSpec(path.join(dir, "b.js"))], () => {}, {
    backup,
    label: "child",
    afterPlant: (_handle, i) => {
      // Aborting on the FIRST target still exercises the whole sidecar loop: `withPlanted` reserves
      // every target — and therefore writes every sidecar pair — before it plants any of them.
      if (i === 0) process.abort();
    },
  });
  // Unreachable. If it is ever reached, the parent's "was the target left mutated" check fails and says
  // so, which is the correct verdict: the drill stopped drilling.
  process.exit(0);
}

// --- the runner -----------------------------------------------------------------------------------
const root = fs.mkdtempSync(path.join(os.tmpdir(), "plant-revert-selftest-"));
let failed = 0;
let checked = 0;

function report(name, problems) {
  checked += 1;
  if (problems.length === 0) {
    console.log(`  ok    ${name}`);
    return;
  }
  failed += 1;
  console.error(`  FAIL  ${name}`);
  for (const p of problems) console.error(`          ${p}`);
}

/**
 * The shape every refusal case shares: run `body`, require it to throw a `PlantError` at `stage` whose
 * lines contain every needle, and require the target to be byte-identical afterwards (a refusal that
 * leaves the file mutated has refused nothing).
 */
function refuses(name, { file, stage, needles, body, expectUntouched = true }) {
  const problems = [];
  const before = fs.readFileSync(file);
  let err = null;
  try {
    body();
  } catch (e) {
    err = e;
  }

  if (!err) {
    problems.push("it did NOT refuse. This is the manufactured-green case: a verification that did");
    problems.push("nothing would have been reported as done.");
  } else if (!(err instanceof PlantError)) {
    problems.push(`threw ${err?.name ?? typeof err}, not a PlantError: ${err?.message ?? err}`);
  } else {
    if (err.stage !== stage) {
      problems.push(`refused at stage "${err.stage}", expected "${stage}" — a refusal for the wrong`);
      problems.push("reason lets one over-eager branch stand in for a dead one.");
    }
    const text = (err.lines ?? []).join("\n");
    for (const needle of needles) {
      if (!text.includes(needle)) problems.push(`the refusal did not name the defect: expected ${JSON.stringify(needle)}`);
    }
    if ((err.lines ?? []).length === 0) problems.push("the refusal carried no diagnosis lines at all");
  }

  if (expectUntouched && !fs.readFileSync(file).equals(before)) {
    problems.push(`${file} was left MUTATED after the refusal — a refusal that does not put the file back`);
    problems.push("is the second failure mode this module exists to close.");
  }
  report(name, problems);
}

const quiet = () => {};
const opts = (backup) => ({ backup, label: "selftest", log: quiet });

console.log("plant-revert-selftest: proving plant-revert.mjs refuses every way it is supposed to");

// --- 0. the known-good anchor -------------------------------------------------------------------
// It runs FIRST and must be entirely green. A fixture or an API that is already broken would make
// every refusal below "fire" for a reason that has nothing to do with its damage.
{
  const dir = newCaseDir(root, "happy");
  const crlfFile = path.join(dir, "crlf.js");
  const lfFile = path.join(dir, "lf.js");
  writeFixture(crlfFile, "\r\n");
  writeFixture(lfFile, "\n");
  const crlfSha = sha256(fs.readFileSync(crlfFile));
  const lfSha = sha256(fs.readFileSync(lfFile));
  const backup = path.join(dir, "sidecar");

  const problems = [];
  let result;
  try {
    result = withPlanted([baseSpec(crlfFile), baseSpec(lfFile)], (handles) => {
      const crlfText = fs.readFileSync(crlfFile, "utf8");
      const lfText = fs.readFileSync(lfFile, "utf8");
      if (!crlfText.includes(MARKER)) problems.push("the marker is not in the planted CRLF file");
      if (!lfText.includes(MARKER)) problems.push("the marker is not in the planted LF file");
      // The whole point of reading the terminator out of the file: a CRLF file must stay CRLF.
      if (/[^\r]\n/.test(crlfText)) {
        problems.push("planting introduced a BARE LF into a CRLF file — the terminator was assumed, not read");
      }
      if (lfText.includes("\r")) problems.push("planting introduced a CR into an LF file");
      // The sidecars must exist WHILE the targets are mutated: that window is the only time the
      // shell's last-resort recovery has anything to recover from.
      for (const h of handles) {
        if (!fs.existsSync(h.slot) || !fs.existsSync(`${h.slot}.path`)) {
          problems.push(`no sidecar pair on disk for ${h.target} while it was planted`);
        }
      }
      return "body-result";
    }, opts(backup));
  } catch (e) {
    problems.push(`the happy path THREW: ${e?.message ?? e}`);
  }

  if (result !== "body-result") problems.push(`withPlanted did not return the body's value (got ${JSON.stringify(result)})`);
  if (sha256(fs.readFileSync(crlfFile)) !== crlfSha) problems.push("the CRLF fixture did not come back byte-identical");
  if (sha256(fs.readFileSync(lfFile)) !== lfSha) problems.push("the LF fixture did not come back byte-identical");
  const leftovers = fs.readdirSync(dir).filter((f) => f.startsWith("sidecar"));
  if (leftovers.length) problems.push(`sidecars left behind after a proven revert: ${leftovers.join(", ")}`);
  report("happy", problems);
}

// --- 1. the CRLF trap, spelled as an anchor ------------------------------------------------------
{
  const dir = newCaseDir(root, "anchor-carries-lf");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  refuses("anchor-carries-lf", {
    file,
    stage: "locate",
    needles: ["NOT FOUND", "contains a LINE ENDING", "CRLF", "silent no-op"],
    body: () => withPlanted([baseSpec(file, { anchors: [`${ANCHOR}\n`] })], () => {}, opts(path.join(dir, "s"))),
  });
}

// --- 2. the CRLF trap, spelled as the payload ----------------------------------------------------
{
  const dir = newCaseDir(root, "line-carries-lf");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  refuses("line-carries-lf", {
    file,
    stage: "spec",
    needles: ["`line` contains a LINE ENDING", "CRLF"],
    body: () => withPlanted([baseSpec(file, { line: `${LINE}\n  const second = 2;` })], () => {}, opts(path.join(dir, "s"))),
  });
}

// --- 3. the mutation that changes nothing --------------------------------------------------------
// The one that matters most: a `mutate` that returns its input is exactly what an LF-based
// `String.replace` against a CRLF file does. If this path ever returns success, every guard verified
// through this module is verified against an unmodified file.
{
  const dir = newCaseDir(root, "noop-mutation");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  refuses("noop-mutation", {
    file,
    stage: "land",
    needles: ["did not land", "bytes changed:   false", "marker present:  false"],
    body: () =>
      withPlanted([{ target: file, marker: MARKER, mutate: (text) => text }], () => {
        throw new Error("the body must never run for a mutation that did not land");
      }, opts(path.join(dir, "s"))),
  });
}

// --- 4. the mutation that changes bytes but plants something else --------------------------------
{
  const dir = newCaseDir(root, "marker-not-planted");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  refuses("marker-not-planted", {
    file,
    stage: "land",
    needles: ["did not land", "bytes changed:   true", "marker present:  false"],
    body: () =>
      withPlanted([{ target: file, marker: MARKER, mutate: (text) => `${text}// something else\r\n` }], () => {}, opts(path.join(dir, "s"))),
  });
}

// --- 5. a marker the file already carries --------------------------------------------------------
// Refused up front, because "the marker is present afterwards" would otherwise be satisfied by a
// mutation that did nothing at all — a landing assertion that cannot fail.
{
  const dir = newCaseDir(root, "marker-already-present");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  refuses("marker-already-present", {
    file,
    stage: "spec",
    needles: ["ALREADY occurs", "would therefore prove nothing"],
    body: () => withPlanted([baseSpec(file, { marker: "loadThing", line: "  const loadThing2 = 1;" })], () => {}, opts(path.join(dir, "s"))),
  });
}

// --- 6. an anchor that no longer occurs ----------------------------------------------------------
{
  const dir = newCaseDir(root, "anchor-gone");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  refuses("anchor-gone", {
    file,
    stage: "locate",
    needles: ["Could not locate the insertion point", "NOT FOUND", "the target is untouched"],
    body: () => withPlanted([baseSpec(file, { anchors: ["export function renamedSinceThisWasWritten("] })], () => {}, opts(path.join(dir, "s"))),
  });
}

// --- 7. a target that is unfit BEFORE the mutation ------------------------------------------------
{
  const dir = newCaseDir(root, "baseline-refuses");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  refuses("baseline-refuses", {
    file,
    stage: "baseline",
    needles: ["BEFORE anything was planted", "The file is untouched.", "verdict: parse_error"],
    body: () =>
      withPlanted(
        [baseSpec(file, { baseline: () => ({ ok: false, detail: ["  verdict: parse_error (expected analyzed)"] }) })],
        () => {},
        opts(path.join(dir, "s"))
      ),
  });
}

// --- 8. a mutation that broke the file it was planted in -------------------------------------------
// The axis none of the scratchpad versions had. A file that stopped parsing produces no findings, which
// is byte-for-byte the same answer as a clean file, so the guard's silence would be misread as proof.
{
  const dir = newCaseDir(root, "recheck-refuses");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  refuses("recheck-refuses", {
    file,
    stage: "recheck",
    needles: ["BROKE", "symbols: 0 (expected 2)"],
    body: () =>
      withPlanted(
        [
          baseSpec(file, {
            baseline: () => ({ ok: true, state: 2 }),
            recheck: (state) => ({ ok: false, detail: [`  symbols: 0 (expected ${state})`] }),
          }),
        ],
        () => {
          throw new Error("the body must never run once the planted file stopped being valid");
        },
        opts(path.join(dir, "s"))
      ),
  });
}

// --- 9. the body throws — the reason `withPlanted` exists -------------------------------------------
// The caller's own `finally` is what four earlier versions of this procedure got wrong. Here the body
// throws from the middle of the guard run and the targets still come back.
{
  const dir = newCaseDir(root, "body-throws");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  const problems = [];
  const before = fs.readFileSync(file);
  let err = null;
  try {
    withPlanted([baseSpec(file)], () => {
      throw new Error("the guard blew up mid-run");
    }, opts(path.join(dir, "s")));
  } catch (e) {
    err = e;
  }
  if (!err || err.message !== "the guard blew up mid-run") {
    problems.push(`the body's own error was not rethrown (got ${err?.message ?? "nothing"})`);
  }
  if (!fs.readFileSync(file).equals(before)) {
    problems.push("the target was left MUTATED after the body threw — the whole reason this wrapper exists");
  }
  if (fs.readdirSync(dir).some((f) => f.startsWith("s."))) problems.push("sidecars left behind after a proven revert");
  report("body-throws", problems);
}

// --- 10. an async body --------------------------------------------------------------------------
// Refused rather than awaited: the reverts run synchronously, so an awaited body would be reading the
// mutated file for an unbounded window after this wrapper thought it was done.
{
  const dir = newCaseDir(root, "async-body");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  refuses("async-body", {
    file,
    stage: "async-body",
    needles: ["returned a Promise", "synchronous by design"],
    body: () => withPlanted([baseSpec(file)], async () => 1, opts(path.join(dir, "s"))),
  });
}

// --- 11. the revert that did not restore ---------------------------------------------------------
// Fired against a REAL handle from a real plant: the target is overwritten behind the module's back —
// which is what a concurrent writer, a half-flushed editor or a crashed revert leaves — and the proof
// is required to say so, loudly, with both hashes and the sidecar's location.
{
  const dir = newCaseDir(root, "revert-not-proven");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  const backup = path.join(dir, "s");
  const problems = [];
  const handle = plant(baseSpec(file), opts(backup));
  fs.writeFileSync(file, "something that is not the pre-plant bytes\r\n");
  let err = null;
  try {
    assertRestored(handle, opts(backup));
  } catch (e) {
    err = e;
  }
  if (!(err instanceof PlantError) || err.stage !== "revert") {
    problems.push(`a target that is NOT back was accepted as restored (got ${err?.stage ?? "no error"})`);
  } else {
    const text = err.lines.join("\n");
    for (const needle of ["FAILED TO REVERT", "before: ", "after:  ", "The pre-plant bytes are at"]) {
      if (!text.includes(needle)) problems.push(`the failure did not name ${JSON.stringify(needle)}`);
    }
  }
  // The sidecar must still be there: it is the only surviving copy, and a proof that deletes its own
  // evidence on the way out is worse than no proof.
  if (!fs.existsSync(handle.slot)) problems.push("the sidecar was deleted even though the revert was NOT proven");
  fs.writeFileSync(file, handle.orig);
  revert(handle, opts(backup));
  report("revert-not-proven", problems);
}

// --- 12. the same, with no sidecar to point at ----------------------------------------------------
{
  const dir = newCaseDir(root, "revert-not-proven-no-sidecar");
  const file = path.join(dir, "crlf.js");
  writeFixture(file, "\r\n");
  const problems = [];
  const handle = plant(baseSpec(file), { label: "selftest", log: quiet });
  fs.writeFileSync(file, "tampered\r\n");
  let err = null;
  try {
    assertRestored(handle);
  } catch (e) {
    err = e;
  }
  if (!(err instanceof PlantError) || err.stage !== "revert") {
    problems.push(`a target that is NOT back was accepted as restored (got ${err?.stage ?? "no error"})`);
  } else if (!err.lines.join("\n").includes("No sidecar was kept")) {
    problems.push("the failure did not say that no sidecar exists — the reader needs to know the bytes are in memory only");
  }
  fs.writeFileSync(file, handle.orig);
  revert(handle, { label: "selftest", log: quiet });
  report("revert-not-proven-no-sidecar", problems);
}

// --- 13. the process that dies where no handler can reach ------------------------------------------
{
  const dir = newCaseDir(root, "abort-recovers");
  const a = path.join(dir, "a.js");
  const b = path.join(dir, "b.js");
  writeFixture(a, "\r\n");
  writeFixture(b, "\r\n");
  const shaA = sha256(fs.readFileSync(a));
  const shaB = sha256(fs.readFileSync(b));
  // Forward slashes on purpose: this prefix is handed to a bash script, whose glob a backslash path
  // would silently defeat. `plant-revert-recover.sh` refuses one outright (case 14 below); this is the
  // spelling every caller of it must use, and the real gate gets it for free from `mktemp -d`.
  const backup = path.join(dir, "sidecar").replace(/\\/g, "/");
  const problems = [];

  let childRc = 0;
  try {
    execFileSync(process.execPath, [SELF, "--child-abort", dir, backup], { stdio: "ignore" });
  } catch (e) {
    childRc = e.status ?? -1;
  }
  if (childRc === 0) problems.push("the child exited 0 — `process.abort()` did not abort, so this drill drilled nothing");

  // The damage has to be REAL before the recovery means anything: a child that tidied up after itself
  // would let a recovery script that does nothing pass this case.
  if (sha256(fs.readFileSync(a)) === shaA) {
    problems.push("the aborted child left a.js UNMUTATED, so the recovery below would have nothing to prove");
  }
  const sidecars = fs.readdirSync(dir).filter((f) => f.startsWith("sidecar"));
  if (sidecars.length !== 4) {
    problems.push(`expected 4 sidecar files (bytes+path for 2 reserved targets), found ${sidecars.length}: ${sidecars.join(", ")}`);
  }

  try {
    execFileSync("bash", [RECOVER_SH, backup], { stdio: "pipe" });
  } catch (e) {
    problems.push(`plant-revert-recover.sh exited ${e.status}: ${String(e.stderr ?? "")}`);
  }
  if (sha256(fs.readFileSync(a)) !== shaA) problems.push("a.js was NOT recovered from its sidecar");
  if (sha256(fs.readFileSync(b)) !== shaB) problems.push("b.js was disturbed by the recovery");
  const after = fs.readdirSync(dir).filter((f) => f.startsWith("sidecar"));
  if (after.length !== 0) problems.push(`the recovery left sidecars behind: ${after.join(", ")}`);
  report("abort-recovers", problems);
}

// --- 14. the recovery script's own silent-no-op trap -----------------------------------------------
// The last-resort recovery has no second line of defence: if its glob matches nothing it exits 0
// having restored nothing, and the file stays mutated on a real working tree. A backslash prefix does
// exactly that, so it is refused rather than attempted.
{
  const problems = [];
  let rc = 0;
  let out = "";
  try {
    execFileSync("bash", [RECOVER_SH, "C:\\Users\\nobody\\sidecar"], { stdio: "pipe" });
  } catch (e) {
    rc = e.status ?? -1;
    out = String(e.stderr ?? "");
  }
  if (rc === 0) problems.push("a backslash prefix was ACCEPTED — it would have recovered nothing and said so with exit 0");
  if (!out.includes("contains a BACKSLASH")) problems.push("the refusal did not name the defect");
  report("recover-refuses-backslash-prefix", problems);
}

fs.rmSync(root, { recursive: true, force: true });

if (failed !== 0) {
  console.error("");
  console.error("plant-revert-selftest: FAILED. A refusal in plant-revert.mjs no longer fires.");
  console.error("  Every 'I verified this guard by breaking something' claim in this repo runs through");
  console.error("  that module. The failure mode it exists to stop is a verification that quietly did");
  console.error("  nothing — which reads as a pass, not as an error.");
  console.error("  Fix the branch, do not relax this script.");
  process.exit(1);
}
console.log(`plant-revert-selftest: OK (${checked}/${checked} — the happy path stays byte-identical and every refusal fired by name)`);
