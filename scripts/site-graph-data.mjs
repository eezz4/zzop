/* Regenerate the data block inside site/graph.html, and every published count that describes it.
 *
 * WHY THIS EXISTS
 * site/graph.html draws this repository's own import graph. The data is INLINE — it has to be,
 * because site/README.md promises the site is `file://`-safe and a `fetch()` of a sibling .json
 * breaks that. Inline data with no generator, though, is a fossil: correct the day it was built and
 * unreproducible afterwards, which is exactly the staleness class this repo keeps paying for. So the
 * page carries markers around one script tag, and this script rewrites only what is between them.
 * Nothing else in the page is touched, so the viewer stays hand-maintained.
 *
 * USAGE
 *   cargo build --release -p zzop-cli-bin
 *   target/release/zzop graph --domain dep --format cosmograph-nodes --config zzop.config.jsonc > /tmp/n.ndjson
 *   target/release/zzop graph --domain dep --format cosmograph-links --config zzop.config.jsonc > /tmp/l.ndjson
 *   node scripts/site-graph-data.mjs /tmp/n.ndjson /tmp/l.ndjson
 *
 * WHY THE LAYOUT IS COMPUTED HERE AND NOT IN THE BROWSER
 * A precomputed layout costs the visitor nothing, and — because there is no RNG in this file — makes
 * the picture a pure function of the graph: same repository, same drawing, every visit. That is the
 * determinism the analyzer itself promises, extended to its illustration. Run this twice on the same
 * input and the bytes are identical; that property is worth more than a prettier layout.
 *
 * WHY THE PHYSICS LOOKS LIKE d3-force RATHER THAN A NAIVE SPRING LOOP
 * The first version used `f = (d - rest) * k` with velocities that only decayed, and it DIVERGED:
 * measured spans hit 1e+86 by step 100 and 1e+200 by step 400, after which normalising into a fixed
 * box collapsed every node onto a single horizontal line — the page shipped that way until someone
 * looked at it on a phone. A spring whose force grows without bound, integrated with too large a
 * step, is positive feedback. Three things fix it and all three are what d3-force does:
 *   1. alpha scales the FORCE, never the position update. Scaling position is not annealing, it is a
 *      variable timestep, and a variable timestep is what makes an unstable integrator explode.
 *   2. Link strength is normalised by 1/min(deg a, deg b), so a hub with 200 edges does not take 200
 *      unattenuated impulses per tick.
 *   3. Velocity decays each tick AND is clamped, so no single step can fling a node out of the world.
 * The aspect-ratio assertion at the bottom is the regression test for that bug: it throws rather than
 * writing a squashed picture back into the page.
 */
import fs from 'fs';
import path from 'path';

const [nodesPath, linksPath] = process.argv.slice(2);
if (!nodesPath || !linksPath) {
  console.error('usage: node scripts/site-graph-data.mjs <nodes.ndjson> <links.ndjson>');
  process.exit(2);
}

const SITE = path.join(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')), '..', 'site');
const PAGE = path.join(SITE, 'graph.html');
const ARCH = path.join(SITE, 'architecture.html');
const BEGIN = '/* zzop:dep-data:begin */';
const END = '/* zzop:dep-data:end */';

const readNdjson = p => fs.readFileSync(p, 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l));
const rawNodes = readNdjson(nodesPath);
const rawLinks = readNdjson(linksPath);

/* Compact the two tables into indexed arrays. The page is inline, so every kilobyte is a kilobyte the
   visitor downloads; folder strings dedupe to an index and the row becomes positional. */
const folders = [], fIdx = new Map(), idx = new Map();
const rows = rawNodes.map((n, i) => {
  idx.set(n.id, i);
  if (!fIdx.has(n.folder)) { fIdx.set(n.folder, folders.length); folders.push(n.folder); }
  return [n.label, fIdx.get(n.folder), n.fanIn, n.fanOut, n.inCycle ? 1 : 0];
});
const links = rawLinks
  .map(l => [idx.get(l.source), idx.get(l.target)])
  .filter(p => p[0] != null && p[1] != null);

// ── layout ────────────────────────────────────────────────────────────────────
const N = rows.map(() => ({ x: 0, y: 0, vx: 0, vy: 0 }));
const deg = new Array(N.length).fill(0);
for (const [a, b] of links) { deg[a]++; deg[b]++; }
const L = links.map(([a, b]) => ({
  a: N[a], b: N[b],
  k: 1 / Math.min(deg[a] || 1, deg[b] || 1),
  bias: (deg[a] || 1) / ((deg[a] || 1) + (deg[b] || 1)),
}));

