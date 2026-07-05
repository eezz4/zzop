'use strict';

// Public library surface for the `zzop` package — the pure, native-free pieces, re-exported so they can
// be imported and unit-tested (and reused by embedders) without going through the CLI entry point.

const { configToRequest, normalizeSeverity, severityRank, ConfigError } = require('./mapper');
const { loadConfig, DEFAULT_CONFIG_FILENAME } = require('./config');
const { stripJsonComments } = require('./jsonc');
const {
  collectFindings,
  formatPretty,
  formatJson,
  computeExitCode,
} = require('./format');
const { CONFIG_TEMPLATE } = require('./init');

module.exports = {
  configToRequest,
  normalizeSeverity,
  severityRank,
  ConfigError,
  loadConfig,
  DEFAULT_CONFIG_FILENAME,
  stripJsonComments,
  collectFindings,
  formatPretty,
  formatJson,
  computeExitCode,
  CONFIG_TEMPLATE,
};
