#!/usr/bin/env bash
# readme-result-block — the numbers README prints as "what this run reports" must be what the run
# actually reports. The subject is the reproduction block for `cases/trees/api-be`, and the prose
# above it that quotes some of the same values.
#
# ## The defect this exists for (2026-09-02, external review at `bd6e624`)
#
# README claimed fileCount 84 / total 84 / critical 5 / warning 71 / info 8 / pain 7.5. The run
# answered 88 / 89 / 8 / 72 / 9 / 7.7. Six values, and a seventh in the prose that teaches what `pain`
# is NOT ("the run below reports 84 findings, 5 of them critical"). The release commit `4c50497` had
# added four security fixtures to that tree and edited the block only PARTWAY -- 85->84 and info 9->8 --
# so the new criticals and the file count never followed. It shipped in v0.34.0 with all guards green.
#
# That was the FOURTH time this block went stale. The 2026-08-11 audit named the class B3 and fixed it
# by hand; the README's own editor note counted three and told the next editor that no guard could
# catch it, which is how a class reaches its fifth recurrence. That note now names this script instead
# (2026-09-06) -- the citation had run one way only, so the guard knew the sentence and the sentence
# did not know the guard. Editing this file means re-reading that paragraph.
#
# ## Why it covers every number in the block, not the ones that were wrong
#
# The first version of this file checked nine values -- the six that had drifted plus the prose. A
# reviewer then mutated two it did NOT read (`security/weak-crypto` 6 -> 60 and `painMeasuredWeight`
# 13.8 -> 3.8) and got a silent green. Those values are correct today, and "correct today, and
# unguarded" is the exact state this block was in through four releases. So the subject is now every
# number the block states.
#
# ## Why it runs the binary, and why that is not negotiable
#
# Every scripts/check-*.sh reads text and compares it to other text. This claim cannot be checked that
# way: the only machine that knows the number is the run. A checker that re-derived the counts from
# the fixture directory would be a second implementation of the engine and would drift on its own.
#
# ## Why this is NOT a scripts/check-*.sh
#
# The same reason `detection-benchmark` gives in .github/workflows/ci.yml: check-guards-wired.sh would
# then oblige it to run in .githooks/pre-commit on every commit, and a release build per commit is not
# a hook's budget. It lives here with the other things that must RUN the product to answer, and the
# hook invokes it the way it invokes the detection gate -- armed, after the guards loop.
#
# ## What arms it
#
# The inputs that can move the answer are files, so the trigger reads the index: README.md, anything
# under the fixture tree -- including a DELETION there, which moves the counts as surely as an
# addition does -- and anything under rules/. That third path was added 2026-09-03 after this very
# check caught a drift its own trigger had let through: demoting one rule from `critical` to
# `warning` moved this block's counts (8/72 -> 7/73) while touching neither README.md nor the fixture
# tree, so the commit that did it passed. A rule edit is a third way to move these numbers.
#
# The known remaining blind spot is crates/: an engine change moves them too, and arming on that
# would arm on nearly every commit. This is the same boundary detection-gate-if-touched.sh draws,
# and for the same reason -- CI runs unconditionally and is the backstop for it.
#
# WITH NO INDEX AT ALL -- CI, or a hand run -- it runs UNCONDITIONALLY rather than skipping, because
# "nothing is staged" outside a hook means "you asked for this", not "nothing changed". A trigger
# that reads an empty index as `no work` is how a check reports green in CI while never having run.
#
# ## What is judged
#
# Every number README states about that run, each parsed OUT OF README rather than repeated here --
# this file holds no copy of the expected values, so it cannot become the next stale copy. A pattern
# that stops matching is a FAILURE, not a silently smaller checklist.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

README="README.md"
FIXTURE_CONFIG="cases/trees/api-be/zzop.config.jsonc"
FIXTURE_TREE="cases/trees/api-be"

staged="$(git diff --cached --name-only --diff-filter=ACMRD || true)"
if [ -n "$staged" ] && ! grep -qE "^(README\.md|${FIXTURE_TREE}/|rules/)" < <(printf '%s\n' "$staged"); then
  echo "readme-result-block: not armed (README.md, ${FIXTURE_TREE}/ and rules/ are all unstaged)."
  exit 0
fi

command -v node > /dev/null || { echo "readme-result-block: node is required."; exit 1; }

