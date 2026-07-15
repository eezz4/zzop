'use strict';

const fs = require('node:fs');
const path = require('node:path');

// PURE config -> native-request mapper. No native calls, no other process access — everything here is a
// deterministic function of its `config` argument (plus, for `overlays`, the filesystem — see the
// "Adapter overlays" section below for the one deliberate exception) so it can be unit-tested without the
// native addon.
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
// CONFIRMED against the engine: `crates/core/src/finding.rs` declares
//   #[serde(rename_all = "lowercase")] enum Severity { Critical, Warning, Info }
// so the engine's JSON `Severity` strings are exactly "critical" / "warning" / "info". The napi
// `AnalyzeRequest.severityOverrides` (crates/facade/src/lib.rs) reuses that same enum. If the engine's
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

// ---------------------------------------------------------------------------------------------------
// Adapter overlays — closes the loop for the napi `adapterOverlays` request field
// (`crates/facade/src/lib.rs`'s `AnalyzeRequest::adapter_overlays`, itself per-tree). Config key
// `overlays: ["path/to/envelope.json", ...]` names Mode-B overlay envelope FILES (partial
// `NormalizedEnvelope` JSON); the CLI reads and parses each one and inlines the parsed object into the
// request's `adapterOverlays` array — the napi layer has no notion of "a path to an overlay file", only
// inline objects.
//
// This is the one place this module is not I/O-free: reading these files is unavoidable disk access.
// Every other function here stays a pure function of its arguments; only the two helpers below touch `fs`.
//
// Path resolution base: deliberately NOT the config file's directory. No other path-ish config key in
// this file is ever resolved relative to the config file's directory — `configToRequest` is called with
// just the parsed `config` object and never receives the config file's own path (see `config.js`'s
// `loadConfig`, whose caller keeps the resolved path to itself); `cacheDir`/`packsDir`/`root(s)` all pass
// through as literal strings for the native engine to resolve against ITS OWN process cwd. So an overlay
// path is instead resolved relative to the TREE'S OWN root — the one directory this mapper already has in
// hand for each tree, and the most sensible anchor for "a file describing (or living near) this source
// tree".
//
// Read/parse failures are NEVER fatal: this mirrors the "narrowed scope self-reports in warnings, never
// silently" contract `collectConfigWarnings` already implements for unknown config keys (see its own doc
// comment below) — a missing/invalid overlay file drops just that one overlay and is reported through the
// exact same warnings channel, never a ConfigError/process exit.
// ---------------------------------------------------------------------------------------------------

/**
 * Validate a config `overlays` array's SHAPE (must be an array of non-empty strings). Throws ConfigError
 * on a shape violation — unlike a per-file read/parse failure (handled by `resolveOverlaysForRoot`, which
 * never throws), a wrong-typed `overlays` value is a config-authoring mistake like any other mistyped
 * array field in this file (see `packs.extraDirs`/`exclude` above).
 *
 * @param {*} value
 * @param {string} label  e.g. "overlays" or "trees[0].overlays", for the error message
 */
function validateOverlaysArray(value, label) {
  if (!Array.isArray(value)) {
    throw new ConfigError(`${label} must be an array of file paths.`);
  }
  for (const entry of value) {
    if (typeof entry !== 'string' || entry === '') {
      throw new ConfigError(`${label} entries must be non-empty strings (paths to overlay JSON files).`);
    }
  }
}

/**
 * Read and parse every overlay path (shared/top-level paths plus this tree's own), resolved relative to
 * `root`, into `NormalizedEnvelope`-shaped objects ready to inline as `adapterOverlays`. Never throws: a
 * file that cannot be read or does not parse as JSON is dropped, with a human-readable warning describing
 * which path and why. The engine independently re-validates each surviving envelope's SHAPE and
 * soft-skips an invalid one with its own warning (see `crates/facade/src/lib.rs`'s
 * `adapter_overlays` doc), so this function only needs to guarantee well-formed JSON, not a well-formed
 * envelope.
 *
 * @param {string} root  the tree's root directory (resolution base for relative overlay paths)
 * @param {string[]} [sharedPaths]  top-level `overlays` config paths (apply to every tree)
 * @param {string[]} [treePaths]  this tree's own `trees[i].overlays` config paths
 * @returns {{ overlays: object[], warnings: string[] }}
 */
