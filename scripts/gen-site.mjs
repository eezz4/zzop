// Next-generation site generator — turns the sentences in site-src/content/*.mjs into two editions.
//
// Run:      node scripts/gen-site.mjs
// Writes:   site/index.html      English edition   (<html lang="en">)
//           site/ko/index.html   Korean edition    (<html lang="ko">)
//
// Run:      node scripts/gen-site.mjs --artifact
// Writes:   target-site/zzop-v2-en.html, target-site/zzop-v2-ko.html
//           FRAGMENTS — no <!doctype>, <html>, <head> or <body>. That is the shape an artifact
//           preview takes, and reviewing this site on a phone goes through that path. The directory
//           is under the repo's `/target-*/` ignore rule, so previews never reach a commit.
//
// The third edition, which toggled both languages inside one file, was removed on 2026-08-14 — once
// the language switch became a link you can compare by moving between two pages, and three editions
// means three editions to maintain.
//
// No sentence lives in this file. Sentences live exactly once, as {ko, en} pairs, in
// site-src/content/*.mjs, and site/render.mjs picks the side each edition needs. If one side of a
// pair is missing the build dies naming the slot — which is why staleness here needs no guard.
//
// That rule now covers the SHELL too (title, description, nav labels, footer, the graph viewer's UI
// strings). Those used to be Korean/English literals in this file; they moved to
// site-src/content/shell.mjs. The reason is mechanical: CI's check-english-source.sh scans the whole
// repo (SUBJECT_PATHSPEC='*') for non-Latin letters, and narrowing its exemption to the single path
// site-src/content/ is only possible if nothing else — this file, site/render.mjs, site-src/*.css —
// carries a Korean character.

import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";
import { renderPage, HANGUL } from "./site/render.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..");
const CONTENT = join(repo, "site-src", "content");
const CSS_SRC = join(repo, "site-src", "site-v2.css");
const SITE = join(repo, "site");
const PREVIEW = join(repo, "target-site");

// Fragment mode, for reviewing the page as an artifact. Everything else about the two editions is
// identical — same content, same stylesheet, same graph viewer.
const ARTIFACT = process.argv.includes("--artifact");

const shell = (await import(pathToFileURL(join(CONTENT, "shell.mjs")).href)).default;
const PAGES = shell.pages;

/** One shell sentence in this edition's language. Dies naming the key rather than writing "undefined"
 *  into a <title> or a <meta content>, which is the failure nobody notices until it is published. */
function say(pair, mode, where) {
  if (!pair || typeof pair[mode] !== "string" || !pair[mode].trim()) {
    console.error(`\nSHELL CONTENT MISSING — shell.${where}.${mode}`);
    console.error(`  site-src/content/shell.mjs has to carry both sides of this pair.\n`);
    process.exit(1);
  }
  return pair[mode];
}

if (!Array.isArray(PAGES) || !PAGES.length) {
  console.error("\nshell.pages is empty — site-src/content/shell.mjs owns the tab list.\n");
  process.exit(1);
}

const missing = PAGES.filter((p) => !existsSync(join(CONTENT, `${p.src}.mjs`)));
if (missing.length) {
  console.error("MISSING CONTENT — build stopped:");
  for (const m of missing) console.error(`  site-src/content/${m.src}.mjs  (${m.id})`);
  process.exit(1);
}

const content = {};
for (const p of PAGES) {
  content[p.src] = (await import(pathToFileURL(join(CONTENT, `${p.src}.mjs`)).href)).default;
}

const css = readFileSync(CSS_SRC, "utf8");

// ── dependency graph viewer ─────────────────────────────────────────────────
// Neither the coordinate data (~105 KB) nor the viewer (~21 KB) is COPIED HERE. Both are sliced out
// of the original site/graph.html at build time, by marker.
//
// Why: that data is owned by scripts/site-graph-data.mjs. Keep a copy and every regeneration of the
// original leaves this side stale with nobody the wiser — coordinates always look plausible. Sliced,
// this page follows the original for free. The price is that a change in the original's shape makes
// this DIE NAMING THE MARKER: quietly shipping a page with no picture is the worst outcome available
// to this design.
const GRAPH_SRC = join(SITE, "graph.html");
const DATA_BEGIN = "/* zzop:dep-data:begin */";
const DATA_END = "/* zzop:dep-data:end */";

