// zzop adapter kit — shared file walking, envelope assembly, and byte-exact HTTP key normalization,
// extracted from the example adapters' formerly hand-rolled copies (the JS adapters now import it).
// Not published to npm; copy or import from within this repo. See README.md.

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
