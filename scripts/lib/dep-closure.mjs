// Which workspace crates can reach a given external crate through NORMAL dependency edges.
//
// ## Why this exists (2026-09-08, review ledger V122)
//
// The four parser-isolation guards answer their question by grepping Cargo.toml lines. That sees a
// DIRECT declaration and nothing else, so `zzop-config` could depend on `zzop-engine`, inherit swc and
// eight parser crates transitively, and every one of those guards stayed green. The claim "the parsers
// are isolated" was true only of direct edges, and nobody was measuring the sentence people actually
// read it as.
//
// `cargo metadata` knows the resolved graph, which is the only thing that can answer transitively.
//
// ## Normal edges only, deliberately
//
// `dep_kinds` marks each edge normal / dev / build. Dev edges are excluded because they do not enter a
// crate's shipped closure — a test may link the whole engine without that saying anything about what
// the crate ships. That distinction is the entire remedy V122 landed: `zzop-config`'s engine dependency
// became a dev-dependency, and this script is what can tell the difference.
//
// Output: one `<external> <workspace-crate>` line per reaching crate, sorted, for a caller to diff
// against a baseline. Nothing here decides what is ALLOWED — the baseline file holds that, so a change
// to the set is a reviewable diff rather than a judgement made in a script.

import { execFileSync } from "child_process";

const TARGETS = process.argv.slice(2);
if (TARGETS.length === 0) {
  console.error("usage: dep-closure.mjs <external-crate-name>...");
  process.exit(2);
}

let meta;
try {
  meta = JSON.parse(
    execFileSync("cargo", ["metadata", "--format-version", "1", "--all-features"], {
      encoding: "utf8",
      maxBuffer: 256 * 1024 * 1024,
    })
  );
} catch (e) {
  console.error("dep-closure: `cargo metadata` failed -- " + (e.message || e));
  process.exit(1);
}

const nameById = new Map(meta.packages.map((p) => [p.id, p.name]));
const members = new Set(meta.workspace_members);

// Normal-edge adjacency over package ids.
const normalDeps = new Map();
for (const node of meta.resolve.nodes) {
  const out = [];
  for (const d of node.deps) {
    // `dep_kinds` is empty on very old cargo; treat that as normal rather than silently dropping the
    // edge, because dropping edges is the failure this script exists to remove.
    const kinds = d.dep_kinds && d.dep_kinds.length ? d.dep_kinds : [{ kind: null }];
    if (kinds.some((k) => k.kind === null || k.kind === undefined)) out.push(d.pkg);
  }
  normalDeps.set(node.id, out);
}

if (normalDeps.size === 0) {
  console.error("dep-closure: the resolve graph is empty -- refusing to report a clean set from it.");
  process.exit(1);
}

const rows = [];
for (const target of TARGETS) {
  let sawTarget = false;
  for (const id of meta.resolve.nodes.map((n) => n.id)) {
    if (nameById.get(id) === target) sawTarget = true;
  }
  if (!sawTarget) {
    console.error(
      `dep-closure: '${target}' is in no resolved package -- an absent subject reads identically to ` +
        `a clean one, so this is a failure and not an empty section.`
    );
    process.exit(1);
  }
  for (const start of members) {
    const seen = new Set([start]);
    const stack = [start];
    let reaches = false;
    while (stack.length) {
      const cur = stack.pop();
      if (nameById.get(cur) === target && cur !== start) {
        reaches = true;
        break;
      }
      for (const next of normalDeps.get(cur) || []) {
        if (!seen.has(next)) {
          seen.add(next);
          stack.push(next);
        }
      }
    }
    if (reaches) rows.push(`${target} ${nameById.get(start)}`);
  }
}
rows.sort();
for (const r of rows) console.log(r);
