// site-render-check — does the site actually DRAW?
//
// Every other site lane checks data: staleness (regenerated from what it claims to draw), links,
// prose guards. None of them look at the picture. The measured cost of that gap: site/graph.html
// shipped a release with the whole 1,137-node graph rendered into a 300x150 corner of a 1272x720
// stage (a <canvas> is a replaced element — absolutely positioned with `inset: 0` but no
// width/height it keeps its INTRINSIC size), and data, regeneration, link and prose guards were all
// green. The finder was the user.
//
// Two checks, chosen to target exactly that accident class and nothing speculative
// (2026-07-30 user ruling — pixel-ratio and golden-image lanes were considered and not built:
// a threshold becomes a policy value, a golden image breaks on font rendering):
//   1. zero console errors / page errors on every page, and
//   2. on graph.html, #depgraph's getBoundingClientRect matches its .graph-stage container.
//
// Run: node scripts/site-render-check/check.mjs [siteDir]   (default: site)
// Requires: npm ci && npx playwright install chromium   (CI does this; the browser download is why
// this is a CI job and not a pre-commit guard).

import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { readdirSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { chromium } from 'playwright';

const siteDir = path.resolve(process.argv[2] ?? 'site');
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.json': 'application/json',
  '.txt': 'text/plain; charset=utf-8',
};

// Serve the tree as-is. 404s are NOT failures here — a missing asset that matters shows up as a
// console error on the page that needed it, which is the signal we actually score.
const server = createServer(async (req, res) => {
  const rel = decodeURIComponent(new URL(req.url, 'http://x').pathname).replace(/^\/+/, '') || 'index.html';
  const file = path.join(siteDir, rel);
  if (!file.startsWith(siteDir)) {
    res.writeHead(403).end();
    return;
  }
  try {
    const body = await readFile(file);
    res.writeHead(200, { 'content-type': MIME[path.extname(file)] ?? 'application/octet-stream' });
    res.end(body);
  } catch {
    res.writeHead(404).end('not found');
  }
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const base = `http://127.0.0.1:${server.address().port}`;

// Every page, discovered rather than listed — a page added later is covered without editing this
// file, the same reason the keyset-parity test walks serialized JSON instead of naming fields.
const pages = readdirSync(siteDir).filter((f) => f.endsWith('.html')).sort();
if (pages.length === 0) {
  console.error(`site-render-check: no .html files under ${siteDir} — the check would be vacuous`);
  process.exit(1);
}

const browser = await chromium.launch();
const failures = [];

for (const name of pages) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
  const errors = [];
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(`console.error: ${msg.text()}`);
  });
  page.on('pageerror', (err) => errors.push(`pageerror: ${err.message}`));

  await page.goto(`${base}/${name}`, { waitUntil: 'networkidle' });
  // The graph page starts its layout after load; give async work one beat to throw if it's going to.
  await page.waitForTimeout(500);

  for (const e of errors) failures.push(`${name}: ${e}`);

  if (name === 'graph.html') {
    const rects = await page.evaluate(() => {
      const stage = document.querySelector('.graph-stage');
      const canvas = document.querySelector('#depgraph');
      const r = (el) => el && { w: el.getBoundingClientRect().width, h: el.getBoundingClientRect().height };
      return { stage: r(stage), canvas: r(canvas) };
    });
    if (!rects.stage || !rects.canvas) {
      failures.push(`graph.html: .graph-stage or #depgraph missing — the size check has nothing to measure`);
    } else if (rects.stage.w < 100 || rects.stage.h < 100) {
      // A collapsed stage would make the match below vacuously easy — that is its own failure.
      failures.push(`graph.html: stage collapsed to ${rects.stage.w}x${rects.stage.h}`);
    } else if (
      Math.abs(rects.canvas.w - rects.stage.w) > 2 ||
      Math.abs(rects.canvas.h - rects.stage.h) > 2
    ) {
      // THE seal: the shipped accident was canvas 300x150 in a 1272x720 stage. `inset: 0` plus
      // explicit width/height:100% means the two rects must agree to the pixel.
      failures.push(
        `graph.html: canvas ${rects.canvas.w}x${rects.canvas.h} does not fill stage ${rects.stage.w}x${rects.stage.h}`
      );
    }
  }
  await page.close();
}

await browser.close();
server.close();

if (failures.length > 0) {
  console.error(`site-render-check: RED — ${failures.length} failure(s)`);
  for (const f of failures) console.error(`  ${f}`);
  process.exit(1);
}
console.log(`site-render-check: ${pages.length} page(s) render clean (0 console/page errors; graph canvas fills its stage)`);
