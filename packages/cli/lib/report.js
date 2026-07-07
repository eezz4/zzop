'use strict';

// PURE report builders — turn a parsed native output object into named report files (filename -> content
// string). No I/O and no clock here: the CLI (bin/zzop.js) owns the fs writes and the per-run epoch-second
// subdirectory. Kept pure so the SARIF/JSON shaping is unit-testable without the native addon or a disk.

const { collectFindings, groupByFile, countBySeverity, foldByRule } = require('./format');

const SARIF_SCHEMA = 'https://json.schemastore.org/sarif-2.1.0.json';
const INFO_URI = 'https://github.com/eezz4/zzop';

// zzop severity -> SARIF result level (SARIF has no "critical"; "error" is its top level).
const SARIF_LEVEL = { critical: 'error', warning: 'warning', info: 'note' };

// Known report formats: id -> { file, build(output, ctx) -> string|Array<{name,content}> }. `file` is the
// single output filename for a format whose `build` returns a string; a format whose `build` returns an
// array instead (currently only `md`, which is inherently multi-file — one file per tree plus an optional
// cross-repo summary) has no `file` and `buildReports` spreads its array straight into the result.
const REPORT_FORMATS = {
  json: {
    file: 'report.json',
    build: (output) => JSON.stringify(output, null, 2),
  },
  sarif: {
    file: 'report.sarif',
    build: (output, ctx) => JSON.stringify(sarifDoc(output, ctx), null, 2),
  },
  md: {
    build: (output, ctx) => buildMarkdownReports(output, ctx),
  },
};

const DEFAULT_FORMATS = ['json', 'sarif'];

// SARIF artifactLocation URIs are forward-slash relative paths.
function toUri(file) {
  return String(file == null ? '' : file).replace(/\\/g, '/');
}

/**
 * Shape a parsed native output into a SARIF 2.1.0 document. Findings from a multi-tree run are flattened
 * (each file path stays relative to its own tree root). Severity maps critical->error, warning->warning,
 * info->note.
 * @param {object} output
 * @param {{ toolVersion?: string }} ctx
 */
function sarifDoc(output, ctx) {
  const { findings } = collectFindings(output);
  const ruleIds = [...new Set(findings.map((f) => String(f.ruleId || 'unknown')))].sort();
  const driver = {
    name: 'zzop',
    informationUri: INFO_URI,
    rules: ruleIds.map((id) => ({ id })),
  };
  if (ctx && ctx.toolVersion) {
    driver.version = String(ctx.toolVersion);
  }
  const results = findings.map((f) => ({
    ruleId: String(f.ruleId || 'unknown'),
    level: SARIF_LEVEL[String(f.severity)] || 'note',
    message: { text: String(f.message || '') },
    locations: [
      {
        physicalLocation: {
          artifactLocation: { uri: toUri(f.file) },
          region: { startLine: Math.max(1, Number(f.line) || 1) },
        },
      },
    ],
  }));
  return { $schema: SARIF_SCHEMA, version: '2.1.0', runs: [{ tool: { driver }, results }] };
}

/**
 * Build the requested report files. Returns `[{ name, content }]` in the order requested (duplicates
 * dropped). A format's `build` may return either a single string (one file, named `REPORT_FORMATS[id]
 * .file`) or an array of `{ name, content }` (a multi-file format, e.g. `md`) — the array is spread
 * straight into the result. Throws on an unknown format id.
 * @param {object} output  parsed native output
 * @param {{ formats?: string[], toolVersion?: string, sourceId?: string, root?: string }} [opts]
 *   `sourceId`/`root` are the single-tree markdown fallback identity (the single-tree output shape carries
 *   neither field itself — see `buildMarkdownReports`'s doc) and are ignored by every other format.
 * @returns {{ name: string, content: string }[]}
 */
