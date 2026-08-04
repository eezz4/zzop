// self-analysis-gate.mjs — zzop analyzing zzop, pinned at ZERO findings, with a canary that proves
// the run can still see. Invoked by `scripts/measure/self-analysis-gate.sh`, which builds the binary
// it hands over; this file is not meant to be run by hand.
//
// ## Why a gate at all
// The repo root `zzop.config.jsonc` is a committed dogfood config: two of this project's capabilities
// (the Rust call graph, and `cache-lane-file-read`) only mean anything if they can be pointed at this
// repo. Until now only the DEP-GRAPH half of that was automated — `zzop graph --domain dep` runs in CI
// and its output is pinned by a diff on `site/`. Nothing ran `zzop analyze`. The findings half of the
// dogfood claim was a config file and a paragraph, which is the same shape as a benchmark nobody
// scores: a document, not a gate.
//
// ## Why ZERO, and not a baseline file
// `findings.total` must be 0. There is deliberately no allow-list: a baseline is a hand-maintained
// record of how much is forgiven, and it goes stale in the one direction nobody notices — silently
// wider. A new rule that legitimately fires on our own code turning this red is the POINT, not a cost.
// It forces the decision (fix the code, or narrow the vocabulary, or turn the rule off in the config
// on purpose) instead of letting a line appear in a file that nobody reads again.
//
// ## Why the canary is the larger half of this file
// `findings.total === 0` is satisfied just as happily by a run that has stopped looking. A parser
// regression, a vocabulary key that stops being read, a walk that skips the directory — every one of
// them reads as a clean tree. So before the zero is trusted, PHASE 1 plants a violation that IS a
// violation, requires it to be found, and reverts. If the planted finding does not appear, this exits
// RED saying the self-analysis has gone BLIND — a worse verdict than a finding, because a finding is
// information and blindness is the absence of it.
//
// ### TWO axes, because `findings.total` sums two evaluators
// The probe below is a NATIVE rule violation, and for a long time it was the whole canary — which
// proved nothing about the DSL rule packs, the other half of what that total sums. On this repo that
// gap was structural, not hypothetical: the native probe lives in a `.rs` file, and not one of the 138
// bundled DSL rules targets `.rs` at all. So a second probe rides alongside it (see `DSL_TARGET` and
// the block above it), planted in the same pass and read out of the same `analyze` run. It is enforced
// when this run's own census says a DSL rule CAN fire, and disclosed loudly when it cannot — never
// pretended.
//
// ### The canary asserts its own validity, because two ways of failing to plant were MEASURED
// Both were hit by hand while wiring this, and both produce "zero findings" — indistinguishable from a
// clean tree unless the canary checks:
//
//   1. THE PROBE WAS INVISIBLE TO THE RULE. The first probe by hand was a non-`pub` helper function,
//      which the Rust parser does not surface as a symbol; the rule had nothing to anchor to, and the
//      conclusion very nearly drawn was "the rule does not work on Rust". Closed here by asserting, on
//      the UNMODIFIED file, that `symbols.exported` contains the anchor this rule keys on. The probe
//      then goes INSIDE that symbol's body, so it is reachable from a symbol the parser demonstrably
//      surfaces rather than from one we hope it does.
//
//   2. THE PROBE BROKE THE PARSE. The second probe by hand was inserted before `fn
//      compute_fresh_artifact`, whose signature spans nine lines; the insertion landed inside the
//      signature and destroyed it. `zzop file` went from `symbols: 4` to `symbols: 0` — an unparseable
//      file yields no findings, which is byte-for-byte the same verdict as a clean one. Closed here by
//      re-running `zzop file` AFTER planting and requiring `verdict === "analyzed"` with the symbol
//      count unchanged, before the finding assertion is allowed to mean anything.
//
//   3. THE PROBE NEVER LANDED. This repo's files are CRLF. A mutation written with LF line endings, or
//      against an anchor string that no longer occurs, can be a silent no-op — and a no-op plant makes
//      the canary agree with a broken engine. Closed here by asserting a non-zero byte delta AND the
//      marker's presence in the bytes read back from disk, not in the string we intended to write.
//
// ## Where the planting machinery actually lives
// Not here. Traps 2 and 3 above, the revert discipline, the sidecars and the signal handlers are
// `scripts/measure/plant-revert.mjs`, and this gate is one of its callers — because four batches wrote
// their own copy of that procedure on 2026-08-01 alone, each in a scratchpad the next one could not
// read. What stays in THIS file is the part that is about zzop: which file to plant in, which rule must
// fire, and what a silence means. Read that module's header for the failure modes; read
// `plant-revert-selftest.mjs` for the proof that each of its refusals still fires.
//
// The one sentence worth repeating here, because it cost a full re-verification on 2026-08-01: the
// probe target is restored from THE BYTES THIS PROCESS READ BEFORE PLANTING — never `git checkout --`,
// which restores from the index and therefore discards any uncommitted work in that file.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { PlantError, withPlanted } from "./plant-revert.mjs";