# The rule packs are embedded at COMPILE time (crates/config/build.rs include_str!), so an existing
# target/release/zzop can be a binary that predates the very edit being verified. On 2026-09-03 that
# happened: 27 rules moved warning -> info, this check reused the stale binary, and it reported
# "22 claims match" over an engine that had never seen the change. A verifier that runs the product
# has to run THIS product, so the build is unconditional whenever cargo is available -- it is
# incremental and costs about a second when nothing moved.
#
# And the artifact path is READ BACK FROM CARGO rather than spelled `target/release/zzop`, for the
# reason detection-gate.sh gives at its own build: that spelling is wrong under $CARGO_TARGET_DIR,
# wrong under a `.cargo/config.toml` `build.target-dir`, and wrong on Windows without `.exe` -- three
# ways to build the current tree into one directory and then score whatever sits at another, which is
# the SAME defect one layer along. The first version of this fix built unconditionally and still
# hardcoded the path; an external reviewer caught that on the day it landed.
#
# With no cargo (a docs-only clone) an already-built binary is used, and BOTH the banner and the
# success line say so -- the two cases must not print the same sentence, or a log cannot tell a
# verified run from a possibly-stale one.
BIN=""
FRESH=0
if command -v cargo > /dev/null; then
  BIN="$(cargo build -p zzop-cli-bin --release --message-format=json-render-diagnostics | node -e '
    let exe = null;
    for (const l of require("fs").readFileSync(0, "utf8").split(/\r?\n/)) {
      if (!l.trim()) continue;
      let m;
      try { m = JSON.parse(l); } catch { continue; }
      if (m.reason === "compiler-artifact" && m.executable && ((m.target && m.target.kind) || []).includes("bin")) exe = m.executable;
    }
    if (!exe) {
      console.error("readme-result-block: cargo emitted NO executable artifact for zzop-cli-bin.");
      console.error("  Refusing to verify: there is no binary this run could honestly claim to have measured.");
      process.exit(1);
    }
    process.stdout.write(exe);
  ')"
  [ -n "$BIN" ] || { echo "readme-result-block: release build FAILED -- cannot verify the block."; exit 1; }
  FRESH=1
else
  echo "readme-result-block: no cargo on PATH -- falling back to the binary already in target/release,"
  echo "  which may predate the edit you are verifying. This run cannot prove otherwise."
  for candidate in target/release/zzop target/release/zzop.exe; do
    [ -x "$candidate" ] && { BIN="$candidate"; break; }
  done
fi
if [ -z "$BIN" ] || [ ! -x "$BIN" ]; then
  echo "readme-result-block: no usable zzop binary at [$BIN], and none in target/release."; exit 1
fi

REPLY="$(mktemp)"
trap 'rm -f "$REPLY"' EXIT
"$BIN" analyze --config "$FIXTURE_CONFIG" > "$REPLY" 2>/dev/null || {
  echo "readme-result-block: the fixture run FAILED -- $BIN analyze --config $FIXTURE_CONFIG"; exit 1; }

RRB_FRESH="$FRESH" node - "$README" "$REPLY" <<'NODE'
const fs = require('fs');
const [, , readmePath, replyPath] = process.argv;
const readme = fs.readFileSync(readmePath, 'utf8');
const reply = JSON.parse(fs.readFileSync(replyPath, 'utf8'));

// Every expected value is READ OUT OF THE README. This file stores none of them.
const claim = (re, what) => {
  const m = readme.match(re);
  if (!m) {
    console.error(`readme-result-block: could not find the claim for ${what} in ${readmePath}.`);
    console.error(`  pattern: ${re}`);
    console.error('  If the block was reshaped on purpose, reshape this verifier with it -- a pattern');
    console.error('  that no longer matches must FAIL, not quietly leave one value unchecked.');
    process.exit(1);
  }
  return m;
};
const num = (re, what) => Number(claim(re, what)[1]);

const arch = reply.architecture ?? {};
const axis = (name) => (arch.painByAxis ?? []).find((a) => a.axis === name) ?? {};
const sev = claim(/"bySeverity":\s+\{ "critical": (\d+), "warning": (\d+), "info": (\d+) \}/, 'bySeverity');
const byRule = claim(/"byRule":\s+\{ "security\/weak-crypto": (\d+), "db\/unawaited-write": (\d+) \}/, 'byRule');
const axDefect = claim(/\{ "axis": "defect",\s+"pain": ([\d.]+), "totalWeight": ([\d.]+) \}/, 'defect axis');
const axOpinion = claim(/\{ "axis": "opinion", "pain": ([\d.]+), "totalWeight": ([\d.]+) \}/, 'opinion axis');
const axHistory = claim(/\{ "axis": "history", "pain": ([\d.]+), "totalWeight": ([\d.]+) \}/, 'history axis');

