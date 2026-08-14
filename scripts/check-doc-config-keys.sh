#!/usr/bin/env bash
# check-doc-config-keys — every JSON key written inside a `zzop.config.jsonc` snippet in shipped
# prose must be a key the config front end actually accepts.
#
# ## The defect this exists for (2026-08-11, lived four days)
#
# `examples/packs/README.md` — the repo's ONLY instruction for loading a pack that is not bundled —
# printed this:
#
#     ```jsonc
#     // zzop.config.jsonc
#     { "packsDir": "path/to/your/packs" }   // or inline the pack under `packDefs`
#     ```
#
# Both keys are real. Neither is a CONFIG-FILE key: `packsDir` and `packDefs` are EMBEDDER REQUEST
# fields (`crates/facade`'s `AnalyzeRequest`), and the config front end answers a config file naming
# one with `unknown config key "packsDir" (ignored)` and loads eleven packs where `packs.extraDirs`
# loads twelve. So the retrieval instruction existed, was copy-pasteable, and did nothing. The same
# wrong key shipped in `docs/rules/authoring-guide.md`, which goes out over MCP as
# `zzop://contract/dsl-authoring-guide` — so pack-authoring AGENTS read it too.
#
# Five guards were green the whole time, and the reason is a scope gap rather than a bug in any of
# them: `crates/engine/tests/rule_contracts/reference_validation.rs` (CHECK B, the closest relative)
# validates backticked config-key-shaped tokens, but only in `.rs` sources, and only within 120 bytes
# of the word "config" — a JSON key inside a fenced Markdown block is outside on both axes.
#
# ## What this checks, and why the dialect split is the whole point
#
# A key inside a config-file snippet is valid when it names a key in `config-surface.json`'s
# `configKeys` (ANY scope — the scopes are read from the file, never re-listed here) or its
# `allowlistedTokens`. `embedderFields` is deliberately NOT unioned in: an embedder field written
# inside a `zzop.config.jsonc` block is precisely the shipped defect, so it must be an offense here,
# and it gets its own message rather than a generic "unknown key" — the two need different fixes and
# a reader who is told "unknown" will go looking for a typo, which is the wrong conclusion (the same
# wrong conclusion the runtime's own `check for a typo` warning invites; see
# `crates/metrics/src/diagnostics/config_reports.rs`).
#
# ## Where the FULL annotated template lives, and why this guard no longer pins a copy of it
#
# Until 2026-08-14 the pin at the bottom named `site/usage.html`'s annotated `zzop.config.jsonc`
# template — the whole starter file, every key commented. The site rewrite folded usage.html into a tab
# of site/index.html and left the page a redirect stub, and the new tab carries a five-key EXCERPT
# instead. The pin went red, which is the pin working.
#
# It was NOT re-pointed at a revived full copy anywhere (site/reference.html, docs/), and that is a
# judgment rather than a shrug. That text has exactly one owner and always did:
# `crates/config/src/template.rs`'s `CONFIG_TEMPLATE_JSONC`, which is what `zzop init` writes, what
# `zzop contract config-template` prints, and what MCP serves as `zzop://contract/config-template`. The
# site copy was a hand-maintained SECOND copy of it, and a second copy of a document nobody regenerates
# is this repo's most-paid-for defect class. Its own header records the drift it has already suffered.
#
# The guard's PURPOSE survives the loss intact, because that file is checked harder where it lives than
# it ever was here: `crates/config/src/template_tests.rs` runs the template's active keys through this
# crate's own unknown-key walk AND checks the keys named in its comments against the very same
# `config-surface.json` this guard reads. A Rust test on the canon beats a shell pin on a copy. This
# guard keeps the job it is uniquely able to do — every OTHER config snippet in shipped prose — and its
# named pin now protects the widest-reach one of those, the config block on the public site.
#
# ## Which blocks count as config snippets (positive anchors only)
#
# A fenced ```json/```jsonc block (Markdown) or a <pre> block (HTML — an inner <code> is optional) is
# judged when EITHER:
#   A. the prose immediately before the block names `zzop.config` (a sentence, or the
#      `<div class="…__tab">zzop.config.jsonc</div>` tab this repo's own examples put above it), or the
#      block carries `// zzop.config.jsonc` as a COMMENT line. Not a bare mention anywhere in the body:
#      a `"config": "/repo/api/zzop.config.jsonc"` inside an analyze-OUTPUT sample is the PATH of a
#      config file, and reading it as the marker judged 41 output field names against config keys; or
#   B. the block's first JSON key is a TOP-LEVEL config key (`configKeys.top`) — the shape of a
#      whole config document, or of the fragments `docs/extending.md` teaches with.
#
# Everything else is skipped, and the skipped set is real: Normalized-AST envelopes (`format`),
# analyze output (`fileCount`, `warnings`), MCP client config (`mcpServers`), DSL rule packs (`id` +
# `schema_version`), and the embedder request dialect (`suppressions`). Those are DIFFERENT
# vocabularies that legitimately share a file surface, and judging them against config keys would
# turn this guard into an allowlist, which is how this repo kills a guard.
#
# TWO KNOWN-UNCOVERED SHAPES, stated rather than silently skipped:
#   - a config FRAGMENT whose first key is nested rather than top-level (`docs/extending.md`'s
#     `"topology": { "mounts": [...] }` block). Admitting it means anchoring on nested key names,
#     and `root`/`sourceId` are nested config keys that also open Normalized-AST tree descriptors —
#     the anchor would start pulling in the envelope dialect it was built to exclude.
#   - the embedder request dialect (`{ "suppressions": [{ "rule": ..., "path": ... }] }`). Its child
#     keys are not enumerated anywhere in `config-surface.json` (only `embedderFieldShapes`, a
#     one-line prose shape per field), so there is nothing to validate them against yet.
#
# Comment lines inside a JSONC block are stripped BEFORE key extraction (a `//` line documenting an
# OUTPUT field is prose about the format, not a knob) but AFTER the anchor test — the
# `// zzop.config.jsonc` marker lives in a comment by convention, and stripping first made the
# invalidation drill pass a planted defect.
#
# Invalidation-checked (2026-08-12), all three directions: planting `{ "packsDir": ... }` under a
# `// zzop.config.jsonc` marker reports EMBEDDER-ONLY; planting `{ "packDefs": {} }` reports the same;
# planting a bare typo (`{ "packDefz": {} }`) reports unknown; and the clean tree reports zero.
# Re-checked 2026-08-14 after the <pre> widening, on the site's own bare-<pre> config block (the shape
# that had just fallen out of the census): a planted `packsDir` there reports EMBEDDER-ONLY, a planted
# typo reports unknown. Recount the clean tree with:
#   bash scripts/check-doc-config-keys.sh     # prints "OK (N config snippet(s) judged across M docs)"
set -euo pipefail
cd "$(dirname "$0")/.."

