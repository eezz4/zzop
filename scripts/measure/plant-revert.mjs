// plant-revert.mjs — plant a mutation in a tracked file, PROVE it landed, and put the file back from
// the bytes this process read itself. The shared owner of "verify a guard by breaking something".
//
// ## Why this is a module and not a paragraph in a runbook
// §5.2 of the working agreements requires a new guard to be seen RED before it is believed. Doing that
// means mutating a real file, running the guard, and reverting — and on this repo that procedure has
// two failure modes that both END IN A GREEN RESULT, which is the worst possible shape for a mistake:
//
//   1. THE MUTATION NEVER LANDED. This repo's working tree is CRLF. A replacement written with LF line
//      endings, or anchored on a string that no longer occurs, changes NOTHING — and a guard run
//      against an unchanged file passes, so "I verified it" becomes a false sentence. Measured, more
//      than once.
//   2. THE REVERT ATE UNCOMMITTED WORK. `git checkout -- <file>` restores from the INDEX, so if the
//      file being probed also carried the batch's own edits, those edits vanish silently. Measured on
//      2026-08-01: a batch lost its work and had to re-run a full verification.
//
// Four separate batches wrote their own version of this on 2026-08-01 alone, each in a scratchpad, so
// the fifth could not read the fourth's. `self-analysis-gate.mjs` was the first version that got
// committed; this file is that logic lifted out of it so the sixth batch can `import` instead of
// rewriting. The gate now calls in here — it does not keep a copy (a copy is not an extraction).
//
// ## What it holds, in the order it enforces them
//   1. PRE-PLANT BYTES ARE OURS. The original is read into memory here, and a sidecar copy is written
//      next to `opts.backup` together with the target's path, so a shell wrapper can put the file back
//      even if this process is killed where no `finally` and no signal handler can reach.
//      `plant-revert-recover.sh` is the reader of that sidecar pair — one owner per side.
//   2. LINE ENDINGS COME FROM THE FILE. The declarative form inserts a line after an anchor line and
//      copies that line's terminator (`\r\n` or `\n`) out of the text. Callers never spell one.
//   3. THE LANDING IS ASSERTED FROM DISK. After writing, the bytes are read BACK and compared with the
//      original, and the caller's `marker` must be present in them. Not in the string we meant to
//      write — in the bytes the filesystem now holds.
//   4. THE MUTATED FILE MUST STILL BE VALID. `spec.recheck` is the caller's hook for "does this file
//      still parse", and it runs BEFORE the caller is allowed to conclude anything from the guard. A
//      file that stopped parsing yields zero findings, which is byte-for-byte the same answer as a
//      clean one. This is the axis the four scratchpad versions did not have.
//   5. THE REVERT IS PROVEN, NOT ASSUMED. sha256 of the restored file against sha256 of the bytes read
//      in step 1. The sidecar is deleted only after that comparison passes.
//
// ## Use `withPlanted`. The other two exports are its parts.
// `plant()` + `revert()` in a caller's own `try/finally` is exactly the shape that has gone wrong here
// before — the caller forgets the `finally`, or takes an early `return` out of it, and leaves a tracked
// file mutated on someone's disk. `withPlanted` owns that `finally`, owns the signal handlers, and
// reverts every target before it rethrows whatever the body threw.
//
// ## Proven by `plant-revert-selftest.mjs`
// Every refusal below is fired on purpose there, including the process-abort path (which no `finally`
// can cover, and which therefore has to be proven through the shell's sidecar recovery). A guard nobody
// has seen go red is not known to work, and this file is a guard for guards.

import fs from "node:fs";
import crypto from "node:crypto";

const sha256 = (buf) => crypto.createHash("sha256").update(buf).digest("hex");

/**
 * Every refusal in this module. `stage` names WHICH invariant refused (`spec`, `baseline`, `locate`,
 * `land`, `recheck`, `revert`, `async-body`) and `lines` carries the diagnosis, already formatted, so a
 * caller can print it under its own banner without re-deriving anything.
 */
export class PlantError extends Error {
  constructor(stage, target, lines) {
    super(`plant-revert: ${stage} refused on ${target}`);
    this.name = "PlantError";
    this.stage = stage;
    this.target = target;
    this.lines = lines;
  }
}

// Handles that currently exist, so the signal handlers can reach targets belonging to a caller that is
// several frames away. A handle joins this set when it is reserved (before anything is written) and
// leaves it when its revert has been PROVEN — never in between.
const liveHandles = new Set();
let signalsInstalled = false;
let slotSeq = 0;

