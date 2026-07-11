'use strict';

// Assembles the full file set for `zzop init adapter --mode <a|b> --kind <consume|provide>`:
//   - lib/keys.mjs, lib/envelope.mjs   — bundled, byte-for-byte starter copies of examples/adapter-kit's
//                                        helpers (read verbatim from disk; NOT string constants, so they
//                                        stay valid, lintable/testable ESM in this very directory).
//   - main.mjs                        — mode/kind-specific skeleton (lib/main-template.js).
//   - README.md                       — mode/kind-specific pointer doc (lib/readme-template.js).
//
// Pure(ish): reads its own bundled template files from disk (deterministic, part of this package), but
// performs no writes — the caller (bin/zzop.js's runInitAdapter) owns the actual fs.writeFileSync calls.

const fs = require('node:fs');
const path = require('node:path');

const { buildMainTemplate } = require('./main-template');
const { buildReadmeTemplate } = require('./readme-template');

const ADAPTER_MODE_VALUES = ['a', 'b'];
const ADAPTER_KIND_VALUES = ['consume', 'provide'];

/**
 * @param {{ mode: 'a'|'b', kind: 'consume'|'provide' }} opts
 * @returns {{ name: string, content: string }[]}  relative paths (forward-slash) + file contents
 */
function buildAdapterScaffold({ mode, kind }) {
  if (!ADAPTER_MODE_VALUES.includes(mode)) {
    throw new Error(`buildAdapterScaffold: invalid mode "${mode}" (expected "a" or "b")`);
  }
  if (!ADAPTER_KIND_VALUES.includes(kind)) {
    throw new Error(`buildAdapterScaffold: invalid kind "${kind}" (expected "consume" or "provide")`);
  }

  const keysContent = fs.readFileSync(path.join(__dirname, 'keys.mjs'), 'utf8');
  const envelopeContent = fs.readFileSync(path.join(__dirname, 'envelope.mjs'), 'utf8');

  return [
    { name: 'main.mjs', content: buildMainTemplate({ mode, kind }) },
    { name: 'lib/keys.mjs', content: keysContent },
    { name: 'lib/envelope.mjs', content: envelopeContent },
    { name: 'README.md', content: buildReadmeTemplate({ mode, kind }) },
  ];
}

module.exports = { buildAdapterScaffold, ADAPTER_MODE_VALUES, ADAPTER_KIND_VALUES };