. ./scripts/lib/tracked-grep.sh

if ! command -v node >/dev/null 2>&1; then
  echo "check-doc-config-keys: \`node\` not found. This guard parses fenced code blocks and JSON" >&2
  echo "  keys rather than scanning text (a config snippet's keys are indistinguishable from any" >&2
  echo "  other quoted string to grep), so it cannot degrade. Install node; skipping would report" >&2
  echo "  clean while checking nothing." >&2
  exit 1
fi

# The subject set is DERIVED, never listed: every tracked-or-untracked `.md`/`.html` with any content,
# minus scripts/lib/tracked-grep.sh's standard exclusions. Untracked files are included for the same
# reason check-english-source.sh includes them — a new doc must be caught before its first `git add`,
# not from the moment it is tracked.
files="$(tracked_and_untracked_files_matching '.' '*.md' '*.html')"

if [ -z "$files" ]; then
  echo "check-doc-config-keys: enumerated ZERO .md/.html files — the enumeration broke. An empty" >&2
  echo "  subject set is a broken guard, never a clean tree." >&2
  exit 1
fi

printf '%s\n' "$files" | node --input-type=module -e '
import fs from "fs";

const surface = JSON.parse(fs.readFileSync("crates/config/config-surface.json", "utf8"));

// Every scope in configKeys, flattened — read from the FILE, so a scope added there is covered on the
// next run. The engine-side mirror of this vocabulary hand-declared its scopes and had drifted to
// seven of thirteen; that is the failure this derivation refuses to repeat.
const configKeys = new Set();
for (const keys of Object.values(surface.configKeys)) for (const k of keys) configKeys.add(k);
for (const t of surface.allowlistedTokens) configKeys.add(t);
const embedderOnly = new Set(surface.embedderFields.filter((f) => !configKeys.has(f)));
const topKeys = new Set(surface.configKeys.top);

