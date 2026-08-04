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
 * WHY LAYERED RATHER THAN FORCE-DIRECTED
 * This used to be a d3-force simulation, and it drew a DISC. A disc is rotationally symmetric, so no
 * direction on the page meant anything: a reader could not tell an entry point from a leaf utility
 * without hovering, and the one question an import graph exists to answer — what sits on top of what —
 * was the question the picture could not answer. The layout is now a ranking. The x axis IS the
 * dependency direction: rank 0 (nothing in this repo imports it) sits at the far LEFT and every import
 * step moves RIGHT, which is how `madge` and graphviz draw the same thing and how people describe
 * their own systems out loud. Same-domain files share a horizontal band, so a domain reads as one body
 * instead of as confetti — that is also what lets the page draw a zoomed-out summary (one bubble per
 * domain) that is the SAME picture at a coarser grain rather than a second, different one.
 *
 * Determinism did not merely survive the change, it got stronger: there is no integration, no repulsion
 * cutoff, no iteration count and no annealing schedule to tune. The coordinates are a pure function of
 * the edge set. Every stability lesson the old physics taught is now moot rather than merely satisfied.
 *
 * WHY RELAXATION AND NOT A TOPOLOGICAL SORT
 * The graph is not a DAG — the repository has import cycles, which is precisely what the `circular`
 * rule reports and what this data marks. A topological order does not exist for a cycle, and breaking
 * one by picking a spanning tree would put a node's column at the mercy of which edge the tie-break
 * happened to cut. Longest-path relaxation saturates instead: a cycle's members settle in the same
 * column, which is the honest drawing, since no member of a cycle is above another.
 *
 * WHY DEGREE-0 FILES ARE PLACED OFF THE AXIS
 * This graph is not connected: it holds well over a hundred components, most of them a SINGLE file that
 * imports nothing and is imported by nothing. No count is written here — every one that was drifted
 * within a batch or two (this paragraph once said 109 and 107 for the same set, three sentences apart),
 * and the live number is emitted into the page by this script anyway ("There are N of them here"), which
 * is the copy that cannot go stale. Such a file has no rank: giving it one would claim a depth the
 * graph never measured, and under the old physics it got worse than that — it drifted to a halo at
 * whatever bearing its seed happened to give it, so its position encoded NOTHING while occupying the
 * frame. They are placed in their own column past the right edge instead, grouped into the same domain
 * bands, which says the only true thing available: nothing imports them, they import nothing, and here
 * is the area they belong to. Nothing is dropped, which matters because this page's whole claim is that
 * it draws everything. Pulling every node toward its folder's centroid was rejected for the same reason
 * it was rejected under the physics: it would make the coordinates encode directory structure as much
 * as imports, on a page titled "import graph".
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
// Five phases: rank (x, the dependency axis) · columns · domain bands (y, wrapped) · the domain
// summary · bubble sizing. WHY it is layered and not a force simulation is in the file header and is
// not restated here — the two copies would drift, and this repository has paid for that already.
const N = rows.map(() => ({ x: 0, y: 0 }));
const deg = new Array(N.length).fill(0);
for (const [a, b] of links) { deg[a]++; deg[b]++; }

const areaOf = i => (folders[rows[i][1]] || '').split('/')[0] || '.';
const pathOf = i => (folders[rows[i][1]] ? folders[rows[i][1]] + '/' : '') + rows[i][0];

// ── phase 1: rank = longest path along import edges, over the CONDENSATION ───
// This graph is NOT a DAG — the repository has import cycles, which is what the `circular` rule
// reports and what this data marks. Longest-path depth is only defined on a DAG, so the cycles are
// removed the one way that invents nothing: collapse each strongly connected component to a single
// vertex (Tarjan), rank the resulting DAG, and give every member of a component that component's
// rank. A cycle's members therefore land in ONE column, which is the honest drawing — no member of a
// cycle sits above another. The rejected alternatives are worth naming because the first one shipped:
//   · Plain relaxation with a pass bound. A cycle ratchets by one every sweep, so the "depth" it
//     reports is the pass bound, not the graph. Measured here: it claimed a deepest rank of 449 on a
//     tree whose real condensation depth is far smaller, and the column compression then squashed the
//     whole repository into 8 columns because the axis was scaled by that phantom 449.
//   · Breaking cycles by picking a spanning tree. A node's column would then depend on which edge the
//     tie-break happened to cut, so the drawing would encode an arbitrary choice as if it were depth.
// Tarjan is iterated rather than recursed: the call depth is the length of the DFS path, and this
// graph has chains long enough that recursion is betting on the engine's stack size.
const out = Array.from({ length: N.length }, () => []);
for (const [s, t] of links) out[s].push(t);