function resolveOverlaysForRoot(root, sharedPaths, treePaths) {
  const paths = [...(sharedPaths || []), ...(treePaths || [])];
  const overlays = [];
  const warnings = [];
  for (const overlayPath of paths) {
    const resolved = path.resolve(root, overlayPath);
    let raw;
    try {
      raw = fs.readFileSync(resolved, 'utf8');
    } catch (err) {
      warnings.push(
        `overlay "${overlayPath}" for tree "${root}" (resolved to ${resolved}) could not be read: ` +
          `${err && err.message}. This overlay is skipped.`
      );
      continue;
    }
    try {
      overlays.push(JSON.parse(raw));
    } catch (err) {
      warnings.push(
        `overlay "${overlayPath}" for tree "${root}" (resolved to ${resolved}) is not valid JSON: ` +
          `${err && err.message}. This overlay is skipped.`
      );
    }
  }
  return { overlays, warnings };
}

/**
 * Best-effort overlay-loading warnings for `collectConfigWarnings` — attempts to resolve/read/parse every
 * overlay this config WOULD load (mirroring `configToRequest`'s tree derivation) purely to surface
 * read/parse failures through the warnings channel. Never throws: a config whose `trees`/`roots` shape is
 * itself invalid is silently skipped here (`configToRequest` raises the real ConfigError for that; this
 * function's only job is overlay diagnostics).
 *
 * @param {object} config
 * @returns {string[]}
 */
function collectOverlayWarnings(config) {
  const warnings = [];
  if (!isPlainObject(config)) return warnings;

  const sharedPaths = Array.isArray(config.overlays) ? config.overlays : [];

  if (Array.isArray(config.trees)) {
    for (const tree of config.trees) {
      if (!isPlainObject(tree) || typeof tree.root !== 'string' || tree.root === '') continue;
      const treePaths = Array.isArray(tree.overlays) ? tree.overlays : [];
      warnings.push(...resolveOverlaysForRoot(tree.root, sharedPaths, treePaths).warnings);
    }
  } else {
    const roots = Array.isArray(config.roots) ? config.roots : config.roots === undefined ? ['.'] : [];
    for (const root of roots) {
      if (typeof root !== 'string' || root === '') continue;
      warnings.push(...resolveOverlaysForRoot(root, sharedPaths, undefined).warnings);
    }
  }
  return warnings;
}

// ---------------------------------------------------------------------------------------------------
// Connection topology — closes the loop for the napi `mountedAt`/`mounts`/`hosts` per-tree request
// fields (`crates/facade/src/lib.rs`'s `AnalyzeRequest`). Config keys `trees[i].mountedAt` (a single
// whole-tree gateway prefix), `trees[i].mounts` (an array of `{dir, at}` deployment-topology mounts), and
// `trees[i].hosts` (hosts this tree owns, for cross-layer absolute-URL re-keying) are ONLY accepted on
// explicit `trees[]` entries — the `roots` shorthand carries no per-tree shape to hang them off, so those
// keys never apply there (see `configToRequest`'s `roots` branch: it never reads these keys at all).
//
// Unlike overlays, this module IS the authoritative fail-fast gate for shape here: the engine's own
// `apply_config_mounts` (see `crates/engine/src/analyze/compose.rs`) only defensively warns and skips a
// malformed mount as a last-resort backstop, so a config author should see a ConfigError immediately
// rather than a warning buried in a run's output.
// ---------------------------------------------------------------------------------------------------

/**
 * Validate one mount "at" value — `trees[i].mountedAt` or a `trees[i].mounts[].at` entry: must be a
 * string, non-empty after trimming leading/trailing "/", starting with "/", and containing no scheme
 * separator ("://"), path-param placeholder ("{}"), or whitespace.
 *
 * @param {*} value
 * @param {string} label  e.g. "trees[0].mountedAt" or "trees[0].mounts[1].at"
 */
function validateMountAt(value, label) {
  if (typeof value !== 'string') {
    throw new ConfigError(`${label} must be a string.`);
  }
  const trimmedSlashes = value.replace(/^\/+/, '').replace(/\/+$/, '');
  if (trimmedSlashes === '') {
    throw new ConfigError(`${label} must be a non-empty path after trimming slashes.`);
  }
  if (!value.startsWith('/')) {
    throw new ConfigError(`${label} must start with "/".`);
  }
  if (value.includes('://')) {
    throw new ConfigError(`${label} must not contain a scheme ("://") — it is a path prefix, not a full URL.`);
  }
  if (value.includes('{}')) {
    throw new ConfigError(`${label} must not contain a path-param placeholder ("{}").`);
  }
  if (/\s/.test(value)) {
    throw new ConfigError(`${label} must not contain whitespace.`);
  }
}

