#!/usr/bin/env bash
# check-runtime-text-rule-ids — a rule id named in text a USER READS AT RUNTIME (a finding message)
# must name a rule that is loaded whenever that text can be printed. No retrieval pointer redeems it.
#
# ## The defect this exists for (2026-08-12, `105c52f` -> `c9ba679`)
#
# `code-hygiene/localhost-url-literal-committed` left the bundle for `examples/packs/code-hygiene.json`.
# Two bundled siblings had narrowed themselves on the premise that it covers the loopback case, and the
# NATIVE one said so in the finding text a user reads: `cross-layer/external-ip-literal` skipped loopback
# hosts and its message named that id as the owner. `105c52f` treated it as a disclosure problem and
# added a `⚠ that owner LEFT THE BUNDLE` clause to the message. Re-measured with the engine, that
# prescription reaches nobody:
#
#     cargo run --release -p zzop-engine --example corpus_rule_counts -- corpus/x \
#       code-hygiene/localhost-url-literal-committed cross-layer/external-ip-literal
#       code-hygiene/localhost-url-literal-committed: 23   <- lines a default run cannot judge
#       cross-layer/external-ip-literal: 0                 <- findings able to carry the notice
#
# The same shape, same day, one rule family over: `sql/destructive-migration` left the bundle and its
# three critical siblings carried a ⚠ notice about the gap — with ZERO runtime carriers across 18
# checkouts, because those rules structurally do not fire on migration paths.
#
# ## Why the pass condition is INVERTED from `check-prose-rule-ids.sh`, and why this is a separate file
#
# That guard PASSES an exported id when the same file names the pack file, because a reader who is
# holding the document can go and fetch the pack. Its subject is a document, which a reader reads as a
# whole. This subject is a finding, which reaches a user ONLY on a line where it FIRES — and the
# population that needs the notice is by construction the population where nothing fires. A retrieval
# pointer cannot redeem a sentence nobody receives, so here the pointer is not an exemption and naming
# an unloaded rule is simply forbidden. Two rules with opposite pass conditions must not share one
# guard file; the next person would read whichever half they found first.
#
# ## What is judged, and why there is no allowlist
#
# Subjects, both enumerated from the directories the engine itself loads:
#   * every `message` of every rule in `rules/dsl/**.json` and `examples/packs/**.json` — the only
#     user-facing string field on `RuleDef`;
#   * every STRING LITERAL in `rules/native/**/*.rs` outside test code. Native findings build their
#     message in Rust, so the message is a string literal and nothing else in that file is.
#
# Comments are NOT subjects, and the difference is load-bearing rather than convenience: a doc comment
# saying "`egress/http-url-literal` uses the same exclusion" is a COMPARISON between two rules and stays
# true whether or not either ships. A finding that says "that is rule X`s turf" is a DELEGATION, and it
# is false the moment X is not running. So the subject is decided by a Rust lexer (string vs comment vs
# code, raw strings and escapes included), not by a line grep — a grep cannot tell `//` inside
# `"http://localhost"` from a comment, and this repo`s own rules put both in the same file.
#
# An id passes when it resolves to (1) a rule of a pack under `rules/dsl/`, (2) a native analysis id from
# `docs/rules/catalog.md`s Native-analyses table, or (3) a rule of the SAME exported pack the subject
# lives in — loading that pack loads both, so the sibling is guaranteed present exactly when the text
# can print. Nothing else passes and there is no list of blessed strings: the fix for a violation is to
# state the rule`s own yardstick instead of pointing at an owner, which is what `c9ba679` did.
#
# Measured on the clean tree (2026-08-12): 52 qualified ids across 15 packs and 21 more across 6410
# string literals in 98 `rules/native/` files, 0 offenders. Run against the `105c52f` tree it reports
# FOUR — the native `external_ip_literal.rs` message, and the ⚠ notice `105c52f` had just written into
# `sql/delete-no-where`, `sql/update-no-where` and `sql/truncate-in-app-code`. That second group is the
# point: those three notices were the prescription a later measurement killed (0 runtime carriers across
# 18 checkouts), and this guard refuses them on the day they are typed rather than after the count.
# The same file`s MODULE DOC carried the identical delegation and is NOT reported, because a comment is
# not runtime text — the honest half of the subject rule, stated again here so the number reads right.
#
# ## What this deliberately does NOT check
#
# 1. A DELEGATION WITH NO ID IN IT. `egress/http-url-literal` deferred the identical loopback case
#    through an `exclude_pattern` regex that named `localhost`/`127.0.0.1` and said nothing anywhere —
#    no id text existed to find, so no id-shaped search of any kind could have reported it. `c9ba679`
#    fixed it by writing the exclusion`s own justification into the message, and NOTHING HERE FORCES
#    THAT. A silent narrowing is invisible to this guard by construction; only a domain review finds it.
# 2. A BARE id that resolves to nothing. `<ns>/<rule>` unbackticked is also the shape of ordinary
#    English (`schema/data operations`, in `sql/truncate-in-app-code` today), so an unresolvable bare
#    token is skipped. A bare token that DOES resolve to an exported rule is still judged — resolution
#    is its own disambiguator — so the delegation shape that caused the incident is covered either way.
# 3. Runtime text built outside `rules/native/` and the packs — engine-appended hints, CLI/MCP wording.
#    This line used to read "those name no rule ids today; widening the subject there is a measurement,
#    not a guess". It was FALSE on the day it was typed, and it was standing as the reason not to widen —
#    the worst shape a boundary note can take, because it removes the next reader's reason to look. Here
#    is the measurement it was asking for, taken 2026-08-13 with THIS file's own lexer and id universe
#    over `crates/engine/src` + `crates/summary/src` (239 `.rs` files, 1743 string literals outside test
#    files and `#[cfg(test)]` bodies): 42 id-shaped tokens whose namespace resolves, 39 of them real ids,
#    every single one NATIVE.
#      * 37 are a literal that IS the id, whole (`is_enabled(gate, "cross-layer/...")` gate keys, one
#        pinned const). Code references, not sentences — the shape this guard already skips.
#      * 5 sit inside a longer literal. TWO are real ids in shipped disclosure prose
#        (`crates/engine/src/analyze/compose/response_refs.rs`, naming
#        `cross-layer/sensitive-response-field` twice); the other three are ordinary English of the same
#        shape (`schema/table`, `http/https`, `schema/versioning`) — gap 2's case, one lane over.
#      * ZERO name an exported rule, so a widened subject would report nothing today.
#
#    NOT WIDENED — and now on the measurement rather than in place of it. Two facts decide it:
#      (a) every id named out there is a NATIVE analysis, compiled into this binary. It cannot leave the
#          bundle the way a DSL pack can, so this guard's pass condition — "loaded whenever that text can
#          print" — has nothing to do on it;
#      (b) the one site that genuinely DELEGATES in shipped runtime text is invisible to ANY string scan,
#          including a widened one: `crates/engine/src/framework_silence/orm_schema_silence.rs` builds
#          its warning around `{SILENCED_RULE_ID}`, so the SOURCE holds a constant while the RUNTIME TEXT
#          names `cross-layer/db-table-name-in-multiple-sources`. That one is covered by a registry pin in
#          its own file (`db_table_rule_id_is_a_real_shipped_rule`), and `response_refs.rs`'s two by
#          `crates/engine/tests/integration/analyze_response_shape.rs`, which asserts the same id both as
#          the finding's `rule_id` and as text in the warning.
#    So widening would judge 2 already-pinned mentions and still miss the interpolated one. Re-decide the
#    moment either fact changes: an engine/summary message that names a DSL PACK's rule id, or an id that
#    reaches runtime text as a literal instead of through a const.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v node >/dev/null 2>&1; then
  echo "check-runtime-text-rule-ids: \`node\` not found. This guard parses pack JSON and lexes Rust" >&2
  echo "  source rather than scanning text, so it cannot degrade to grep. Install node; skipping" >&2
  echo "  would report clean while checking nothing." >&2
  exit 1
