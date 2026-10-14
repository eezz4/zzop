#!/usr/bin/env bash
# Keeps ONE sentence true: how many of the engine's own parser call sites sit inside a
# `std::panic::catch_unwind`.
#
# ## Why this guard exists
# `parser/tests/input_strategy.rs` is the single owner of the rationale for all eight crates'
# arbitrary-input no-panic properties, and until 2026-10-09 it opened by asserting the opposite of the
# truth: *"`crates/engine/src/pipeline/parsers.rs` wraps every frontend in `std::panic::catch_unwind`,
# so a parser panic genuinely cannot take the engine down."* Measured (external review round 9),
# 11 of 125 engine-side call sites are wrapped, and `parsers.rs` names `zzop_parser_sql` zero times —
# so for SQL the sentence was false with no qualification at all.
#
# The repair was the sentence, not the defences (that reasoning lives in `input_strategy.rs`, not
# duplicated here). This guard exists because the repaired sentence carries a NUMBER, and a number in
# prose next to a population that moves is the exact shape that produced the original error. So the
# number is DERIVED here and compared against the one the doc states — the same stance
# `check-guards-wired.sh` takes toward `core.hooksPath` and `check-max-file-lines.sh` toward
# CONTRIBUTING.md's cap.
#
# ⚠ WHAT THIS GUARD IS NOT. It does not judge whether a call site OUGHT to be wrapped. Wrapping the
# unwrapped ones would need a per-site answer to "what does this return on `Err`", and the property is
# already held by eight proptests; `input_strategy.rs` owns that judgement. This guard only refuses to
# let the stated count drift away from the tree.
set -euo pipefail
cd "$(dirname "$0")/.."

OWNER=parser/tests/input_strategy.rs

node - "$OWNER" <<'NODE'
const fs = require('fs');
const { execFileSync } = require('child_process');
const owner = process.argv[2];

// ── The census ────────────────────────────────────────────────────────────────────────────────────
// Subjects: every place the ENGINE'S OWN non-test source calls a `zzop_parser_*` function. Not the
// parser crates' internals — a helper called from inside a wrapped `parse_*` is already covered by
// that wrap, and counting it would inflate the number in the safe direction, which is the worse one
// for a sentence a reader trusts.
const dirs = ["crates/engine/src", "crates/facade/src", "crates/summary/src", "crates/core/src"];
let raw = "";
try {
  raw = execFileSync(
    "git",
    ["grep", "-n", "zzop_parser_[a-z0-9_]*::[a-z_]", "--", ...dirs],
    { encoding: "utf8", maxBuffer: 1 << 28 }
  );
} catch (e) {
  // git grep exits 1 on no match; anything else is a broken needle, not an empty tree.
  if (e.status !== 1) throw e;
}

const CALL = /zzop_parser_([a-z0-9_]+)::([a-z_][A-Za-z0-9_]*)\s*\(/;
const sites = [];
for (const line of raw.split(/\r?\n/)) {
  const m = line.match(/^([^:]+):(\d+):(.*)$/);
  if (!m) continue;
  const [, file, no, text] = m;
  if (/^\s*(\/\/|\*|\/\*)/.test(text)) continue; // a mention in prose is not a call
  const c = text.match(CALL);
  if (!c) continue;
  sites.push({ file, line: Number(no), crate: c[1], fn: c[2] });
}

// `catch_unwind(...)` spans, by paren balance from the keyword's own line. Paren balance rather than a
// line test because two call sites in the SAME file can straddle the span — `parsers.rs` has exactly
// that shape, and a filename test would call `lexical_loc`'s call wrapped.
const spanCache = new Map();
function spans(file) {
  if (spanCache.has(file)) return spanCache.get(file);
  const L = fs.readFileSync(file, "utf8").split("\n");
  const out = [];
  for (let i = 0; i < L.length; i++) {
    if (!L[i].includes("catch_unwind")) continue;
    if (/^\s*(\/\/|\*)/.test(L[i])) continue;
    let depth = 0, started = false, end = i;
    for (let k = i; k < L.length; k++) {
      for (const ch of L[k]) {
        if (ch === "(") { depth += 1; started = true; }
        else if (ch === ")") depth -= 1;
      }
      end = k;
      if (started && depth <= 0) break;
    }
    out.push([i + 1, end + 1]);
  }
  spanCache.set(file, out);
  return out;
}

const wrapped = sites.filter((s) => spans(s.file).some(([a, b]) => s.line >= a && s.line <= b));

// ── Population floor, before any verdict ─────────────────────────────────────────────────────────
// A census that saw nothing is a broken needle, and it reads exactly like a clean tree: zero drift,
// exit 0. The floor is derived from the thing that cannot be zero in this repo rather than from a
// remembered total — eight parser crates ship, and the engine calls into more than one of them.
let crates = 0;
try {
  crates = execFileSync("git", ["ls-files", "--", "parser/parser-*/Cargo.toml"], { encoding: "utf8" })
    .trim().split("\n").filter(Boolean).length;
} catch (e) {
  if (e.status !== 1) throw e;
}
const seen = new Set(sites.map((s) => s.crate)).size;
if (crates === 0 || sites.length === 0 || seen < 2) {
  console.error(
    `check-parser-unwind-census: REFUSING TO JUDGE — the census saw ${sites.length} call site(s) ` +
    `across ${seen} parser crate(s), with ${crates} crate(s) shipping. A zero or near-zero here is a ` +
    `broken needle, never a fact about the tree: this engine cannot call fewer than two frontends.`
  );
  process.exit(1);
}

// ── The derivation check ─────────────────────────────────────────────────────────────────────────
const doc = fs.readFileSync(owner, "utf8");
const claim = doc.match(/\*\*(\d+) of (\d+)\*\* are inside a `catch_unwind`/);
if (!claim) {
  console.error(
    `check-parser-unwind-census: ${owner} no longer states the count in the shape this guard reads ` +
    `(\`**<w> of <t>** are inside a \\\`catch_unwind\\\`\`). Either the sentence moved — then move this ` +
    `needle with it — or it lost its number, in which case delete this guard rather than leave it green.`
  );
  process.exit(1);
}
const [, claimedW, claimedT] = claim.map(Number);

if (claimedW !== wrapped.length || claimedT !== sites.length) {
  console.error(
    `check-parser-unwind-census: ${owner} says ${claimedW} of ${claimedT} engine-side parser call ` +
    `sites are inside a catch_unwind; the tree says ${wrapped.length} of ${sites.length}.`
  );
  console.error("  Wrapped sites the census found:");
  for (const s of wrapped) console.error(`    ${s.file}:${s.line}  ${s.crate}::${s.fn}`);
  console.error(
    "  Recount and update the sentence together. Raising the wrapped count is a real improvement and " +
    "should be stated; lowering it silently is how the original false sentence survived four releases."
  );
  process.exit(1);
}

const byCrate = {};
for (const s of sites) byCrate[s.crate] = (byCrate[s.crate] || 0) + 1;
const breakdown = Object.keys(byCrate).sort().map((c) => `${c} ${byCrate[c]}`).join(" · ");
console.log(
  `check-parser-unwind-census: OK (${wrapped.length} of ${sites.length} engine-side parser call ` +
  `sites inside a catch_unwind, matching ${owner}; ${seen} of ${crates} frontends called — ${breakdown})`
);
NODE
