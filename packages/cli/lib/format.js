'use strict';

// PURE output formatting + exit-code computation. Takes the parsed native output object (already
// JSON.parse'd) and produces terminal text / exit codes. No process/stdout access here except the
// optional `color` flag the caller derives from `process.stdout.isTTY`, so this is unit-testable.

const { severityRank, normalizeSeverity, OFF } = require('./mapper');

// Minimal ANSI — only emitted when the caller passes `color: true` (gated on isTTY at the call site).
const ANSI = {
  reset: '[0m',
  dim: '[2m',
  bold: '[1m',
  red: '[31m',
  yellow: '[33m',
  cyan: '[36m',
};

const SEVERITY_COLOR = {
  critical: ANSI.red,
  warning: ANSI.yellow,
  info: ANSI.cyan,
};

function paint(text, code, color) {
  return color ? `${code}${text}${ANSI.reset}` : text;
}

/**
 * Collect findings from a native output object into a flat array, regardless of whether it came from
 * `analyze()` (top-level `findings`, `fileCount`) or `analyzeTrees()` (`{ trees: [{ root, sourceId,
 * output }], crossLayer }`). Each finding is returned as-is (ruleId/severity/file/line/message), with a
 * `sourceId`/`root` tag added when it came from a multi-tree run.
 *
 * @param {object} output  parsed native output
 * @returns {{ findings: object[], fileCount: number }}
 */
function collectFindings(output) {
  if (!output || typeof output !== 'object') {
    return { findings: [], fileCount: 0 };
  }

  // Multi-tree shape.
  if (Array.isArray(output.trees)) {
    const findings = [];
    let fileCount = 0;
    for (const tree of output.trees) {
      const treeOutput = (tree && tree.output) || {};
      fileCount += Number(treeOutput.fileCount) || 0;
      const treeFindings = Array.isArray(treeOutput.findings) ? treeOutput.findings : [];
      for (const f of treeFindings) {
        findings.push({ ...f, sourceId: tree.sourceId, root: tree.root });
      }
    }
    return { findings, fileCount };
  }

  // Single-tree shape.
  const findings = Array.isArray(output.findings) ? output.findings : [];
  const fileCount = Number(output.fileCount) || 0;
  return { findings, fileCount };
}

function countBySeverity(findings) {
  const counts = { critical: 0, warning: 0, info: 0, other: 0 };
  for (const f of findings) {
    if (counts[f.severity] !== undefined) {
      counts[f.severity] += 1;
    } else {
      counts.other += 1;
    }
  }
  return counts;
}

/**
 * Group findings by file and sort: files alphabetically, findings within a file by line then ruleId.
 * @param {object[]} findings
 * @returns {Map<string, object[]>}
 */
function groupByFile(findings) {
  const groups = new Map();
  for (const f of findings) {
    const key = f.file || '(unknown file)';
    if (!groups.has(key)) {
      groups.set(key, []);
    }
    groups.get(key).push(f);
  }
  const sortedKeys = [...groups.keys()].sort();
  const sorted = new Map();
  for (const key of sortedKeys) {
    const list = groups.get(key).slice().sort((a, b) => {
      const la = Number(a.line) || 0;
      const lb = Number(b.line) || 0;
      if (la !== lb) return la - lb;
      return String(a.ruleId).localeCompare(String(b.ruleId));
    });
    sorted.set(key, list);
  }
  return sorted;
}

/**
 * Pretty terminal report: critical/warning findings grouped by file, then a summary footer. Info-level
 * findings are FOLDED into a per-rule count block by default so a flood of hygiene-tier signals can't bury
 * actionable warnings; pass `showAllInfo` (the CLI's `--all`) to expand them inline like everything else.
 * The footer always tallies every finding, folded or not.
 *
 * @param {object} output  parsed native output
 * @param {{ color?: boolean, showAllInfo?: boolean }} [opts]
 * @returns {string}
 */
