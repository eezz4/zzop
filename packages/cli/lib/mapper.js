'use strict';

// PURE config -> native-request mapper. No I/O, no native calls, no process access — everything here is
// a deterministic function of its `config` argument so it can be unit-tested without the native addon.
// The merge model (layered precedence: bundled packs, then extra pack dirs, then disabled rules,
// then per-rule severity/exclude overrides) is documented in this package's README.

/**
 * A configuration/usage error the CLI should report and exit(2) on. Thrown by the mapper and config
 * loader for anything the user can fix in their config file or flags.
 */
class ConfigError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ConfigError';
  }
}

// ---------------------------------------------------------------------------------------------------
// Severity normalization — the SINGLE source of truth for turning friendly config severities into the
// engine's `Severity` serde values.
//
// CONFIRMED against the engine: `packages/core/src/finding.rs` declares
//   #[serde(rename_all = "lowercase")] enum Severity { Critical, Warning, Info }
// so the engine's JSON `Severity` strings are exactly "critical" / "warning" / "info". The napi
// `AnalyzeRequest.severityOverrides` (packages/napi/src/api.rs) reuses that same enum. If the engine's
// Severity serde ever changes, adjust ONLY the ENGINE_SEVERITY values below.
// ---------------------------------------------------------------------------------------------------

const ENGINE_SEVERITY = {
  CRITICAL: 'critical',
  WARNING: 'warning',
  INFO: 'info',
};

// Sentinel returned for "off" — the caller routes this to disabledRules instead of severityOverrides.
const OFF = 'off';

// Friendly alias -> engine severity (or OFF). Lower-cased before lookup.
const SEVERITY_ALIASES = {
  off: OFF,
  none: OFF,
  disable: OFF,
  disabled: OFF,

  critical: ENGINE_SEVERITY.CRITICAL,
  error: ENGINE_SEVERITY.CRITICAL,
  err: ENGINE_SEVERITY.CRITICAL,
  high: ENGINE_SEVERITY.CRITICAL,

  warning: ENGINE_SEVERITY.WARNING,
  warn: ENGINE_SEVERITY.WARNING,
  medium: ENGINE_SEVERITY.WARNING,

  info: ENGINE_SEVERITY.INFO,
  information: ENGINE_SEVERITY.INFO,
  note: ENGINE_SEVERITY.INFO,
  low: ENGINE_SEVERITY.INFO,
};

/**
 * Normalize a friendly severity string to the engine's `Severity` value, the sentinel `"off"`, or throw
 * a ConfigError for an unrecognized value.
 *
 * @param {string} value  raw severity from config (e.g. "warn", "off", "critical")
 * @param {string} [context]  optional label for error messages (e.g. a rule id)
 * @returns {'critical'|'warning'|'info'|'off'}
 */
function normalizeSeverity(value, context) {
  if (typeof value !== 'string') {
    throw new ConfigError(
      `Invalid severity ${JSON.stringify(value)}${context ? ` for "${context}"` : ''}: expected a string.`
    );
  }
  const key = value.trim().toLowerCase();
  const mapped = SEVERITY_ALIASES[key];
  if (mapped === undefined) {
    const valid = Object.keys(SEVERITY_ALIASES).join(', ');
    throw new ConfigError(
      `Unknown severity ${JSON.stringify(value)}${context ? ` for "${context}"` : ''}. ` +
        `Expected one of: ${valid}.`
    );
  }
  return mapped;
}

// Severity ordering for failOn comparisons: info < warning < critical.
const SEVERITY_RANK = {
  [ENGINE_SEVERITY.INFO]: 1,
  [ENGINE_SEVERITY.WARNING]: 2,
  [ENGINE_SEVERITY.CRITICAL]: 3,
};

/**
 * Numeric rank of an engine severity, or 0 for an unknown value (so unknown never trips failOn).
 * @param {string} severity
 * @returns {number}
 */
function severityRank(severity) {
  return SEVERITY_RANK[severity] || 0;
}

function isPlainObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

// A `rules[].exclude` entry is treated as a glob (full-path, anchored, engine-side) when it carries a
// glob metacharacter; otherwise it is a plain substring filter. `[`/`]` are excluded on purpose so raw
// Next.js dynamic-segment paths like `app/[locale]/` are matched as substrings, not char classes.
function isGlobPattern(value) {
  return /[*?{}]/.test(value);
}