// --- what the canary plants, and where -----------------------------------------------------------
// Hardcoded rather than parameterized ON PURPOSE. A probe target passed in from outside is a thing
// that can go stale while the gate stays green, which is the same defect class the canary exists to
// detect, one level up.
const TARGET = "crates/engine/src/pipeline/fresh.rs";
const RULE_ID = "cache-lane-file-read";
// The config's `vocabulary.cacheLaneAnchorPattern` is `^compute_fresh_artifact$`. If that name is ever
// renamed, the baseline assertion below fails loudly instead of the run quietly finding nothing.
const ANCHOR = "compute_fresh_artifact";
const SIG_OPEN = `fn ${ANCHOR}(`;
const SIG_CLOSE = ") -> FileArtifact {";
const MARKER = "_zzop_canary_probe";
// A real violation of the rule, not a lookalike: a filesystem read reachable from the cached per-file
// lane, whose result cannot be in the cache key. Kept to one line so the insertion cannot span a
// construct.
const PROBE = `    let ${MARKER} = std::fs::read_to_string("Cargo.toml").map(|s| s.len()).unwrap_or(0);`;

// --- the DSL half of the same proof --------------------------------------------------------------
// The canary above plants a NATIVE rule violation, and for a long time that was the whole canary. It
// proves the walk, the Rust parser and the native evaluator can still see — and NOTHING about the DSL
// rule packs, which are the other half of what `findings.total` sums. That gap is not theoretical on
// this repo: 987 of its ~1400 files are `.rs`, and not one of the 138 bundled DSL rules carries a
// `file_pattern` matching `.rs` at all (measured — the engine now says so itself in `warnings`). So the
// native probe lives in a file no DSL rule could ever have looked at, and a DSL evaluator that had
// stopped running entirely would have kept this gate green.
//
// This probe closes that: a REAL violation of a real bundled DSL rule, in a `.mjs` file that is inside
// that rule's `file_pattern` and outside the config's `exclude`. It is planted in the SAME pass as the
// native one and read out of the SAME `analyze` run — two independent files, two independent rules, no
// extra process.
//
// ## Why it is CONDITIONAL, and why that is not a loophole
// This repo's committed config disables every DSL pack on purpose (see zzop.config.jsonc's `packs`
// block: a self-audit that also reports 30 unrelated findings is one nobody reads twice). Under that
// config no DSL rule can fire, and demanding this probe be SEEN would pin the gate permanently red for
// a decision the config states outright. Demanding it be seen only when it CAN be seen is the honest
// shape, so the gate derives that from the run itself (`packsLoaded` x `ruleOverridesApplied.disabled`,
// see `dslFireability`) and then:
//   * fireable  -> the finding is REQUIRED; its absence is BLIND, exactly like the native half.
//   * not fireable -> the finding is required to be ABSENT (a fired-anyway probe means the census is
//     lying about what this run covers), and the gate prints the DSL-AXIS-NOT-PROVEN banner so the zero
//     below is never read as covering the DSL half.
// The condition is measured every run, so the day someone enables a pack in the config, this probe
// starts being enforced with no edit here.
const DSL_TARGET = "scripts/measure/diff.mjs";
const DSL_PACK = "security";
const DSL_RULE_ID = "security/hardcoded-secret";
// The rule is a `line-scan`, so it anchors to text rather than to a symbol — but the probe still goes
// INSIDE a function body, for trap 2: an insertion that lands mid-construct breaks the parse, and a
// broken parse is indistinguishable from a clean file everywhere else in this run.
const DSL_ANCHOR = "function loadRun(label) {";
const DSL_MARKER = "_zzop_dsl_canary_probe";
// A real `security/hardcoded-secret`: a credential-shaped assignment whose VALUE passes that rule's
// identifier-shape veto (no `-`/`_`-joined words, not UPPER_SNAKE, not repeated PascalCase). Verified
// by hand in a scratch tree with the packs enabled before this was written — it reports
// `security/hardcoded-secret`, so a silence here is the engine's, not the probe's.
//
// The value is JOINED AT RUNTIME rather than written as one literal, and that is not cosmetic: spelled
// out, this very line is itself a `security/hardcoded-secret` violation, and it was one — measured, in
// the verification run that proved the branch below works (`security/hardcoded-secret` at
// self-analysis-gate.mjs:119). The committed config disables that pack so the gate stayed green, which
// is the worst version of the trap: a landmine that arms itself the day someone enables one pack, in
// the file whose whole job is to stay trustworthy. Split, the assignment operator is followed by an
// identifier rather than a quoted literal and the rule's `file_pattern`-plus-value shape no longer
// matches, while the PLANTED text is byte-for-byte what it always was.
const DSL_PROBE_VALUE = ["Xk9Qp2Lm", "7Vd4Rt6B"].join("");
const DSL_PROBE = `  const ${DSL_MARKER}_api_key = ${JSON.stringify(DSL_PROBE_VALUE)};`;

