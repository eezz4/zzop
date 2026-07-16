#!/usr/bin/env node
'use strict';

// `zzop` CLI entry point. Commands:
//   zzop init                -> write an annotated zzop.config.jsonc to cwd (init adapter: scaffold)
//   zzop [run]               -> load config, analyze via @zzop/native, print, exit per failOn
//   zzop endpoint <pattern>  -> definitive io-key query over the same analysis (exit 0 on any verdict)
//   zzop adapter validate    -> offline envelope check
//   zzop pack validate       -> offline rule-pack structure check
//
// Exit codes (run): 0 = ok (or findings below failOn); 1 = findings at/above failOn; 2 = config/usage error.

const fs = require('node:fs');
const path = require('node:path');

const { loadConfig, DEFAULT_CONFIG_FILENAME } = require('../lib/config');
const { configToRequest, collectConfigWarnings, ConfigError } = require('../lib/mapper');
const { expandAutoTrees } = require('../lib/workspaces');
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
const { lintEnvelope } = require('../lib/validate');
const { buildAdapterScaffold, ADAPTER_MODE_VALUES, ADAPTER_KIND_VALUES } = require('../lib/adapter-templates');
const { renderDebugIo } = require('../lib/debug-io');
const { renderEndpointReport } = require('../lib/endpoint');
const { buildSubcommandHelp } = require('../lib/help');

// Default scaffold directory for `zzop init adapter` — sibling to DEFAULT_CONFIG_FILENAME, mirrors how
// `zzop init` always writes to a fixed, unconfigurable target in the current directory.
const DEFAULT_ADAPTER_DIR = 'zzop-adapter';