const comp = new Array(N.length).fill(-1);
const low = new Array(N.length).fill(0);
const num = new Array(N.length).fill(-1);
const onStack = new Array(N.length).fill(false);
const sccStack = [];
let counter = 0, compCount = 0;
for (let root = 0; root < N.length; root++) {
  if (num[root] !== -1) continue;
  // Each frame is [vertex, index of the next out-edge to walk].
  const work = [[root, 0]];
  while (work.length) {
    const frame = work[work.length - 1];
    const v = frame[0];
    if (frame[1] === 0) { num[v] = low[v] = counter++; sccStack.push(v); onStack[v] = true; }
    let descended = false;
    while (frame[1] < out[v].length) {
      const w = out[v][frame[1]++];
      if (num[w] === -1) { work.push([w, 0]); descended = true; break; }
      if (onStack[w]) low[v] = Math.min(low[v], num[w]);
    }
    if (descended) continue;
    if (low[v] === num[v]) {
      for (;;) {
        const w = sccStack.pop();
        onStack[w] = false;
        comp[w] = compCount;
        if (w === v) break;
      }
      compCount++;
    }
    work.pop();
    if (work.length) { const p = work[work.length - 1][0]; low[p] = Math.min(low[p], low[v]); }
  }
}

// Tarjan closes a component only after everything it can reach is already closed, so emission order is
// reverse topological: walking component ids downward visits every predecessor before its successors,
// which is exactly the order a longest-path pass needs. No separate sort, and no chance of the sort
// disagreeing with the components it was built from.
const members = Array.from({ length: compCount }, () => []);
for (let v = 0; v < N.length; v++) members[comp[v]].push(v);
const compRank = new Array(compCount).fill(0);
for (let c = compCount - 1; c >= 0; c--) {
  for (const v of members[c]) {
    for (const w of out[v]) {
      if (comp[w] !== c && compRank[comp[w]] < compRank[c] + 1) compRank[comp[w]] = compRank[c] + 1;
    }
  }
}
const rank = N.map((_, i) => compRank[comp[i]]);

// Isolated files (degree 0) are not ON the dependency axis at all — giving them a rank would claim a
// depth the graph never measured. They get their own column past the right edge, which says the only
// true thing about them: nothing imports them and they import nothing.
const connected = [], isolated = [];
for (let i = 0; i < N.length; i++) (deg[i] > 0 ? connected : isolated).push(i);
if (connected.length === 0) {
  throw new Error('layout: no connected nodes — every file is degree 0, so there is no axis to lay out');
}

// ── phase 2: one column per rank, with a compression fuse ────────────────────
// This repository's condensation is shallow enough that every rank gets its own column. MAX_COLS is a
// fuse, not the normal path: a much deeper tree would otherwise draw a strip nobody can read, so ranks
// bucket into at most that many columns. The bucketing is by rank VALUE and never by population —
// equal ranks must not split across columns, or two files at the same depth would read as different
// depths. (The realised count is printed at the end of the run rather than asserted here; a number in
// a comment about a tree that changes every commit is a stale number waiting to happen.)
const MAX_COLS = 26;
const maxRank = connected.reduce((m, i) => Math.max(m, rank[i]), 0);
const colOf = r => (maxRank <= MAX_COLS ? r : Math.round((r / maxRank) * MAX_COLS));
const cols = new Map();
for (const i of connected) {
  const c = colOf(rank[i]);
  if (!cols.has(c)) cols.set(c, []);
  cols.get(c).push(i);
}

// Isolated files get a column of their own two slots past the right edge — far enough to read as "off
// the axis" rather than as a deepest layer. Making it an ordinary column rather than a special case is
// what lets every rule below (domain bands, wrapping, ordering) apply to them unchanged; they simply
// have no neighbours, so the barycentre step has nothing to say about them and their order falls to
// path, which is the only ordering their data supports anyway.
const rankedCols = [...cols.keys()].sort((a, b) => a - b);
if (isolated.length) cols.set(rankedCols[rankedCols.length - 1] + 2, isolated);
const colKeys = [...cols.keys()].sort((a, b) => a - b);