const args = process.argv.slice(2);
function flag(name) {
  const i = args.indexOf(name);
  if (i < 0 || !args[i + 1]) {
    console.error(`self-analysis-gate: missing ${name} <value>`);
    process.exit(2);
  }
  return args[i + 1];
}
const BIN = flag("--bin");
const CONFIG = flag("--config");
const BACKUP = flag("--backup");

/** Run a zzop subcommand and parse its JSON reply. A non-JSON reply is a broken run, never a clean one. */
function zzop(argv) {
  const out = execFileSync(BIN, argv, { encoding: "utf8", maxBuffer: 1 << 28 });
  try {
    return JSON.parse(out);
  } catch {
    console.error(`self-analysis-gate: \`zzop ${argv.join(" ")}\` did not emit JSON. First 400 bytes:`);
    console.error(out.slice(0, 400));
    process.exit(1);
  }
}

/** The canary's own verdict. Distinct banner from a finding: this says the gate cannot see. */
function blind(lines) {
  console.error("");
  console.error("=================================================================");
  console.error(" SELF-ANALYSIS HAS GONE BLIND");
  console.error("=================================================================");
  for (const l of lines) console.error(`  ${l}`);
  console.error("");
  console.error("  This is MORE serious than a finding. A finding tells you something about the code;");
  console.error("  this tells you the zero the gate reports below means nothing, because a violation");
  console.error("  planted on purpose was not seen. Do not silence it by relaxing the gate.");
  console.error("=================================================================");
  process.exitCode = 1;
}

/**
 * The DSL half's own verdict when the probe could not be REQUIRED: not a failure, but never silent
 * either. The whole point of this gate is that a zero must say what it covers, and a run whose DSL
 * packs are all disabled has a zero that covers the native analyses only.
 */
let dslAxisProven = false;
function dslAxisNotProven(lines) {
  console.log("");
  console.log("-----------------------------------------------------------------");
  console.log(" DSL AXIS NOT PROVEN — the zero below covers the NATIVE half only");
  console.log("-----------------------------------------------------------------");
  for (const l of lines) console.log(`  ${l}`);
  console.log("  The native canary above still holds: the walk, the parser and the native evaluator");
  console.log("  are proven able to see. Nothing here proves the same about the DSL rule packs, so");
  console.log("  read `findings.total = 0` as \"no NATIVE findings\", not as \"nothing to report\".");
  console.log("-----------------------------------------------------------------");
}

