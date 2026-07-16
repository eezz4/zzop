'use strict';

// Focused `zzop <command> --help` blocks, SLICED at runtime out of bin/zzop.js's USAGE template
// literal — never re-typed. Two properties fall out of slicing instead of duplicating:
//   1. Per-command help cannot drift from the global `zzop --help` text (one source of truth).
//   2. scripts/check-cli-readme-sync.sh greps the USAGE literal for flag tokens and asserts
//      two-way parity with packages/cli/README.md — because every line printed here comes from
//      that literal, the guard automatically covers the per-command help too.
// The slice anchors below (entry-line prefixes, section header lines) are tripwires: if an edit
// to USAGE breaks one, `findLine` throws and test/help.test.js fails, instead of a subcommand
// silently printing empty help.

/**
 * Index of the first line satisfying `predicate`; throws (tripwire, see header) when absent.
 * @param {string[]} lines
 * @param {(line: string) => boolean} predicate
 * @param {string} what  anchor description for the tripwire error
 * @returns {number}
 */
function findLine(lines, predicate, what) {
  const idx = lines.findIndex(predicate);
  if (idx === -1) throw new Error(`subcommand help: USAGE anchor not found: ${what}`);
  return idx;
}

/**
 * One `Usage:` entry: the line starting with `  <prefix>` (two-space command indent) plus its
 * continuation lines (deeper-indented, until the next entry or a blank line).
 * @param {string[]} lines  USAGE split into lines
 * @param {string} prefix  distinguishing start of the entry's command text
 * @returns {string[]}
 */
function usageEntry(lines, prefix) {
  const start = findLine(lines, (l) => l.startsWith(`  ${prefix}`), `usage entry "${prefix}"`);
  let end = start + 1;
  while (end < lines.length && /^ {4,}\S/.test(lines[end])) end++;
  return lines.slice(start, end);
}

/**
 * A USAGE section: its exact header line plus following lines until `stop` matches (or a blank
 * line / end of text).
 * @param {string[]} lines
 * @param {string} header  exact section header line (e.g. "Options (run):")
 * @param {(line: string) => boolean} [stop]  extra terminator beyond the default blank line
 * @returns {string[]}
 */
function usageSection(lines, header, stop) {
  const start = findLine(lines, (l) => l === header, `section "${header}"`);
  let end = start + 1;
  while (end < lines.length && lines[end] !== '' && !(stop && stop(lines[end]))) end++;
  return lines.slice(start, end);
}

/**
 * Build the focused help text for the command named on the argv, or return null when the full
 * USAGE should print instead (bare `zzop --help`, or an unknown command — main() reports those).
 * @param {string} usage  the full USAGE text from bin/zzop.js
 * @param {{command: string|null, commandGiven: boolean, initSubcommand: string|null}} opts
 *   parsed CLI opts (see bin/zzop.js `parseArgs`)
 * @returns {string|null}
 */
function buildSubcommandHelp(usage, opts) {
  if (!opts.commandGiven) return null;
  const lines = usage.split('\n');
  const parts = [lines[0], '', 'Usage:'];
  if (opts.command === 'init' && opts.initSubcommand === 'adapter') {
    parts.push(...usageEntry(lines, 'zzop init adapter'));
  } else if (opts.command === 'init') {
    // Bare `zzop init --help` shows both init forms — `init adapter` is a sibling under it.
    parts.push(...usageEntry(lines, 'zzop init ['));
    parts.push(...usageEntry(lines, 'zzop init adapter'));
  } else if (opts.command === 'run') {
    parts.push(...usageEntry(lines, 'zzop [run]'), '');
    parts.push(...usageSection(lines, 'Options (run):'), '');
    // The run exit-code table ends where the `zzop adapter validate` footnote begins (they are
    // adjacent in USAGE, no blank line between).
    parts.push(...usageSection(lines, 'Exit codes (zzop [run]):', (l) => l.startsWith('`')));
  } else if (opts.command === 'endpoint') {
    parts.push(...usageEntry(lines, 'zzop endpoint'));
  } else if (opts.command === 'adapter') {
    parts.push(...usageEntry(lines, 'zzop adapter validate'), '');
    parts.push(lines[findLine(lines, (l) => l.startsWith('`zzop adapter validate`'), 'validate exit-code note')]);
  } else if (opts.command === 'pack') {
    parts.push(...usageEntry(lines, 'zzop pack validate'), '');
    parts.push(lines[findLine(lines, (l) => l.startsWith('`zzop pack validate`'), 'pack validate exit-code note')]);
  } else {
    return null;
  }
  return `${parts.join('\n')}\n`;
}

module.exports = { buildSubcommandHelp };