function buildReports(output, opts = {}) {
  const formats =
    Array.isArray(opts.formats) && opts.formats.length ? opts.formats : DEFAULT_FORMATS;
  const ctx = { toolVersion: opts.toolVersion, sourceId: opts.sourceId, root: opts.root };
  const out = [];
  const seen = new Set();
  for (const fmt of formats) {
    const key = String(fmt).toLowerCase();
    const spec = REPORT_FORMATS[key];
    if (!spec) {
      throw new Error(
        `Unknown report format ${JSON.stringify(fmt)}. Known: ${Object.keys(REPORT_FORMATS).join(', ')}.`
      );
    }
    if (seen.has(key)) continue;
    seen.add(key);
    const built = spec.build(output, ctx);
    if (Array.isArray(built)) {
      out.push(...built);
    } else {
      out.push({ name: spec.file, content: built });
    }
  }
  return out;
}

// ---------------------------------------------------------------------------------------------------
// Markdown report — the default persisted format (see bin/zzop.js). One `<sourceId>.md` per analyzed
// tree, plus a `cross-repo.md` summary when the run is multi-tree. Deterministic and clock-free like every
// other builder here: no timestamps/durations in the body, everything sorted before being printed.
// ---------------------------------------------------------------------------------------------------

// Compare helpers: `null`/`undefined` sort as the empty string / 0, never throw on odd input shapes.
function cmpStr(a, b) {
  return String(a == null ? '' : a).localeCompare(String(b == null ? '' : b));
}
function cmpNum(a, b) {
  return (Number(a) || 0) - (Number(b) || 0);
}

// The two "this tree is blind to its own consumes" self-report rules (see
// rules/native/rules-cross-layer/src/cross_layer/{sdk_import_no_visible_consume,unresolved_consume_ratio}
// .rs) — surfaced FIRST, under "Coverage & blindness", and excluded from the generic
// "Cross-layer findings" bucket so they are not shown twice.
const COVERAGE_RULE_IDS = new Set([
  'cross-layer/sdk-import-no-visible-consume',
  'cross-layer/unresolved-consume-ratio',
]);

/**
 * Sanitize a sourceId into a safe, deterministic filename stem (no extension): lowercase, any character
 * outside `[a-z0-9._-]` becomes `-`, runs of `-` collapse to one, leading/trailing `-` trimmed. Falls back
 * to `tree-<index>` when the result is empty (a missing/blank sourceId, or one that is all symbols).
 * @param {*} sourceId
 * @param {number} index  zero-based tree index — only used for the empty-slug fallback
 * @returns {string}
 */
function slugify(sourceId, index) {
  const cleaned = String(sourceId == null ? '' : sourceId)
    .toLowerCase()
    .replace(/[^a-z0-9._-]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-+|-+$/g, '');
  return cleaned || `tree-${index}`;
}

/**
 * Assign deterministic, collision-free `<slug>.md` filenames for a list of tree sourceIds, in tree order:
 * a base slug repeated by a later tree gets `-2`, `-3`, ... appended, in the order those trees appear.
 * @param {(string|undefined)[]} sourceIds
 * @returns {string[]}  filenames, same length/order as `sourceIds`
 */
function assignTreeFilenames(sourceIds) {
  const counts = new Map();
  return sourceIds.map((sourceId, index) => {
    const base = slugify(sourceId, index);
    const n = (counts.get(base) || 0) + 1;
    counts.set(base, n);
    return `${n === 1 ? base : `${base}-${n}`}.md`;
  });
}

// A tree's `ir.io` is normally at `output.ir.io` (CommonIr's MinimalIr fields are serde-flattened onto
// `ir` — see packages/core/src/ir.rs). Also checks the doubly-nested `ir.ir.io` shape defensively, so a
// future/alternate wire shape degrades to "no HTTP interface section" rather than throwing.
function treeIo(treeOutput) {
  const ir = treeOutput && treeOutput.ir;
  if (!ir) return {};
  return ir.io || (ir.ir && ir.ir.io) || {};
}