// ── phase 3: y = domain band, wrapped to keep the drawing wide ───────────────
// Same-domain files share a horizontal band, so a domain reads as one body rather than as confetti
// spread over the frame — that is also what makes the zoomed-out view (one bubble per domain, drawn by
// the page) a summary of this same picture rather than a second, different one. Band order is by size
// then name, the order the page's legend already uses, so legend and drawing agree.
const areaSize = new Map();
for (let i = 0; i < N.length; i++) areaSize.set(areaOf(i), (areaSize.get(areaOf(i)) || 0) + 1);
const areaOrder = [...areaSize.entries()]
  .sort((x, y) => y[1] - x[1] || (x[0] < y[0] ? -1 : 1))
  .map(([a]) => a);

const SUB_W = 30, ROW_H = 26, COL_GAP = 86, BAND_GAP = 46;
// The brief was "wide, and do not let it get long", and width is the dependency axis, so the drawing
// should read at least TARGET_ASPECT times wider than it is tall. Naively stacking every file of a
// domain into one vertical run does the opposite: the busiest (column, domain) group here holds well
// over a hundred files, and eight such runs stacked made a tall strip — measured aspect 0.36, i.e.
// nearly three times taller than wide, on a layout whose whole point is horizontal.
//
// So a group taller than CAP rows WRAPS into sub-columns: a run of 136 becomes a block, not a line.
// Sub-column spacing is a fixed SUB_W rather than a fraction of a fixed slot, and the column then
// takes whatever width its widest block needs. That ordering matters — the first attempt divided a
// fixed 150-unit slot among the sub-columns, which packed a 20-wide block into 5.7 units of spacing
// against 26 units between rows, so the "block" was a smear. Uniform spacing costs nothing: columns
// are laid end to end, so a variable width still orders ranks exactly.
//
// CAP is solved for rather than picked. Shrinking it trades height for width monotonically, so take
// the largest cap (fewest sub-columns, least distortion) that still meets the aspect target. What
// survives in the source is the SHAPE this drawing wants, not a row count that silently stops fitting
// the day the repository grows.
const TARGET_ASPECT = 2.0;
const colAreaCount = new Map();
const peak = new Map();
for (const [c, group] of cols) {
  const perArea = new Map();
  for (const i of group) perArea.set(areaOf(i), (perArea.get(areaOf(i)) || 0) + 1);
  colAreaCount.set(c, perArea);
  for (const [a, n] of perArea) peak.set(a, Math.max(peak.get(a) || 0, n));
}
const blockCols = (m, cap) => Math.ceil(m / Math.min(cap, m));
const innerWidth = (c, cap) =>
  Math.max(...[...colAreaCount.get(c).values()].map(m => (blockCols(m, cap) - 1) * SUB_W));
const widthAt = cap => colKeys.reduce((w, c) => w + innerWidth(c, cap) + COL_GAP, 0);
const heightAt = cap =>
  [...peak.values()].reduce((h, p) => h + Math.min(p, cap) * ROW_H + BAND_GAP, 0);
const tallest = Math.max(...peak.values());
let CAP = 1;
for (let cap = tallest; cap >= 1; cap--) {
  if (heightAt(cap) * TARGET_ASPECT <= widthAt(cap)) { CAP = cap; break; }
}

// One band per domain, tall enough for that domain's busiest column after wrapping — bands never
// overlap, so a vertical position always names exactly one domain.
const bandTop = new Map();
let yCursor = 0;
for (const a of areaOrder) {
  if (!peak.has(a)) continue;
  bandTop.set(a, yCursor);
  yCursor += Math.min(peak.get(a), CAP) * ROW_H + BAND_GAP;
}

// Columns are laid end to end at the settled cap, so each one starts where the previous one ended.
const colLeft = new Map();
let xCursor = 0;
for (const c of colKeys) {
  colLeft.set(c, xCursor);
  xCursor += innerWidth(c, CAP) + COL_GAP;
}