/**
 * Can any DSL rule fire in this run at all? Derived from the run's own reply — `packsLoaded` (which
 * carries each pack's rule count and `filesInScope`, the engine's own path-candidacy census) crossed
 * with `ruleOverridesApplied.disabled` (what the config actually gated off, by bare pack id or by
 * `<pack>/<rule>`). Never inferred from reading zzop.config.jsonc here: a second reader of that file is
 * a second thing to forget to update, and the engine already publishes the answer.
 *
 * That holds for FIREABILITY and stops exactly there — see `packCensusBlindness` for the one question
 * the run cannot answer about itself.
 */
function dslFireability(run) {
  const disabled = new Set(run.ruleOverridesApplied?.disabled ?? []);
  const loaded = run.packsLoaded ?? [];
  const fireable = loaded.filter((p) => (p.rules ?? 0) > 0 && (p.filesInScope ?? 0) > 0 && !disabled.has(p.id));
  const probeRuleFireable =
    fireable.some((p) => p.id === DSL_PACK) && !disabled.has(DSL_RULE_ID);
  return { loaded, disabled, fireable, probeRuleFireable };
}

/**
 * The declared pack-disable list out of the committed config — `packs.disabled`, an array of pack ids.
 *
 * WHY THIS READS THE CONFIG when `dslFireability` deliberately does not. The two ask different
 * questions, and only one of them the run can answer about itself. `ruleOverridesApplied.disabled` is
 * FILTERED BY WHAT LOADED (`analyze::diagnostics::coverage_report::rule_overrides_applied` keeps only
 * ids present in `known_rule_ids`, which is built from the loaded packs), so a pack loader that stopped
 * loading anything erases the very evidence that the disable was requested: `packsLoaded: []` produces
 * `disabled: []`, and a gate reading only the run then sees a config that asked for nothing and a loader
 * that returned nothing — indistinguishable from "the config turned everything off on purpose". The
 * declaration has to come from outside the run for the comparison to mean anything. That is not a second
 * copy of a fact the engine publishes; it is the independent side of a two-sided check.
 *
 * JSONC: `//` line comments are stripped with string state tracked, so a `"http://…"` inside a value is
 * not mistaken for one. Block comments are not stripped — the committed config has none, and a
 * half-clever stripper that silently mangles one is worse than a parse error that names the file.
 */
function configDeclaredDisabledPacks() {
  const raw = readFileSync(CONFIG, "utf8");
  let out = "";
  let inString = false;
  for (let i = 0; i < raw.length; i += 1) {
    const c = raw[i];
    if (inString) {
      out += c;
      if (c === "\\") { out += raw[i + 1] ?? ""; i += 1; continue; }
      if (c === '"') inString = false;
      continue;
    }
    if (c === '"') { inString = true; out += c; continue; }
    if (c === "/" && raw[i + 1] === "/") {
      while (i < raw.length && raw[i] !== "\n") i += 1;
      out += "\n";
      continue;
    }
    if (c === "/" && raw[i + 1] === "*") {
      console.error(`self-analysis-gate: ${CONFIG} carries a /* */ block comment, which this reader does`);
      console.error("  not strip. Teach `configDeclaredDisabledPacks` or use // comments — guessing here");
      console.error("  would let the pack-census check silently read a mangled config.");
      process.exit(1);
    }
    out += c;
  }
  // Trailing commas are legal JSONC and illegal JSON; the committed config uses none, and the same
  // "name it rather than guess" rule applies.
  let parsed;
  try {
    parsed = JSON.parse(out);
  } catch (err) {
    console.error(`self-analysis-gate: could not parse ${CONFIG} after comment-stripping: ${err.message}`);
    console.error("  The pack-census check below needs the config's own `packs.disabled` declaration.");
    process.exit(1);
  }
  return parsed?.packs?.disabled ?? [];
}

