// Envelope assembly for zzop's NormalizedEnvelope v1 contract (docs/NORMALIZED_AST.md;
// crates/core/src/normalized.rs). Field names/shapes mirror the Rust serde types field-for-field —
// see docs/adapters/envelope.schema.json for the full JSON Schema this builder's output satisfies.

export const NORMALIZED_AST_FORMAT = 'zzop-normalized-ast';
export const SUPPORTED_NORMALIZED_AST_VERSION = 1;

function emptyFile(filePath) {
  return {
    path: filePath,
    loc: 0,
    symbols: [],
    imports: {},
    re_exports: [],
    dynamic_imports: [],
    used_names: [],
    const_map_fragment: {},
    procedure_router_fragments: [],
    router_mount_fragments: [],
    io: { provides: [], consumes: [] },
    degraded: false,
    is_entry: false,
  };
}

/**
 * Incrementally assembles one NormalizedEnvelope. Typical Mode B (overlay) usage:
 *
 *   const b = new EnvelopeBuilder({ parser: 'my-adapter/1', source: 'web' });
 *   b.addFile('src/api.ts', { loc: 42 });
 *   b.addConsume('src/api.ts', { kind: 'http', key: normalizeConsumeKey('GET', '/users'), line: 10 });
 *   const envelope = b.toEnvelope(); // throws with every issue at once instead of emitting a broken envelope
 *
 * See README.md for the Mode A (full envelope) vs Mode B (overlay) usage sketch.
 */
export class EnvelopeBuilder {
  constructor({ parser, source } = {}) {
    if (typeof parser !== 'string' || parser.length === 0) {
      throw new Error('EnvelopeBuilder: parser id is required (e.g. "my-adapter/1")');
    }
    if (typeof source !== 'string' || source.length === 0) {
      throw new Error('EnvelopeBuilder: source id is required (the tree/source tag)');
    }
    this.parser = parser;
    this.source = source;
    this._files = new Map();
  }

  /**
   * Registers a file projection. `path` must be relative, forward-slash (matching the tree's own
   * convention — see docs/NORMALIZED_AST.md's FileProjection.path). `opts` may set any other
   * FileProjection field verbatim, snake_case, matching the wire contract (`loc`, `symbols`,
   * `imports`, `re_exports`, `dynamic_imports`, `used_names`, `const_map_fragment`,
   * `procedure_router_fragments`, `router_mount_fragments`, `degraded`) — `path` and `io` are owned by this
   * builder and ignored if passed here (use `addProvide`/`addConsume` for `io`, `markEntry` for
   * `is_entry`).
   */
  addFile(filePath, opts = {}) {
    if (typeof filePath !== 'string' || filePath.length === 0) {
      throw new Error('addFile: path must be a non-empty string');
    }
    if (this._files.has(filePath)) {
      throw new Error(`addFile: duplicate path '${filePath}' (each file may be added once)`);
    }
    const file = emptyFile(filePath);
    for (const [key, value] of Object.entries(opts)) {
      if (key === 'path' || key === 'io') continue; // owned by this builder, not overridable here
      file[key] = value;
    }
    this._files.set(filePath, file);
    return this;
  }

  _fileOrThrow(filePath, caller) {
    const file = this._files.get(filePath);
    if (!file) {
      throw new Error(`${caller}: unknown file '${filePath}' — call addFile('${filePath}') first`);
    }
    return file;
  }

  /** Adds one `IoProvide` to a file already registered via `addFile`. `key` must already be
   * normalized (see lib/keys.js's `normalizeProvideKey`). */
  addProvide(filePath, { kind, key, line, symbol } = {}) {
    const file = this._fileOrThrow(filePath, 'addProvide');
    if (typeof kind !== 'string' || kind.length === 0) {
      throw new Error('addProvide: kind is required (e.g. "http")');
    }
    if (typeof key !== 'string' || key.length === 0) {
      throw new Error('addProvide: key is required and must already be normalized (see lib/keys.js)');
    }
    if (!Number.isInteger(line) || line < 1) {
      throw new Error('addProvide: line must be a positive 1-based integer');
    }
    const provide = { kind, key, file: filePath, line };
    if (symbol !== undefined) provide.symbol = symbol;
    file.io.provides.push(provide);
    return this;
  }