function graphDie(what, how) {
  console.error(`\nGRAPH VIEWER ASSEMBLY FAILED — ${what}`);
  console.error(`  original: ${GRAPH_SRC}`);
  console.error(`  ${how}`);
  console.error(`  Stopping here rather than emitting a graph page with no picture in it.\n`);
  process.exit(1);
}

/** Slice the data block and the viewer script out of the original. Missing either one dies by name. */
function sliceGraph() {
  if (!existsSync(GRAPH_SRC)) graphDie("the original file is not there", "site/graph.html has to exist.");
  // The original is CRLF. Every needle below is written against \n, so flatten it first.
  const src = readFileSync(GRAPH_SRC, "utf8").replace(/\r\n/g, "\n");

  const i = src.indexOf(DATA_BEGIN);
  const j = src.indexOf(DATA_END, i + 1);
  if (i < 0) graphDie(`marker '${DATA_BEGIN}' not found`, "that is the data block's opening marker.");
  if (j < 0) graphDie(`marker '${DATA_END}' not found`, "that is the data block's closing marker.");

  const data = src.slice(i + DATA_BEGIN.length, j).trim();
  if (!data.startsWith("window.ZZOP_DEP")) {
    graphDie(
      "what sits between the markers does not start with 'window.ZZOP_DEP'",
      `first 40 characters sliced: ${JSON.stringify(data.slice(0, 40))}`
    );
  }

  // The viewer is the <script> AFTER the data. There is no '</script>' literal inside the viewer, so
  // the first closing tag is its end.
  const afterData = src.indexOf("</script>", j);
  const vOpen = afterData < 0 ? -1 : src.indexOf("<script>", afterData);
  const vClose = vOpen < 0 ? -1 : src.indexOf("</script>", vOpen);
  if (vClose < 0) graphDie("no <script> after the data block", "the viewer script has to be there.");
  const viewer = src.slice(vOpen + "<script>".length, vClose);

  for (const needle of ["window.ZZOP_DEP", "getElementById('depgraph')"]) {
    if (!viewer.includes(needle)) {
      graphDie(`the <script> after the data has no '${needle}'`, "something other than the viewer got sliced.");
    }
  }
  return { data, viewer };
}

/** Die by name when a needle is absent — it means the original viewer changed, and letting that pass
 *  ships a page whose UI strings are English only (and nobody finds out). */
function swap(src, needle, next, label) {
  const hit = typeof needle === "string" ? src.includes(needle) : needle.test(src);
  if (!hit) {
    graphDie(
      `could not find the '${label}' slot in the viewer`,
      `looking for: ${String(needle).replace(/\s+/g, " ").trim().slice(0, 100)}`
    );
  }
  return src.replace(needle, () => next);
}

const CENSUS_NEEDLE =
  `document.getElementById('gcensus').textContent =\n` +
  `    nodes.length.toLocaleString() + ' of ' + nodes.length.toLocaleString() + ' files, ' +\n` +
  `    links.length.toLocaleString() + ' of ' + links.length.toLocaleString() +\n` +
  `    ' import edges, 0 circular finding(s). UNCAPPED, --top does not apply to this format.';`;

const DETAIL_NEEDLE =
  `'<ul><li>imported by <b></b></li><li>imports <b></b></li><li>degree <b></b></li></ul>' +\n` +
  `      (n.cycle ? '<span class="cyc">in cycle</span>' : '');`;