/**
 * BLINDNESS in the pack census itself — the check that has to run BEFORE the DSL axis is allowed to
 * report NOT PROVEN.
 *
 * The NOT-PROVEN banner asserts something specific: *the config disables every DSL pack on purpose*. It
 * used to be printed on the sole evidence that no pack was fireable, which is the same observation a
 * TOTAL PACK-LOADER REGRESSION produces. Under that regression `packsLoaded` is `[]`, every pack is
 * trivially not fireable, the gate blames the config, and exits 0 — the DSL half of `findings.total`
 * silently stops being computed and the banner explains it away.
 *
 * The materials to tell the two apart are already in hand, and neither needs a new engine field:
 *   1. `packsLoaded` lists a pack even when the config DISABLED it (`filesInScope: 0`; pinned by
 *      `crates/engine/tests/analyze_zero_scope_packs.rs`, "loading is not gating"). So a disabled pack
 *      MISSING from `packsLoaded` is a pack that was never loaded — not a pack that was turned off.
 *   2. The config's own `packs.disabled` says which ids to expect there.
 * Zero loaded packs is the degenerate case of the same test and is named separately, because it is the
 * shape a regression actually takes and deserves its own sentence.
 *
 * Returns `null` when the census is trustworthy, or the `blind()` lines when it is not.
 */
function packCensusBlindness(loaded) {
  const declared = configDeclaredDisabledPacks();
  if (loaded.length === 0) {
    return [
      `No DSL rule pack was loaded AT ALL — \`packsLoaded\` is empty.`,
      `  ${CONFIG} declares packs.disabled: ${JSON.stringify(declared)}`,
      `A disabled pack is still LOADED and still listed (loading is not gating — see`,
      `crates/engine/tests/analyze_zero_scope_packs.rs), so an empty list is not what disabling looks`,
      `like. It is what a broken pack loader, an empty packs directory or a lost embed looks like.`,
      `Half of what \`findings.total\` sums is not being computed, and the DSL-AXIS-NOT-PROVEN banner`,
      `would have blamed the config for it and exited 0.`,
    ];
  }
  const missing = declared.filter((id) => !loaded.some((p) => p.id === id));
  if (missing.length > 0) {
    return [
      `${CONFIG} disables pack(s) that were never LOADED: ${JSON.stringify(missing)}`,
      `  packsLoaded: ${JSON.stringify(loaded.map((p) => p.id))}`,
      `A disabled pack is still loaded and still listed, so its absence here means the loader never`,
      `saw it — a renamed/deleted pack file, a lost embed, or a loader regression. Whatever the zero`,
      `below covers, it is not what this config says it covers.`,
    ];
  }
  return null;
}

// --- PHASE 1: canary --------------------------------------------------------------------------
// Two probes, planted in the same pass and read out of ONE `analyze` run: the native one above and the
// DSL one beside it. The MECHANICS — pre-plant bytes, the line ending, the on-disk landing assertion,
// the sidecar pair under $BACKUP.<n>, the signal handlers, the proven revert — belong to
// `plant-revert.mjs`. What is spelled here is only the zzop-specific half of each probe: whether the
// target is fit to carry one (`baseline`), whether it survived being carried (`recheck`), and the
// advice a reader needs when one of those refuses (`hints`).
const LABEL = "self-analysis-gate";

