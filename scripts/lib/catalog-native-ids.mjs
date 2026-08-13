/* The NATIVE ANALYSIS ID universe, derived from docs/rules/catalog.md's own Native-analyses table.
 *
 * ## Why this is a shared file (2026-08-13)
 *
 * check-prose-rule-ids.sh and check-runtime-text-rule-ids.sh each carried a byte-identical copy of
 * this derivation, and that copy carried TWO widening bugs at once — neither of them visible while
 * reading either guard on its own:
 *
 *   1. `catalog.indexOf("## Native analyses")` matched the catalog's INTRO PROSE at line 13, which
 *      quotes the heading BY NAME ("the generator reads the `## DSL packs` and `## Native analyses`
 *      sections and nothing else"), not the heading at line 453. The slice began near the top of the
 *      file. An `indexOf` for a heading is a substring search, and a document that talks about its
 *      own structure will always defeat it; the anchor has to be line-anchored.
 *   2. Nothing closed the section, so whatever the anchor matched ran to EOF.
 *
 * Measured on the clean tree: 211 ids, where the catalog publishes 60 native ones. The other 151 were
 * every bundled AND every exported DSL rule id, plus the 8 rows of `### Recommendation ids` — a table
 * this catalog introduces with the words "These are not rule ids".
 *
 * ## Who owns the reasoning, and why this file is not the owner of all three copies
 *
 * scripts/check-docs-rule-ids.sh hit bug 2 in its own awk and was repaired the same day; it OWNS the
 * why, including the FAILURE BIAS this file follows — a heading the extraction does not recognize must
 * NARROW the universe, never widen it, so a doc line under it goes red and a human looks. It is not a
 * third caller of this file and should not become one: it builds a different, larger universe (three
 * tables, with pack sections' rows prefixed by their `### `<pack>`` heading), in awk. Two languages,
 * two jobs. This file is the one owner of the JS side, not of all three.
 *
 * ## Why `guard` and `consequence` are parameters
 *
 * The two callers judge OPPOSITE things (see check-runtime-text-rule-ids.sh's header on why they are
 * deliberately separate files), so an abort here has a different meaning in each. A message that
 * cannot name which guard broke, and what goes wrong there, is a worse message than the two it
 * replaced — the whole reason a broken extraction aborts instead of returning an empty set.
 */
import fs from "node:fs";

const CATALOG = "docs/rules/catalog.md";

// Floor, not a pin: the exact total is machine-checked against the engine by
// crates/engine/tests/rule_contracts/'s catalog_totals_match_loaded_rule_and_analysis_counts, so
// restating 60 here would be a second copy of a number that already has an owner. This only has to
// separate "the table was read" from "the extraction stopped matching".
const FLOOR = 20;

export function nativeAnalysisIds(guard, consequence) {
  const catalog = fs.readFileSync(CATALOG, "utf8");
  const at = catalog.search(/^## Native analyses/m);
  if (at < 0) {
    console.error(guard + ": " + CATALOG + " has no `## Native analyses` heading — the native-id");
    console.error("  extraction lost its anchor and " + consequence);
    process.exit(1);
  }
  // Cut at the next heading of ANY level. A fenced code block whose content starts with `#` would cut
  // early; that is the narrowing direction, which is the safe one.
  const section = catalog.slice(at).split(/\r?\n#{1,6} /)[0];
  const ids = new Set();
  for (const m of section.matchAll(/^\| `([a-z][a-z0-9-/]*)` \|/gm)) ids.add(m[1]);
  if (ids.size < FLOOR) {
    console.error(guard + ": derived " + ids.size + " native analysis id(s) from " + CATALOG +
      " (60 on 2026-08-13, floor " + FLOOR + "). The Native-analyses table stopped matching and");
    console.error("  " + consequence);
    process.exit(1);
  }
  return ids;
}
