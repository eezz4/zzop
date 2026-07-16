'use strict';

// Pure renderer for `zzop endpoint <pattern>` — turns the shared facade query core's JSON output
// (`@zzop/native`'s `queryIo`, the same core the zzop-mcp `check_endpoint` tool returns raw) into a
// human-readable report. No I/O here: bin/zzop.js owns the stdout write, same split as
// lib/debug-io.js. The verdict tokens are the query core's sealed wire vocabulary — see
// crates/facade/src/query.rs; this table only adds a one-line human meaning per token.

// Bucket order mirrors the query output's own (engine) order.
const BUCKETS = [
  'edges',
  'unconsumedProvides',
  'unprovidedConsumes',
  'unresolvedConsumes',
  'externalConsumes',
  'ambiguousConsumes',
];

const VERDICT_MEANING = {
  linked: 'consumed AND provided — the cross-layer join links them.',
  'provided-only': 'provided, but nothing analyzed consumes it (cross-layer dead code).',
  'consumed-unprovided': 'consumed, but nothing analyzed provides it (drift, or a tree is missing from the analysis).',
  external: 'third-party egress (absolute external URL) — not expected to be provided by this analysis.',
  'unresolved-only': 'matched only call sites whose key could not be statically determined.',
  ambiguous: 'consumed, with matching provides in 2+ source trees — not auto-linked.',
  mixed: 'matches span multiple classes — the per-bucket counts below disambiguate.',
  'not-found': 'no io key in this analysis matches the pattern.',
};

// `<key>` for a resolved entry, `<raw> [METHOD]` for an unresolved one — same fallback vocabulary
// as lib/debug-io.js's keyOrRaw (an unresolved consume is identified by what was written).
function keyOrRaw(entry) {
  if (entry && entry.key != null) return String(entry.key);
  const raw = entry && entry.raw != null ? String(entry.raw) : '(no key/raw)';
  return entry && entry.method ? `${raw} [${entry.method}]` : raw;
}

// One matched entry as a `file:line (source)` line, plus its key. Edges pair two sites, so the
// consumer (`from`) fills the file:line slot and the provider follows after `->`.
function entryLine(bucket, entry) {
  if (bucket === 'edges') {
    const from = (entry && entry.from) || {};
    const to = (entry && entry.to) || {};
    return `  ${from.file}:${from.line} (${from.source}) ${entry && entry.key} -> ${to.file}:${to.line} (${to.source})`;
  }
  const source = entry && entry.source != null ? entry.source : '(unknown source)';
  const base = `  ${entry && entry.file}:${entry && entry.line} (${source}) ${keyOrRaw(entry)}`;
  if (bucket === 'ambiguousConsumes') {
    const candidates = Array.isArray(entry && entry.candidates) ? entry.candidates : [];
    return `${base} candidates=${candidates.length}`;
  }
  return base;
}

/**
 * Render the human-readable endpoint report from a parsed `queryIo` result. Deterministic (the
 * query output's own engine order is preserved); no trailing newline (caller adds one).
 *
 * @param {object} result  parsed query JSON: `{pattern, verdict, counts, matches, truncated?,
 *   relatedFindings, truncatedFindings?, suggestions?, disclosure}`
 * @returns {string}
 */
function renderEndpointReport(result) {
  const r = result && typeof result === 'object' ? result : {};
  const counts = r.counts && typeof r.counts === 'object' ? r.counts : {};
  const matches = r.matches && typeof r.matches === 'object' ? r.matches : {};
  const truncated = r.truncated && typeof r.truncated === 'object' ? r.truncated : {};
  const verdict = String(r.verdict);

  const lines = [`endpoint "${r.pattern}": ${verdict}`];
  const meaning = VERDICT_MEANING[verdict];
  if (meaning) lines.push(`  ${meaning}`);

  for (const bucket of BUCKETS) {
    const count = Number(counts[bucket]) || 0;
    if (count === 0) continue;
    lines.push('', `${bucket} (${count} matched):`);
    for (const entry of Array.isArray(matches[bucket]) ? matches[bucket] : []) {
      lines.push(entryLine(bucket, entry));
    }
    if (truncated[bucket] != null) {
      lines.push(`  ... ${truncated[bucket]} more matched (list capped; the count above is complete — use --json for the raw output)`);
    }
  }

  const findings = Array.isArray(r.relatedFindings) ? r.relatedFindings : [];
  if (findings.length) {
    lines.push('', `related findings (${findings.length}${r.truncatedFindings != null ? ` shown, ${r.truncatedFindings} more` : ''}):`);
    for (const f of findings) {
      // Same null-guard vocabulary as lib/format.js's renderFindingLine: a finding without a
      // line (or file/ruleId) renders `?`, never the literal string "null".
      const file = f && f.file != null ? f.file : '?';
      const line = f && f.line != null ? f.line : '?';
      const ruleId = f && f.ruleId != null ? f.ruleId : '?';
      lines.push(`  ${file}:${line} [${ruleId}] ${f && f.message}`);
    }
  }

  if (verdict === 'not-found') {
    const suggestions = Array.isArray(r.suggestions) ? r.suggestions : [];
    if (suggestions.length) {
      lines.push('', 'did you mean:');
      for (const key of suggestions) lines.push(`  ${key}`);
    } else {
      lines.push('', 'no similar keys found in this analysis.');
    }
  }

  return lines.join('\n');
}

module.exports = { renderEndpointReport };