function installSignalHandlers() {
  if (signalsInstalled) return;
  signalsInstalled = true;
  // `finally` does not run when the process is signalled, and a tracked file left mutated is the worst
  // thing this module could leave behind. Best-effort by construction: a handler cannot report, so it
  // restores what it can and gets out with the conventional 128+SIGINT code.
  for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.on(sig, () => {
      for (const h of [...liveHandles]) {
        try {
          if (h.planted) {
            fs.writeFileSync(h.target, h.orig);
            h.planted = false;
          }
        } catch {
          /* a handler that throws restores nothing for the handles after it */
        }
      }
      process.exit(130);
    });
  }
}

function optLabel(opts) {
  return opts.label ?? "plant-revert";
}

/**
 * Read the target, keep its bytes and their sha256, and drop a sidecar pair for the shell's last-resort
 * recovery. Nothing is written to the target here — reserving is the step that makes a later failure
 * recoverable, so it happens before any mutation, for EVERY target, including ones that never get
 * planted because an earlier target refused.
 */
function reserve(spec, opts) {
  if (!spec || typeof spec.target !== "string" || !spec.target) {
    throw new PlantError("spec", String(spec?.target ?? "(none)"), [
      "A plant spec needs a `target` path. Nothing was read and nothing was written.",
    ]);
  }
  const orig = fs.readFileSync(spec.target);
  const handle = {
    spec,
    target: spec.target,
    orig,
    sha: sha256(orig),
    planted: false,
    slot: null,
    state: undefined,
  };

  validateSpec(handle, orig.toString("utf8"));

  if (opts.backup) {
    handle.slot = `${opts.backup}.${slotSeq++}`;
    fs.writeFileSync(handle.slot, orig);
    // The PATH lives beside the bytes rather than in the calling shell, so "which files are probe
    // targets" has exactly one owner. A second spelling is a second thing to forget to update.
    fs.writeFileSync(`${handle.slot}.path`, spec.target);
  }
  installSignalHandlers();
  liveHandles.add(handle);
  return handle;
}

/**
 * Refusals that can be decided before the file is touched. Each one exists because its absence would
 * make a LATER assertion vacuous rather than because it is tidy.
 */
function validateSpec(handle, text) {
  const spec = handle.spec;
  const bad = (lines) => {
    throw new PlantError("spec", handle.target, lines);
  };

  if (typeof spec.marker !== "string" || !spec.marker) {
    bad([
      "A plant spec needs a non-empty `marker`: the substring that must be readable back OUT OF THE",
      "FILE once the mutation has been written. Without it the only landing evidence is the byte",
      "comparison, and a mutation can change bytes while planting something other than what was meant.",
    ]);
  }
  if (text.includes(spec.marker)) {
    // A marker the file already carries would be "present" after a no-op, which is the exact reading
    // this module exists to make impossible.
    bad([
      `The marker "${spec.marker}" ALREADY occurs in ${handle.target} before anything was planted.`,
      "Its presence after the write would therefore prove nothing — a mutation that silently did not",
      "land would still satisfy it. Choose a marker that does not occur in the unmodified file.",
    ]);
  }

  const declarative = Array.isArray(spec.anchors);
  const imperative = typeof spec.mutate === "function";
  if (declarative === imperative) {
    bad([
      "A plant spec needs EITHER `anchors` + `line` (insert a line after the line holding the last",
      "anchor, with the line ending copied out of the file) OR `mutate(text, ctx)` returning the whole",
      `new file text. Got anchors: ${declarative}, mutate: ${imperative}.`,
    ]);
  }

  if (declarative) {
    if (spec.anchors.length < 1 || spec.anchors.some((a) => typeof a !== "string" || !a)) {
      bad(["`anchors` must be a non-empty array of non-empty strings."]);
    }
    if (typeof spec.line !== "string" || !spec.line) {
      bad(["`line` must be the non-empty text to insert, WITHOUT a line ending."]);
    } else if (/[\r\n]/.test(spec.line)) {
      // The recurring accident in one sentence: a caller spelling its own `\n` into the payload on a
      // CRLF tree. This module owns the terminator precisely so that spelling is never needed, so a
      // spelled one is refused rather than silently rewritten.
      bad([
        "`line` contains a LINE ENDING. This module copies the terminator out of the file being",
        "planted (this working tree is CRLF), so a caller that spells its own is either mixing endings",
        "into a CRLF file or assuming an ending the file does not use — the exact silent-no-op class",
        "this module exists to close. Pass one line; pass several specs for several lines.",
      ]);
    }
    if (typeof spec.line === "string" && !spec.line.includes(spec.marker)) {
      bad([
        `The line to insert does not contain the marker "${spec.marker}", so the landing assertion`,
        "could never pass. One of the two is wrong; they are the same fact written twice.",
      ]);
    }
  }
}