/**
 * Build the per-tree option bundle shared by every tree/root — the rule/pack/git/cache knobs that are
 * global to the config (not per-tree). Returns an object with only the fields that are actually set, so
 * omitted config keys fall through to the engine/napi-wrapper defaults.
 *
 * @param {object} config
 * @returns {object}
 */
function buildSharedOptions(config) {
  const shared = {};

  // --- packs.extraDirs -> packsDir (user dirs only; the napi wrapper PREPENDS the bundled dir). ---
  const packs = config.packs || {};
  if (packs.extraDirs !== undefined && !Array.isArray(packs.extraDirs)) {
    throw new ConfigError('packs.extraDirs must be an array of directory paths.');
  }
  const extraDirs = Array.isArray(packs.extraDirs) ? packs.extraDirs.filter((d) => d !== '') : [];
  if (extraDirs.length > 0) {
    shared.packsDir = [...extraDirs];
  }

  // --- disabledRules: whole disabled packs + any rule set to "off". ---
  if (packs.disabled !== undefined && !Array.isArray(packs.disabled)) {
    throw new ConfigError('packs.disabled must be an array of pack ids.');
  }
  const disabled = new Set(Array.isArray(packs.disabled) ? packs.disabled : []);

  // --- rules.<id> -> severityOverrides / suppressions / disabledRules. ---
  const severityOverrides = {};
  const suppressions = [];
  const rules = config.rules || {};
  if (!isPlainObject(rules)) {
    throw new ConfigError('rules must be an object mapping rule ids to a severity or a rule object.');
  }

  for (const ruleId of Object.keys(rules)) {
    const entry = rules[ruleId];

    if (typeof entry === 'string') {
      // "off" -> disable; any other string -> severity override.
      const sev = normalizeSeverity(entry, ruleId);
      if (sev === OFF) {
        disabled.add(ruleId);
      } else {
        severityOverrides[ruleId] = sev;
      }
      continue;
    }

    if (isPlainObject(entry)) {
      if (entry.severity !== undefined) {
        const sev = normalizeSeverity(entry.severity, ruleId);
        if (sev === OFF) {
          disabled.add(ruleId);
        } else {
          severityOverrides[ruleId] = sev;
        }
      }
      if (entry.exclude !== undefined) {
        if (!Array.isArray(entry.exclude)) {
          throw new ConfigError(`rules.${ruleId}.exclude must be an array of path substrings or globs.`);
        }
        for (const path of entry.exclude) {
          if (typeof path !== 'string') {
            throw new ConfigError(`rules.${ruleId}.exclude entries must be strings.`);
          }
          // A pattern with glob metacharacters (`*`, `?`, `{}`) matches the full path via the engine's
          // glob filter; a plain fragment stays a substring match. `[`/`]` are deliberately NOT treated
          // as glob chars so raw Next.js dynamic-segment paths (e.g. `app/[locale]/`) work as substrings.
          if (isGlobPattern(path)) {
            suppressions.push({ rule: ruleId, glob: path });
          } else {
            suppressions.push({ rule: ruleId, path });
          }
        }
      }
      continue;
    }

    throw new ConfigError(
      `rules.${ruleId} must be a severity string (e.g. "warn"/"off") or an object ` +
        `({ "severity": ..., "exclude": [...] }).`
    );
  }

  if (disabled.size > 0) {
    shared.disabledRules = [...disabled];
  }
  if (Object.keys(severityOverrides).length > 0) {
    shared.severityOverrides = severityOverrides;
  }
  if (suppressions.length > 0) {
    shared.suppressions = suppressions;
  }

  // --- pass-through knobs. ---
  if (config.git !== undefined) {
    shared.git = config.git;
  }
  if (config.cacheDir !== undefined) {
    shared.cacheDir = config.cacheDir;
  }
  if (config.sizeCap !== undefined) {
    shared.sizeCap = config.sizeCap;
  }

  return shared;
}

// Known config keys, by scope. Used ONLY to warn on drift — never to reject (the engine deliberately
// ignores unknown fields; see `packages/napi/src/api.rs`). An unknown key almost always means a typo or a
// config written for a different zzop version, and silently ignoring it defeats zzop's "narrowed scope
// self-reports in warnings, never silently" contract — so we surface it as a warning, not an error.
const KNOWN_KEYS = {
  top: ['roots', 'trees', 'packs', 'rules', 'git', 'cacheDir', 'sizeCap', 'format', 'failOn', 'report'],
  packs: ['extraDirs', 'disabled'],
  git: ['since', 'recentDays'],
  report: ['dir', 'formats'],
  tree: ['root', 'sourceId'],
  ruleObject: ['severity', 'exclude'],
};