// Within a band+column, order by the mean y of the neighbours already placed to the LEFT (a
// barycentre sweep, the standard crossing-reduction step). Columns are walked left to right so the
// pass always reads settled positions; ties fall back to path, keeping the result a pure function of
// the input.
const placed = new Array(N.length).fill(false);
for (const c of colKeys) {
  const group0 = cols.get(c);
  const bary = new Map();
  for (const i of group0) {
    let sum = 0, n = 0;
    for (const [s, t] of links) {
      if (t === i && placed[s]) { sum += N[s].y; n++; }
      else if (s === i && placed[t]) { sum += N[t].y; n++; }
    }
    bary.set(i, n ? sum / n : Number.POSITIVE_INFINITY);
  }
  const byArea = new Map();
  for (const i of group0) {
    const a = areaOf(i);
    if (!byArea.has(a)) byArea.set(a, []);
    byArea.get(a).push(i);
  }
  for (const [a, group] of byArea) {
    group.sort((p, q) => {
      const bp = bary.get(p), bq = bary.get(q);
      if (bp !== bq) return bp - bq;
      return pathOf(p) < pathOf(q) ? -1 : pathOf(p) > pathOf(q) ? 1 : 0;
    });
    const rowsHere = Math.min(CAP, group.length);
    const subCols = blockCols(group.length, CAP);
    // Blocks are centred in the column, so a narrow domain does not read as if it were shifted
    // leftward relative to a wide one sharing the same rank.
    const indent = (innerWidth(c, CAP) - (subCols - 1) * SUB_W) / 2;
    group.forEach((i, k) => {
      N[i].x = colLeft.get(c) + indent + Math.floor(k / rowsHere) * SUB_W;
      N[i].y = bandTop.get(a) + (k % rowsHere) * ROW_H;
      placed[i] = true;
    });
  }
}

// ── phase 4: the domain summary the page draws when zoomed out ───────────────
// Emitted rather than derived in the browser for the same reason the coordinates are: the page should
// not have to recompute a layout to know what it is looking at, and a summary the browser recomputed
// could drift from the detail the moment this file changed. One entry per domain — file count, a
// position seeded from the centroid of that domain's own files (phase 5 nudges it clear of its
// neighbours), and a radius — plus the domain-to-domain edge weights.
const domains = areaOrder
  .filter(a => areaSize.has(a))
  .map(a => {
    const members = [];
    for (let i = 0; i < N.length; i++) if (areaOf(i) === a) members.push(i);
    const cx = members.reduce((s, i) => s + N[i].x, 0) / members.length;
    const cy = members.reduce((s, i) => s + N[i].y, 0) / members.length;
    return { a, n: members.length, x: cx, y: cy };
  });
const domainIdx = new Map(domains.map((d, i) => [d.a, i]));
const domainEdgeWeight = new Map();
for (const [s, t] of links) {
  const from = domainIdx.get(areaOf(s)), to = domainIdx.get(areaOf(t));
  if (from == null || to == null || from === to) continue; // a domain's internal traffic is not a bubble-to-bubble edge
  const key = from + ',' + to;
  domainEdgeWeight.set(key, (domainEdgeWeight.get(key) || 0) + 1);
}
const domainLinks = [...domainEdgeWeight.entries()]
  .map(([key, w]) => [...key.split(',').map(Number), w])
  .sort((p, q) => p[0] - q[0] || p[1] - q[1]);


const xs = N.map(n => n.x), ys = N.map(n => n.y);
const minX = Math.min(...xs), maxX = Math.max(...xs);
const minY = Math.min(...ys), maxY = Math.max(...ys);
const spanX = maxX - minX, spanY = maxY - minY;
if (!isFinite(spanX) || !isFinite(spanY)) throw new Error('layout diverged to a non-finite span');
// The old guard here rejected an aspect ratio over 3:1, which was the right check for the disc the
// force layout drew and is the WRONG one now: a layered drawing is deliberately wide (that width is
// the dependency axis). What is still a pathology is a layout with no extent on an axis — every node
// in one column means ranking failed, every node on one row means the domain bands collapsed — so the
// guard now names those two directly instead of policing a shape the design chose.
if (spanX <= 0) throw new Error('layout collapsed: every node landed in one column — ranking produced no depth');
if (spanY <= 0) throw new Error('layout collapsed: every node landed on one row — the domain bands produced no extent');
// Width is a design constraint, not a correctness one, so it is capped rather than rejected: x is
// squeezed to at most WIDEST times the height. Squeezing (instead of dropping columns) keeps every
// rank distinguishable and keeps the ordering exact — only the spacing narrows.
const WIDEST = 2.6;
const squeeze = spanY > 0 && spanX > spanY * WIDEST ? (spanY * WIDEST) / spanX : 1;