fi

node --input-type=module -e '
import fs from "fs";
import path from "path";
import { nativeAnalysisIds } from "./scripts/lib/catalog-native-ids.mjs";

// --- the id universe, every source DERIVED ------------------------------------------------------
const packsUnder = (dir) => {
  const out = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name).split(path.sep).join("/");
      if (e.isDirectory()) { walk(p); continue; }
      if (!e.name.endsWith(".json")) continue;
      const pack = JSON.parse(fs.readFileSync(p, "utf8"));
      if (!pack.id || !Array.isArray(pack.rules)) continue;
      out.push({ file: p, id: pack.id, rules: new Set(pack.rules.map((r) => r.id)), defs: pack.rules });
    }
  };
  walk(dir);
  return out;
};
const bundled = packsUnder("rules/dsl");
const exported = packsUnder("examples/packs");

// The universe behind pass condition (2). Shared with check-prose-rule-ids.sh, which read the same
// table with the same two widening bugs until 2026-08-13 — scripts/lib/catalog-native-ids.mjs
// carries the measurement and names who owns the reasoning.
const natives = nativeAnalysisIds("check-runtime-text-rule-ids",
  "every native rule naming its own id would read as an offender.");

if (bundled.length < 8 || exported.length < 2) {
  console.error("check-runtime-text-rule-ids: derived " + bundled.length + " bundled pack(s) and " +
    exported.length + " exported pack(s) — far under the live counts. The extraction broke; every id");
  console.error("  would be judged against an empty universe.");
  process.exit(1);
}

