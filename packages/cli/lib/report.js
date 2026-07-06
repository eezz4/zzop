'use strict';

// PURE report builders — turn a parsed native output object into named report files (filename -> content
// string). No I/O and no clock here: the CLI (bin/zzop.js) owns the fs writes and the per-run epoch-second
// subdirectory. Kept pure so the SARIF/JSON shaping is unit-testable without the native addon or a disk.

const { collectFindings } = require('./format');

const SARIF_SCHEMA = 'https://json.schemastore.org/sarif-2.1.0.json';
const INFO_URI = 'https://github.com/eezz4/zzop';

// zzop severity -> SARIF result level (SARIF has no "critical"; "error" is its top level).
const SARIF_LEVEL = { critical: 'error', warning: 'warning', info: 'note' };

// Known report formats: id -> { file, build(output, ctx) -> string }.
const REPORT_FORMATS = {
  json: {
    file: 'report.json',
    build: (output) => JSON.stringify(output, null, 2),
  },
  sarif: {
    file: 'report.sarif',
    build: (output, ctx) => JSON.stringify(sarifDoc(output, ctx), null, 2),
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
 * dropped). Throws on an unknown format id.
 * @param {object} output  parsed native output
 * @param {{ formats?: string[], toolVersion?: string }} [opts]
 * @returns {{ name: string, content: string }[]}
 */
function buildReports(output, opts = {}) {
  const formats =
    Array.isArray(opts.formats) && opts.formats.length ? opts.formats : DEFAULT_FORMATS;
  const ctx = { toolVersion: opts.toolVersion };
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
    out.push({ name: spec.file, content: spec.build(output, ctx) });
  }
  return out;
}

module.exports = { buildReports, DEFAULT_FORMATS, REPORT_FORMATS };