if (configKeys.size < 20 || topKeys.size < 5) {
  console.error("check-doc-config-keys: config-surface.json yielded " + configKeys.size + " keys and " +
    topKeys.size + " top-level keys — the parse broke, and every snippet would pass vacuously.");
  process.exit(1);
}

const files = fs.readFileSync(0, "utf8").split("\n").map((s) => s.trim()).filter(Boolean);

const decode = (s) =>
  s.replace(/&quot;/g, "\"").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");

const stripTags = (s) => s.replace(/<[^>]*>/g, "");

// Every fenced (Markdown) / <pre> (HTML) block, with the prose that introduces it.
//
// The HTML arm used to REQUIRE a <pre><code> pair, and the 2026-08-14 site rewrite showed what that
// cost: the new one-page site ships its config example as a bare <pre> whose tokens are highlighted
// with <span> — no <code> anywhere — so the public site fell out of this census entirely and nothing
// went red. A <pre> IS the block element; the <code> was never the thing that made it a code block. So
// the needle is <pre>…</pre>, an inner <code> is unwrapped when present, and any remaining markup is
// stripped before the JSON keys are read (highlighting a key must not hide it from this guard).
// Tags are stripped BEFORE entities are decoded, so a &lt;tag&gt; a doc is TALKING about survives.
function blocksIn(file, text) {
  const out = [];
  if (file.endsWith(".html")) {
    const re = /<pre[^>]*>([\s\S]*?)<\/pre>/g;
    let m;
    while ((m = re.exec(text))) {
      let body = m[1];
      const inner = /^\s*<code[^>]*>([\s\S]*?)<\/code>\s*$/.exec(body);
      if (inner) body = inner[1];
      out.push({
        line: text.slice(0, m.index).split("\n").length,
        lead: decode(text.slice(Math.max(0, m.index - 400), m.index)),
        body: decode(stripTags(body)),
      });
    }
    return out;
  }
  const lines = text.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    if (!/^```jsonc?$/i.test(lines[i].trim())) continue;
    const body = [];
    let j = i + 1;
    for (; j < lines.length && !/^```/.test(lines[j].trim()); j++) body.push(lines[j]);
    out.push({
      line: i + 1,
      lead: lines.slice(Math.max(0, i - 3), i).join("\n"),
      body: body.join("\n"),
    });
    i = j;
  }
  return out;
}

const keyRe = /"([A-Za-z_][A-Za-z0-9_]*)"\s*:/g;
const stripComments = (body) =>
  body.split("\n").filter((l) => !/^\s*(\/\/|\/\*|\*)/.test(l)).join("\n");

const offenders = [];
let scanned = 0;
let scannedSiteConfig = false;