/** The declarative insertion, and the escape hatch. Never writes — returns the text to write. */
function buildMutation(handle, text) {
  const spec = handle.spec;
  if (typeof spec.mutate === "function") {
    const out = spec.mutate(text, { target: handle.target, eol: text.includes("\r\n") ? "\r\n" : "\n" });
    if (typeof out !== "string") {
      throw new PlantError("locate", handle.target, [
        "`mutate(text, ctx)` must return the COMPLETE new file text as a string; it returned",
        `${out === undefined ? "undefined" : typeof out}. Nothing was written.`,
        ...(spec.hints?.locate ?? []),
      ]);
    }
    return out;
  }

  // Anchors are matched in sequence, each from the end of the previous one, so a caller can pin the
  // insertion between two landmarks (a signature's opening and its closing brace) and a signature that
  // grows or shrinks cannot make the insertion land inside it.
  let from = 0;
  let at = -1;
  const probed = [];
  for (const anchor of spec.anchors) {
    const i = text.indexOf(anchor, from);
    probed.push([anchor, i]);
    if (i < 0) {
      at = -1;
      break;
    }
    at = i;
    from = i + anchor.length;
  }
  const eol = at < 0 ? -1 : text.indexOf("\n", at);
  if (at < 0 || eol < 0) {
    const lines = [
      `Could not locate the insertion point in ${handle.target}.`,
      ...probed.map(([a, i]) => `  ${JSON.stringify(a)} -> ${i < 0 ? "NOT FOUND" : `offset ${i}`}`),
    ];
    if (at >= 0 && eol < 0) {
      lines.push("  The anchor line is the last line of the file and carries no terminator, so there is");
      lines.push("  no line to insert AFTER.");
    }
    if (spec.anchors.some((a) => /[\r\n]/.test(a))) {
      // Named explicitly because this is the measured shape of the accident: an anchor carrying `\n`
      // against a CRLF file matches nothing, the replacement is a no-op, and the guard reads green.
      lines.push("  An anchor above contains a LINE ENDING. This working tree is CRLF: an anchor written");
      lines.push("  with `\\n` matches nothing here, the mutation becomes a silent no-op, and a guard run");
      lines.push("  against an unchanged file passes. Anchor on text WITHIN one line.");
    }
    lines.push("Nothing was written; the target is untouched.");
    throw new PlantError("locate", handle.target, [...lines, ...(spec.hints?.locate ?? [])]);
  }
  const nl = text[eol - 1] === "\r" ? "\r\n" : "\n";
  return text.slice(0, eol + 1) + spec.line + nl + text.slice(eol + 1);
}

/**
 * Plant ONE spec into an already-reserved handle: baseline -> build -> write -> assert from disk ->
 * recheck. Every refusal throws; the caller (or `withPlanted`) is responsible for reverting.
 */
function doPlant(handle, index, opts) {
  const spec = handle.spec;

  // The caller's "is this file fit to carry a probe AT ALL" question, asked on the UNMODIFIED file. A
  // target that does not parse before the mutation cannot prove anything after it.
  if (typeof spec.baseline === "function") {
    const r = spec.baseline() ?? {};
    if (!r.ok) {
      throw new PlantError("baseline", handle.target, [
        `${handle.target} is not fit to carry a mutation BEFORE anything was planted, so nothing`,
        "planted in it could prove anything. The file is untouched.",
        ...(r.detail ?? []),
        ...(spec.hints?.baseline ?? []),
      ]);
    }
    handle.state = r.state;
  }

  const mutated = buildMutation(handle, handle.orig.toString("utf8"));
  fs.writeFileSync(handle.target, mutated);
  handle.planted = true;

  // The one hook that runs while the target is mutated and before anything has been asserted. It exists
  // for the abort drill: `process.abort()` skips `finally` AND every signal handler, which is the only
  // failure mode those two do not cover, so the shell's sidecar recovery can only be proven from here.
  if (typeof opts.afterPlant === "function") opts.afterPlant(handle, index);

  const onDisk = fs.readFileSync(handle.target);
  const changed = !onDisk.equals(handle.orig);
  const markerPresent = onDisk.toString("utf8").includes(spec.marker);
  if (!changed || !markerPresent) {
    throw new PlantError("land", handle.target, [
      `The mutation did not land in ${handle.target}.`,
      `  bytes changed:   ${changed} (expected true)`,
      `  byte delta:      ${onDisk.length - handle.orig.length}`,
      `  marker present:  ${markerPresent} (expected true — ${JSON.stringify(spec.marker)})`,
      "A no-op mutation makes a guard agree with a broken one: it runs against an unchanged file and",
      "passes, and the verification that was supposed to happen did not. This working tree is CRLF —",
      "a mutation written with LF endings, or against an anchor that no longer occurs, changes nothing.",
      "Asserted against the bytes read BACK FROM DISK, never against the string we meant to write.",
      ...(spec.hints?.land ?? []),
    ]);
  }

  // The caller's "is the mutated file still valid" question. An unparseable file produces no findings,
  // which reads exactly like a clean one — so this runs before any conclusion is drawn from the guard.
  if (typeof spec.recheck === "function") {
    const r = spec.recheck(handle.state) ?? {};
    if (!r.ok) {
      throw new PlantError("recheck", handle.target, [
        `The planted mutation BROKE ${handle.target}, so whatever the guard says about it next would`,
        "say nothing about the guard.",
        ...(r.detail ?? []),
        ...(spec.hints?.recheck ?? []),
      ]);
    }
  }
  return handle;
}