// First sentence of a message (up to and including the first ". "), or the whole message when it has no
// sentence break — used to keep the "Cross-layer findings" bucket skimmable without truncating mid-word.
function firstSentence(message) {
  const s = String(message == null ? '' : message);
  const idx = s.indexOf('. ');
  return idx === -1 ? s : s.slice(0, idx + 1);
}

/**
 * Render one tree's markdown report body (used both for a multi-tree run's per-tree file and for a
 * single-tree run's only file). Deterministic: same inputs -> byte-identical output.
 * @param {string} sourceId
 * @param {string} [root]
 * @param {object} treeOutput  a PerTree `output` (single-tree shape): `{ findings, fileCount, warnings, ir }`
 * @returns {string}
 */
function buildTreeMarkdown(sourceId, root, treeOutput) {
  const findings = Array.isArray(treeOutput.findings) ? treeOutput.findings : [];
  const fileCount = Number(treeOutput.fileCount) || 0;
  const warnings = Array.isArray(treeOutput.warnings) ? treeOutput.warnings : [];
  const counts = countBySeverity(findings);
  const io = treeIo(treeOutput);
  const provides = (Array.isArray(io.provides) ? io.provides : []).filter((p) => p && p.kind === 'http');
  const consumes = (Array.isArray(io.consumes) ? io.consumes : []).filter((c) => c && c.kind === 'http');

  const lines = [];
  lines.push(`# ${sourceId}`, '');
  lines.push(`- Root: \`${root == null ? '' : root}\``);
  lines.push(`- Files analyzed: ${fileCount}`);
  lines.push(
    `- Findings: ${findings.length} (${counts.critical} critical, ${counts.warning} warning, ${counts.info} info)`
  );
  lines.push('');

  if (warnings.length > 0) {
    lines.push('## Warnings');
    for (const w of warnings) lines.push(`- ${w}`);
    lines.push('');
  }

  const provideLines = provides
    .slice()
    .sort((a, b) => cmpStr(a.key, b.key) || cmpStr(a.file, b.file) || cmpNum(a.line, b.line))
    .map((p) => `- \`${p.key}\` — ${p.file}:${p.line}`);
  const consumeLines = consumes
    .slice()
    .sort(
      (a, b) =>
        cmpStr(a.key || a.raw, b.key || b.raw) || cmpStr(a.file, b.file) || cmpNum(a.line, b.line)
    )
    .map((c) => {
      const label = c.key
        ? `\`${c.key}\``
        : `\`${c.raw == null ? '(unresolved)' : c.raw}\` (unresolved)`;
      return `- ${label} — ${c.file}:${c.line}`;
    });

  if (provideLines.length > 0 || consumeLines.length > 0) {
    lines.push('## HTTP interface');
    if (provideLines.length > 0) {
      lines.push('### Provides (routes served)');
      lines.push(...provideLines);
    }
    if (consumeLines.length > 0) {
      lines.push('### Consumes (routes called)');
      lines.push(...consumeLines);
    }
    lines.push('');
  }

  if (findings.length > 0) {
    lines.push('## Findings');
    const elevated = findings.filter((f) => f.severity === 'critical' || f.severity === 'warning');
    const info = findings.filter((f) => f.severity === 'info');
    for (const [file, list] of groupByFile(elevated)) {
      lines.push(`### ${file}`);
      for (const f of list) {
        const loc = f.line != null ? f.line : '?';
        lines.push(`- **${f.severity}** L${loc} — ${f.message || ''} \`${f.ruleId || ''}\``);
      }
    }
    if (info.length > 0) {
      lines.push('#### info (folded)');
      for (const [ruleId, n] of foldByRule(info)) {
        lines.push(`- ${n} × \`${ruleId}\``);
      }
    }
    lines.push('');
  }

  while (lines.length && lines[lines.length - 1] === '') lines.pop();
  return `${lines.join('\n')}\n`;
}

