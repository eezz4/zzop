#!/usr/bin/env node
'use strict';

// `zzop` CLI entry point. Two commands:
//   zzop init            -> write an annotated zzop.config.jsonc to cwd
//   zzop [run]           -> load config, analyze via @zzop/native, print, exit per failOn
//
// Exit codes: 0 = ok (or findings below failOn); 1 = findings at/above failOn; 2 = config/usage error.

const fs = require('node:fs');
const path = require('node:path');

const { loadConfig, DEFAULT_CONFIG_FILENAME } = require('../lib/config');
const { configToRequest, collectConfigWarnings, ConfigError } = require('../lib/mapper');
const {
  collectFindings,
  collectWarnings,
  filterOutputBySeverity,
  formatPretty,
  formatJson,
  computeExitCode,
} = require('../lib/format');
const { buildReports } = require('../lib/report');
const { CONFIG_TEMPLATE } = require('../lib/init');

const USAGE = `zzop — zero-config multi-language SAST/architecture analysis

Usage:
  zzop init [--force]              Write an annotated ${DEFAULT_CONFIG_FILENAME} to the current directory.
  zzop [run] [options]             Analyze using the config file (default command).

Options (run):
  --config <path>                  Config file to use (default ./${DEFAULT_CONFIG_FILENAME}).
  --format <pretty|json>           Output format (overrides config).
  --json                           Alias for --format json.
  --out <dir>                      Write reports to <dir>/zzop.<epoch>/ (default dir ./zzop-reports).
                                    Default format is markdown: one file per tree, plus cross-repo.md for
                                    a multi-tree run. Set config report.formats to also/instead emit
                                    json/sarif, or report.enabled: false to disable report writing.
  -a, --all                        Expand info-level findings (folded to per-rule counts by default).
  --severity <critical|warning|info|off>
                                    Only display findings at/above this severity (default: off = show all).
  -h, --help                       Show this help.
  --version                        Show the CLI and engine versions.
`;

// Valid `--severity` values. Exact match only (no friendly aliases like the config's "warn") — this is a
// small, literal display-filter flag, not the config's severity-override surface.
const SEVERITY_VALUES = ['critical', 'warning', 'info', 'off'];

function fail(message, code = 2) {
  process.stderr.write(`zzop: ${message}\n`);
  process.exit(code);
}

/**
 * Parse argv (after `node zzop.js`) into a command + options. Throws ConfigError on an unknown flag.
 * @param {string[]} argv
 */
function parseArgs(argv) {
  const opts = {
    command: null,
    config: null,
    format: null,
    force: false,
    help: false,
    version: false,
    all: false,
    out: null,
    severity: null,
  };
  const rest = [];
  // Scoped flags seen, for the command/flag cross-check below: `{ flag, scope }`.
  const scoped = [];

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    switch (arg) {
      case '-h':
      case '--help':
        opts.help = true;
        break;
      case '--version':
        opts.version = true;
        break;
      case '--force':
        opts.force = true;
        scoped.push({ flag: arg, scope: 'init' });
        break;
      case '--json':
        opts.format = 'json';
        scoped.push({ flag: arg, scope: 'run' });
        break;
      case '-a':
      case '--all':
        opts.all = true;
        scoped.push({ flag: arg, scope: 'run' });
        break;
      case '--config':
        opts.config = argv[++i];
        if (opts.config === undefined) throw new ConfigError('--config requires a <path> argument.');
        scoped.push({ flag: arg, scope: 'run' });
        break;
      case '--format':
        opts.format = argv[++i];
        if (opts.format === undefined) throw new ConfigError('--format requires <pretty|json>.');
        scoped.push({ flag: arg, scope: 'run' });
        break;
      case '--out':
        opts.out = argv[++i];
        if (opts.out === undefined) throw new ConfigError('--out requires a <dir> argument.');
        scoped.push({ flag: arg, scope: 'run' });
        break;
      case '--severity':
        opts.severity = argv[++i];
        if (opts.severity === undefined) {
          throw new ConfigError('--severity requires <critical|warning|info|off>.');
        }
        if (!SEVERITY_VALUES.includes(opts.severity)) {
          throw new ConfigError(
            `Invalid severity "${opts.severity}". Expected one of: ${SEVERITY_VALUES.join(', ')}.`
          );
        }
        scoped.push({ flag: arg, scope: 'run' });
        break;
      default:
        if (arg.startsWith('-')) {
          throw new ConfigError(`Unknown option "${arg}". Run \`zzop --help\`.`);
        }
        rest.push(arg);
    }
  }

  opts.command = rest[0] || 'run';
  if (rest.length > 1) {
    throw new ConfigError(`Unexpected argument "${rest[1]}". Run \`zzop --help\`.`);
  }

  // Reject a flag used with the wrong command (e.g. `zzop init --all`, `zzop run --force`). Skipped when
  // `--help`/`--version` is present (both are global escape hatches that short-circuit the command) and
  // for an unknown command (main() reports that more specifically). Only `init` and `run` scope flags.
  if (!opts.help && !opts.version && (opts.command === 'init' || opts.command === 'run')) {
    for (const { flag, scope } of scoped) {
      if (scope !== opts.command) {
        throw new ConfigError(
          `"${flag}" is not valid for the \`${opts.command}\` command. Run \`zzop --help\`.`
        );
      }
    }
  }
  return opts;
}