const k = 1000 / Math.max(spanX * squeeze, spanY);
const cx = (minX + maxX) / 2, cy = (minY + maxY) / 2;
const r1 = v => Math.round(v * 10) / 10;
const toX = x => (x - cx) * squeeze * k;
const toY = y => (y - cy) * k;

// ── phase 5: bubble radii, and pushing them apart ────────────────────────────
// Radii are sized and de-overlapped HERE, in the same normalised space the page draws in, because the
// squeeze above scales x and y differently — a radius carried through that transform would not be a
// radius on the other side. Area (not radius) tracks the file count: a bubble drawn twice as wide
// reads as four times as much, so a linear radius would overstate every large domain.
//
// Centroids alone are not a layout. A domain's centroid is a mean, so two domains whose files
// interleave land almost on top of each other — measured, `rules` and `scripts` came out 19px apart
// while their radii summed to 66. So the centroids are a STARTING POINT that a pairwise push-apart
// relaxes until nothing collides. It is deterministic (fixed pair order, no RNG, converges or throws)
// and it keeps every bubble near the files it stands for, which is the property that makes the two
// zoom levels one drawing.
const R_MIN = 26, R_SPAN = 64, R_PAD = 12;
const maxDomainN = Math.max(...domains.map(d => d.n));
const bubbles = domains.map(d => ({
  ...d,
  bx: toX(d.x),
  by: toY(d.y),
  r: R_MIN + Math.sqrt(d.n / maxDomainN) * R_SPAN,
}));
let settled = false;
for (let pass = 0; pass < 600 && !settled; pass++) {
  settled = true;
  for (let i = 0; i < bubbles.length; i++) {
    for (let j = i + 1; j < bubbles.length; j++) {
      const A = bubbles[i], B = bubbles[j];
      let dx = B.bx - A.bx, dy = B.by - A.by;
      let d = Math.hypot(dx, dy);
      const need = A.r + B.r + R_PAD;
      if (d >= need) continue;
      // Exactly coincident centroids have no separation axis, so one is chosen from the pair's index
      // order rather than left to a 0/0. Vertical, because the bands are stacked vertically.
      if (d === 0) { dx = 0; dy = 1; d = 1; }
      const push = (need - d) / 2 / d;
      A.bx -= dx * push; A.by -= dy * push;
      B.bx += dx * push; B.by += dy * push;
      settled = false;
    }
  }
}
if (!settled) {
  throw new Error('domain bubbles never stopped colliding — the summary would draw a smear over the domain names');
}
const payload = JSON.stringify({
  f: folders,
  n: rows.map((r, i) => [...r, r1(toX(N[i].x)), r1(toY(N[i].y))]),
  l: links,
  // The zoomed-out summary: [area, fileCount, x, y, radius] per domain plus [from, to, weight] edges
  // between them, all in the same normalised space as the nodes above — so the page scales the two
  // grains with the same view transform and never has to reconcile two coordinate systems.
  d: bubbles.map(b => [b.a, b.n, r1(b.bx), r1(b.by), r1(b.r)]),
  dl: domainLinks,
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
    file: PAGE,
    what: 'off-axis block note, degree-0 count',
    re: /There are [\d,]+ of them here/,
    to: `There are ${commas(isolated.length)} of them here`,
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
  `site/graph.html: ${rows.length} nodes, ${links.length} links, ${colKeys.length} columns` +
    ` (deepest rank ${maxRank}), ${isolated.length} off-axis, wrap cap ${CAP} rows,` +
    ` aspect ${(spanX * squeeze / spanY).toFixed(2)}, ${domains.length} domains` +
    ` / ${domainLinks.length} domain edges, ${kb(PAGE)} KB` +
    ` · site/architecture.html: counts rewritten (${commas(nodeCount)} files, ${commas(linkCount)} imports)`
);