/** Trap 1, for the native probe: the parser must surface the very symbol the rule keys on. */
const nativeSpec = {
  target: TARGET,
  // Between the signature's opening and its closing brace, so a signature that grows or shrinks cannot
  // make the insertion land inside it (trap 2) — the probe goes on the first line of the BODY.
  anchors: [SIG_OPEN, SIG_CLOSE],
  line: PROBE,
  marker: MARKER,
  baseline() {
    const before = zzop(["file", TARGET, "--config", CONFIG]);
    const symbols = before.symbols?.count ?? 0;
    const exported = before.symbols?.exported ?? [];
    return {
      ok: before.verdict === "analyzed" && symbols >= 1 && exported.includes(ANCHOR),
      state: symbols,
      detail: [
        `  verdict:  ${before.verdict} (expected "analyzed")`,
        `  symbols:  ${symbols}`,
        `  exported: ${JSON.stringify(exported)} (expected to contain "${ANCHOR}")`,
      ],
    };
  },
  recheck(baseSymbols) {
    const after = zzop(["file", TARGET, "--config", CONFIG]);
    const symbols = after.symbols?.count ?? 0;
    return {
      ok: after.verdict === "analyzed" && symbols === baseSymbols,
      detail: [
        `  verdict:  ${after.verdict} (expected "analyzed")`,
        `  symbols:  ${symbols} (expected ${baseSymbols}, unchanged — the probe is a statement,`,
        `            not a new item)`,
      ],
    };
  },
  hints: {
    baseline: [
      `If \`${ANCHOR}\` was renamed, rename it here and in zzop.config.jsonc's`,
      `vocabulary.cacheLaneAnchorPattern together — they are the same fact in two files.`,
    ],
    locate: [
      `The probe was never planted, so the zero below is unproven. Re-anchor the constants at the`,
      `top of this file on the function as it is spelled today.`,
    ],
    recheck: [
      `An unparseable file yields no findings, which is byte-for-byte the same verdict as a clean one,`,
      `so the absence of findings below would say nothing about the rule.`,
    ],
  },
};

/** The DSL probe, under the same traps. A `line-scan` rule anchors to text, but the probe still goes
 *  INSIDE a function body: an insertion that lands mid-construct breaks the parse. */
const dslSpec = {
  target: DSL_TARGET,
  anchors: [DSL_ANCHOR],
  line: DSL_PROBE,
  marker: DSL_MARKER,
  baseline() {
    const before = zzop(["file", DSL_TARGET, "--config", CONFIG]);
    const symbols = before.symbols?.count ?? 0;
    return {
      ok: before.verdict === "analyzed" && symbols >= 1,
      state: symbols,
      detail: [
        `  verdict:  ${before.verdict} (expected "analyzed")`,
        `  symbols:  ${symbols} (expected >= 1)`,
      ],
    };
  },
  recheck(dslBase) {
    const after = zzop(["file", DSL_TARGET, "--config", CONFIG]);
    const symbols = after.symbols?.count ?? 0;
    return {
      ok: after.verdict === "analyzed" && symbols === dslBase,
      detail: [
        `  verdict:  ${after.verdict} (expected "analyzed")`,
        `  symbols:  ${symbols} (expected ${dslBase}, unchanged — the probe is a statement`,
        `            inside a function body, not a new top-level item)`,
      ],
    };
  },
  hints: {
    baseline: [
      `Move the probe to another file inside \`${DSL_RULE_ID}\`'s file_pattern and outside the`,
      `config's exclude, or fix whatever stopped this one parsing.`,
    ],
    locate: [
      `Re-anchor DSL_ANCHOR at the top of this file on ${DSL_TARGET} as it is spelled today.`,
    ],
    recheck: [
      `Half of what \`findings.total\` sums would then be reading a broken file.`,
    ],
  },
};