/** Swap the viewer's English literals for this edition's strings. The logic is left alone. */
function patchViewer(viewer, mode) {
  const T = {};
  for (const [k, v] of Object.entries(shell.viewer)) T[k] = say(v, mode, `viewer.${k}`);

  let s = viewer;
  s = swap(
    s,
    "var D = window.ZZOP_DEP;",
    `var D = window.ZZOP_DEP;\n\n` +
      `  /* UI strings, injected per language edition by scripts/gen-site.mjs. The viewer's own\n` +
      `     English literals were the one place a sentence could not be translated. */\n` +
      `  var T = ${JSON.stringify(T)};`,
    "var D = window.ZZOP_DEP;"
  );

  // Still textContent — one edition carries one language, so there is no reason to put markup in.
  s = swap(
    s,
    CENSUS_NEEDLE,
    `document.getElementById('gcensus').textContent = T.census\n` +
      `    .replace(/\\{files\\}/g, nodes.length.toLocaleString())\n` +
      `    .replace(/\\{edges\\}/g, links.length.toLocaleString());`,
    "gcensus census line"
  );

  s = swap(
    s,
    /var EMPTY = '<p class="empty">[\s\S]*?<\/p>';/,
    `var EMPTY = '<p class="empty">' + T.empty + '</p>';`,
    "empty-state text (EMPTY)"
  );

  s = swap(
    s,
    DETAIL_NEEDLE,
    `'<ul><li>' + T.fanIn + ' <b></b></li><li>' + T.fanOut + ' <b></b></li>' +\n` +
      `      '<li>' + T.degree + ' <b></b></li></ul>' +\n` +
      `      (n.cycle ? '<span class="cyc">' + T.cyc + '</span>' : '');`,
    "detail panel labels"
  );

  return s;
}

const GRAPH = sliceGraph();

// The KO/EN switch is a LINK. The editions are separate pages, so the URL is the language, and the
// English page holds no Hangul at all — which is what keeps the whole of site/ under the English
// guard. Deployed, the two sit one directory apart; the defaults below say exactly that.
// The env overrides exist only for artifact review, where the two previews have to point at each
// other's artifact URL. In deploy mode they are ignored: site/ has a fixed layout, and a stray
// environment variable must not be able to rewrite a published link.
const ALT = ARTIFACT
  ? { en: process.env.ZZOP_ALT_KO || "ko/", ko: process.env.ZZOP_ALT_EN || "../" }
  : { en: "ko/", ko: "../" };

// The standalone pages that are NOT generated by this file — they live at the root
// of site/, are English-only, and every edition links to the same copy. The English
// index sits at that root, so `href="rules.html"` is already correct for it. The
// Korean index sits one directory down (site/ko/index.html), so the SAME relative
// href resolves to site/ko/rules.html — a 404. There is no site/ko/ copy of these
// pages, so any link to one from the Korean edition must climb one directory. The
// link author writes the plain root href once; the Korean pass below prefixes `../`.
// A new root page just needs its name added here.
const ROOT_PAGES = ["x-showcase", "rules", "reference", "graph", "privacy", "architecture", "usage"];
const ROOT_LINK_RE = new RegExp(`href="((?:${ROOT_PAGES.join("|")})\\.html)`, "g");
const fixRootLinks = (html, mode) =>
  mode === "ko" ? html.replace(ROOT_LINK_RE, 'href="../$1') : html;

// Published origin. Only hreflang uses it: those links have to be fully qualified to be honoured,
// while every other link on the page stays relative so site/ keeps working over file://.
const ORIGIN = "https://eezz4.github.io/zzop/";

// One identical, self-referential set on both editions — that is the shape hreflang requires.
const ALTERNATES = [
  `<link rel="alternate" hreflang="en" href="${ORIGIN}index.html">`,
  `<link rel="alternate" hreflang="ko" href="${ORIGIN}ko/index.html">`,
  `<link rel="alternate" hreflang="x-default" href="${ORIGIN}index.html">`,
].join("\n");

// ---------------------------------------------------------------------------

const MALGUN = `"Malgun Gothic"`;

