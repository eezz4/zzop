'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { stripJsonComments } = require('./jsonc');
const { ConfigError } = require('./mapper');

const DEFAULT_CONFIG_FILENAME = 'zzop.config.jsonc';

/**
 * Load and parse a zzop.config.jsonc from disk. Throws ConfigError with an actionable message on a
 * missing file or a JSON syntax error.
 *
 * @param {string} configPath  absolute or cwd-relative path to the config file
 * @returns {object} parsed config object
 */
function loadConfig(configPath) {
  const resolved = path.resolve(configPath);

  let raw;
  try {
    raw = fs.readFileSync(resolved, 'utf8');
  } catch (err) {
    if (err && err.code === 'ENOENT') {
      throw new ConfigError(
        `No config file at ${resolved}.\n` +
          `Create one with \`zzop init\`, or point at an existing file with --config <path>.`
      );
    }
    throw new ConfigError(`Could not read config at ${resolved}: ${err && err.message}`);
  }

  let parsed;
  try {
    parsed = JSON.parse(stripJsonComments(raw));
  } catch (err) {
    throw new ConfigError(`Invalid JSONC in ${resolved}: ${err && err.message}`);
  }

  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new ConfigError(`Config in ${resolved} must be a JSON object.`);
  }

  return parsed;
}

module.exports = { loadConfig, DEFAULT_CONFIG_FILENAME };