  /** Adds one `IoConsume`. `key: null` (the default) is the documented "unresolved, never guessed"
   * state — pass `raw`/`method` alongside a null key so the engine's late cross-file resolution has
   * something to work with. */
  addConsume(filePath, { kind, key = null, line, raw, method } = {}) {
    const file = this._fileOrThrow(filePath, 'addConsume');
    if (typeof kind !== 'string' || kind.length === 0) {
      throw new Error('addConsume: kind is required (e.g. "http")');
    }
    if (key !== null && (typeof key !== 'string' || key.length === 0)) {
      throw new Error('addConsume: key must be a non-empty string or null (never an empty string)');
    }
    if (!Number.isInteger(line) || line < 1) {
      throw new Error('addConsume: line must be a positive 1-based integer');
    }
    const consume = { kind, key, file: filePath, line };
    if (raw !== undefined) consume.raw = raw;
    if (method !== undefined) consume.method = method;
    file.io.consumes.push(consume);
    return this;
  }

  /** Marks a file a framework/runtime entry (`FileProjection.is_entry`) — exempts it from
   * dead-candidates/unreachable in Mode B overlay composition. */
  markEntry(filePath) {
    this._fileOrThrow(filePath, 'markEntry').is_entry = true;
    return this;
  }

  /** Assembles the final envelope object and validates it (`validateEnvelope`) before returning —
   * throws with every issue listed at once, rather than handing the caller a broken envelope one
   * field at a time. */
  toEnvelope() {
    const envelope = {
      format: NORMALIZED_AST_FORMAT,
      version: SUPPORTED_NORMALIZED_AST_VERSION,
      parser: this.parser,
      source: this.source,
      files: [...this._files.values()],
    };
    const errors = validateEnvelope(envelope);
    if (errors.length > 0) {
      throw new Error(`toEnvelope: invalid envelope:\n  - ${errors.join('\n  - ')}`);
    }
    return envelope;
  }
}

/**
 * Structural validation mirroring `zzop_core::validate_envelope` (crates/core/src/normalized.rs):
 * an unknown `format`, a `version` greater than this kit's `SUPPORTED_NORMALIZED_AST_VERSION`, an
 * empty or duplicate file `path`, and a symbol whose body end is less than its body start (both the
 * canonical camelCase `bodyStart`/`bodyEnd` and the frozen-v1 snake_case alias are read, matching the
 * engine's serde aliases). Collects every issue instead of stopping at the first. Like the Rust validator, this does NOT check
 * fragment (`procedure_router_fragments`/`router_mount_fragments`) specifier resolvability — that is a
 * composition-time concern, silently skipped by the engine, never a validation-time rejection.
 */
export function validateEnvelope(envelope) {
  if (!envelope || typeof envelope !== 'object') {
    return ['envelope must be an object'];
  }

  const errors = [];
  if (envelope.format !== NORMALIZED_AST_FORMAT) {
    errors.push(`unknown format: '${envelope.format}' (expected '${NORMALIZED_AST_FORMAT}')`);
  }
  if (typeof envelope.version !== 'number' || envelope.version > SUPPORTED_NORMALIZED_AST_VERSION) {
    errors.push(
      `unsupported version: ${envelope.version} (this kit supports up to ${SUPPORTED_NORMALIZED_AST_VERSION})`
    );
  }

  const seenPaths = new Set();
  const files = Array.isArray(envelope.files) ? envelope.files : [];
  files.forEach((file, idx) => {
    if (!file || typeof file.path !== 'string' || file.path.length === 0) {
      errors.push(`files[${idx}]: empty path`);
    } else if (seenPaths.has(file.path)) {
      errors.push(`files[${idx}] ('${file.path}'): duplicate path`);
    } else {
      seenPaths.add(file.path);
    }
    const symbols = (file && file.symbols) || [];
    for (const sym of symbols) {
      // Canonical wire names are camelCase (bodyStart/bodyEnd); frozen-v1 snake_case is an accepted
      // input alias (envelope.schema.json) — the engine's serde alias reads both, so this must too.
      const start = sym.bodyStart != null ? sym.bodyStart : sym.body_start;
      const end = sym.bodyEnd != null ? sym.bodyEnd : sym.body_end;
      if (start != null && end != null && end < start) {
        errors.push(
          `files[${idx}] ('${file.path}') symbol '${sym.name}': body_end (${end}) < body_start (${start})`
        );
      }
    }
  });

  return errors;
}