/**
 * Render the multi-tree `cross-repo.md` summary: coverage self-reports first (the "this tree is blind"
 * rules), then cross-repo edges, unprovided/unconsumed near-misses, the other cross-layer buckets, and
 * finally every remaining cross-layer finding grouped by rule.
 * @param {object} output  multi-tree native output: `{ trees, crossLayer, crossLayerFindings }`
 * @returns {string}
 */
function buildCrossRepoMarkdown(output) {
  const trees = Array.isArray(output.trees) ? output.trees : [];
  const crossLayer = output.crossLayer || {};
  const crossLayerFindings = Array.isArray(output.crossLayerFindings) ? output.crossLayerFindings : [];

  const lines = [];
  lines.push('# Cross-repo analysis', '');
  lines.push('Trees analyzed:');
  for (const t of trees) {
    const treeOutput = (t && t.output) || {};
    lines.push(`- \`${t && t.sourceId}\` — ${Number(treeOutput.fileCount) || 0} files (${t && t.root})`);
  }
  lines.push('');

  lines.push('## Coverage & blindness');
  const coverage = crossLayerFindings
    .filter((f) => COVERAGE_RULE_IDS.has(String(f && f.ruleId)))
    .slice()
    .sort((a, b) => {
      const sa = (a.data && a.data.source) || a.file;
      const sb = (b.data && b.data.source) || b.file;
      return cmpStr(sa, sb) || cmpStr(a.file, b.file) || cmpNum(a.line, b.line);
    });
  if (coverage.length === 0) {
    lines.push('No coverage gaps detected — consume extraction was visible for all trees.');
  } else {
    for (const f of coverage) {
      const tag = (f.data && f.data.source) || f.file;
      lines.push(`- **${tag}** — ${f.message || ''}`);
    }
  }
  lines.push('');

  const edges = Array.isArray(crossLayer.edges) ? crossLayer.edges : [];
  const crossEdges = edges
    .filter((e) => e && e.crossSource)
    .slice()
    .sort(
      (a, b) =>
        cmpStr(a.key, b.key) ||
        cmpStr(a.from && a.from.file, b.from && b.from.file) ||
        cmpNum(a.from && a.from.line, b.from && b.from.line)
    );
  lines.push(`## Cross-repo edges (${crossEdges.length})`);
  if (crossEdges.length === 0) {
    lines.push('None.');
  } else {
    for (const e of crossEdges) {
      const from = e.from || {};
      const to = e.to || {};
      let row = `- \`${e.key}\`: \`${from.source}\` (${from.file}:${from.line}) -> \`${to.source}\` (${to.file}:${to.line})`;
      if (e.lowConfidenceReason) {
        row += ` — low confidence: ${e.lowConfidenceReason}`;
      }
      lines.push(row);
    }
  }
  lines.push('');

  const unprovided = (Array.isArray(crossLayer.unprovidedConsumes) ? crossLayer.unprovidedConsumes : [])
    .slice()
    .sort(
      (a, b) => cmpStr(a.key || a.raw, b.key || b.raw) || cmpStr(a.file, b.file) || cmpNum(a.line, b.line)
    );
  lines.push(`## Unprovided consumes (${unprovided.length})`);
  if (unprovided.length === 0) {
    lines.push('None.');
  } else {
    for (const c of unprovided) {
      lines.push(`- \`${c.key || c.raw || '(no key)'}\` consumed by \`${c.source}\` (${c.file}:${c.line})`);
    }
  }
  lines.push('');

  const unconsumed = (Array.isArray(crossLayer.unconsumedProvides) ? crossLayer.unconsumedProvides : [])
    .slice()
    .sort((a, b) => cmpStr(a.key, b.key) || cmpStr(a.file, b.file) || cmpNum(a.line, b.line));
  lines.push(`## Unconsumed provides (${unconsumed.length})`);
  if (unconsumed.length === 0) {
    lines.push('None.');
  } else {
    for (const p of unconsumed) {
      lines.push(`- \`${p.key}\` provided by \`${p.source}\` (${p.file}:${p.line})`);
    }
  }
  lines.push('');

  const unresolvedCount = Array.isArray(crossLayer.unresolvedConsumes)
    ? crossLayer.unresolvedConsumes.length
    : 0;
  const externalCount = Array.isArray(crossLayer.externalConsumes) ? crossLayer.externalConsumes.length : 0;
  const ambiguousCount = Array.isArray(crossLayer.ambiguousConsumes)
    ? crossLayer.ambiguousConsumes.length
    : 0;
  lines.push('## Other buckets');
  lines.push(
    `- Unresolved consumes: ${unresolvedCount}   External consumes: ${externalCount}   Ambiguous consumes: ${ambiguousCount}`
  );
  lines.push('');

  const remaining = crossLayerFindings.filter((f) => !COVERAGE_RULE_IDS.has(String(f && f.ruleId)));
  lines.push(`## Cross-layer findings (${remaining.length})`);
  if (remaining.length === 0) {
    lines.push('None.');
  } else {
    const byRule = new Map();
    for (const f of remaining) {
      const key = String(f.ruleId || '(unknown rule)');
      if (!byRule.has(key)) byRule.set(key, []);
      byRule.get(key).push(f);
    }
    for (const ruleId of [...byRule.keys()].sort()) {
      const list = byRule
        .get(ruleId)
        .slice()
        .sort((a, b) => cmpStr(a.file, b.file) || cmpNum(a.line, b.line));
      lines.push(`### ${ruleId} (${list.length})`);
      for (const f of list) {
        lines.push(`- ${firstSentence(f.message)} (${f.file}:${f.line})`);
      }
    }
  }

  while (lines.length && lines[lines.length - 1] === '') lines.pop();
  return `${lines.join('\n')}\n`;
}

