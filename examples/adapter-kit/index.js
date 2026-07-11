// zzop adapter kit — the shared boilerplate every hand-rolled JS adapter in this repo's examples/
// tree (openapi-sdk-adapter, react-query-adapter, wrapper-adapter, svelte-adapter) re-derives: file
// walking, envelope assembly, and byte-exact HTTP key normalization matching zzop_core. Not
// published — a reference kit meant to be copied or imported from within this repo; see README.md
// for how the three pieces compose.

export {
  normalizeProvideKey,
  normalizeConsumeKey,
  resolveConsumeKey,
  isExternalUrl,
  baseRelativePath,
} from './lib/keys.js';

export {
  EnvelopeBuilder,
  validateEnvelope,
  NORMALIZED_AST_FORMAT,
  SUPPORTED_NORMALIZED_AST_VERSION,
} from './lib/envelope.js';

export { walk } from './lib/walk.js';
