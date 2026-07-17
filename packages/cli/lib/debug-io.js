'use strict';

// Pure renderer for `--debug-io`: the join-debug surface. Turns a parsed native output's `crossLayer`
// result (already carried on a multi-tree `analyzeTrees` output — see crates/core/src/io.rs's
// `CrossLayerResult`) into a deterministic plain-text dump, one section per bucket, one line per entry,
// nothing capped/omitted (unlike the markdown report's `UNRESOLVED_LIST_CAP` — this is the "give me
// everything" surface). No I/O here: bin/zzop.js owns the stdout write, same split as lib/report.js.

// Bucket render order — mirrors CrossLayerResult's own field order (crates/core/src/io.rs) and
// lib/report.js's "edges, unconsumed, unprovided, then the Other-buckets trio" grouping.
const BUCKETS = [
  'edges',
  'unconsumedProvides',
  'unprovidedConsumes',
  'unresolvedConsumes',
  'externalConsumes',
  'ambiguousConsumes',
];

function cmpStr(a, b) {
  return String(a == null ? '' : a).localeCompare(String(b == null ? '' : b));
}
function cmpNum(a, b) {
  return (Number(a) || 0) - (Number(b) || 0);
}

// `<key-or-raw(+method)>`: a resolved entry (`key` non-null) prints its key; an unresolved one falls
// back to `raw` (or a literal placeholder when even `raw` is missing), with `[METHOD]` appended when
// `method` is present — `method` is only ever set alongside a null `key` (see TaggedConsume's doc in
// crates/core/src/io.rs), so this never doubles up with a resolved key.
function keyOrRaw(entry) {
  if (entry && entry.key != null) return String(entry.key);
  const raw = entry && entry.raw != null ? String(entry.raw) : '(no key/raw)';
  return entry && entry.method ? `${raw} [${entry.method}]` : raw;
}

// One line for a TaggedProvide/TaggedConsume/AmbiguousConsume-shaped entry: `<bucket> <sourceId>
// <file>:<line> <key-or-raw(+method)>`. `extra` (ambiguousConsumes' candidate list) is appended verbatim
// when present.
function renderEntryLine(bucket, entry, extra) {
  const source = entry && entry.source != null ? entry.source : '(unknown source)';
  const file = entry && entry.file != null ? entry.file : '(unknown file)';
  const line = entry && entry.line != null ? entry.line : '?';
  const base = `${bucket} ${source} ${file}:${line} ${keyOrRaw(entry)}`;
  return extra ? `${base} ${extra}` : base;
}

function sortTagged(list) {
  return list
    .slice()
    .sort(
      (a, b) =>
        cmpStr(a && a.source, b && b.source) ||
        cmpStr(a && a.file, b && b.file) ||
        cmpNum(a && a.line, b && b.line) ||
        cmpStr(keyOrRaw(a), keyOrRaw(b))
    );
}

// Edges pair a consumer site (`from`) with a provider site (`to`) — there is no single `sourceId` for
// an edge, so `from` (the call site, the "debugging this join" side) fills the `<sourceId> <file>:<line>`
// slot and `to` is appended after the key, `-> <to.source> <to.file>:<to.line>`.
function renderEdgesSection(edges) {
  const list = (Array.isArray(edges) ? edges : [])
    .slice()
    .sort(
      (a, b) =>
        cmpStr(a && a.key, b && b.key) ||
        cmpStr(a && a.from && a.from.source, b && b.from && b.from.source) ||
        cmpStr(a && a.from && a.from.file, b && b.from && b.from.file) ||
        cmpNum(a && a.from && a.from.line, b && b.from && b.from.line)
    );
  const lines = [`edges (${list.length})`];
  for (const e of list) {
    const from = (e && e.from) || {};
    const to = (e && e.to) || {};
    const source = from.source != null ? from.source : '(unknown source)';
    const file = from.file != null ? from.file : '(unknown file)';
    const line = from.line != null ? from.line : '?';
    const key = e && e.key != null ? e.key : '(no key)';
    const toSource = to.source != null ? to.source : '(unknown source)';
    const toFile = to.file != null ? to.file : '(unknown file)';
    const toLine = to.line != null ? to.line : '?';
    lines.push(`edges ${source} ${file}:${line} ${key} -> ${toSource} ${toFile}:${toLine}`);
  }
  return lines;
}

