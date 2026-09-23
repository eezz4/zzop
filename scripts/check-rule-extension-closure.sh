#!/usr/bin/env bash
# check-rule-extension-closure.sh — a DSL rule that reads `.js` must also read `.jsx`, `.mjs` and
# `.cjs`; one that reads `.ts` must also read `.tsx`, `.mts` and `.cts`.
#
# ## The defect this exists for (2026-09-09, review ledger V133)
#
# Every rule carries a hand-written `file_pattern` regex listing extensions. The dispatch table sends
# eight extensions to the TypeScript frontend; the rule patterns list whichever subset someone typed.
# Measured the day this guard was written: 61 of 118 rules admitted one member of a same-syntax family
# and not the others. The single most common pattern -- `(?i)\.(ts|tsx|js|mjs|cjs)$`, carried by 86
# rules across 5 packs -- silently skipped every `.jsx` and every `.mts`/`.cts` file in a repository.
#
# The failure is SILENT, which is what makes it worth a guard: a React app's `.jsx` components simply
# were not read by `db/update-delete-no-where`, `sql/delete-no-where` or `reliability/fetch-no-timeout`,
# and the reply said what it always says. Nothing reported a gap, because nothing was looking.
#
# ## Why the labelled corpus could not catch it
#
# `cases/trees` contains no `.jsx`, `.mts`, `.cts`, `.mjs` or `.cjs` file at all. The detection gate
# scores 258/258 with the gap present and with it closed -- the population that would show the
# difference is absent (the same shape as review ledger V121). A guard is the instrument here, not a
# case.
#
# ## What this guard does NOT require, and why
#
#   - `.ts` does NOT imply `.js`. A TypeScript-only rule is a real choice and two rules make it.
#   - `.py` does NOT imply `.pyi`. A `.pyi` is a type stub with no runtime code, so a runtime rule
#     cannot fire in one. Dispatch sends `.pyi` to Python and that stays correct.
#   - A RULE may declare that it deliberately skips a family member (`FAMILY_EXCEPTIONS`). Today
#     exactly one does: `console-in-be` skips `tsx`/`jsx`. See instance (4) below for why that is a
#     decision rather than a gap, and why this channel had to exist before the guard was trustworthy.
#
# So this checks CLOSURE WITHIN a syntax family, never membership across families. The families live
# in scripts/lib/rule-extension-closure.mjs and are the only place to change them.
#
# ## The traps this guard's own implementation hit -- twice, the same shape
#
# (1) Rust's `regex` accepts a leading `(?i)`; JS `RegExp` throws on it. A first draft wrapped the test
# in `try/catch` returning `false`, which reported zero admitting rules for every extension -- a result
# indistinguishable from "no gaps".
#
# (2) The fix for (1) still asked `re.test("x." + ext)`, and that synthetic subject cannot reach a
# PATH-SCOPED pattern: `(^|/)(migrations?|...)/.*\.(sql|ts|...)$` never matches a bare `x.ts`. Four
# shipped patterns are path-scoped, and one of them -- `sql/destructive-migration` -- was a REAL gap of
# exactly the kind this guard exists for. It shipped in the same commit that created the guard, and the
# guard said `clean` (review ledger V138). The hole was in the INPUT, not in an exception handler.
#
# Both times the failure spelled itself the same way: "I read nothing" printed as "there is nothing".
# So the mechanism no longer synthesises a subject -- it reads each pattern's own extension vocabulary
# and THROWS when a pattern yields none -- and the SELFTEST below runs first, on fixed patterns whose
# answers are written down here, so a mechanism that has gone blind cannot reach the real population.
#
# (3) 2026-09-12, review ledger V166 -- the SAME sentence a third time, and this time about the
# POPULATION rather than the needle. Everything above proves the mechanism; nothing above proved there
# was anything to run it on. `collectRules()` defaulted to `rules/dsl` alone, and the `gaps.length === 0`
# branch printed `clean` without ever asking how many rules it had read. Emptying `rules/dsl` produced
# `clean (0 rules across 0 packs)`, exit 0.
#
# Two repairs, and the first one found real defects immediately:
#
#   - POPULATION. `examples/packs/*.json` is in this repo's compatibility surface list
#     (`VERSIONING.md`), so those rule ids are frozen and a user who copies `typescript-lint.json`
#     inherits its gaps. Eight other guards already read that directory; the one guard whose entire
#     subject is `file_pattern` did not. Adding it surfaced 26 rules with gaps of exactly the kind
#     this guard exists for -- every one of them invisible for as long as the guard has existed.
#     All 26 were widened in the same commit.
#   - FLOOR. Per-root, because one root going empty (a rename, a moved directory) is the realistic
#     shape and a still-non-zero total would hide it.
#
# The lesson is narrower than "add a floor": a canary proves the NEEDLE, never the HAYSTACK. This
# guard had two needle selftests and no haystack check, and the third instance of its own recurring
# failure walked in through the gap between them.
#
# (4) 2026-09-13, review ledger V177 -- and this one the guard CAUSED. Instance (3) widened 26
# patterns to close the gaps the new population exposed. Twenty-five were real. The twenty-sixth,
# `console-in-be`, is path-scoped to backend-role directories and its message promises "backend-path
# source" -- its pattern excluded BOTH `tsx` and `jsx`, which is a consistent choice, not a missing
# case. This guard had nowhere to SAY that, so it read the choice as a gap and the repair batch
# widened it. Measured: a Remix-style `app/routes/index.tsx` React component went 0 findings -> 1.
#
# The lesson is not "be careful with batch repairs". It is that a guard with no channel for a
# legitimate exception does not report "I cannot tell" -- it reports a GAP, and a gap invites a fix.
# The exception list is therefore a contract (each entry owes a `why`) and `assertExceptionsAreLive`
# makes an entry that stopped being true RED, so it cannot quietly absolve the next rule.
#
# ⚠ This file contains a literal NUL byte (review ledger V183), so `git diff` reports it as binary
# and edits here are not reviewable as text. That is tracked separately; do not add more.
#
# No deps beyond node. Exit 1 on any gap, listing rule id, pack file, and the missing extensions.
set -euo pipefail