/**
 * Build the markdown report file set: `cross-repo.md` + one `<slug(sourceId)>.md` per tree for a
 * multi-tree run (`Array.isArray(output.trees)`), or a single `<slug(sourceId)>.md` otherwise. Pure and
 * deterministic (no fs/clock) like every other builder in this module.
 *
 * The single-tree native output shape (`analyze()`'s `AnalyzeOutputView`) carries neither `sourceId` nor
 * `root` itself — those are request-side, not response-side — so the caller (bin/zzop.js, which built the
 * request) passes them through `ctx.sourceId`/`ctx.root`. `ctx.sourceId` falls back to `"report"` when
 * absent (e.g. this function called directly in a test with no ctx).
 *
 * @param {object} output  parsed native output (single- or multi-tree shape)
 * @param {{ sourceId?: string, root?: string }} [ctx]
 * @returns {{ name: string, content: string }[]}
 */
function buildMarkdownReports(output, ctx = {}) {
  const safeOutput = output && typeof output === 'object' ? output : {};

  if (Array.isArray(safeOutput.trees)) {
    const sourceIds = safeOutput.trees.map((t) => t && t.sourceId);
    const filenames = assignTreeFilenames(sourceIds);
    const files = [{ name: 'cross-repo.md', content: buildCrossRepoMarkdown(safeOutput) }];
    safeOutput.trees.forEach((t, i) => {
      const treeOutput = (t && t.output) || {};
      files.push({
        name: filenames[i],
        content: buildTreeMarkdown(t && t.sourceId, t && t.root, treeOutput),
      });
    });
    return files;
  }

  const sourceId = safeOutput.sourceId || ctx.sourceId || 'report';
  const root = safeOutput.root != null ? safeOutput.root : ctx.root;
  const filename = `${slugify(sourceId, 0)}.md`;
  return [{ name: filename, content: buildTreeMarkdown(sourceId, root, safeOutput) }];
}

module.exports = { buildReports, buildMarkdownReports, DEFAULT_FORMATS, REPORT_FORMATS };