try {
  // `withPlanted` owns the `finally`. Every earlier version of this procedure in this repo left that to
  // the caller, and a caller that forgets it leaves a tracked file mutated on a real working tree.
  withPlanted([nativeSpec, dslSpec], () => {
    // The assertion everything above exists to make meaningful. ONE run, read for BOTH probes.
    const seen = zzop(["analyze", "--config", CONFIG]);
    // JUDGED ON `findings.byRule` — the uncapped per-rule census PHASE 2 already trusts as "the
    // complete count" — never on `findings.shown`, which is the summary surface's DISPLAY sample:
    // capped and sorted, so a probe finding that sorts past the cap is simply absent from it, and a
    // judgment read off `shown` reports BLIND about a run that saw the probe fine — a false alarm
    // wearing the canary's worst-verdict banner (measured 2026-08-03: with the displacement simulated
    // below, the pre-byRule judgment declared BLIND while its own diagnostic printed
    // `findings.byRule: {"cache-lane-file-read":1}`). What `byRule` cannot carry is the FILE axis —
    // a count says the rule fired, not where — and no uncapped per-file channel is added for that,
    // because PHASE 2's zero pin already closes the gap in the same gate run: a non-probe firing of a
    // probe rule is a finding on the UNPLANTED tree too, so it turns PHASE 2 red regardless — a
    // miscounted canary can change which words the red arrives in, never mint a green. `shown` is
    // still read below, for the log line's file:line — display, not judgment.
    const shown = seen.findings?.shown ?? [];
    // Reproduce the displacement by hand: empty the display sample while `byRule` keeps the truth.
    // Under the old shown-read judgment this was a false BLIND; under byRule it must change nothing
    // but the log line's location note.
    if (process.env.ZZOP_CANARY_TEST_SHOWN_CAP) shown.length = 0;
    const byRule = seen.findings?.byRule ?? {};
    const at = (ruleId, file) => {
      const f = shown.find((x) => x.ruleId === ruleId && x.file === file);
      return f ? `at ${f.file}:${f.line}` : `in ${file} (displaced past findings.shown's cap)`;
    };
    if ((byRule[RULE_ID] ?? 0) < 1) {
      blind([
        `A REAL violation was planted in ${TARGET}, that file still parses with its symbols intact,`,
        `and \`${RULE_ID}\` fired ZERO times in the whole run (findings.byRule — the uncapped count).`,
        `  findings.total:  ${seen.findings?.total ?? "(absent)"}`,
        `  findings.byRule: ${JSON.stringify(byRule)}`,
        `The rule, its vocabulary, the walk or the parser stopped reaching this file. Until that is`,
        `fixed, "zzop analyze reports zero on zzop" is a claim about nothing.`,
      ]);
      return;
    }
    console.log(`${LABEL}: native canary OK — planted violation seen as ${RULE_ID} (byRule ${byRule[RULE_ID]}) ${at(RULE_ID, TARGET)}`);

    // The DSL half. `probeRuleFireable` is measured from THIS run, not assumed.
    const { loaded, disabled, fireable, probeRuleFireable } = dslFireability(seen);
    // ...but "not fireable" is only a DECISION if the packs were actually loaded. Reproduce a loader
    // regression at the exact seam this gate reads, so the BLIND branch below is provable rather than
    // merely written — the same reasoning `afterPlant`'s abort hook carries for the revert path.
    if (process.env.ZZOP_CANARY_TEST_BLIND_PACKS) loaded.length = 0;
    const censusBlind = packCensusBlindness(loaded);
    const dslFired = (byRule[DSL_RULE_ID] ?? 0) >= 1;
    if (probeRuleFireable && !dslFired) {
      blind([
        `A REAL violation was planted in ${DSL_TARGET}, that file still parses, \`${DSL_RULE_ID}\` is`,
        `LOADED and ENABLED with files in scope — and it fired ZERO times (findings.byRule).`,
        `  findings.total:  ${seen.findings?.total ?? "(absent)"}`,
        `  findings.byRule: ${JSON.stringify(byRule)}`,
        `  packsLoaded:     ${JSON.stringify(loaded)}`,
        `The DSL evaluator, the pack loader or the walk stopped reaching this file. Half of what`,
        `\`findings.total\` sums is not being computed, and the zero below hides it.`,
      ]);
    } else if (probeRuleFireable) {
      dslAxisProven = true;
      console.log(`${LABEL}: DSL canary OK — planted violation seen as ${DSL_RULE_ID} (byRule ${byRule[DSL_RULE_ID]}) ${at(DSL_RULE_ID, DSL_TARGET)}`);
    } else if (dslFired) {
      // The census said this rule cannot fire and it fired. Whichever of the two is wrong, the gate's
      // account of what its zero covers is wrong with it.
      blind([
        `\`${DSL_RULE_ID}\` fired in this run (findings.byRule), but the run's own census says it CANNOT:`,
        `  packsLoaded:  ${JSON.stringify(loaded)}`,
        `  disabled:     ${JSON.stringify([...disabled])}`,
        `Either \`packsLoaded\`/\`ruleOverridesApplied\` misreport what ran, or \`dslFireability\` in this`,
        `file reads them wrongly. Until that is settled, nothing here can say what the zero covers.`,
      ]);
    } else if (censusBlind) {
      // BEFORE the NOT-PROVEN banner, because that banner ASSERTS the config's intent and this is the
      // check that the assertion has any evidence at all. Blaming a config for a loader regression is
      // how the DSL half of `findings.total` stops being computed behind an exit 0.
      blind(censusBlind);
    } else {
      dslAxisNotProven([
        `A real \`${DSL_RULE_ID}\` violation was planted in ${DSL_TARGET} and correctly reported`,
        `nothing: no DSL rule can fire in this run at all.`,
        `  DSL packs loaded:            ${loaded.length} (${loaded.reduce((n, p) => n + (p.rules ?? 0), 0)} rule(s))`,
        `  ...of those, fireable here:  ${fireable.length}`,
        `  disabled by the config:      ${JSON.stringify([...disabled].filter((d) => loaded.some((p) => p.id === d)))}`,
        `${CONFIG} disables every DSL pack on purpose, and says why in its own \`packs\` block. That is a`,
        `decision, not a defect — but it means this gate proves the NATIVE evaluator can see and proves`,
        `nothing about the DSL one. Enable one pack in that config and this probe is enforced instead,`,
        `with no edit to the gate.`,
      ]);
    }
  }, {
    backup: BACKUP,
    label: LABEL,
    // The one line that makes the shell's last-resort recovery PROVABLE. `process.abort()` skips
    // `finally` and every signal handler, which is the only failure mode those two do not cover, so
    // this reproduces it exactly: run the gate with ZZOP_CANARY_TEST_ABORT=1 and every probe target
    // must come back byte-identical anyway, restored from the sidecars by plant-revert-recover.sh. A
    // safety net nobody can fire is indistinguishable from one that does not work. (Aborting on the
    // FIRST probe still exercises the whole sidecar loop: `withPlanted` reserves every target, and
    // therefore writes every sidecar pair, before it plants any of them.)
    afterPlant: (_handle, i) => {
      if (i === 0 && process.env.ZZOP_CANARY_TEST_ABORT) process.abort();
    },
  });
} catch (err) {
  if (!(err instanceof PlantError)) throw err;
  if (err.stage === "revert") {
    // Not a BLIND verdict: this one says a tracked file may still be mutated on disk, which outranks
    // anything the gate has to say about findings.
    console.error(`${LABEL}: ${err.lines[0]}`);
    for (const line of err.lines.slice(1)) console.error(line);
    process.exit(1);
  }
  // Every other stage means the planted violation never became a fair question — which is exactly what
  // BLIND means: the zero below would be unproven.
  blind(err.lines);
}