// ambiguousConsumes carries a `candidates: TaggedProvide[]` list (already sorted by (source, file, line)
// per CrossLayerResult's doc) — rendered as a trailing `candidates=<n>: <source>@<file>:<line>, ...` so
// nothing about the ambiguity is lost to the single-line format.
function renderAmbiguousSection(entries) {
  const list = sortTagged(Array.isArray(entries) ? entries : []);
  const lines = [`ambiguousConsumes (${list.length})`];
  for (const entry of list) {
    const candidates = Array.isArray(entry.candidates) ? entry.candidates : [];
    const candidateList = candidates
      .map((c) => `${c && c.source != null ? c.source : '(unknown source)'}@${c && c.file}:${c && c.line}`)
      .join(', ');
    lines.push(renderEntryLine('ambiguousConsumes', entry, `candidates=${candidates.length}: ${candidateList}`));
  }
  return lines;
}

function renderTaggedSection(bucket, entries) {
  const list = sortTagged(Array.isArray(entries) ? entries : []);
  const lines = [`${bucket} (${list.length})`];
  for (const entry of list) {
    lines.push(renderEntryLine(bucket, entry));
  }
  return lines;
}

/**
 * Render `--debug-io`'s plain-text dump from a `CrossLayerResult`-shaped object (`output.crossLayer` on
 * a multi-tree `analyzeTrees` output — absent/`{}` on a single-tree `analyze` output, which has no
 * cross-layer join to debug; every section then renders empty at count 0, never thrown on).
 *
 * @param {object} [crossLayer]  `{ edges, unconsumedProvides, unprovidedConsumes, unresolvedConsumes,
 *   externalConsumes, ambiguousConsumes }` — any/all fields may be absent.
 * @returns {string}  deterministic, no trailing newline (caller adds one, matching formatPretty/formatJson)
 */
function renderDebugIo(crossLayer) {
  const cl = crossLayer && typeof crossLayer === 'object' ? crossLayer : {};
  const sections = [];
  for (const bucket of BUCKETS) {
    if (bucket === 'edges') {
      sections.push(renderEdgesSection(cl.edges));
    } else if (bucket === 'ambiguousConsumes') {
      sections.push(renderAmbiguousSection(cl.ambiguousConsumes));
    } else {
      sections.push(renderTaggedSection(bucket, cl[bucket]));
    }
  }
  return sections.map((lines) => lines.join('\n')).join('\n\n');
}

/**
 * How many trees a parsed native output analyzed — 1 for a single-tree `analyze()` shape (no `trees`
 * array at all: there is exactly one tree, itself), `output.trees.length` for a multi-tree
 * `analyzeTrees()` shape (which can legitimately be a single explicit `trees: [...]` entry, not just
 * >= 2 — see `packages/cli/lib/mapper.js`'s `configToRequest`: an explicit `trees` config always takes
 * the `analyzeTrees` method even with one entry). Falls back to 1 for anything else absent/malformed —
 * the same "assume the least-crossed-over shape" default as `renderDebugIo`'s own `{}` fallback.
 *
 * @param {object} [output]  parsed native output
 * @returns {number}
 */
function debugIoTreeCount(output) {
  if (output && Array.isArray(output.trees)) return output.trees.length;
  return 1;
}

/**
 * `--debug-io`'s "why is every bucket empty" explainer: every cross-layer bucket is join output, and a
 * join needs at least two trees to have anything to join — a single-tree run's buckets are ALWAYS empty,
 * which reads as a bug/silent failure unless labeled. Returns `null` when the run analyzed >= 2 trees (no
 * note needed — the buckets speak for themselves); otherwise a one-line, deterministic note naming the
 * actual tree count analyzed.
 *
 * @param {object} [output]  parsed native output
 * @returns {string | null}
 */
function debugIoTreeCountNote(output) {
  const treeCount = debugIoTreeCount(output);
  if (treeCount >= 2) return null;
  return `note: cross-layer buckets need >= 2 trees; this run analyzed ${treeCount} tree${treeCount === 1 ? '' : 's'}`;
}

module.exports = { renderDebugIo, debugIoTreeCount, debugIoTreeCountNote, BUCKETS };