function runInit(opts) {
  const target = path.resolve(process.cwd(), DEFAULT_CONFIG_FILENAME);
  if (fs.existsSync(target) && !opts.force) {
    fail(`${DEFAULT_CONFIG_FILENAME} already exists. Use --force to overwrite.`, 2);
  }
  fs.writeFileSync(target, CONFIG_TEMPLATE, 'utf8');
  process.stdout.write(`Wrote ${path.relative(process.cwd(), target) || DEFAULT_CONFIG_FILENAME}\n`);
  process.exit(0);
}

function resolveFormat(opts, config) {
  const format = opts.format || config.format || 'pretty';
  if (format !== 'pretty' && format !== 'json') {
    throw new ConfigError(`Invalid format "${format}". Expected "pretty" or "json".`);
  }
  return format;
}

// Emit warnings to stderr (prefixed, one per line) so they never pollute stdout — pretty or JSON.
function emitWarnings(warnings) {
  for (const w of warnings) {
    process.stderr.write(`zzop: warning: ${w}\n`);
  }
}

function runAnalyze(opts) {
  const configPath = opts.config || DEFAULT_CONFIG_FILENAME;
  const config = loadConfig(configPath);
  const format = resolveFormat(opts, config);
  // Surface unknown config keys (typos / cross-version drift) — ignored by the engine, but not silently.
  emitWarnings(collectConfigWarnings(config));
  const { method, request } = configToRequest(config);

  // Load the native engine lazily so `zzop init` / `--help` work without the addon installed/built.
  let native;
  try {
    native = require('@zzop/native');
  } catch (err) {
    fail(
      `Failed to load the @zzop/native engine: ${err && err.message}\n` +
        `Ensure @zzop/native is installed for this platform (it is a dependency of zzop).`,
      2
    );
    return;
  }

  let outputJson;
  try {
    outputJson = native[method](JSON.stringify(request));
  } catch (err) {
    fail(`Analysis failed: ${err && err.message}`, 2);
    return;
  }

  let output;
  try {
    output = JSON.parse(outputJson);
  } catch (err) {
    fail(`Engine returned malformed JSON: ${err && err.message}`, 2);
    return;
  }

  // Surface the engine's own self-reported warnings (narrowed scope, git not requested, no packs found, …).
  emitWarnings(collectWarnings(output));

  if (format === 'json') {
    process.stdout.write(`${formatJson(filterOutputBySeverity(output, opts.severity))}\n`);
  } else {
    const color = Boolean(process.stdout.isTTY);
    process.stdout.write(
      `${formatPretty(output, { color, showAllInfo: opts.all, minSeverity: opts.severity })}\n`
    );
  }

  writeReports(opts, config, output, method, request);

  // Exit code is ALWAYS computed from the unfiltered findings — `--severity` is a display-only filter and
  // must never change whether the process exits nonzero for `failOn`.
  const { findings } = collectFindings(output);
  const failOn = config.failOn == null ? 'warn' : config.failOn;
  process.exit(computeExitCode(findings, failOn));
}