/**
 * Validate one `trees[i].mounts[].dir` value: must be a string, tree-relative (must not start with "/"),
 * using forward slashes only (must not contain a backslash).
 *
 * @param {*} value
 * @param {string} label  e.g. "trees[0].mounts[1].dir"
 */
function validateMountDir(value, label) {
  if (typeof value !== 'string') {
    throw new ConfigError(`${label} must be a string.`);
  }
  if (value.startsWith('/')) {
    throw new ConfigError(`${label} must be tree-relative and must not start with "/".`);
  }
  if (value.includes('\\')) {
    throw new ConfigError(`${label} must use forward slashes, not backslashes.`);
  }
}

/**
 * Validate a `trees[i].mounts` array's shape: an array of `{dir, at}` objects, each field checked by
 * `validateMountDir`/`validateMountAt`.
 *
 * @param {*} value
 * @param {string} label  e.g. "trees[0].mounts"
 */
function validateMountsArray(value, label) {
  if (!Array.isArray(value)) {
    throw new ConfigError(`${label} must be an array of { dir, at } objects.`);
  }
  value.forEach((entry, i) => {
    if (!isPlainObject(entry)) {
      throw new ConfigError(`${label}[${i}] must be an object with "dir" and "at" strings.`);
    }
    validateMountDir(entry.dir, `${label}[${i}].dir`);
    validateMountAt(entry.at, `${label}[${i}].at`);
  });
}

/**
 * Validate a `trees[i].hosts` array's shape: an array of non-empty bare-host strings — no path separator
 * ("/"), no scheme ("://"), no whitespace.
 *
 * @param {*} value
 * @param {string} label  e.g. "trees[0].hosts"
 */