// Golden-angle spiral seed: deterministic, and evenly spread so the first ticks do useful work
// instead of untangling a pile.
const R0 = Math.sqrt(N.length) * 14;
N.forEach((n, i) => {
  const t = i * 2.399963229728653, r = R0 * Math.sqrt((i + 0.5) / N.length);
  n.x = Math.cos(t) * r; n.y = Math.sin(t) * r;
});

const ITER = 600;
const REPEL = 190, CUTOFF = 190, CELL = CUTOFF, CUTOFF2 = CUTOFF * CUTOFF;
const REST = 26;
// This graph is NOT connected — the repository's files fall into hundreds of components, many of them
// single files with no import at all. Repulsion has a cutoff (it must, or this is O(n^2) per tick), so
// separate components feel nothing from each other and would drift apart forever. This weak pull is
// what holds them in one frame.
const CENTRE = 0.0009;
const DECAY = 0.6, VMAX = 18;
const alphaDecay = 1 - Math.pow(0.001, 1 / ITER);

let alpha = 1;
for (let step = 0; step < ITER; step++) {
  const grid = new Map();
  for (const n of N) {
    const key = Math.floor(n.x / CELL) + ',' + Math.floor(n.y / CELL);
    let c = grid.get(key); if (!c) grid.set(key, c = []);
    c.push(n);
  }
  for (const n of N) {
    const cx = Math.floor(n.x / CELL), cy = Math.floor(n.y / CELL);
    for (let dx = -1; dx <= 1; dx++) for (let dy = -1; dy <= 1; dy++) {
      const c = grid.get((cx + dx) + ',' + (cy + dy));
      if (!c) continue;
      for (const m of c) {
        if (m === n) continue;
        let ddx = n.x - m.x, ddy = n.y - m.y, d2 = ddx * ddx + ddy * ddy;
        if (d2 > CUTOFF2) continue;
        // Two nodes exactly on top of each other have no direction to separate along. Nudge along a
        // deterministic axis derived from their order, never a random one.
        if (d2 < 1e-6) { ddx = n.x < m.x ? -0.5 : 0.5; ddy = 0.5; d2 = 0.5; }
        const w = REPEL * alpha / d2;
        n.vx += ddx * w; n.vy += ddy * w;
      }
    }
  }
  for (const l of L) {
    let dx = l.b.x - l.a.x, dy = l.b.y - l.a.y;
    let d = Math.hypot(dx, dy);
    if (d < 1e-6) { dx = 0.5; dy = 0.5; d = 0.7071; }
    const f = (d - REST) / d * alpha * l.k;
    l.b.vx -= dx * f * l.bias;       l.b.vy -= dy * f * l.bias;
    l.a.vx += dx * f * (1 - l.bias); l.a.vy += dy * f * (1 - l.bias);
  }
  for (const n of N) {
    n.vx -= n.x * CENTRE; n.vy -= n.y * CENTRE;
    n.vx *= DECAY; n.vy *= DECAY;
    const v = Math.hypot(n.vx, n.vy);
    if (v > VMAX) { n.vx = n.vx / v * VMAX; n.vy = n.vy / v * VMAX; }
    n.x += n.vx; n.y += n.vy;   // alpha scales force, NOT this line — see the header.
  }
  alpha -= alpha * alphaDecay;
}

const xs = N.map(n => n.x), ys = N.map(n => n.y);
const minX = Math.min(...xs), maxX = Math.max(...xs);
const minY = Math.min(...ys), maxY = Math.max(...ys);
const spanX = maxX - minX, spanY = maxY - minY;
if (!isFinite(spanX) || !isFinite(spanY)) throw new Error('layout diverged to a non-finite span');
const ratio = Math.max(spanX, spanY) / Math.max(1e-9, Math.min(spanX, spanY));
if (ratio > 3) {
  throw new Error(`layout collapsed: aspect ratio ${ratio.toFixed(1)} (x ${spanX.toFixed(0)}, y ${spanY.toFixed(0)}) — see this file's header`);
}

const k = 1000 / Math.max(spanX, spanY);
const cx = (minX + maxX) / 2, cy = (minY + maxY) / 2;
const r1 = v => Math.round(v * 10) / 10;
const payload = JSON.stringify({
  f: folders,
  n: rows.map((r, i) => [...r, r1((N[i].x - cx) * k), r1((N[i].y - cy) * k)]),
  l: links,
});