// Base report directory when neither `--out` nor config `report.dir` names one — reports are written by
// default now (markdown is meant to be the delivery surface for a cross-repo review), so this always
// applies unless report writing is explicitly disabled (see `report.enabled` below).
const DEFAULT_REPORT_BASE_DIR = 'zzop-reports';

/**
 * Write report files. Reports are written BY DEFAULT (default format `md`, default base dir
 * `./zzop-reports`) — set config `report.enabled: false` to opt out entirely (e.g. for CI runs that don't
 * want files on disk). `--out <dir>` (or config `report.dir`) overrides the base dir; config
 * `report.formats` (e.g. `["md", "json", "sarif"]`) overrides which formats are written. Each run lands in
 * its own `<baseDir>/zzop.<epoch-seconds>/` subdirectory so successive runs accumulate rather than
 * overwrite — two runs within the same wall-clock second share a subdir and the later one overwrites.
 *
 * @param {object} opts    parsed CLI opts (`--out`)
 * @param {object} config  loaded config (`report.dir`/`report.formats`/`report.enabled`)
 * @param {object} output  parsed native output
 * @param {'analyze'|'analyzeTrees'} method  which native entry point produced `output`
 * @param {object} request  the request object passed to that native entry point (its `root`/`sourceId`
 *   back-fill the single-tree markdown report's identity — see `buildMarkdownReports`'s doc for why the
 *   single-tree output shape alone doesn't carry them)
 */
function writeReports(opts, config, output, method, request) {
  const reportCfg = (config && config.report) || {};
  if (reportCfg.enabled === false) {
    return;
  }
  const baseDir = opts.out || reportCfg.dir || DEFAULT_REPORT_BASE_DIR;
  const formats = Array.isArray(reportCfg.formats) && reportCfg.formats.length ? reportCfg.formats : ['md'];

  let files;
  try {
    files = buildReports(output, {
      formats,
      toolVersion: require('../package.json').version,
      sourceId: method === 'analyze' ? request.sourceId : undefined,
      root: method === 'analyze' ? request.root : undefined,
    });
  } catch (err) {
    fail(`Report generation failed: ${err && err.message}`, 2);
    return;
  }

  const stamp = String(Math.floor(Date.now() / 1000));
  const dir = path.resolve(process.cwd(), String(baseDir), `zzop.${stamp}`);
  try {
    fs.mkdirSync(dir, { recursive: true });
    for (const f of files) {
      fs.writeFileSync(path.join(dir, f.name), f.content, 'utf8');
    }
  } catch (err) {
    fail(`Failed to write reports to ${dir}: ${err && err.message}`, 2);
    return;
  }
  const rel = path.relative(process.cwd(), dir) || dir;
  process.stdout.write(`Wrote ${files.length} report${files.length === 1 ? '' : 's'} to ${rel}\n`);
}

function main() {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (err) {
    if (err instanceof ConfigError) fail(err.message, 2);
    throw err;
  }

  if (opts.help) {
    process.stdout.write(USAGE);
    process.exit(0);
  }

  if (opts.version) {
    const pkg = require('../package.json');
    let engine = '(not loaded)';
    try {
      engine = require('@zzop/native').version();
    } catch {
      /* engine not installed/built — fine for --version */
    }
    process.stdout.write(`zzop ${pkg.version}\nengine ${engine}\n`);
    process.exit(0);
  }

  try {
    if (opts.command === 'init') {
      runInit(opts);
    } else if (opts.command === 'run') {
      runAnalyze(opts);
    } else {
      fail(`Unknown command "${opts.command}". Run \`zzop --help\`.`, 2);
    }
  } catch (err) {
    if (err instanceof ConfigError) {
      fail(err.message, 2);
    }
    throw err;
  }
}

// Run as a CLI; stay import-safe so `parseArgs` can be unit-tested without executing the tool.
if (require.main === module) {
  main();
}

module.exports = { parseArgs };