function validateHostsArray(value, label) {
  if (!Array.isArray(value)) {
    throw new ConfigError(`${label} must be an array of host strings.`);
  }
  value.forEach((entry, i) => {
    if (typeof entry !== 'string' || entry === '') {
      throw new ConfigError(`${label}[${i}] must be a non-empty string.`);
    }
    // Checked BEFORE the bare-"/" check below: every "://" value also contains "/", so if the "/" check
    // ran first it would always win and the URL-specific message below would be unreachable dead code —
    // a user pasting a full URL like "https://api.foo.com" deserves the URL-specific message, not the
    // generic path one.
    if (entry.includes('://')) {
      throw new ConfigError(`${label}[${i}] must be a bare host, not a full URL ("://" is not allowed).`);
    }
    if (entry.includes('/')) {
      throw new ConfigError(`${label}[${i}] must be a bare host, not a path ("/" is not allowed).`);
    }
    if (/\s/.test(entry)) {
      throw new ConfigError(`${label}[${i}] must not contain whitespace.`);
    }
  });
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

  // --- top-level exclude -> globalExcludes (rule-agnostic finding-level filter, applied to EVERY rule). ---
  if (config.exclude !== undefined) {
    if (!Array.isArray(config.exclude)) {
      throw new ConfigError('exclude must be an array of path substrings or globs.');
    }
    const globalExcludes = [];
    for (const path of config.exclude) {
      if (typeof path !== 'string') {
        throw new ConfigError('exclude entries must be strings.');
      }
      // Same glob-vs-substring split as `rules.<id>.exclude` (see `isGlobPattern`'s doc above).
      if (isGlobPattern(path)) {
        globalExcludes.push({ glob: path });
      } else {
        globalExcludes.push({ path });
      }
    }
    if (globalExcludes.length > 0) {
      shared.globalExcludes = globalExcludes;
    }
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
// ignores unknown fields; see `crates/facade/src/lib.rs`). An unknown key almost always means a typo or a
// config written for a different zzop version, and silently ignoring it defeats zzop's "narrowed scope
// self-reports in warnings, never silently" contract — so we surface it as a warning, not an error.
//
// Sourced from `config-surface.json`'s `configKeys` — the single vocabulary file shared with
// `crates/engine/tests/rule_contracts.rs`'s reference-validation meta-test, so the CLI's own drift
// warnings and the engine's "does every message name a real knob" check can never disagree about what a
// valid config key is.
const KNOWN_KEYS = require('./config-surface.json').configKeys;

/**
 * Collect warnings for config keys the CLI does not recognize (typos, or a config written for a different
 * zzop version). Never throws and never rejects — unknown keys stay ignored, exactly as the engine treats
 * them; this only makes the drift visible. Returns an array of human-readable warning strings (possibly
 * empty). Covers the top level plus the fixed-shape nested objects (`packs`/`git`/`report`), each tree,
 * each rule object, and each `trees[i].mounts[]` entry; `rules`' own keys are rule ids (open set) so they
 * are not checked.
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
    config.trees.forEach((tree, i) => {
      check(tree, KNOWN_KEYS.tree, `trees[${i}].`);
      if (isPlainObject(tree) && Array.isArray(tree.mounts)) {
        tree.mounts.forEach((entry, j) => {
          if (isPlainObject(entry)) check(entry, KNOWN_KEYS.mount, `trees[${i}].mounts[${j}].`);
        });
      }
    });
  }
  if (isPlainObject(config.rules)) {
    for (const [ruleId, entry] of Object.entries(config.rules)) {
      if (isPlainObject(entry)) check(entry, KNOWN_KEYS.ruleObject, `rules.${ruleId}.`);
    }
  }
  warnings.push(...collectOverlayWarnings(config));
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

  // --- overlays: top-level `overlays` applies to every tree; `trees[i].overlays` adds to that tree only.
  // See the "Adapter overlays" section above for the resolution-base rationale (tree root, not config
  // file dir) and the never-fatal read/parse contract (failures surface via `collectConfigWarnings`, not
  // here — this function silently drops what it cannot load rather than duplicating the warning). ---
  if (config.overlays !== undefined) {
    validateOverlaysArray(config.overlays, 'overlays');
  }
  const sharedOverlayPaths = Array.isArray(config.overlays) ? config.overlays : [];

  // Attach any resolved overlays to a tree-request object, in place, only when non-empty — mirrors this
  // file's "omitted key falls through to the napi/engine default" convention (see `packsDir` above).
  const attachOverlays = (treeRequest, root, treePaths) => {
    const { overlays } = resolveOverlaysForRoot(root, sharedOverlayPaths, treePaths);
    if (overlays.length > 0) {
      treeRequest.adapterOverlays = overlays;
    }
    return treeRequest;
  };

  // --- determine tree layout: explicit trees, or roots (default ["."]). ---
  let trees;
  if (config.trees !== undefined) {
    // The `trees: "auto"` shorthand must be expanded to a concrete array BEFORE reaching this pure mapper
    // (the CLI calls `expandAutoTrees` in bin/zzop.js). If the string leaks through here — e.g. an
    // embedder calling `configToRequest` directly — fail with an actionable message rather than the
    // generic "must be a non-empty array" below.
    if (config.trees === 'auto') {
      throw new ConfigError(
        'trees: "auto" must be expanded before configToRequest — call expandAutoTrees(config, cwd) first ' +
          '(the zzop CLI does this automatically).'
      );
    }
    if (!Array.isArray(config.trees) || config.trees.length === 0) {
      throw new ConfigError('trees, when present, must be a non-empty array of { root, sourceId }.');
    }
    trees = config.trees.map((tree, i) => {
      if (!isPlainObject(tree) || typeof tree.root !== 'string' || tree.root === '') {
        throw new ConfigError(`trees[${i}] must be an object with a non-empty "root" string.`);
      }
      if (tree.overlays !== undefined) {
        validateOverlaysArray(tree.overlays, `trees[${i}].overlays`);
      }
      // Default sourceId to the tree's root so distinct trees get distinct sources: the cross-source
      // rules (shared-db-table, cross-tree route shadowing, ...) only fire with >= 2 distinct sourceIds.
      const sourceId = tree.sourceId !== undefined ? tree.sourceId : tree.root;
      const treeRequest = attachOverlays({ root: tree.root, sourceId, ...shared }, tree.root, tree.overlays);

      // --- connection topology: mountedAt / mounts / hosts. Explicit trees[] entries only — the `roots`
      // shorthand below never reads these keys at all (see this section's module doc above). ---
      if (tree.mountedAt !== undefined) {
        validateMountAt(tree.mountedAt, `trees[${i}].mountedAt`);
        treeRequest.mountedAt = tree.mountedAt;
      }
      if (tree.mounts !== undefined) {
        validateMountsArray(tree.mounts, `trees[${i}].mounts`);
        if (tree.mounts.length > 0) {
          treeRequest.mounts = tree.mounts;
        }
      }
      if (tree.hosts !== undefined) {
        validateHostsArray(tree.hosts, `trees[${i}].hosts`);
        if (tree.hosts.length > 0) {
          treeRequest.hosts = tree.hosts;
        }
      }

      return treeRequest;
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
        ? roots.map((root) => attachOverlays({ root, sourceId: root, ...shared }, root, undefined))
        : roots.map((root) => attachOverlays({ root, ...shared }, root, undefined));
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