const bundledRule = (ns, rule) => bundled.some((p) => p.id === ns && p.rules.has(rule));
const exportedPackOf = (ns, rule) => exported.find((p) => p.id === ns && p.rules.has(rule)) || null;
const namespaces = new Set([
  ...bundled.map((p) => p.id),
  ...exported.map((p) => p.id),
  ...[...natives].filter((n) => n.includes("/")).map((n) => n.split("/")[0]),
]);

// --- Rust lexer: string literals only, comments and code discarded -------------------------------
// Tracks escapes, raw strings (r"..", r#".."#, b-prefixed) and char literals, because every one of
// those can hold or hide a `//`. Returns {start, text} for each string literal.
const SQ = String.fromCharCode(39);
const lexStrings = (src) => {
  const out = [];
  const n = src.length;
  let i = 0;
  while (i < n) {
    const c = src[i];
    if (c === "/" && src[i + 1] === "/") { const j = src.indexOf("\n", i); i = j < 0 ? n : j; continue; }
    if (c === "/" && src[i + 1] === "*") {
      let depth = 1, j = i + 2;
      while (j < n && depth > 0) {
        if (src[j] === "/" && src[j + 1] === "*") { depth++; j += 2; }
        else if (src[j] === "*" && src[j + 1] === "/") { depth--; j += 2; }
        else j++;
      }
      i = j; continue;
    }
    if ((c === "r" || c === "b") && !/[A-Za-z0-9_]/.test(src[i - 1] || " ")) {
      let j = c === "b" ? i + 1 : i;
      if (src[j] === "r") {
        j++;
        let hashes = 0;
        while (src[j] === "#") { hashes++; j++; }
        if (src[j] === "\"") {
          const term = "\"" + "#".repeat(hashes);
          const k = src.indexOf(term, j + 1);
          out.push({ start: i, text: src.slice(j + 1, k < 0 ? n : k) });
          i = k < 0 ? n : k + term.length;
          continue;
        }
      }
    }
    // A char literal (`SQ` is the quote itself, spelled by code point so this script can stay inside a
    // single-quoted shell string). Skipped whole, because `SQ + double-quote + SQ` would otherwise open
    // a string literal that never closes; a lifetime (`&SQ a T`) falls through to the i++ below.
    if (c === SQ) { const m = new RegExp("^" + SQ + "(\\\\.|[^" + SQ + "\\\\])" + SQ).exec(src.slice(i)); if (m) { i += m[0].length; continue; } i++; continue; }
    if (c === "\"") {
      let j = i + 1;
      while (j < n) { if (src[j] === "\\") { j += 2; continue; } if (src[j] === "\"") break; j++; }
      out.push({ start: i, text: src.slice(i + 1, j) });
      i = j + 1; continue;
    }
    i++;
  }
  return out;
};