const checks = [
  // the prose two paragraphs above the block, which quotes two of the same numbers
  ['prose: findings',        num(/the run below reports (\d+)/, 'prose findings count'),          reply.findings.total],
  ['prose: criticals',       num(/findings, (\d+) of them critical/, 'prose critical count'),     reply.findings.bySeverity.critical ?? 0],
  ['prose: defect pain',     num(/while its `defect` pain is `([\d.]+)`/, 'prose defect pain'),   axis('defect').pain],
  // the block itself
  ['fileCount',              num(/"fileCount": (\d+)/, 'fileCount'),                              reply.fileCount],
  ['findings.total',         num(/"total": (\d+)/, 'findings.total'),                             reply.findings.total],
  ['bySeverity.critical',    Number(sev[1]),                                                      reply.findings.bySeverity.critical ?? 0],
  ['bySeverity.warning',     Number(sev[2]),                                                      reply.findings.bySeverity.warning ?? 0],
  ['bySeverity.info',        Number(sev[3]),                                                      reply.findings.bySeverity.info ?? 0],
  ['byRule weak-crypto',     Number(byRule[1]),                                                   reply.findings.byRule?.['security/weak-crypto']],
  ['byRule unawaited-write', Number(byRule[2]),                                                   reply.findings.byRule?.['db/unawaited-write']],
  ['shown length',           num(/"shown":\s+\[ \/\* (\d+) here/, 'shown length'),                (reply.findings.shown ?? []).length],
  ['pain',                   num(/"pain": ([\d.]+), "painMeasuredWeight"/, 'pain'),               arch.pain],
  ['painMeasuredWeight',     num(/"painMeasuredWeight": ([\d.]+)/, 'painMeasuredWeight'),         arch.painMeasuredWeight],
  ['painTotalWeight',        num(/"painTotalWeight": ([\d.]+)/, 'painTotalWeight'),               arch.painTotalWeight],
  ['axis defect pain',       Number(axDefect[1]),                                                 axis('defect').pain],
  ['axis defect weight',     Number(axDefect[2]),                                                 axis('defect').totalWeight],
  ['axis opinion pain',      Number(axOpinion[1]),                                                axis('opinion').pain],
  ['axis opinion weight',    Number(axOpinion[2]),                                                axis('opinion').totalWeight],
  ['axis history pain',      Number(axHistory[1]),                                                axis('history').pain],
  ['axis history weight',    Number(axHistory[2]),                                                axis('history').totalWeight],
];

// The two structural claims in the block: both are shapes, not numbers.
claim(/"topRecommendation": null/, 'topRecommendation null');
claim(/"criticalTop": \[\]/, 'criticalTop empty');
const structural = [];
if (arch.topRecommendation !== null) structural.push(`topRecommendation: README says null, run says ${JSON.stringify(arch.topRecommendation)}`);
if (!Array.isArray(arch.criticalTop) || arch.criticalTop.length !== 0) structural.push(`criticalTop: README says [], run says ${JSON.stringify(arch.criticalTop)}`);

const drift = checks.filter(([, claimed, measured]) => claimed !== measured);
if (drift.length === 0 && structural.length === 0) {
  const built = process.env.RRB_FRESH === "1";
  console.log(`readme-result-block: ${checks.length + 2} claims match the run` + (built ? " (binary built from this tree).": " (binary NOT rebuilt -- no cargo; freshness unproven)."));
  process.exit(0);
}
console.error('readme-result-block: the README block does not match what the run reports.');
console.error('');
for (const [name, claimed, measured] of drift) {
  console.error(`  ${name.padEnd(22)} README says ${String(claimed).padEnd(8)} run says ${measured}`);
}
for (const s of structural) console.error(`  ${s}`);
console.error('');
console.error('  Reproduce:  ./target/release/zzop analyze --config cases/trees/api-be/zzop.config.jsonc');
console.error('  Fix the README to what the run says. Do NOT fix the run to what the README says --');
console.error('  the run owns these numbers, and this block is a copy that has already gone stale');
console.error('  four times. If you changed the fixture tree on purpose, this is that reminder.');
process.exit(1);
NODE
