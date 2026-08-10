/* Generate site/rules.html's rule ROWS from docs/rules/catalog.md, the machine-pinned SSOT.
 *
 * WHY THIS EXISTS
 * The public site mirrored the catalog BY HAND. Every row was a second copy of a fact whose first
 * copy is machine-checked against the engine (`crates/engine/tests/rule_contracts/`), so the mirror
 * could only ever be as fresh as the last person who remembered it -- and on 2026-08-08 it was 41
 * rows stale at once. scripts/check-rules-catalog-sync.sh caught the TOKENS (ids, severities,
 * matcher names) precisely because tokens are comparable as sets; it deliberately did not read the
 * prose, which is where the drift actually lived. Deriving the rows removes the second copy instead
 * of watching it: after this, a catalog edit that is not mirrored is not a stale site, it is a red
 * guard.
 *
 * WHAT IS GENERATED, AND WHAT IS NOT
 * Only the `<tbody>` contents of the tables inside `<section id="pack-*">` and
 * `<section id="native-analyses">`. Everything else on that page stays hand-written: the intro
 * paragraphs, the native-analyses prose, the matcher glossary table (`<section id="matchers">` -- its
 * rows are Matcher VARIANTS, not rules, and have no catalog row to derive from), the custom-pack
 * example, and the table of contents. Section scaffolding is NOT generated either; a catalog pack
 * with no `<section id="pack-<id>">` on the site is a hard error here that prints the scaffold to
 * paste, because inventing a section would also mean inventing its TOC entry.
 *
 * USAGE
 *   node scripts/gen-site-rules.mjs            # rewrite site/rules.html in place
 *   node scripts/gen-site-rules.mjs --check    # exit 1 if the file is not what this would write
 * `--check` is what scripts/check-rules-catalog-sync.sh runs; the bare form is what a human runs
 * after editing the catalog.
 *
 * THE MARKDOWN -> HTML TRANSFORM IS DELIBERATELY TINY
 * It handles exactly the constructs the catalog table cells actually use, measured rather than
 * assumed (203 rows: 1 link, 0 double-backtick spans, 0 single-* emphasis, 0 raw HTML, 0 unbalanced
 * backticks). Anything outside that set is a hard error rather than a silent passthrough, so the day
 * someone writes a construct this does not model, the generator says so instead of shipping literal
 * markdown onto the public site.
 *   `code`      -> <code>code</code>          (contents HTML-escaped)
 *   **bold**    -> <strong>bold</strong>
 *   [t](u)      -> <code>t</code>
 *   & < >       -> &amp; &lt; &gt;            (inside code spans and out)
 * The link rule needs its own justification: the catalog's one link target is REPO-relative
 * (`../../VERSIONING.md`), which resolves to nothing from the published site, so following it is not
 * an option and neither is emitting a dead href. Rendering the link TEXT as a literal is what the
 * hand-written page already did, and it is the honest reading -- the site names the document, the
 * repo copy links it. (Emitting a github.com blob URL instead would work; it is not done here
 * because that is a site-design decision, not a transcription of the catalog.)
 * Apostrophes are NOT escaped. `'` is legal character data in HTML; the hand-written page escaped
 * exactly one of them (`reqwest&#39;s`) and left every other apostrophe on the page bare, which is
 * the kind of inconsistency a generator exists to end.
 */
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const here = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1"));
const repoRoot = path.resolve(here, "..");
const CATALOG = path.join(repoRoot, "docs/rules/catalog.md");
const SITE = path.join(repoRoot, "site/rules.html");
const INDENT = " ".repeat(12);

function die(msg) {
  console.error(`gen-site-rules: ${msg}`);
  process.exit(1);
}

/* ---------- markdown inline -> HTML ---------------------------------------------------------- */

const escapeHtml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/* One pass, three token kinds: a backtick code span, a `**` bold toggle, and plain text. A single
 * pass rather than "split on code spans, then handle bold inside each piece", because bold WRAPS
 * code spans in this catalog (`**Id keeps its `java-` prefix**`) and a per-piece pass sees only half
 * a delimiter on each side. */
