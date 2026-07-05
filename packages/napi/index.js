'use strict';

const fs = require('node:fs');
const path = require('node:path');

// Standard napi-rs `optionalDependencies` loader cascade — main `@zpz/native` loader + one
// `@zpz/native-<platform>` sub-package per prebuild target, npm/<platform>/ (see
// docs/modules/napi.md's "Packaging layout" section). For each platform/arch this process is
// running on:
//
//   1. Try `require("@zpz/native-<platform>")` — the prebuilt binary, installed as an optional dependency.
//   2. Fall back to `./zpz-napi.node` next to this file — a local dev build (see the error message below
//      for the build command). This is what `smoke.mjs` exercises before any sub-package is published.
//   3. If neither resolves, throw with the full list of supported platforms and the local-build command.
//
// musl (Alpine) and WASM targets are explicitly out of scope for now — see README.md's
// "Unsupported platform" section.

const PLATFORM_PACKAGES = {
  'win32-x64': '@zpz/native-win32-x64-msvc',
  'darwin-x64': '@zpz/native-darwin-x64',
  'darwin-arm64': '@zpz/native-darwin-arm64',
  'linux-x64': '@zpz/native-linux-x64-gnu',
  'linux-arm64': '@zpz/native-linux-arm64-gnu',
};

const platformKey = `${process.platform}-${process.arch}`;
const platformPackage = PLATFORM_PACKAGES[platformKey];

let native = null;
const attempts = [];

if (platformPackage) {
  try {
    native = require(platformPackage);
  } catch (err) {
    attempts.push(`  - ${platformPackage}: ${err && err.message}`);
  }
} else {
  attempts.push(`  - (no prebuilt package registered for platform "${platformKey}")`);
}

if (!native) {
  try {
    native = require('./zpz-napi.node');
  } catch (err) {
    attempts.push(`  - ./zpz-napi.node (local build): ${err && err.message}`);

    const supported = Object.keys(PLATFORM_PACKAGES)
      .map((key) => `${key} (${PLATFORM_PACKAGES[key]})`)
      .join(', ');

    const buildCommand =
      process.platform === 'win32'
        ? 'cargo +stable-x86_64-pc-windows-msvc build -p zpz-napi --release --features addon'
        : 'cargo build -p zpz-napi --release --features addon';

    throw new Error(
      `zpz-napi: failed to load the native addon for "${platformKey}".\n` +
        `Tried:\n${attempts.join('\n')}\n` +
        `Supported prebuilt platforms: ${supported}.\n` +
        `For unsupported platforms (or local development), build from source: \`${buildCommand}\`, ` +
        'then copy the produced binary (.dll/.dylib/.so) to packages/napi/zpz-napi.node. ' +
        'See packages/napi/README.md for details.'
    );
  }
}

// "Zero-config = full analysis": a bare `{ root }` request should exercise the full engine
// (native analyses + DSL rule packs + git-derived signals), not silently degrade to native-only
// with no self-report. This wrapper injects two defaults into the config JSON before it crosses
// into Rust — `packsDir` (pointing at the npm-shipped or repo-dev rule packs) and `git` (an empty
// object, so the engine applies its own `recentDays` default) — but only when the caller didn't
// already specify them. See docs/modules/napi.md's "Defaults" section for the full contract.
//
// `packsDir` uses MERGE semantics, not replace: the Rust side (`packages/napi/src/api.rs`'s
// `PacksDir`) accepts either a single directory string or an array of directories, loading and
// merging all of them (same pack id in two directories: the LATER directory wins whole-pack). So
// when the caller supplies their own `packsDir` (string or array), this wrapper PREPENDS the
// bundled default directory rather than replacing it — effective order `[bundled, ...user]` — so
// adding a custom pack never silently drops the shipped rule packs, and a user pack with the same
// id as a shipped one wins the collision (it loads later). An explicit `packsDir: null` is left
// untouched: it still disables all DSL packs (both bundled and any the caller would have added).

function directoryExists(candidate) {
  try {
    return fs.statSync(candidate).isDirectory();
  } catch {
    return false;
  }
}

function defaultPacksDir() {
  // Repo dev layout FIRST: in a source checkout, <repo root>/rules/dsl is the live truth, while
  // ./rules is a prepack-time copy that can silently go stale after a rule edit and shadow the
  // live packs. An npm install has no ../../rules/dsl, so published consumers still get the
  // shipped copy.
  const candidates = [
    path.join(__dirname, '..', '..', 'rules', 'dsl'), // repo dev layout: <repo root>/rules/dsl
    path.join(__dirname, 'rules'), // npm-shipped copy (populated by the `prepack` script)
  ];
  for (const candidate of candidates) {
    if (directoryExists(candidate)) {
      return candidate;
    }
  }
  return undefined; // leave packsDir unset — the engine's own capability warning covers this
}

function withDefaults(config, { includeGit }) {
  if (config.packsDir === undefined) {
    // No packsDir at all: bundled-only (unchanged behavior).
    const packsDir = defaultPacksDir();
    if (packsDir !== undefined) {
      config.packsDir = packsDir;
    }
  } else if (config.packsDir !== null) {
    // Caller supplied a packsDir (string or array): prepend the bundled default so it MERGES
    // rather than replaces — effective order [bundled, ...user], user packs win id collisions.
    const bundled = defaultPacksDir();
    if (bundled !== undefined) {
      const userDirs = Array.isArray(config.packsDir) ? config.packsDir : [config.packsDir];
      config.packsDir = [bundled, ...userDirs];
    }
  }
  // config.packsDir === null: explicit opt-out — leave as null, disabling all DSL packs.
  if (includeGit && config.git === undefined) {
    config.git = {};
  }
  return config;
}

function isPlainObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function withDefaultedConfigJson(configJson, { includeGit }) {
  let config;
  try {
    config = JSON.parse(configJson);
  } catch {
    return configJson; // let the native layer produce its normal parse error
  }
  if (!isPlainObject(config)) {
    return configJson;
  }
  return JSON.stringify(withDefaults(config, { includeGit }));
}

function analyze(configJson) {
  return native.analyze(withDefaultedConfigJson(configJson, { includeGit: true }));
}

function analyzeTrees(configJson) {
  let config;
  try {
    config = JSON.parse(configJson);
  } catch {
    return native.analyzeTrees(configJson);
  }
  if (!isPlainObject(config) || !Array.isArray(config.trees)) {
    return native.analyzeTrees(configJson);
  }
  config.trees = config.trees.map((tree) =>
    isPlainObject(tree) ? withDefaults(tree, { includeGit: true }) : tree
  );
  return native.analyzeTrees(JSON.stringify(config));
}

function analyzeEnvelope(envelopeJson, configJson) {
  return native.analyzeEnvelope(envelopeJson, withDefaultedConfigJson(configJson, { includeGit: false }));
}

module.exports = {
  analyze,
  analyzeTrees,
  analyzeEnvelope,
  version: native.version,
};