cd "$(dirname "$0")/.."

node --input-type=module -e '
import {
  admittedExtensions,
  assertExceptionsAreLive,
  collectRules,
  findGaps,
  RULE_PACK_ROOTS,
  SAME_SYNTAX_FAMILIES,
} from "./scripts/lib/rule-extension-closure.mjs";

// Selftest. Each case is a shape that shipped in this repo, with the answer written down beside it.
// The path-scoped one is here because its predecessor silently answered "nothing" for that shape.
const SELFTEST = [
  ["(?i)\\\\.(ts|tsx|mts|cts)$", ["ts", "tsx", "mts", "cts"]],
  ["(?i)(^|/)(migrations?|migrate|alembic/versions?)/.*\\\\.(sql|ts|jsx)$", ["sql", "ts", "jsx"]],
  ["(?:^|/)api/.+\\\\.tsx?$", ["ts", "tsx"]],
  ["(?i)\\\\.(properties|ya?ml|jsonc?)$", ["properties", "yml", "yaml", "json", "jsonc"]],
];
for (const [pattern, expected] of SELFTEST) {
  const got = [...admittedExtensions(pattern)].sort();
  const want = [...expected].sort();
  if (got.join(",") !== want.join(",")) {
    console.error(`rule extension closure SELFTEST failed for ${pattern}`);
    console.error(`  expected: ${want.join(", ")}`);
    console.error(`  got:      ${got.join(", ")}`);
    console.error("The mechanism cannot read a shape it must read; every result below would be a lie.");
    process.exit(1);
  }
}
// And a pattern with no readable extension must THROW, never return an empty set.
let threw = false;
try {
  admittedExtensions("(?i)(^|/)Dockerfile$");
} catch {
  threw = true;
}
if (!threw) {
  console.error("rule extension closure SELFTEST failed: an unreadable pattern did not throw.");
  console.error("Silently reporting it as `admits nothing` is how V138 shipped.");
  process.exit(1);
}

// POPULATION FLOOR. Everything above proves the NEEDLE; nothing above proves the HAYSTACK, and a
// run that read zero packs printed `clean (0 rules across 0 packs)` and exited 0 (review ledger
// V166). That is the third spelling of this guard`s own recurring failure -- "I read nothing"
// printed as "there is nothing" -- and the first two got a selftest each while this one had no
// check at all. Per-root, because one root going empty is the realistic shape (a rename, a moved
// directory), and a total that is still non-zero would hide it.
const rules = [];
for (const root of RULE_PACK_ROOTS) {
  const found = collectRules([root]);
  if (found.length === 0) {
    console.error(`rule extension closure: read ZERO rules from ${root}.`);
    console.error("  That is a moved/renamed root or a broken walk, never a pack directory with no");
    console.error("  rules. Reporting it as `clean` is exactly the defect this guard exists for.");
    process.exit(1);
  }
  rules.push(...found);
}
// Stale-exemption assert (review ledger V177). A rule-level family exception that no longer
// describes reality absolves whatever takes the same shape next -- the same discipline
// check-mcp-tools-listed.sh runs on its own exempted line.
assertExceptionsAreLive(rules);

const gaps = findGaps(rules);

if (gaps.length === 0) {
  const families = Object.entries(SAME_SYNTAX_FAMILIES)
    .map(([b, v]) => `${b} -> ${v.join("/")}`)
    .join(", ");
  console.log(
    `rule extension closure: clean (${rules.length} rules across ${new Set(rules.map((r) => r.file)).size} packs; families: ${families}).`
  );
  process.exit(0);
}

const byRule = new Map();
for (const g of gaps) {
  // \u0000 as the separator, ESCAPED rather than a literal byte: a literal NUL makes git read
  // this whole script as binary, and every repair to it then happens outside `git diff` (review
  // ledger V183). The character is deliberate -- neither a path nor a rule id can contain it, so
  // `file + sep + id` is unambiguous; only the spelling was.
  const k = `${g.file}\u0000${g.id}`;
  if (!byRule.has(k)) byRule.set(k, { ...g, missing: new Set() });
  g.missing.forEach((m) => byRule.get(k).missing.add(m));
}

console.error("rule extension closure: rules that read one extension of a family but not its siblings:");
for (const g of byRule.values()) {
  console.error(`  ${g.id}  (${g.file})`);
  console.error(`      pattern: ${g.filePattern}`);
  console.error(`      missing: ${[...g.missing].join(", ")}`);
}
console.error("");
console.error(`${byRule.size} rule(s) of ${rules.length}. A pattern that reads .js and not .jsx does not`);
console.error("report a gap -- it reports nothing, on files it never opened. Widen the file_pattern;");
console.error("the families this enforces are in scripts/lib/rule-extension-closure.mjs.");
process.exit(1);
'