function formatPretty(output, opts = {}) {
  const color = Boolean(opts.color);
  const showAllInfo = Boolean(opts.showAllInfo);
  const { findings, fileCount } = collectFindings(output);

  if (findings.length === 0) {
    const ok = paint('No findings.', ANSI.dim, color);
    return `${ok}\n\n${summaryFooter(findings, fileCount, color)}`;
  }

  // Split info (foldable, hygiene-tier) from elevated (warning/critical/other — always shown inline).
  const info = [];
  const elevated = [];
  for (const f of findings) {
    (String(f.severity) === 'info' ? info : elevated).push(f);
  }

  const visible = showAllInfo ? findings : elevated;
  const lines = [];

  if (visible.length === 0) {
    lines.push(paint('No warnings or errors.', ANSI.dim, color));
    lines.push('');
  } else {
    for (const [file, list] of groupByFile(visible)) {
      lines.push(paint(file, ANSI.bold, color));
      for (const f of list) {
        const sevRaw = String(f.severity || 'info');
        const sev = paint(sevRaw.padEnd(8), SEVERITY_COLOR[sevRaw] || '', color);
        const loc = paint(`${f.line != null ? f.line : '?'}`, ANSI.dim, color);
        const rule = paint(String(f.ruleId || ''), ANSI.dim, color);
        lines.push(`  ${sev} ${loc}  ${f.message || ''}  ${rule}`);
      }
      lines.push('');
    }
  }

  if (!showAllInfo && info.length > 0) {
    for (const line of foldedInfoBlock(info, color)) {
      lines.push(line);
    }
    lines.push('');
  }

  lines.push(summaryFooter(findings, fileCount, color));
  return lines.join('\n');
}

/**
 * Render folded info findings as a per-rule count block, highest count first. Returns an array of lines
 * (no trailing blank). Only called when there is at least one info finding.
 * @param {object[]} info
 * @param {boolean} color
 * @returns {string[]}
 */
function foldedInfoBlock(info, color) {
  const byRule = new Map();
  for (const f of info) {
    const key = String(f.ruleId || '(unknown rule)');
    byRule.set(key, (byRule.get(key) || 0) + 1);
  }
  const rows = [...byRule.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  const width = String(rows[0][1]).length;
  const header = paint(
    `info — ${info.length} finding${info.length === 1 ? '' : 's'} folded (pass --all to show):`,
    ANSI.dim,
    color
  );
  const body = rows.map(([rule, n]) => {
    const count = paint(String(n).padStart(width), ANSI.cyan, color);
    return `  ${count}  ${paint(rule, ANSI.dim, color)}`;
  });
  return [header, ...body];
}

function summaryFooter(findings, fileCount, color) {
  const counts = countBySeverity(findings);
  const parts = [
    paint(`${counts.critical} critical`, counts.critical ? SEVERITY_COLOR.critical : ANSI.dim, color),
    paint(`${counts.warning} warning`, counts.warning ? SEVERITY_COLOR.warning : ANSI.dim, color),
    paint(`${counts.info} info`, counts.info ? SEVERITY_COLOR.info : ANSI.dim, color),
  ];
  if (counts.other) {
    parts.push(`${counts.other} other`);
  }
  const summary = `${findings.length} finding${findings.length === 1 ? '' : 's'} in ${fileCount} file${
    fileCount === 1 ? '' : 's'
  }`;
  return `${paint(summary, ANSI.bold, color)}  (${parts.join(', ')})`;
}

/**
 * JSON output: the raw native output, pretty-printed.
 * @param {object} output
 * @returns {string}
 */
function formatJson(output) {
  return JSON.stringify(output, null, 2);
}

/**
 * Compute the process exit code from findings + a failOn threshold.
 *   failOn "off" -> always 0.
 *   otherwise -> 1 if any finding's severity rank >= failOn's rank, else 0.
 *
 * @param {object[]} findings
 * @param {string} failOn  friendly severity ("warn"/"critical"/...) or "off"
 * @returns {0|1}
 */
function computeExitCode(findings, failOn) {
  const normalized = normalizeSeverity(failOn == null ? 'warning' : failOn, 'failOn');
  if (normalized === OFF) {
    return 0;
  }
  const threshold = severityRank(normalized);
  for (const f of findings) {
    if (severityRank(f.severity) >= threshold) {
      return 1;
    }
  }
  return 0;
}

module.exports = {
  collectFindings,
  groupByFile,
  countBySeverity,
  formatPretty,
  formatJson,
  computeExitCode,
};