function sheetFor(mode) {
  // Strip the author notes (comments). `.langbtn` stays — the language switch LINK wears that class.
  let s = css.replace(/\/\*[\s\S]*?\*\//g, "");

  // The Korean-language alias of Malgun Gothic cannot be written in the stylesheet (English-only
  // path), so it is appended here, for the Korean edition only. The English edition gets nothing
  // appended: the Latin name right there is the same font.
  if (mode === "ko") {
    const alias = shell.hangulFontAlias;
    if (typeof alias !== "string" || !alias.trim()) {
      console.error("\nshell.hangulFontAlias is empty — site-src/content/shell.mjs owns that value.\n");
      process.exit(1);
    }
    const n = s.split(MALGUN).length - 1;
    if (n !== 1) {
      console.error(`\nSTYLESHEET FONT SLOT — expected ${MALGUN} exactly once, found ${n}.`);
      console.error(`  ${CSS_SRC}`);
      console.error(`  The Korean edition appends the Hangul font alias right after it; a missing or`);
      console.error(`  duplicated slot would silently drop or duplicate the fallback.\n`);
      process.exit(1);
    }
    s = s.replace(MALGUN, `${MALGUN},"${alias}"`);
  }

  return s.replace(/\n{3,}/g, "\n\n").trim();
}

// One nav, one footer — the SINGLE source for the generated index AND for the
// standalone pages gen-site injects into (x-showcase, graph, reference, rules,
// privacy). Before this, the index built its nav here while each standalone page
// hand-wrote its own; the two drifted (a separator here, a language switch there)
// and that drift is exactly what "the menus look different" was. Two context knobs:
//   crossPage  the index's tabs toggle IN-PAGE (#id, .nav__link + data-page, the
//              section script drives is-here); a standalone page's tabs are links
//              INTO the index (index.html#id), and `active` marks one is-here here
//              since there is no script there to do it.
//   active     for a standalone page: which entry is the current page — a page id
//              ("p-graph"), an extra's href ("x-showcase.html"), or null for a
//              footer-reached reference page that highlights nothing.
function navMarkup(mode, { crossPage = false, active = null } = {}) {
  const tabHref = (p) =>
    crossPage ? (p.id === "p-index" ? "index.html" : `index.html#${p.id}`) : `#${p.id}`;
  const tabs = PAGES.map((p, i) => {
    const here = crossPage ? active === p.id : i === 0;
    const data = crossPage ? "" : ` data-page="${p.id}"`;
    return (
      `<a href="${tabHref(p)}" class="nav__link${here ? " is-here" : ""}"${data}>` +
      `${say(p.label, mode, `pages.${p.src}.label`)}</a>`
    );
  }).join("\n    ");

  // Extras link to standalone pages and render on BOTH editions, so the menu layout
  // is identical in English and Korean. The label stays English (the target page is
  // English-only); the Korean edition resolves the href to ../x-showcase.html via
  // fixRootLinks. The separator that sets them off from the tabs is the `.nav__extra`
  // class (defined in both stylesheets), not an inline style, so the same markup
  // styles correctly whether the page inlines site-v2.css or links assets/site.css.
  const extras = (shell.extras || [])
    .map((x) => `<a href="${x.href}" class="nav__extra${active === x.href ? " is-here" : ""}">${x.label}</a>`)
    .join("\n    ");

  // The language switch. On the index the section script rewrites its href per tab;
  // a standalone page has no such script, so it points at the other edition's home.
  const langBtn =
    `<a class="langbtn" id="langlink" href="${ALT[mode]}" hreflang="${mode === "en" ? "ko" : "en"}"` +
    ` aria-label="${say(shell.langLink, mode, "langLink")}">` +
    (mode === "en" ? `<b>EN</b> · KO` : `EN · <b>KO</b>`) +
    `</a>`;

  return `<nav class="nav">
  <span class="nav__brand">zzop</span>
  <div style="display:flex;flex-wrap:wrap;align-items:baseline;gap:.1rem">
    ${tabs}
    ${extras}
  </div>
  ${langBtn}
</nav>`;
}

function footMarkup(mode) {
  // Privacy is a legal page and must be reachable from EVERY page and edition —
  // it is not a tab and not an extra, so before this unified footer each hand-written
  // page footer carried its own <a href="privacy.html">. Folding all footers into one
  // source almost dropped that link everywhere; it lives here, on both editions. It is
  // English-only, so the Korean index resolves it to ../privacy.html via fixRootLinks;
  // on the standalone pages (already at site/ root) the plain href is already correct.
  return `<footer class="foot">
  <span class="foot__brand">zzop</span>
  <span>Zero Zone Of Pain · MIT</span>
  ${mode === "en" ? shell.footEnOnly : ""}
  <a href="privacy.html">Privacy</a>
  <a href="https://github.com/eezz4/zzop" target="_blank" rel="noreferrer">GitHub</a>
</footer>`;
}

/** Everything between <body> and </body>, identical in both output modes. */
function bodyFor(mode) {
  const body = PAGES.map((p, i) => {
    const frag = renderPage(content[p.src], mode, p.src);
    return `<section class="page" id="${p.id}"${i === 0 ? "" : " hidden"}>\n${frag}\n</section>`;
  }).join("\n\n");

  const doc = `${navMarkup(mode)}

${body}

${footMarkup(mode)}

<script>
(function () {
  var links = [].slice.call(document.querySelectorAll(".nav__link"));
  var pages = [].slice.call(document.querySelectorAll(".page"));

  // the language link carries the tab you are on, so KO/EN lands on the same page
  var alt = document.getElementById("langlink");
  var altBase = alt ? alt.getAttribute("href") : null;

  function show(id) {
    pages.forEach(function (p) { p.hidden = p.id !== id; });
    links.forEach(function (a) { a.classList.toggle("is-here", a.dataset.page === id); });
    if (window.history && history.replaceState) history.replaceState(null, "", "#" + id);
    if (alt) alt.href = altBase + "#" + id;
    window.scrollTo(0, 0);
    // A hidden page has no box, so the graph canvas measures 0x0 and draws nothing. Modern
    // browsers get this from the ResizeObserver the viewer puts on the canvas; this line is
    // for the fallback path, where the viewer listens on window instead.
    window.dispatchEvent(new Event("resize"));
  }
  links.forEach(function (a) {
    a.addEventListener("click", function (e) { e.preventDefault(); show(a.dataset.page); });
  });
  // A link INTO a section that is NOT a nav tab changes the hash without passing
  // through the click handler above: an in-content cross-reference like "see the
  // Contract" (href="#p-contract"), a deep link landing from another page, or the
  // back/forward button. Without this the target section would stay hidden and the
  // click would do nothing visible. Route the initial load AND every later hash
  // change through one guarded show that fires only for a real section id.
  function showHash() {
    var id = location.hash.slice(1);
    for (var i = 0; i < pages.length; i++) {
      if (pages[i].id === id) { show(id); return; }
    }
  }
  window.addEventListener("hashchange", showHash);
  if (location.hash) showHash();
})();
</script>

<script>
/* zzop:dep-data — sliced from site/graph.html at build time by scripts/gen-site.mjs.
   Do not edit here: scripts/site-graph-data.mjs owns this block. */
${GRAPH.data}
</script>

<script>${patchViewer(GRAPH.viewer, mode)}</script>`;

  // Korean edition sits in site/ko/, so links to the root-level standalone pages
  // climb one directory. English is already at the root and passes through unchanged.
  return fixRootLinks(doc, mode);
}

/** Fragment, for artifact review: title + style + body, no document wrapper. */
function buildFragment(mode) {
  return `<title>${say(shell.title, mode, "title")}</title>
<style>
${sheetFor(mode)}
</style>

${bodyFor(mode)}
`;
}

/** Complete document, for site/. The <head> follows the convention every other page in site/ already
 *  uses — charset, title, description, viewport — in that order. The one thing it does not carry is
 *  <link rel="stylesheet" href="assets/site.css">: this page ships its own design system inline, and
 *  it has to, because an artifact preview of the same bytes cannot fetch an external stylesheet. */
function buildDocument(mode) {
  return `<!doctype html>
<html lang="${mode}">
<head>
<meta charset="utf-8">
<title>${say(shell.title, mode, "title")}</title>
<meta name="description" content="${say(shell.description, mode, "description")}">
<meta name="viewport" content="width=device-width, initial-scale=1">
${ALTERNATES}
<style>
${sheetFor(mode)}
</style>
</head>
<body>

${bodyFor(mode)}

</body>
</html>
`;
}

const TARGETS = ARTIFACT
  ? [
      ["en", join(PREVIEW, "zzop-v2-en.html")],
      ["ko", join(PREVIEW, "zzop-v2-ko.html")],
    ]
  : [
      ["en", join(SITE, "index.html")],
      ["ko", join(SITE, "ko", "index.html")],
    ];

for (const [mode, path] of TARGETS) {
  let out;
  try {
    out = ARTIFACT ? buildFragment(mode) : buildDocument(mode);
  } catch (e) {
    console.error(`\nBUILD FAILED (${mode}):\n  ${e.message}\n`);
    process.exit(1);
  }
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, out, "utf8");

  // A single Hangul character left in the English edition means a slot lost its pair — never pass
  // that over in silence. This is also what keeps site/index.html publishable under the repo-wide
  // English guard, so the <head> assembled above is inside the check, not beside it.
  if (mode === "en" && HANGUL.test(out)) {
    const hits = out
      .split("\n")
      .map((l, i) => [i + 1, l])
      .filter(([, l]) => HANGUL.test(l));
    console.error(`\nHANGUL LEAKED INTO THE ENGLISH BUILD — ${hits.length} line(s):`);
    for (const [n, l] of hits.slice(0, 20)) console.error(`  ${path}:${n}  ${l.trim().slice(0, 110)}`);
    if (hits.length > 20) console.error(`  … and ${hits.length - 20} more line(s)`);
    process.exitCode = 1;
  }

  const rel = path.slice(repo.length + 1).replace(/\\/g, "/");
  const kb = (Buffer.byteLength(out, "utf8") / 1024).toFixed(1);
  console.log(`OK  ${rel}  ${kb} KB  (${PAGES.length} pages, ${mode}${ARTIFACT ? ", fragment" : ""})`);
}