/**
 * Reserve + plant one target. Exported for callers that genuinely cannot use `withPlanted` — and they
 * then owe the `finally` that `withPlanted` would have owned. Prefer `withPlanted`.
 */
export function plant(spec, opts = {}) {
  const handle = reserve(spec, opts);
  return doPlant(handle, 0, opts);
}

/**
 * Put the file back FROM THE BYTES READ BEFORE PLANTING — never from the index, which is what
 * `git checkout -- <file>` reads and which discards any uncommitted work in the target. Then prove it,
 * because an unproven revert is a claim.
 */
export function revert(handle, opts = {}) {
  if (handle.planted) {
    fs.writeFileSync(handle.target, handle.orig);
    handle.planted = false;
  }
  assertRestored(handle, opts);
  liveHandles.delete(handle);
  if (handle.slot) {
    fs.rmSync(handle.slot, { force: true });
    fs.rmSync(`${handle.slot}.path`, { force: true });
  }
  (opts.log ?? console.log)(
    `${optLabel(opts)}: ${handle.target} restored byte-identical (sha256 ${handle.sha.slice(0, 16)}…)`
  );
}

/**
 * The revert's proof, split out so it can be fired on purpose from the selftest: byte identity by
 * sha256 rather than by a diff, and the sidecar deliberately still on disk when it fails.
 */
export function assertRestored(handle, opts = {}) {
  const now = sha256(fs.readFileSync(handle.target));
  if (now === handle.sha) return;
  throw new PlantError("revert", handle.target, [
    `FAILED TO REVERT ${handle.target}.`,
    `  before: ${handle.sha}`,
    `  after:  ${now}`,
    handle.slot
      ? `  The pre-plant bytes are at ${handle.slot} — copy them back before doing anything else.`
      : "  No sidecar was kept (this call passed no `backup`), so the pre-plant bytes exist only in the",
    handle.slot
      ? "  Do not run any command that writes to that path until it is back."
      : "  memory of this process. Do not run anything that writes to that path.",
  ]);
}

/**
 * THE ONE TO USE. Reserve every target, plant them in order, run `body`, and revert every target on the
 * way out — on success, on a throw, and (via the handlers installed at reserve time) on a signal.
 *
 * Returns whatever `body` returns. Rethrows whatever `body` threw, AFTER reverting. If a revert itself
 * fails, that failure wins: it is the one that leaves damage on disk.
 */
export function withPlanted(specs, body, opts = {}) {
  const handles = [];
  let bodyErr = null;
  let result;
  try {
    // Reserve ALL of them before planting ANY of them, so the sidecars for later targets already exist
    // if an earlier target dies mid-plant — including under `process.abort()`, which reaches no code.
    for (const spec of specs) handles.push(reserve(spec, opts));
    handles.forEach((handle, i) => doPlant(handle, i, opts));
    result = body(handles);
    if (result && typeof result.then === "function") {
      // A promise here means the body's real work is still running while the reverts below undo the
      // very mutation it is reading. Refused loudly rather than awaited: awaiting would put the target
      // back only after an unbounded window in which it is mutated on a real working tree.
      result = undefined;
      throw new PlantError("async-body", handles[0]?.target ?? "(none)", [
        "`withPlanted` is synchronous by design and its body returned a Promise. The revert would have",
        "run while that promise was still reading the mutated file, so the mutation window would be",
        "unbounded and the guard's verdict would be a race. Do the work synchronously (this repo's",
        "gates use `execFileSync`), or block before returning.",
      ]);
    }
  } catch (err) {
    bodyErr = err;
  }

  const revertErrs = [];
  for (const handle of handles) {
    try {
      revert(handle, opts);
    } catch (err) {
      revertErrs.push(err);
    }
  }

  if (revertErrs.length) {
    if (bodyErr) revertErrs[0].suppressed = bodyErr;
    throw revertErrs[0];
  }
  if (bodyErr) throw bodyErr;
  return result;
}