function mdInlineToHtml(src, where) {
  if ((src.match(/`/g) || []).length % 2 !== 0) die(`${where}: unbalanced backtick in cell: ${src.slice(0, 120)}`);
  if (src.includes("``")) die(`${where}: double-backtick code span is not modeled: ${src.slice(0, 120)}`);
  let out = "";
  let plain = "";
  let bold = false;

  // Plain text is buffered so the not-modeled checks below see whole runs, then escaped as one.
  const flush = () => {
    if (plain === "") return;
    if (/<[A-Za-z/!]/.test(plain)) die(`${where}: raw HTML outside a code span is not modeled: ${plain.slice(0, 120)}`);
    if (plain.includes("*")) die(`${where}: single-* emphasis is not modeled: ${plain.slice(0, 120)}`);
    if (/\[[^\]]*\]\([^)]*\)/.test(plain)) {
      // The one link in the catalog. Repo-relative targets cannot be followed from the published
      // site, so the label is rendered as a literal rather than as a dead href (see header).
      plain = plain.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_m, label) => `\u0001${label}\u0002`);
      out += escapeHtml(plain).replace(/\u0001/g, "<code>").replace(/\u0002/g, "</code>");
      plain = "";
      return;
    }
    out += escapeHtml(plain);
    plain = "";
  };

  for (let i = 0; i < src.length; ) {
    if (src[i] === "`") {
      const end = src.indexOf("`", i + 1);
      flush();
      out += `<code>${escapeHtml(src.slice(i + 1, end))}</code>`;
      i = end + 1;
      continue;
    }
    if (src.startsWith("**", i)) {
      flush();
      out += bold ? "</strong>" : "<strong>";
      bold = !bold;
      i += 2;
      continue;
    }
    plain += src[i];
    i += 1;
  }
  flush();
  if (bold) die(`${where}: unclosed ** in cell: ${src.slice(0, 120)}`);
  return out;
}

/* A severity cell is either a bare level (`warning`, `n/a`) or a level plus a hand-written
 * qualifier (`warning (critical when consumed)`). Only the LEVEL is a token, so only the level is
 * code-wrapped and the qualifier goes through the ordinary prose transform -- which is what the
 * hand-written page did, and what check-rules-catalog-sync.sh's triple extractor assumes. */
function severityCell(raw, where) {
  const m = /^([a-z][a-z/]*)(\s+\S[\s\S]*)?$/.exec(raw.trim());
  if (!m) die(`${where}: severity cell does not start with a bare level: ${raw}`);
  const rest = m[2] ? mdInlineToHtml(m[2], where) : "";
  return `<code>${m[1]}</code>${rest}`;
}

/* ---------- catalog parsing ------------------------------------------------------------------ */

/* A table data row. Cells are split on " | " rather than on a bare "|", because prose cells carry
 * "|" inside code spans (alternated regex fragments); the delimiter in this file is always
 * space-pipe-space. */
function splitRow(line, where, expected) {
  const body = line.replace(/^\|\s?/, "").replace(/\s?\|$/, "");
  const cells = body.split(" | ");
  if (cells.length !== expected) {
    die(`${where}: expected ${expected} cells, got ${cells.length}: ${line.slice(0, 140)}`);
  }
  return cells.map((c) => c.trim());
}

function unwrapId(cell, where) {
  const m = /^`([a-z0-9][a-z0-9/_-]*)`$/.exec(cell);
  if (!m) die(`${where}: first cell is not a backticked rule id: ${cell.slice(0, 80)}`);
  return m[1];
}

function parseCatalog(md) {
  const lines = md.split(/\r?\n/);
  const packs = [];
  const native = [];
  let mode = null; // "dsl" | "native" | null
  let pack = null;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const where = `docs/rules/catalog.md:${i + 1}`;

    if (/^## DSL packs\b/.test(line)) { mode = "dsl"; pack = null; continue; }
    if (/^## Native analyses\b/.test(line)) { mode = "native"; pack = null; continue; }
    if (/^## /.test(line)) { mode = null; pack = null; continue; }

    if (mode === "dsl" && /^### /.test(line)) {
      const m = /^### `([a-z0-9-]+)` \((\d+) rules?\)\s*$/.exec(line);
      if (!m) die(`${where}: pack heading is not '### \`<id>\` (<n> rules)': ${line}`);
      pack = { id: m[1], declared: Number(m[2]), rules: [] };
      packs.push(pack);
      continue;
    }
    // "### Roadmap" closes the native table.
    if (mode === "native" && /^### /.test(line)) { mode = null; continue; }

    if (!/^\| `/.test(line)) continue;

    if (mode === "dsl") {
      if (!pack) die(`${where}: a DSL table row appears before any pack heading`);
      const [id, sev, matcher, detects] = splitRow(line, where, 4);
      pack.rules.push({
        id: unwrapId(id, where),
        severity: severityCell(sev, where),
        matcher: mdInlineToHtml(matcher, where),
        detects: mdInlineToHtml(detects, where),
      });
    } else if (mode === "native") {
      const [id, sev, detects] = splitRow(line, where, 3);
      native.push({
        id: unwrapId(id, where),
        severity: severityCell(sev, where),
        detects: mdInlineToHtml(detects, where),
      });
    }
  }

  /* Every published count carries the command that recounts it -- and here the generator IS that
   * command: each pack heading's "(N rules)" is re-derived from the rows under it on every run. The
   * repo-wide totals line is machine-checked elsewhere (rule_contracts), but nothing watched the
   * per-pack numbers, and a heading that miscounts its own table is a published number with no
   * metric behind it. */
  for (const p of packs) {
    if (p.declared !== p.rules.length) {
      die(`docs/rules/catalog.md: pack heading '${p.id} (${p.declared} rules)' disagrees with the ${p.rules.length} rows under it`);
    }
  }
  if (packs.length === 0) die("parsed ZERO DSL packs from the catalog -- refusing to write an empty site table");
  if (native.length === 0) die("parsed ZERO native analyses from the catalog -- refusing to write an empty site table");

  /* The native table is the one table with no per-section floor. Every DSL pack heading declares
   * "(N rules)" and is held to its rows above, but the native section has no such heading -- so a
   * native row whose spelling drifts off the row needle (`| \``: pipe, SPACE, backtick -- measured:
   * `|\`` without the space parses as zero rows lost, exit 0, row absent from the published site)
   * would simply vanish. The floor is the catalog's own Totals sentence ("N native analysis ids"),
   * which crates/engine/tests/rule_contracts/'s catalog_totals_match_loaded_rule_and_analysis_counts
   * pins to the engine registry -- derived from a machine-checked value, not a second hand count. */
  const totals = /^\*\*Totals\*\*.*?(\d+) native analysis ids/m.exec(md);
  if (!totals) {
    die("docs/rules/catalog.md: found no Totals sentence matching 'N native analysis ids' -- the " +
        "native table's row-count floor lost its anchor. Re-point this needle at the machine-checked " +
        "totals line; do not delete the floor.");
  }
  const declaredNative = Number(totals[1]);
  if (native.length !== declaredNative) {
    die(`parsed ${native.length} native table rows, but the catalog's machine-checked Totals sentence ` +
        `declares ${declaredNative} native analysis ids. Either a native row is malformed (rows are ` +
        "recognized by '| `' exactly: pipe, space, backtick) or the table and the totals disagree -- " +
        "both mean a row this generator cannot see, which would otherwise vanish from the site silently.");
  }
  return { packs, native };
}

/* ---------- row rendering -------------------------------------------------------------------- */

const dslRow = (r) =>
  `${INDENT}<tr><td><code>${r.id}</code></td><td>${r.severity}</td><td><code>${r.matcher}</code></td><td>${r.detects}</td></tr>`;
const nativeRow = (r) =>
  `${INDENT}<tr><td><code>${r.id}</code></td><td>${r.severity}</td><td>${r.detects}</td></tr>`;

/* ---------- site splicing -------------------------------------------------------------------- */

/* Rows are anchored by `<section id="...">` plus the first own-line `<tbody>` inside it, not by
 * comment markers. The section ids are already load-bearing on this page (the table of contents
 * links to every one of them, and check-rules-catalog-sync.sh's own comment anchors on #matchers),
 * so reusing them adds nothing new to keep in sync -- a marker pair per table would have been 26
 * more lines that can themselves drift. `<section id="matchers">` also holds a `<tbody>` and is left
 * alone precisely because it is neither pack-* nor native-analyses. */
function spliceSite(html, { packs, native }) {
  const eol = html.includes("\r\n") ? "\r\n" : "\n";
  const sectionRe = /<section id="(pack-[a-z0-9-]+|native-analyses)">([\s\S]*?)<\/section>/g;
  const tbodyRe = /(^[ \t]*<tbody>[ \t]*$)[\s\S]*?(^[ \t]*<\/tbody>[ \t]*$)/m;
  const seen = new Set();

  const out = html.replace(sectionRe, (whole, id, body) => {
    seen.add(id);
    let rows;
    if (id === "native-analyses") {
      rows = native.map(nativeRow);
    } else {
      const packId = id.slice("pack-".length);
      const pack = packs.find((p) => p.id === packId);
      if (!pack) {
        die(`site/rules.html has <section id="${id}"> but docs/rules/catalog.md ships no '${packId}' pack.\n` +
            `  Delete the section and its table-of-contents entry -- the catalog is the SSOT.`);
      }
      rows = pack.rules.map(dslRow);
    }
    if (!tbodyRe.test(body)) die(`site/rules.html: <section id="${id}"> has no own-line <tbody>...</tbody> pair to fill`);
    // Function replacers on BOTH calls, deliberately. A string in the replacement position makes JS
    // expand `$&`, `$$`, `` $` `` and `$'` — and the replacement here is catalog prose, which writes
    // about regexes and sed constantly. A cell containing `$&` would splice this whole <tbody> into
    // one <td>. No catalog cell carries such a token today; that is a fact about today, not a design.
    const filled = body.replace(tbodyRe, (_m, open, close) => [open, ...rows, close].join(eol));
    return whole.replace(body, () => filled);
  });

  for (const p of packs) {
    if (seen.has(`pack-${p.id}`)) continue;
    die(`docs/rules/catalog.md ships pack '${p.id}' and site/rules.html has no <section id="pack-${p.id}">.\n` +
        `  Sections are hand-written (heading + table-of-contents entry), so add this shell and re-run:\n\n` +
        [`    <section id="pack-${p.id}">`,
         `      <h2><code>${p.id}</code></h2>`,
         `      <div class="table-scroll">`,
         `        <table>`,
         `          <thead>`,
         `            <tr><th>Rule id</th><th>Severity</th><th>Matcher</th><th>Detects</th></tr>`,
         `          </thead>`,
         `          <tbody>`,
         `          </tbody>`,
         `        </table>`,
         `      </div>`,
         `    </section>`,
         ``,
         `  ...plus <a href="#pack-${p.id}" class="docs-toc__link">${p.id}</a> in the page's docs-toc.`].join("\n"));
  }
  if (!seen.has("native-analyses")) die("site/rules.html has no <section id=\"native-analyses\"> to fill");
  return out;
}

/* ---------- main ------------------------------------------------------------------------------ */

const check = process.argv.includes("--check");
for (const extra of process.argv.slice(2)) {
  if (extra !== "--check") die(`unknown argument '${extra}' (usage: gen-site-rules.mjs [--check])`);
}

const catalog = parseCatalog(fs.readFileSync(CATALOG, "utf8"));
const before = fs.readFileSync(SITE, "utf8");
const after = spliceSite(before, catalog);

const dslCount = catalog.packs.reduce((n, p) => n + p.rules.length, 0);
const total = dslCount + catalog.native.length;
const census = `${catalog.packs.length} packs, ${dslCount} DSL rows, ${catalog.native.length} native rows`;

if (!check) {
  if (before !== after) fs.writeFileSync(SITE, after);
  console.log(`gen-site-rules: ${before === after ? "already current" : "rewrote"} site/rules.html (${census})`);
  process.exit(0);
}

if (before === after) {
  console.log(`gen-site-rules: OK -- site/rules.html rule rows are exactly what docs/rules/catalog.md generates (${census})`);
  process.exit(0);
}

/* A whole-file diff would be unreadable here (a single row runs past 3500 columns), so the failure
 * is reported per ROW: which ids differ, and for each, the column at which the two texts part. */
const rowsOf = (s) => {
  const map = new Map();
  for (const line of s.split(/\r?\n/)) {
    const m = /^\s*<tr><td><code>([a-z0-9][a-z0-9/_-]*)<\/code><\/td>/.exec(line);
    if (m) map.set(m[1], line);
  }
  return map;
};
const oldRows = rowsOf(before);
const newRows = rowsOf(after);
const changed = [];
for (const [id, line] of newRows) {
  const old = oldRows.get(id);
  if (old === undefined) { changed.push([id, "row is MISSING from site/rules.html"]); continue; }
  if (old === line) continue;
  let i = 0;
  while (i < old.length && i < line.length && old[i] === line[i]) i++;
  changed.push([id, `differs at column ${i}\n      site:    ...${old.slice(i, i + 100)}\n      catalog: ...${line.slice(i, i + 100)}`]);
}
for (const id of oldRows.keys()) if (!newRows.has(id)) changed.push([id, "row on the site has no catalog row (stale or invented)"]);

console.error(`gen-site-rules: FAILED -- ${changed.length} of ${total} rule rows in site/rules.html are not what docs/rules/catalog.md generates.`);
for (const [id, why] of changed) console.error(`  ${id}: ${why}`);
console.error("");
console.error("  site/rules.html's rule rows are GENERATED. Fix docs/rules/catalog.md (the SSOT, machine-pinned");
console.error("  to the engine by crates/engine/tests/rule_contracts/), then run: node scripts/gen-site-rules.mjs");
process.exit(1);