// `#[cfg(test)] mod ... { .. }` spans, brace-counted with strings and comments blanked out first so a
// brace inside either cannot move the boundary. Grep over these files over-counts by ~3x.
const testSpans = (src) => {
  let masked = src.split("");
  for (const s of lexStrings(src)) for (let k = s.start; k < s.start + s.text.length + 2 && k < masked.length; k++) masked[k] = " ";
  masked = masked.join("").replace(/\/\/[^\n]*/g, (m) => " ".repeat(m.length)).replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "));
  const spans = [];
  for (const m of masked.matchAll(/#\[cfg\(test\)\]/g)) {
    let depth = 0, started = false, end = src.length;
    for (let p = m.index; p < masked.length; p++) {
      if (masked[p] === "{") { depth++; started = true; }
      else if (masked[p] === "}") { depth--; if (started && depth === 0) { end = p; break; } }
    }
    spans.push([m.index, end]);
  }
  return spans;
};

const rustFiles = [];
(function walk(d) {
  for (const e of fs.readdirSync(d, { withFileTypes: true })) {
    const p = path.join(d, e.name).split(path.sep).join("/");
    if (e.isDirectory()) { walk(p); continue; }
    if (e.name.endsWith(".rs")) rustFiles.push(p);
  }
})("rules/native");
if (rustFiles.length < 40) {
  console.error("check-runtime-text-rule-ids: walked rules/native and found only " + rustFiles.length +
    " .rs file(s) (98 on 2026-08-12). The walk broke; native runtime text would go unjudged.");
  process.exit(1);
}

// --- judgment ------------------------------------------------------------------------------------
const tokenRe = /([a-z][a-z0-9-]*)\/([a-z][a-z0-9-]*)/g;
const offenders = [];
let judged = 0;

// `where` is the subject location, `ownPack` the exported pack whose rules ship alongside this text.
const judge = (text, where, ownPack) => {
  tokenRe.lastIndex = 0;
  let m;
  while ((m = tokenRe.exec(text))) {
    const [ns, rule] = [m[1], m[2]];
    if (!namespaces.has(ns)) continue;
    const id = m[0];
    const backticked = text[m.index - 1] === "`" && text[m.index + id.length] === "`";
    if (natives.has(id) || bundledRule(ns, rule)) { if (backticked) judged++; continue; }
    const owner = exportedPackOf(ns, rule);
    if (owner && ownPack && owner.file === ownPack) { if (backticked) judged++; continue; }
    if (!backticked && !owner) continue; // ordinary English of the same shape; see the header
    judged++;
    offenders.push(owner
      ? where + "  " + id + "  — lives in " + owner.file + ", which a default run does NOT load. A" +
        " finding reaches a user only where it FIRES, so a reader who needs this pointer is exactly the" +
        " reader nothing fired for. State this rule OWN yardstick instead of naming an owner."
      : where + "  " + id + "  — names no rule this repo ships. A finding cannot send a user to a rule" +
        " that does not exist.");
  }
};

for (const pack of [...bundled, ...exported]) {
  const isExported = exported.includes(pack);
  for (const r of pack.defs) {
    judge(r.message || "", pack.file + "  rule `" + pack.id + "/" + r.id + "`", isExported ? pack.file : null);
  }
}
const packJudged = judged;

let literals = 0;
for (const file of rustFiles) {
  const src = fs.readFileSync(file, "utf8");
  const isTestFile = /(^|\/)tests?\.rs$|\/tests\//.test(file);
  const spans = isTestFile ? null : testSpans(src);
  for (const s of lexStrings(src)) {
    literals++;
    if (isTestFile || spans.some(([a, b]) => s.start >= a && s.start <= b)) continue;
    // The bare id form used as a code identifier (`rule_id: "<id>"`, `disable_hint("<id>")`, the
    // NATIVE_ANALYSES table): the whole string IS the id, so it is a reference, not a sentence.
    if (s.text.trim().includes("/") && /^[a-z][a-z0-9-]*\/[a-z][a-z0-9-]*$/.test(s.text.trim())) continue;
    const line = src.slice(0, s.start).split("\n").length;
    judge(s.text, file + ":" + line, null);
  }
}

if (packJudged < 30 || judged - packJudged < 10 || literals < 3000) {
  console.error("check-runtime-text-rule-ids: judged " + packJudged + " id(s) in pack messages and " +
    (judged - packJudged) + " in " + literals + " native string literal(s) (52 / 21 / 6410 on" +
    " 2026-08-13, unchanged since 2026-08-12). An extraction stopped matching — a broken guard" +
    " reporting clean.");
  process.exit(1);
}

if (offenders.length) {
  console.error("check-runtime-text-rule-ids: runtime text names a rule that may not be loaded:");
  for (const o of offenders) console.error("  " + o);
  process.exit(1);
}

console.log("check-runtime-text-rule-ids: OK (" + packJudged + " qualified id(s) across " +
  (bundled.length + exported.length) + " packs, " + (judged - packJudged) + " across " + literals +
  " string literal(s) in " + rustFiles.length + " rules/native files)");
'