// This literal is the single source of truth for ALL help text: `zzop <command> --help` prints a
// focused slice of it (lib/help.js), and scripts/check-cli-readme-sync.sh greps this block for
// flag-token parity with README.md. Editing a Usage entry's first line or a section header? The
// slice anchors in lib/help.js must keep matching — test/help.test.js trips if one breaks.
const USAGE = `zzop — multi-language SAST/architecture analysis, one \`zzop init\` away

Usage:
  zzop init [--force]              Write an annotated ${DEFAULT_CONFIG_FILENAME} to the current directory.
  zzop init adapter --mode <a|b> --kind <consume|provide> [--force]
                                    Scaffold a self-contained starter adapter in ./${DEFAULT_ADAPTER_DIR}/.
                                    --mode a = full envelope (replaces native analysis for the tree);
                                    --mode b = io-only overlay (merged via the overlays config key).
                                    --kind selects which side's extraction TODOs are stubbed in. See
                                    docs/adapters/README.md.
  zzop [run] [options]             Analyze using the config file (default command).
  zzop endpoint <pattern>          Definitive io-key query: is <pattern> (a case-insensitive substring
                                    of any io key — http routes, env keys, DB tables, topics) provided, consumed,
                                    or joined? Runs the same config-driven analysis as \`zzop run\` (the
                                    cache makes the re-run cheap) and prints ONE verdict — linked |
                                    provided-only | consumed-unprovided | external | unresolved-only |
                                    ambiguous | mixed | not-found — with the matching sites and, on
                                    not-found, key suggestions. Honors --config; --json prints the raw
                                    query JSON. Exits 0 regardless of verdict (a query is not a gate);
                                    2 = config/usage error.
  zzop adapter validate <path>     Offline check of an external-parser envelope (docs/NORMALIZED_AST.md)
                                    against the v1 contract, plus semantic hints. No config/root needed.
  zzop pack validate <path>        Offline structure check of a DSL rule-pack JSON file
                                    (docs/rules/dsl-reference.md) — the same judgments the engine's
                                    pack loader makes at load time (bad JSON, missing field, wrong
                                    type, too-new schema_version, a matcher regex that cannot
                                    compile), surfaced before loading. Shape only — never judges
                                    rule quality or semantics. No config/root needed.

Options (run):
  --config <path>                  Config file to use (default ./${DEFAULT_CONFIG_FILENAME}).
  --format <pretty|json>           Output format (overrides config).
  --json                           Alias for --format json.
  --out <dir>                      Write reports to <dir>/zzop.<epoch>/ (default dir ./zzop-reports).
                                    Default format is markdown: one file per tree, plus cross-repo.md for
                                    a multi-tree run. Set config report.formats to also/instead emit
                                    json/sarif, or report.enabled: false to disable report writing.
  -a, --all                        Show everything expanded: info-level findings (folded to per-rule
                                    counts by default) AND each finding's full message (folded to a
                                    one-line summary by default). The complete message is always in the
                                    JSON output and markdown reports regardless of this flag.
  --severity <critical|warning|info|off>
                                    Only display findings at/above this severity (default: off = show all).
  --debug-io                       After the normal output, dump every cross-layer join bucket (edges,
                                    unconsumedProvides, unprovidedConsumes, unresolvedConsumes,
                                    externalConsumes, ambiguousConsumes) as deterministic plain text —
                                    the join-debug surface for troubleshooting an adapter/overlay.
  -h, --help                       Show this help.
  --version                        Show the CLI and engine versions.

Exit codes (zzop [run]):
  0   No finding at or above failOn (config default: warn).
  1   At least one finding at or above failOn.
  2   Config or usage error.
\`zzop adapter validate\` ignores failOn: 0 = envelope structurally valid, 1 = invalid, 2 = usage error.
\`zzop pack validate\` ignores failOn: 0 = pack structurally valid, 1 = invalid, 2 = usage error.
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
    // Whether a command was named on the argv at all — distinguishes bare `zzop --help` (full
    // usage) from `zzop run --help` (focused run help) after `command` defaults to 'run' below.
    commandGiven: false,
    config: null,
    format: null,
    force: false,
    help: false,
    version: false,
    all: false,
    out: null,
    severity: null,
    debugIo: false,
    // `adapter validate <path>` only:
    subcommand: null,
    envelopePath: null,
    // `pack validate <path>` only:
    packSubcommand: null,
    packPath: null,
    // `endpoint <pattern>` only:
    pattern: null,
    // `init adapter --mode <a|b> --kind <consume|provide>` only:
    initSubcommand: null,
    mode: null,
    kind: null,
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
        // `run|endpoint`: valid under both commands — the cross-check below splits on `|`.
        scoped.push({ flag: arg, scope: 'run|endpoint' });
        break;
      case '-a':
      case '--all':
        opts.all = true;
        scoped.push({ flag: arg, scope: 'run' });
        break;
      case '--config':
        opts.config = argv[++i];
        if (opts.config === undefined) throw new ConfigError('--config requires a <path> argument.');
        scoped.push({ flag: arg, scope: 'run|endpoint' });
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
      case '--debug-io':
        opts.debugIo = true;
        scoped.push({ flag: arg, scope: 'run' });
        break;
      case '--mode':
        opts.mode = argv[++i];
        if (opts.mode === undefined) throw new ConfigError('--mode requires <a|b>.');
        if (!ADAPTER_MODE_VALUES.includes(opts.mode)) {
          throw new ConfigError(
            `Invalid mode "${opts.mode}". Expected one of: ${ADAPTER_MODE_VALUES.join(', ')}.`
          );
        }
        // Scoped to `init adapter` specifically, not just `init` — see the cross-check below, which
        // special-cases this scope value (a bare `zzop init --mode a` is rejected too).
        scoped.push({ flag: arg, scope: 'init-adapter' });
        break;
      case '--kind':
        opts.kind = argv[++i];
        if (opts.kind === undefined) throw new ConfigError('--kind requires <consume|provide>.');
        if (!ADAPTER_KIND_VALUES.includes(opts.kind)) {
          throw new ConfigError(
            `Invalid kind "${opts.kind}". Expected one of: ${ADAPTER_KIND_VALUES.join(', ')}.`
          );
        }
        scoped.push({ flag: arg, scope: 'init-adapter' });
        break;
      default:
        if (arg.startsWith('-')) {
          throw new ConfigError(`Unknown option "${arg}". Run \`zzop --help\`.`);
        }
        rest.push(arg);
    }
  }

  opts.commandGiven = rest.length > 0;
  opts.command = rest[0] || 'run';
  if (opts.command === 'adapter') {
    // `zzop adapter validate <path>` — a second positional (subcommand) plus a third (the envelope
    // path), unlike every other command's single positional. Stashed here regardless of validity so
    // `--help`/`--version` can still read `opts.command` before the escape-hatch-gated checks below run.
    opts.subcommand = rest[1];
    opts.envelopePath = rest[2];
  } else if (opts.command === 'pack') {
    // `zzop pack validate <path>` — same two-extra-positionals shape as `adapter validate`, same
    // "stash regardless of validity" escape-hatch reasoning for `--help`/`--version`.
    opts.packSubcommand = rest[1];
    opts.packPath = rest[2];
  } else if (opts.command === 'init') {
    // `zzop init adapter --mode <a|b> --kind <consume|provide>` — a second positional (`adapter`),
    // unlike bare `init`'s zero positionals. Stashed regardless of validity, same escape-hatch reasoning
    // as `adapter`'s subcommand above.
    opts.initSubcommand = rest[1];
    // Same help/version escape hatch as `adapter`/`pack`'s positional-shape checks below — a
    // trailing `--help` must print help, not trip on the extra positional it is escaping.
    if (!opts.help && !opts.version && rest.length > 2) {
      throw new ConfigError(`Unexpected argument "${rest[2]}". Run \`zzop --help\`.`);
    }
  } else if (opts.command === 'endpoint') {
    // `zzop endpoint <pattern>` — a second positional (the pattern). Stashed regardless of
    // validity, same `--help`/`--version` escape-hatch reasoning as `adapter`'s subcommand above
    // (the escape hatch also gates the extra-positional check, mirroring `adapter`/`pack`).
    opts.pattern = rest[1];
    if (!opts.help && !opts.version && rest.length > 2) {
      throw new ConfigError(`Unexpected argument "${rest[2]}". Run \`zzop --help\`.`);
    }
  } else if (rest.length > 1) {
    throw new ConfigError(`Unexpected argument "${rest[1]}". Run \`zzop --help\`.`);
  }

  // Reject a flag used with the wrong command (e.g. `zzop init --all`, `zzop run --force`). Skipped when
  // `--help`/`--version` is present (both are global escape hatches that short-circuit the command) and
  // for an unknown command (main() reports that more specifically). `adapter` scopes no flags of its
  // own, so EVERY scoped flag is invalid there (`scope !== 'adapter'` always holds) — including it here
  // rejects e.g. `zzop adapter validate x.json --json` instead of silently ignoring the flag. `--mode`/
  // `--kind` carry the synthetic scope `init-adapter` instead of `init` — they are valid only under
  // `zzop init adapter`, not bare `zzop init`, so they get their own branch here rather than reusing the
  // command-name-equality check every other scope uses.
  if (
    !opts.help &&
    !opts.version &&
    (opts.command === 'init' ||
      opts.command === 'run' ||
      opts.command === 'adapter' ||
      opts.command === 'pack' ||
      opts.command === 'endpoint')
  ) {
    for (const { flag, scope } of scoped) {
      if (scope === 'init-adapter') {
        if (opts.command !== 'init' || opts.initSubcommand !== 'adapter') {
          throw new ConfigError(`"${flag}" is only valid with \`zzop init adapter\`. Run \`zzop --help\`.`);
        }
        continue;
      }
      // A scope may name several valid commands, `|`-separated (`--config`/`--json` are shared by
      // `run` and `endpoint`); a single-command scope degenerates to the old equality check.
      if (!scope.split('|').includes(opts.command)) {
        throw new ConfigError(
          `"${flag}" is not valid for the \`${opts.command}\` command. Run \`zzop --help\`.`
        );
      }
    }
  }

  // `adapter` positional-shape checks (its scoped-flag rejection is handled by the shared cross-check
  // above); same help/version escape hatch.
  if (!opts.help && !opts.version && opts.command === 'adapter') {
    if (rest.length > 3) {
      throw new ConfigError(`Unexpected argument "${rest[3]}". Run \`zzop --help\`.`);
    }
    if (opts.subcommand !== 'validate') {
      throw new ConfigError(
        `Unknown "adapter" subcommand "${opts.subcommand || ''}" — only "adapter validate <path>" is supported. Run \`zzop --help\`.`
      );
    }
    if (!opts.envelopePath) {
      throw new ConfigError('"zzop adapter validate" requires a <envelope.json> path argument.');
    }
  }

  // `pack` positional-shape checks — mirror `adapter`'s exactly (its scoped-flag rejection is handled
  // by the shared cross-check above, where `pack` scopes no flags of its own); same escape hatch.
  if (!opts.help && !opts.version && opts.command === 'pack') {
    if (rest.length > 3) {
      throw new ConfigError(`Unexpected argument "${rest[3]}". Run \`zzop --help\`.`);
    }
    if (opts.packSubcommand !== 'validate') {
      throw new ConfigError(
        `Unknown "pack" subcommand "${opts.packSubcommand || ''}" — only "pack validate <path>" is supported. Run \`zzop --help\`.`
      );
    }
    if (!opts.packPath) {
      throw new ConfigError('"zzop pack validate" requires a <pack.json> path argument.');
    }
  }

  // `endpoint` positional-shape check — the pattern is required; same help/version escape hatch.
  if (!opts.help && !opts.version && opts.command === 'endpoint' && !opts.pattern) {
    throw new ConfigError('"zzop endpoint" requires a <pattern> argument. Run `zzop endpoint --help`.');
  }

  // `init adapter` positional-shape + required-flags checks (its scoped-flag rejection is handled by the
  // shared cross-check above); same help/version escape hatch. A bare `zzop init` (no `initSubcommand`)
  // skips this block entirely.
  if (!opts.help && !opts.version && opts.command === 'init' && opts.initSubcommand != null) {
    if (opts.initSubcommand !== 'adapter') {
      throw new ConfigError(
        `Unknown "init" subcommand "${opts.initSubcommand}" — only "init adapter" is supported. Run \`zzop --help\`.`
      );
    }
    const missing = [];
    if (!opts.mode) missing.push('--mode <a|b>');
    if (!opts.kind) missing.push('--kind <consume|provide>');
    if (missing.length > 0) {
      throw new ConfigError(`"zzop init adapter" requires ${missing.join(' and ')}.`);
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

/**
 * `zzop init adapter --mode <a|b> --kind <consume|provide> [--force]` — scaffolds a self-contained
 * starter adapter into `./${DEFAULT_ADAPTER_DIR}/` (main.mjs, lib/keys.mjs, lib/envelope.mjs, README.md;
 * see lib/adapter-templates/index.js). Refuses to overwrite an existing target directory without
 * `--force`, mirroring `runInit`'s existing-config refusal above.
 * @param {object} opts  parsed CLI opts (`mode`, `kind`, `force`)
 */
function runInitAdapter(opts) {
  const targetDir = path.resolve(process.cwd(), DEFAULT_ADAPTER_DIR);
  if (fs.existsSync(targetDir) && !opts.force) {
    fail(`${DEFAULT_ADAPTER_DIR}/ already exists. Use --force to overwrite.`, 2);
  }

  const files = buildAdapterScaffold({ mode: opts.mode, kind: opts.kind });
  for (const f of files) {
    const dest = path.join(targetDir, f.name);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, f.content, 'utf8');
  }
  const rel = path.relative(process.cwd(), targetDir) || DEFAULT_ADAPTER_DIR;
  process.stdout.write(`Wrote ${files.length} file${files.length === 1 ? '' : 's'} to ${rel}/\n`);
  process.exit(0);
}

/**
 * Renders `zzop adapter validate <path>`'s combined report: the native structural verdict
 * (`report.valid`/`report.issues`, from `zzop_core::validate_envelope` via `validateEnvelopeOnly`) plus
 * this package's own offline semantic `hints` (`lib/validate.js`'s `lintEnvelope`). Hints are advisory —
 * they never appear as "Issues" and never affect the exit code, only `report.valid` does.
 *
 * @param {string} filePath  the path as given on the command line (echoed back, not resolved)
 * @param {{valid: boolean, issues: string[]}} report  parsed `validateEnvelopeOnly` output
 * @param {string[]} hints  `lintEnvelope` output
 * @returns {string}
 */
function formatValidateReport(filePath, report, hints) {
  const issues = Array.isArray(report.issues) ? report.issues : [];
  const lines = [report.valid ? `${filePath}: valid` : `${filePath}: INVALID`];
  if (issues.length) {
    lines.push('Issues:');
    for (const issue of issues) lines.push(`  - ${issue}`);
  }
  if (hints.length) {
    lines.push('Hints:');
    for (const hint of hints) lines.push(`  - ${hint}`);
  }
  if (!issues.length && !hints.length) {
    lines.push('No issues or hints.');
  }
  return lines.join('\n');
}

function runAdapterValidate(opts) {
  const resolvedPath = path.resolve(process.cwd(), opts.envelopePath);
  let raw;
  try {
    raw = fs.readFileSync(resolvedPath, 'utf8');
  } catch (err) {
    fail(`Failed to read "${opts.envelopePath}": ${err && err.message}`, 2);
    return;
  }

  // Load the native engine lazily, same as `runAnalyze` — `zzop adapter validate` should still fail with
  // a clear message (not a stack trace) if the addon isn't installed/built.
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

  let reportJson;
  try {
    reportJson = native.validateEnvelopeOnly(raw);
  } catch (err) {
    fail(`Envelope validation failed: ${err && err.message}`, 2);
    return;
  }

  let report;
  try {
    report = JSON.parse(reportJson);
  } catch (err) {
    fail(`Engine returned malformed JSON: ${err && err.message}`, 2);
    return;
  }

  // Semantic hints (`lib/validate.js`, pure JS, no native call) run alongside the native structural
  // check. They need a parsed envelope OBJECT, not the raw string; when `raw` itself isn't valid JSON
  // there is nothing to lint — the native `issues` list already reports "invalid JSON" for that case.
  let hints = [];
  try {
    hints = lintEnvelope(JSON.parse(raw));
  } catch {
    /* raw isn't valid JSON — nothing to lint; native's issues already cover this. */
  }

  process.stdout.write(`${formatValidateReport(opts.envelopePath, report, hints)}\n`);
  process.exit(report.valid ? 0 : 1);
}

/**
 * `zzop pack validate <path>` — offline structure check of a DSL rule-pack JSON file, mirroring
 * `runAdapterValidate` exactly (read file -> lazy native load -> `{valid, issues}` report -> exit
 * 0/1/2). The verdicts are the engine pack loader's own load-time judgments plus non-compiling
 * matcher regexes, surfaced via the native `validateRulePackOnly` (see `crates/facade/src/rule_pack.rs`)
 * — shape only, never rule-quality semantics. No semantic hints layer here (unlike the envelope
 * validator): the loader's judgments ARE the whole contract.
 * @param {object} opts  parsed CLI opts (`packPath`)
 */
function runPackValidate(opts) {
  const resolvedPath = path.resolve(process.cwd(), opts.packPath);
  let raw;
  try {
    raw = fs.readFileSync(resolvedPath, 'utf8');
  } catch (err) {
    fail(`Failed to read "${opts.packPath}": ${err && err.message}`, 2);
    return;
  }

  // Load the native engine lazily, same as `runAdapterValidate` — a missing addon must be a clear
  // message, not a stack trace.
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

  let reportJson;
  try {
    reportJson = native.validateRulePackOnly(raw);
  } catch (err) {
    fail(`Rule-pack validation failed: ${err && err.message}`, 2);
    return;
  }

  let report;
  try {
    report = JSON.parse(reportJson);
  } catch (err) {
    fail(`Engine returned malformed JSON: ${err && err.message}`, 2);
    return;
  }

  process.stdout.write(`${formatValidateReport(opts.packPath, report, [])}\n`);
  process.exit(report.valid ? 0 : 1);
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
  const rawConfig = loadConfig(configPath);
  // Expand `trees: "auto"` into a concrete per-workspace-package trees array before anything else reads
  // the config (a no-op for every other `trees`/`roots` shape). Resolved against process.cwd() — the same
  // directory the native engine resolves relative roots against — so a derived tree root means the same
  // thing to the engine as a hand-written one. Its notes go through the same stderr warnings channel.
  const { config, warnings: autoTreeWarnings } = expandAutoTrees(rawConfig, process.cwd());
  emitWarnings(autoTreeWarnings);
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

  // `--debug-io`: the join-debug surface, printed AFTER the normal output (never instead of it) and
  // regardless of `--format`/`--severity` — those are display filters over findings, not over the
  // cross-layer join data this dumps. `output.crossLayer` only exists on a multi-tree (`analyzeTrees`)
  // output; `renderDebugIo` treats an absent/empty one as "every bucket is empty" rather than throwing,
  // so this is safe on a single-tree run too.
  if (opts.debugIo) {
    process.stdout.write(`${renderDebugIo(output && output.crossLayer)}\n`);
  }

  writeReports(opts, config, output, method, request);

  // Exit code is ALWAYS computed from the unfiltered findings — `--severity` is a display-only filter and
  // must never change whether the process exits nonzero for `failOn`.
  const { findings } = collectFindings(output);
  const failOn = config.failOn == null ? 'warn' : config.failOn;
  process.exit(computeExitCode(findings, failOn));
}

/**
 * `zzop endpoint <pattern>` — the definitive io-key query. Same config discovery and analysis
 * pipeline as `zzop [run]` (a `cacheDir` in the config makes the re-run cheap), then the shared
 * facade query core (`@zzop/native`'s `queryIo` — the exact core the zzop-mcp `check_endpoint`
 * tool uses, so both hosts give the identical answer). The analysis ALWAYS routes through
 * `analyzeTrees`, even for a single-tree config: a verdict is a cross-layer JOIN fact and the
 * query core rejects a plain single-tree `analyze` output as pre-join — one tree passed through
 * `analyzeTrees` still gets the join, intra-tree edges included. Exit 0 regardless of verdict
 * (a query is not a gate); 2 = config/usage/engine error.
 * @param {object} opts  parsed CLI opts (`pattern`, `config`, `format`)
 */
function runEndpoint(opts) {
  const configPath = opts.config || DEFAULT_CONFIG_FILENAME;
  const rawConfig = loadConfig(configPath);
  const { config, warnings: autoTreeWarnings } = expandAutoTrees(rawConfig, process.cwd());
  emitWarnings(autoTreeWarnings);
  emitWarnings(collectConfigWarnings(config));
  const { method, request } = configToRequest(config);
  const treesRequest = method === 'analyzeTrees' ? request : { trees: [request] };

  // Load the native engine lazily, same as `runAnalyze`.
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

  let resultJson;
  try {
    const analysisJson = native.analyzeTrees(JSON.stringify(treesRequest));
    // The engine's self-reported warnings keep their stderr honesty channel here too.
    emitWarnings(collectWarnings(JSON.parse(analysisJson)));
    resultJson = native.queryIo(analysisJson, JSON.stringify({ pattern: opts.pattern }));
  } catch (err) {
    fail(`Endpoint query failed: ${err && err.message}`, 2);
    return;
  }

  if (opts.format === 'json') {
    // Raw query JSON, verbatim from the shared core — the machine-readable surface.
    process.stdout.write(`${resultJson}\n`);
  } else {
    let result;
    try {
      result = JSON.parse(resultJson);
    } catch (err) {
      fail(`Engine returned malformed JSON: ${err && err.message}`, 2);
      return;
    }
    process.stdout.write(`${renderEndpointReport(result)}\n`);
  }
  process.exit(0);
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
  // stderr, not stdout: stdout is the analysis output surface (`--format json` must stay parseable as
  // pure JSON), and this notice is operational chatter like the warnings above.
  process.stderr.write(`Wrote ${files.length} report${files.length === 1 ? '' : 's'} to ${rel}\n`);
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
    // `zzop <command> --help` prints a focused slice of USAGE for that command; a bare
    // `zzop --help` (or an unknown command) falls back to the full text.
    process.stdout.write(buildSubcommandHelp(USAGE, opts) || USAGE);
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
      if (opts.initSubcommand === 'adapter') {
        runInitAdapter(opts);
      } else {
        runInit(opts);
      }
    } else if (opts.command === 'run') {
      runAnalyze(opts);
    } else if (opts.command === 'endpoint') {
      runEndpoint(opts);
    } else if (opts.command === 'adapter') {
      runAdapterValidate(opts);
    } else if (opts.command === 'pack') {
      runPackValidate(opts);
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

module.exports = { parseArgs, USAGE };