// ---------------------------------------------------------------------------
// Standalone pages share the index's nav and footer. gen-site is the one source
// and injects them between markers, so the menus cannot drift apart again (the
// drift between a hand-written nav and this generated one is exactly what made
// "the menus look different"). Each page carries an empty <!--zzop:nav-->…
// <!--/zzop:nav--> (and …:foot…) pair; a page missing a marker dies by name
// rather than shipping a stale hand-written header nobody noticed.

/** Replace whatever sits between <!--tag--> and <!--/tag-->, keeping the markers.
 *  Matches the file's line endings so a CRLF page (graph.html) stays CRLF. */
function injectBetween(html, tag, replacement, file) {
  const open = `<!--${tag}-->`;
  const close = `<!--/${tag}-->`;
  const i = html.indexOf(open);
  const j = html.indexOf(close, i + 1);
  if (i < 0 || j < 0) {
    console.error(`\nSTANDALONE INJECT FAILED — ${file}`);
    console.error(`  marker pair ${open} … ${close} not found.`);
    console.error(`  Every standalone page must carry it where its nav/footer goes.\n`);
    process.exit(1);
  }
  const crlf = html.slice(0, i).includes("\r\n");
  const eol = crlf ? "\r\n" : "\n";
  const block = crlf ? replacement.replace(/\n/g, "\r\n") : replacement;
  return html.slice(0, i + open.length) + eol + block + eol + html.slice(j);
}

if (!ARTIFACT) {
  // Which nav entry is the current page: the X showcase is the "In the field"
  // extra; the reference pages (graph/reference/rules/privacy) are reached from
  // the footer, not a tab, so they highlight nothing — same as the index's own
  // privacy page would.
  const STANDALONE = [
    { file: "x-showcase.html", active: "x-showcase.html" },
    { file: "graph.html", active: null },
    { file: "reference.html", active: null },
    { file: "rules.html", active: null },
    { file: "privacy.html", active: null },
  ];
  for (const { file, active } of STANDALONE) {
    const p = join(SITE, file);
    if (!existsSync(p)) {
      console.error(`\nSTANDALONE PAGE MISSING — ${p} is in the inject list but not on disk.\n`);
      process.exitCode = 1;
      continue;
    }
    let html = readFileSync(p, "utf8");
    html = injectBetween(html, "zzop:nav", navMarkup("en", { crossPage: true, active }), file);
    html = injectBetween(html, "zzop:foot", footMarkup("en"), file);
    writeFileSync(p, html, "utf8");
    console.log(`OK  site/${file}  (nav + footer injected${active ? `, is-here ${active}` : ""})`);
  }
}