if (process.exitCode) process.exit(process.exitCode);

// --- PHASE 2: the gate ------------------------------------------------------------------------
const run = zzop(["analyze", "--config", CONFIG]);
const total = run.findings?.total ?? -1;
console.log(`self-analysis-gate: ${run.fileCount} files analyzed, findings.total = ${total}, ${(run.warnings ?? []).length} warning(s)`);

if (total !== 0) {
  console.error("");
  console.error(`self-analysis-gate: FAILED — zzop reports ${total} finding(s) on zzop itself.`);
  console.error(`  by rule: ${JSON.stringify(run.findings?.byRule ?? {})}   <- the complete count`);
  console.error("");
  console.error("  Sample below (`findings.shown` is capped by the summary surface's own limit, so it");
  console.error("  may be shorter than the counts above — those are the totals):");
  for (const f of run.findings?.shown ?? []) {
    console.error(`  ${f.severity}  ${f.ruleId}  ${f.file}:${f.line}`);
    console.error(`      ${String(f.message).split(". ")[0]}.`);
  }
  console.error("");
  console.error("  There is no baseline file to add this to, on purpose. Fix the code, narrow the");
  console.error("  vocabulary in zzop.config.jsonc, or turn the rule off there deliberately — each of");
  console.error("  those is a decision someone makes and a reviewer can see.");
  process.exit(1);
}
console.log(
  dslAxisProven
    ? "self-analysis-gate: clean, and proven not blind on BOTH axes (native rule + DSL pack rule)."
    : "self-analysis-gate: clean, and proven not blind on the NATIVE axis. The DSL axis was NOT proven -- see the DSL AXIS NOT PROVEN block above for what this zero does and does not cover."
);
