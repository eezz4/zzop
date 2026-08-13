#!/usr/bin/env bash
# check-pack-id-values — every pack id written inside a `packs.disabled` / `packs.only` array in a
# shipped surface must name a pack this build actually loads.
#
# ## The hole this closes, declared by the guard that could not close it
#
# `scripts/check-docs-rule-ids.sh` validates RULE ids in config examples and says so in its own header:
# "Known-uncovered shapes: `packs.disabled` entries (bare pack ids by design — a different, pack-id-only
# key space; validating it against the pack-id set would be a separate check, not this drift class)."
# It was right about the key space and right that it needed a separate check. This is that check.
#
# The consequence of an unvalidated entry is not symmetric between the two keys, and that asymmetry is
# why this is worth a guard rather than a lint:
#   - `packs.disabled: ["typo"]` disables nothing and earns a config warning. Under-suppression, loud.
#   - `packs.only: ["typo"]` names no loaded pack, and `is_pack_enabled` admits a pack only when a
#     NON-EMPTY allowlist names it — so an all-typo allowlist turns EVERY DSL rule off while the report
#     still lists eleven `packsLoaded`. Silent, and the opposite sign from its three siblings (ae9511b).
# `zzop.config.jsonc`'s own comment records that its first draft named five packs that do not exist, so
# this is a defect this repo has already shipped once in the file it dogfoods with.
#
# ## Subject set
#
# Tracked `*.md`, `*.html`, `*.jsonc`, `*.json` — every surface a reader can copy from, including the
# repo's own committed config and the config-parity fixture. Paths with a `test`/`tests` segment are
# subtracted, with the reason: `crates/config/src/mapper/tests/options.rs` and its siblings write
# deliberately bogus ids to prove the unknown-id warning fires, so admitting them would flag the tests
# that prove the runtime catches this — and the allowlist that followed would end the guard.
# `.rs` is outside the set entirely: the one Rust surface that carries a pack-id list is the shipped
# starter template, and `crates/config/src/template_tests.rs` already pins its prose list against
# `BUNDLED_PACK_SOURCES` in BOTH directions (2026-08-11).
#
# ## Deliberately NOT checked: whether an enumerating list is COMPLETE
#
# `zzop.config.jsonc`'s `packs.disabled` names all eleven packs under a comment claiming "Every shipped
# DSL pack" — a completeness claim that a twelfth pack would silently falsify. This guard checks
# MEMBERSHIP only. Closing the completeness half means deciding, per array, whether it is a choice or
# an enumeration, and the only evidence is the prose above it; anchoring a machine check on a sentence
# means the check goes quiet the day the sentence is reworded, which is a worse failure than the gap.
# The template's list is the case where completeness IS pinned, and it is pinned by a unit test that
# can read the claim's subject directly (`BUNDLED_PACK_SOURCES`) rather than inferring it from prose.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v node >/dev/null 2>&1; then
  echo "check-pack-id-values: \`node\` not found. This guard parses pack JSON to derive the id set," >&2
  echo "  so it cannot degrade to grep. Install node; skipping would report clean while checking" >&2
  echo "  nothing." >&2
  exit 1
fi

node --input-type=module -e '
import fs from "fs";
import path from "path";
import { execSync } from "child_process";

// The pack id set, read from the directory `crates/config/build.rs` compiles into the binary — never
// a hand list, so a pack added or exported changes what this guard demands on the very next run.
const packIds = new Set();
const walk = (d) => {
  for (const e of fs.readdirSync(d, { withFileTypes: true })) {
    const p = path.join(d, e.name);
    if (e.isDirectory()) { walk(p); continue; }
    if (!e.name.endsWith(".json")) continue;
    const pack = JSON.parse(fs.readFileSync(p, "utf8"));
    if (pack.id && Array.isArray(pack.rules)) packIds.add(pack.id);
  }
};
walk("rules/dsl");

if (packIds.size < 5) {
  console.error("check-pack-id-values: derived only " + packIds.size + " pack id(s) from rules/dsl — the");
  console.error("  enumeration broke. Every entry would read as unknown, or nothing would be judged.");
  process.exit(1);
}

const files = execSync("git ls-files \"*.md\" \"*.html\" \"*.jsonc\" \"*.json\"", { encoding: "utf8" })
  .split("\n")
  .filter((f) => f && !f.startsWith("corpus/") && !/(^|\/)tests?(\/|$)/.test(f));

const offenders = [];
const seen = new Set();
let sites = 0;

for (const file of files) {
  let text;
  try { text = fs.readFileSync(file, "utf8"); } catch { continue; }
  // An array counts only when it sits under a `"packs"` object — `disabled`/`only` are ordinary words
  // and appear as keys in other dialects. The window is generous rather than exact (a JSONC object
  // with comments is not worth a parser here); overlapping windows are deduped by match offset, so a
  // second `"packs"` nearby cannot double-report the same array.
  for (const pm of text.matchAll(/"packs"/g)) {
    const window = text.slice(pm.index, pm.index + 2000);
    for (const m of window.matchAll(/"(disabled|only)"\s*:\s*\[([^\]]*)\]/g)) {
      const at = pm.index + m.index;
      if (seen.has(file + ":" + at)) continue;
      seen.add(file + ":" + at);
      sites++;
      const line = text.slice(0, at).split("\n").length;
      for (const v of m[2].matchAll(/"([^"]+)"/g)) {
        if (packIds.has(v[1])) continue;
        offenders.push(
          file + ":" + line + '"'"'  packs.'"'"' + m[1] + '"'"' names "'"'"' + v[1] + '"'"'", which is not a loaded pack id'"'"' +
          (m[1] === "only"
            ? " — and an allowlist naming no loaded pack turns EVERY DSL rule off while packsLoaded still reports them"
            : " — so it disables nothing")
        );
      }
    }
  }
}

// Non-emptiness floor: the `"packs"`-anchored window is what finds the arrays, and a markup or
// formatting change that breaks it would leave this guard green over a tree it never read.
if (sites < 4) {
  console.error("check-pack-id-values: found only " + sites + " packs.disabled/only array(s) (expected at " +
    "least 4; 7 on 2026-08-12). The `\"packs\"`-anchored extraction stopped matching — a broken guard");
  console.error("  reporting clean, not a repo that stopped documenting the pack gate.");
  process.exit(1);
}

if (offenders.length) {
  console.error("check-pack-id-values: a shipped surface names a pack id this build does not load:");
  for (const o of offenders) console.error("  " + o);
  console.error("");
  console.error("  The loaded pack ids are the `id` of each rules/dsl/**/*.json (`ls rules/dsl/`), and any");
  console.error("  run'"'"'s own `packsLoaded[]` is the honest answer for a shipped binary.");
  process.exit(1);
}

console.log("check-pack-id-values: OK (" + sites + " packs.disabled/only array(s) across " + files.length +
  " surfaces, " + packIds.size + " pack ids)");
'
