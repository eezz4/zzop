#!/usr/bin/env node
// What `ruleCaveats` costs and what it covers — the recount behind every percentage this repo
// publishes about that key.
//
// ## Why this file exists
// The feature shipped with three ranges in its prose (the messages' cost, the caveats' cost, the
// share of the screen covered) and NO command that produced them. This repo's standing rule is that
// a number without its measuring stick is a finding even while the number is correct, and a release
// audit caught all three at once. It also caught something a command would have prevented: two
// different figures for the same quantity, published in the same batch, because the first was taken
// at `--limit 1000` and the second at the default window and neither said which.
//
// So this script fixes the metric as well as deriving it:
//   * DENOMINATOR = the whole reply, pretty-printed, exactly as the CLI writes it.
//   * CAVEAT COST = `findings.ruleCaveats` AND `findings.ruleCaveatsMeaning` together, because a
//     table without its legend is not a shippable state — `Folded::publish` writes both or neither.
//   * MESSAGE COST = what the same screen would cost if every DISTINCT shown rule's full message
//     rode inline instead, which is the alternative the caveat table was chosen over. Rules whose
//     message this binary cannot rebuild (natives, external packs) already carry theirs inline, so
//     they are excluded from the hypothetical — counting them would charge the alternative for
//     bytes the reply pays either way.
//   * COVERAGE = shown findings whose `ruleId` is in the table, over shown findings.
//   * WINDOW = whatever the config and flags give; the window is PRINTED, never assumed.
//
// ## Usage
//   node scripts/measure/caveat-cost.mjs <config.jsonc> [more configs...] [-- extra zzop flags]
//   node scripts/measure/caveat-cost.mjs corpus/audit/{immich,nocodb,cal.com}/zzop.config.jsonc
//
// Prints one row per tree plus the range across them, which is the form the prose quotes.
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";

const argv = process.argv.slice(2);
const sep = argv.indexOf("--");
const configs = (sep >= 0 ? argv.slice(0, sep) : argv).filter(Boolean);
const extra = sep >= 0 ? argv.slice(sep + 1) : [];

const BIN = `./target/release/zzop${process.platform === "win32" ? ".exe" : ""}`;
if (!existsSync(BIN)) {
  console.error(`caveat-cost: no binary at ${BIN} — build it first (cargo build --release -p zzop-cli-bin).`);
  console.error("  A cost measured with a stale binary describes a reply that is not the one shipping.");
  process.exit(1);
}
if (configs.length === 0) {
  console.error("caveat-cost: name at least one config. See this file's header for the usual invocation.");
  process.exit(1);
}

const rows = [];
for (const config of configs) {
  if (!existsSync(config)) {
    console.error(`caveat-cost: no config at ${config}`);
    process.exit(1);
  }
  const raw = execFileSync(BIN, ["analyze", "--config", config, ...extra], {
    encoding: "utf8",
    maxBuffer: 512 * 1024 * 1024,
  });
  const reply = JSON.parse(raw);
  const f = reply.findings;
  if (!f) {
    console.error(`caveat-cost: ${config} produced no \`findings\` block.`);
    process.exit(1);
  }
  const shown = f.shown ?? [];
  // Floor before any verdict: a screen with no findings makes every ratio below 0/0, and a row of
  // zeroes reads like a measured result rather than an absent one.
  if (shown.length === 0) {
    console.error(`caveat-cost: ${config} shows 0 findings — nothing to measure, refusing to print a row.`);
    process.exit(1);
  }

  const table = f.ruleCaveats ?? {};
  const legend = f.ruleCaveatsMeaning ?? "";
  const caveatBytes =
    Object.keys(table).length === 0
      ? 0
      : Buffer.byteLength(JSON.stringify(table)) + Buffer.byteLength(JSON.stringify(legend));
  const total = Buffer.byteLength(raw);

  // The alternative: one full message per distinct rule that is currently POINTED AWAY. A finding
  // without `messageBy` already carries its text, so its rule costs nothing extra either way.
  const pointed = new Map();
  for (const x of shown) {
    if (!x.messageBy || pointed.has(x.ruleId)) continue;
    pointed.set(x.ruleId, null);
  }
  let messageBytes = 0;
  for (const ruleId of pointed.keys()) {
    const text = execFileSync(BIN, ["explain", ruleId], { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
    const line = text.split("\n").find((l) => l.startsWith("message: "));
    if (!line) continue;
    messageBytes += Buffer.byteLength(JSON.stringify(line.slice("message: ".length)));
  }

  const covered = shown.filter((x) => table[x.ruleId] !== undefined).length;
  rows.push({
    tree: config.replace(/\/zzop\.config\.jsonc$/, "").replace(/^.*\//, ""),
    shown: shown.length,
    rules: Object.keys(table).length,
    caveatPct: (caveatBytes / total) * 100,
    messagePct: (messageBytes / total) * 100,
    coveragePct: (covered / shown.length) * 100,
    bytes: total,
  });
}

const pad = (s, n) => String(s).padEnd(n);
const pct = (n) => `${n.toFixed(1)}%`;
console.log(`window: ${extra.length ? extra.join(" ") : "default (DEFAULT_FINDINGS_LIMIT)"}`);
console.log(`${pad("tree", 12)} ${pad("shown", 6)} ${pad("rules", 6)} ${pad("caveats", 9)} ${pad("messages", 9)} coverage`);
for (const r of rows) {
  console.log(
    `${pad(r.tree, 12)} ${pad(r.shown, 6)} ${pad(r.rules, 6)} ${pad(pct(r.caveatPct), 9)} ${pad(pct(r.messagePct), 9)} ${pct(r.coveragePct)}`
  );
}
const range = (k) => {
  const v = rows.map((r) => r[k]);
  return `${Math.min(...v).toFixed(1)}-${Math.max(...v).toFixed(1)}%`;
};
console.log(
  `\nranges across ${rows.length} tree(s) — caveats ${range("caveatPct")} · ` +
    `messages ${range("messagePct")} · coverage ${range("coveragePct")}`
);
console.log("These are the three ranges the prose quotes. Re-run after any caveat edit; they move.");