// ── splice ────────────────────────────────────────────────────────────────────
const page = fs.readFileSync(PAGE, 'utf8');
const from = page.indexOf(BEGIN), to = page.indexOf(END);
if (from < 0 || to < 0) throw new Error(`markers not found in ${PAGE} — expected ${BEGIN} … ${END}`);
// The tracked site is CRLF on a Windows checkout. Splicing bare \n in left the file `w/mixed`, which
// git reports as modified forever after even though the normalised content is identical — a permanent
// false positive in `git status` is the same class of noise this script exists to remove.
const eol = page.includes('\r\n') ? '\r\n' : '\n';
const next0 = page.slice(0, from + BEGIN.length) + eol + 'window.ZZOP_DEP = ' + payload + ';' + eol + page.slice(to);

// ── the prose counts, which are part of the same claim ────────────────────────
// The page argues that NOTHING IS DROPPED, so a total in its text that the drawing disagrees with is
// not a typo — it is the page contradicting itself. Those numbers were hand-typed and drifted on the
// very first regeneration after they were written (text 1,074/1,969 while the tables held 1,078/1,974).
// They are rewritten here from the counts this script just computed, so there is nothing left for
// anyone to remember.
//
// THE LIST IS NOT LIMITED TO graph.html, and that was learned the expensive way. The first version of
// this block anchored the two sentences on the page it was already editing and stopped there — while a
// THIRD copy of the same count sat in site/architecture.html's CommonIr `dep` row, one link away, still
// reading 1,074/1,969. A generator that owns two of three copies has not fixed the defect class it was
// written to fix; it has only moved the survivor somewhere less likely to be looked at. So the anchor
// list is keyed by FILE, and the question to ask before adding a sentence anywhere on the site is
// whether this script should own its number — `grep -rn 'files and .* imports' site/` is the check.
//
// KNOWN-UNOWNED (documented, not silently ignored): crates/summary/src/graph/mod.rs's module doc says
// "Measured on zzop's own tree, mermaid draws 40 of 1073 files". Same number class, and it has already
// drifted (1073 vs 1078). It is left out on purpose: it is a PAST MEASUREMENT justifying why the
// cosmograph format exists, correct as written about the tree it was taken on — and a node script that
// rewrites Rust source would make `cargo build`'s inputs depend on having run this. If that doc comment
// is ever reworded into a present-tense claim about today's tree, it belongs in the list above instead.
//
// A missing anchor is a HARD ERROR, never a silent skip: the stale-count case is exactly the one
// where someone believes they have already regenerated the page. A skip would report success while
// leaving the stale number in place, which is strictly worse than never having run at all. If a
// sentence is reworded, re-anchor it here in the same edit.
const nodeCount = rows.length;
const linkCount = links.length;
const commas = (n) => n.toLocaleString('en-US');
const proseSites = [
  {
    file: PAGE,
    what: 'lede total',
    re: /— [\d,]+ nodes, [\d,]+ edges, nothing/,
    to: `— ${commas(nodeCount)} nodes, ${commas(linkCount)} edges, nothing`,
  },
  {
    file: PAGE,
    what: 'canvas aria-label',
    re: /zzop repository: \d+ files, \d+ imports/,
    to: `zzop repository: ${nodeCount} files, ${linkCount} imports`,
  },
  {
    file: ARCH,
    what: 'CommonIr `dep` row',
    re: /Rendered against this repository<\/a>: [\d,]+ files and [\d,]+ imports/,
    to: `Rendered against this repository</a>: ${commas(nodeCount)} files and ${commas(linkCount)} imports`,
  },
];

// One buffer per file: graph.html arrives with the data block already spliced in, anything else is
// read on first use. Nothing is written until every anchor has matched, so a hard error leaves the
// whole site untouched rather than half-rewritten.
const buffers = new Map([[PAGE, next0]]);
for (const site of proseSites) {
  if (!buffers.has(site.file)) buffers.set(site.file, fs.readFileSync(site.file, 'utf8'));
  const before = buffers.get(site.file);
  if (!site.re.test(before)) {
    throw new Error(
      `${site.what} not found in ${site.file} — this script owns that number, so a reworded sentence ` +
        'must be re-anchored here rather than left to drift'
    );
  }
  buffers.set(site.file, before.replace(site.re, site.to));
}
for (const [file, content] of buffers) fs.writeFileSync(file, content);

const kb = (f) => (buffers.get(f).length / 1024).toFixed(1);
console.log(
  `site/graph.html: ${rows.length} nodes, ${links.length} links, aspect ${ratio.toFixed(2)}, ${kb(PAGE)} KB` +
    ` · site/architecture.html: counts rewritten (${commas(nodeCount)} files, ${commas(linkCount)} imports)`
);