/**
 * Collect warnings for config keys the CLI does not recognize (typos, or a config written for a different
 * zzop version). Never throws and never rejects — unknown keys stay ignored, exactly as the engine treats
 * them; this only makes the drift visible. Returns an array of human-readable warning strings (possibly
 * empty). Covers the top level plus the fixed-shape nested objects (`packs`/`git`/`report`), each tree, and
 * each rule object; `rules`' own keys are rule ids (open set) so they are not checked.
 *
 * @param {object} config  parsed config object
 * @returns {string[]}
 */
function collectConfigWarnings(config) {
  const warnings = [];
  if (!isPlainObject(config)) return warnings;

  const check = (obj, known, scope) => {
    if (!isPlainObject(obj)) return;
    for (const key of Object.keys(obj)) {
      if (!known.includes(key)) {
        const where = scope ? `under "${scope.replace(/\.$/, '')}"` : 'at the top level';
        warnings.push(
          `unknown config key "${scope}${key}" (ignored) — a typo, or a key from a different zzop ` +
            `version. Known keys ${where}: ${known.join(', ')}.`
        );
      }
    }
  };

  check(config, KNOWN_KEYS.top, '');
  check(config.packs, KNOWN_KEYS.packs, 'packs.');
  check(config.git, KNOWN_KEYS.git, 'git.');
  check(config.report, KNOWN_KEYS.report, 'report.');
  if (Array.isArray(config.trees)) {
    config.trees.forEach((tree, i) => check(tree, KNOWN_KEYS.tree, `trees[${i}].`));
  }
  if (isPlainObject(config.rules)) {
    for (const [ruleId, entry] of Object.entries(config.rules)) {
      if (isPlainObject(entry)) check(entry, KNOWN_KEYS.ruleObject, `rules.${ruleId}.`);
    }
  }
  return warnings;
}

/**
 * Map a validated config object to a native request: `{ method, request }` where `method` is
 * `"analyze"` (single tree) or `"analyzeTrees"` (multi-tree) and `request` is the JSON-serializable
 * object to hand to `@zzop/native`'s corresponding function.
 *
 * @param {object} config
 * @returns {{ method: 'analyze'|'analyzeTrees', request: object }}
 */
function configToRequest(config) {
  if (!isPlainObject(config)) {
    throw new ConfigError('Config must be a JSON object.');
  }

  const shared = buildSharedOptions(config);

  // --- determine tree layout: explicit trees, or roots (default ["."]). ---
  let trees;
  if (config.trees !== undefined) {
    if (!Array.isArray(config.trees) || config.trees.length === 0) {
      throw new ConfigError('trees, when present, must be a non-empty array of { root, sourceId }.');
    }
    trees = config.trees.map((tree, i) => {
      if (!isPlainObject(tree) || typeof tree.root !== 'string' || tree.root === '') {
        throw new ConfigError(`trees[${i}] must be an object with a non-empty "root" string.`);
      }
      // Default sourceId to the tree's root so distinct trees get distinct sources: the cross-source
      // rules (shared-db-table, cross-tree route shadowing, ...) only fire with >= 2 distinct sourceIds.
      const sourceId = tree.sourceId !== undefined ? tree.sourceId : tree.root;
      return { root: tree.root, sourceId, ...shared };
    });
  } else {
    let roots = config.roots;
    if (roots === undefined) {
      roots = ['.'];
    }
    if (!Array.isArray(roots) || roots.length === 0) {
      throw new ConfigError('roots must be a non-empty array of directory paths.');
    }
    for (const r of roots) {
      if (typeof r !== 'string' || r === '') {
        throw new ConfigError('roots entries must be non-empty strings.');
      }
    }
    // Multiple roots => give each tree a distinct sourceId (its root) so cross-source analysis works;
    // a single root needs no source tag (it takes the single-source `analyze` path below).
    trees =
      roots.length > 1
        ? roots.map((root) => ({ root, sourceId: root, ...shared }))
        : roots.map((root) => ({ root, ...shared }));
  }

  if (trees.length === 1 && config.trees === undefined) {
    // Single root, no explicit trees -> single-tree analyze().
    return { method: 'analyze', request: trees[0] };
  }
  return { method: 'analyzeTrees', request: { trees } };
}

module.exports = {
  ConfigError,
  normalizeSeverity,
  severityRank,
  configToRequest,
  collectConfigWarnings,
  ENGINE_SEVERITY,
  OFF,
};