for (const file of files) {
  let text;
  try { text = fs.readFileSync(file, "utf8"); } catch { continue; }
  for (const b of blocksIn(file, text)) {
    // Anchor A, two arms, both read the body BEFORE comments are stripped — the
    // `// zzop.config.jsonc` marker is itself a comment line.
    //   · LEAD: the prose or the code-panel tab that introduces the block.
    //   · BODY: that marker — and it is restricted to a COMMENT line, which is what this arm was
    //     always described as being for. Unrestricted it also matched a JSON string VALUE, and the
    //     rewritten site shipped one: the analyze-OUTPUT sample carries
    //     `"config": "/repo/api/zzop.config.jsonc"`, i.e. the path of a config file, not a config
    //     file. That single line was enough to judge 41 output field names (`fileCount`, `warnings`,
    //     `bySeverity`, …) against the config vocabulary — the exact "different vocabularies sharing
    //     a file surface" confusion the skip list below exists to prevent. Widening the <pre> needle
    //     above is what made it reachable, so the two changes land together.
    const named = /^[ \t]*\/\/.*zzop\.config/im.test(b.body) || /zzop\.config/i.test(b.lead);
    const body = stripComments(b.body);
    if (!/[{[]/.test(body)) continue;
    keyRe.lastIndex = 0;
    const first = keyRe.exec(body);
    const firstKey = first ? first[1] : null;
    if (!named && !(firstKey && topKeys.has(firstKey))) continue;

    scanned++;
    if (file === "site/index.html" && named && body.includes("\"vocabulary\"")) {
      scannedSiteConfig = true;
    }
    keyRe.lastIndex = 0;
    let m;
    while ((m = keyRe.exec(body))) {
      const key = m[1];
      if (configKeys.has(key)) continue;
      const why = embedderOnly.has(key)
        ? "is an EMBEDDER REQUEST field, not a config-file key — a config file naming it gets " +
          "`unknown config key (ignored)` and the run silently differs"
        : "names no config key at all";
      offenders.push(file + ":" + b.line + '"'"'  "'"'"' + key + '"'"'"  '"'"' + why);
    }
  }
}

// Non-emptiness floor. `blocksIn` degrades to "no blocks" on a doc whose fence style changes, and a
// guard that scans nothing passes vacuously — the failure this repo has now measured four times.
// The floor is deliberately well under the live count (13 on 2026-08-14; also 13 on 2026-08-12, but a
// different thirteen — the site rewrite took two site/usage.html blocks out and the <pre> widening put
// site/index.html and site/ko/index.html in) so ordinary doc edits do not trip it, while a wholesale
// extraction break does. Recount with a plain run: it prints the number it judged.
if (scanned < 6) {
  console.error("check-doc-config-keys: only " + scanned + " config snippet(s) were judged (expected " +
    "at least 6; 13 on 2026-08-14). The block extraction or the anchors stopped matching — this is a " +
    "broken guard reporting clean, not a repo with no config examples.");
  process.exit(1);
}

// Named pin, the leg the numeric floor cannot cover: the config block on the PUBLIC SITE is the
// widest-reach config example this guard can read, and a rewrite that drops it out of the scanned set
// must say so here rather than quietly narrowing the census. This pin has now caught that exact event
// once — see the header for what moved on 2026-08-14 and where the full template lives.
if (!scannedSiteConfig) {
  console.error("check-doc-config-keys: the `zzop.config.jsonc` block on site/index.html was NOT among " +
    "the judged blocks. It is the widest-reach config example this guard can read; if it moved or its " +
    "markup changed, re-point this pin rather than letting the widest surface fall out of the scan. " +
    "Note site/index.html is GENERATED from site-src/content/*.mjs by scripts/gen-site.mjs.");
  process.exit(1);
}

if (offenders.length) {
  console.error("check-doc-config-keys: shipped prose teaches a config key the config front end does not accept:");
  for (const o of offenders) console.error("  " + o);
  console.error("");
  console.error("  The valid key set is crates/config/config-surface.json'"'"'s `configKeys` (every scope).");
  console.error("  Print it from a shipped binary with `zzop contract config-surface`.");
  process.exit(1);
}

console.log("check-doc-config-keys: OK (" + scanned + " config snippet(s) judged across " + files.length + " docs)");
'
