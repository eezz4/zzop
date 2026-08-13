#!/usr/bin/env node
// Measurement instrument, not a guard: sizes the `Matcher::MethodScan` span-boundary axis.
//
// WHY THIS EXISTS. `egress/get-and-body` is a confirmed false positive on
// corpus/oss/fe-vue/src/services/api.ts: a class property's whole object literal projects as ONE
// symbol span, so `method:"GET"` on line 434 paired with a `body:` under `method:"POST"` 47 lines
// away. That is not a rule defect — it is the span boundary. The open question was never "is it
// real" but "how many rules ride on the same shape", and a large fraction of the bundle does, while
// only a handful fire on the corpus we have. A rule that is silent is not thereby
// correct; it is unmeasured. This script separates those two. The size is deliberately not written
// here — it read `60` against a bundle that had since exported five method-scan rules, and this script
// PRINTS the live figure on every run from the same `rules/dsl` walk the claim would have described.
//
// THE AXIS HAS TWO DIRECTIONS, and reading only the first undercounts it by more than half:
//   FP — two or more `patterns` mis-paired across an oversized span (get-and-body's shape).
//   FN — any `absent` veto, which is tested over that same span, so a bigger span is MORE likely
//        to find a suppressor and silence a true finding.
// Only `after_in_same_function` narrows the PAIRING window below the symbol span. `after` alone
// fixes order, `trigger_in_loop` / `require_call_kind` constrain the trigger's own site, and
// `absent` only vetoes — none of them shrink the window two patterns may pair across. See the
// field docs on `MethodScan` (crates/core/src/dsl/def/matcher/method_scan.rs) for each gate's
// contract and its degrade direction.
//
// USAGE
//   node scripts/measure/classify-method-scan-spans.mjs                  # classification only
//   node scripts/measure/classify-method-scan-spans.mjs <byrule.json>    # + fired/silent split
// The optional argument is a JSON object of `{ "<rule-id>": <count> }` for rules that fired on a
// corpus. Build it by running `zzop analyze --config <single-tree cfg> --limit 0` per tree and
// merging each reply's `findings.byRule` (keys are `pack/rule`; this script matches on the tail).
// `--limit` caps only `shown`, never `byRule`, so 0 is correct and cheapest.

import fs from 'node:fs';
import path from 'node:path';

const RULES = 'rules/dsl';
const byRule = process.argv[2]
  ? Object.fromEntries(
      Object.entries(JSON.parse(fs.readFileSync(process.argv[2], 'utf8')))
        .map(([k, v]) => [k.split('/').pop(), v]),
    )
  : null;

const rows = [];
for (const dir of fs.readdirSync(RULES)) {
  const file = path.join(RULES, dir, `${dir}.json`);
  if (!fs.existsSync(file)) continue;
  const pack = JSON.parse(fs.readFileSync(file, 'utf8'));
  for (const rule of pack.rules || []) {
    const m = rule.matcher || {};
    if (m.type !== 'method-scan') continue;
    const nPat = (m.patterns || []).length;
    const nAbsent = (m.absent || []).length;
    const cls =
      nPat >= 2
        ? m.after_in_same_function
          ? 'B-narrowed' // pairs within the innermost function span (no-op where none is projected)
          : 'A-exposed' // pairs across the whole symbol span — get-and-body's shape
        : nAbsent > 0
          ? 'C-veto-only' // cannot mis-pair, but a span-wide `absent` can over-suppress
          : 'D-immune'; // one pattern, no veto — span size cannot change the verdict
    rows.push({ pack: pack.id, id: rule.id, cls, nPat, nAbsent, hits: byRule ? byRule[rule.id] || 0 : null });
  }
}

const ORDER = { 'A-exposed': 0, 'B-narrowed': 1, 'C-veto-only': 2, 'D-immune': 3 };
rows.sort((a, b) => ORDER[a.cls] - ORDER[b.cls] || a.pack.localeCompare(b.pack) || a.id.localeCompare(b.id));

const tally = {};
for (const r of rows) {
  const key = byRule ? `${r.cls} ${r.hits > 0 ? 'FIRED' : 'silent'}` : r.cls;
  tally[key] = (tally[key] || 0) + 1;
}

console.log(`method-scan rules: ${rows.length}`);
for (const k of Object.keys(tally).sort()) console.log(`  ${k.padEnd(20)} ${tally[k]}`);
if (byRule) {
  const unverified = rows.filter((r) => r.cls === 'A-exposed' && r.hits === 0);
  console.log(`\nA-exposed AND silent (FP-shaped, unverified): ${unverified.length}`);
  for (const r of unverified) console.log(`  ${r.pack}/${r.id}`);
}
