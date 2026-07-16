#!/usr/bin/env node
// Mode B adapter: resolve oazapfts-generated OpenAPI SDK call sites into cross-layer IO facts,
// emitted as a NormalizedEnvelope overlay for zzop's `adapterOverlays` config (docs/NORMALIZED_AST.md).
//
// RECOGNIZED CALL FAMILY
//   oazapfts.fetchJson(url, opts?)
//   oazapfts.fetchText(url, opts?)
//   oazapfts.fetchBlob(url, opts?)
// - The receiver identifier must be EXACTLY `oazapfts` (a `client.fetchJson(...)` call on any other
//   receiver is not recognized — no bare-name allowlist).
// - `url` (first arg): a string or template literal. Template `${...}` interpolations collapse to the
//   `{}` route-param placeholder, EXCEPT a *trailing* interpolation (nothing but the closing backtick
//   after it) whose expression text starts with `QS.` (oazapfts's query-string helper) — that one is
//   DROPPED entirely (empty string, not `{}`), so the query part never pollutes the key. A `QS.`
//   interpolation that is not the trailing piece still collapses to `{}` like any other.
// - Method: `GET` by default; overridden by a literal `method: "..."` property found either directly in
//   the 2nd-arg options object, or inside an `oazapfts.<helper>({ ... })` wrapper call passed as the 2nd
//   arg — the wrapper is recognized by its `oazapfts.` receiver, not a helper allowlist. The captured
//   method value is upper-cased.
// - A call nested inside `oazapfts.ok(...)` is still detected — `ok(...)` itself is not special-cased;
//   the scan simply finds `oazapfts.fetchJson(...)` wherever it appears in the file text.
//
// USAGE
//   node adapter.mjs --root <tree-root> [--source <id>] > overlay.json
// Overlay envelope JSON to stdout; one-line summary to stderr. Feed stdout to a tree's
// `adapterOverlays` array on an `analyze`/`analyzeTrees` request.
//
// CONTRACT LIMITATIONS (see README.md):
// - Call detection is lexical over the whole file text (not a real AST), via a bracket/quote/template
//   -aware balanced scanner — method resolution must look INSIDE a (possibly wrapped) options object.
// - Request-body shape (`IoConsume.body`) is NOT captured: `adapter-kit`'s `EnvelopeBuilder.addConsume`
//   accepts only `kind`/`key`/`line`/`raw`/`method`, and this adapter must not modify adapter-kit. A
//   `body:` property next to `method:` is ignored; the consume is still emitted with its resolved key.
// - `client: 'oazapfts'` is attached by post-processing the built envelope (`addConsume` has no
//   `client` option). Unconditional and exact: every consume this adapter can emit is oazapfts-flavored.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { walk, EnvelopeBuilder, resolveConsumeKey } from '../adapter-kit/index.js';

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : def;
}

const treeRoot = arg('root');
const source = arg('source', 'web');
if (!treeRoot) {
  console.error('usage: node adapter.mjs --root <tree-root> [--source <id>] > overlay.json');
  process.exit(2);
}

// --- Balanced lexical scanning helpers ---
// A hand-rolled, bracket/quote/template-literal-aware scanner — NOT a full JS parser (no regex/ASI/
// tagged-template/optional-chaining edge cases modeled). It finds the matching close of a call's
// argument list and splits top-level arguments, including template literals with nested `${ ... }`
// expressions carrying their own brackets (the oazapfts `QS.` helper shape).

const OPEN = new Set(['(', '[', '{']);
const CLOSE_FOR = { '(': ')', '[': ']', '{': '}' };

/**
 * Given `text[openIdx]` is one of `(`/`[`/`{`, returns the index of its matching close bracket, or -1 if
 * the text ends before one is found (truncated/invalid input — caller must handle). Strings, template
 * literals (including nested `${...}` expressions, recursively), and `//`/`/* *​/` comments are skipped
 * without contributing to bracket depth.
 */
function findMatchingClose(text, openIdx) {
  const stack = [text[openIdx]];
  let i = openIdx + 1;
  while (i < text.length) {
    const ch = text[i];
    const top = stack[stack.length - 1];
    if (top === '`') {
      if (ch === '\\') {
        i += 2;
        continue;
      }
      if (ch === '`') {
        stack.pop();
        i++;
        continue;
      }
      if (ch === '$' && text[i + 1] === '{') {
        stack.push('{');
        i += 2;
        continue;
      }
      i++;
      continue;
    }
    if (ch === "'" || ch === '"') {
      const q = ch;
      i++;
      while (i < text.length && text[i] !== q) {
        if (text[i] === '\\') i++;
        i++;
      }
      i++;
      continue;
    }
    if (ch === '`') {
      stack.push('`');
      i++;
      continue;
    }
    if (ch === '/' && text[i + 1] === '/') {
      while (i < text.length && text[i] !== '\n') i++;
      continue;
    }
    if (ch === '/' && text[i + 1] === '*') {
      i += 2;
      while (i < text.length && !(text[i] === '*' && text[i + 1] === '/')) i++;
      i += 2;
      continue;
    }
    if (OPEN.has(ch)) {
      stack.push(ch);
      i++;
      continue;
    }
    if (ch === ')' || ch === ']' || ch === '}') {
      if (CLOSE_FOR[top] === ch) {
        stack.pop();
        if (stack.length === 0) return i;
      }
      i++;
      continue;
    }
    i++;
  }
  return -1;
}

/** Splits a call's argument-list text (the content between its outer parens) into top-level argument
 * strings, respecting nested brackets/strings/templates (commas inside any of those don't split). */
function splitTopLevelArgs(text) {
  const args = [];
  let cur = '';
  let i = 0;
  const stack = [];
  while (i < text.length) {
    const ch = text[i];
    const top = stack[stack.length - 1];
    if (top === '`') {
      if (ch === '\\') {
        cur += ch + (text[i + 1] || '');
        i += 2;
        continue;
      }
      if (ch === '`') {
        stack.pop();
        cur += ch;
        i++;
        continue;
      }
      if (ch === '$' && text[i + 1] === '{') {
        stack.push('{');
        cur += '${';
        i += 2;
        continue;
      }
      cur += ch;
      i++;
      continue;
    }
    if (ch === "'" || ch === '"') {
      const q = ch;
      cur += ch;
      i++;
      while (i < text.length && text[i] !== q) {
        cur += text[i];
        if (text[i] === '\\') {
          cur += text[i + 1] || '';
          i++;
        }
        i++;
      }
      cur += text[i] || '';
      i++;
      continue;
    }
    if (ch === '`') {
      stack.push('`');
      cur += ch;
      i++;
      continue;
    }
    if (OPEN.has(ch)) {
      stack.push(ch);
      cur += ch;
      i++;
      continue;
    }
    if (ch === ')' || ch === ']' || ch === '}') {
      if (stack.length > 0) stack.pop();
      cur += ch;
      i++;
      continue;
    }
    if (ch === ',' && stack.length === 0) {
      args.push(cur);
      cur = '';
      i++;
      continue;
    }
    cur += ch;
    i++;
  }
  if (cur.trim().length > 0 || args.length > 0) args.push(cur);
  return args.map((a) => a.trim());
}

/** Splits a template literal's inner text (content between the outer backticks) into an ordered list of
 * `{ type: 'text', value }` / `{ type: 'expr', value }` pieces — `value` for an `expr` piece is the raw
 * expression text of one `${...}` interpolation, with its own nested brackets/strings/templates already
 * balanced (never truncated at the first inner `}`). */
function parseTemplatePieces(inner) {
  const pieces = [];
  let curText = '';
  let i = 0;
  while (i < inner.length) {
    if (inner[i] === '\\') {
      curText += inner[i] + (inner[i + 1] || '');
      i += 2;
      continue;
    }
    if (inner[i] === '$' && inner[i + 1] === '{') {
      pieces.push({ type: 'text', value: curText });
      curText = '';
      const start = i + 2;
      let j = start;
      const stack = ['{'];
      while (j < inner.length && stack.length > 0) {
        const ch = inner[j];
        const top = stack[stack.length - 1];
        if (top === '`') {
          if (ch === '\\') {
            j += 2;
            continue;
          }
          if (ch === '`') {
            stack.pop();
            j++;
            continue;
          }
          if (ch === '$' && inner[j + 1] === '{') {
            stack.push('{');
            j += 2;
            continue;
          }
          j++;
          continue;
        }
        if (ch === "'" || ch === '"') {
          const q = ch;
          j++;
          while (j < inner.length && inner[j] !== q) {
            if (inner[j] === '\\') j++;
            j++;
          }
          j++;
          continue;
        }
        if (ch === '`') {
          stack.push('`');
          j++;
          continue;
        }
        if (OPEN.has(ch)) {
          stack.push(ch);
          j++;
          continue;
        }
        if (ch === ')' || ch === ']' || ch === '}') {
          if (CLOSE_FOR[top] === ch) stack.pop();
          j++;
          continue;
        }
        j++;
      }
      pieces.push({ type: 'expr', value: inner.slice(start, j - 1).trim() });
      i = j;
      continue;
    }
    curText += inner[i];
    i++;
  }
  pieces.push({ type: 'text', value: curText });
  return pieces;
}

/** Collapses a template literal's inner text to a URL path, applying the QS-trailing-drop rule: the
 * LAST `${...}` interpolation, when nothing but the closing backtick follows it AND its expression text
 * starts with `QS.`, is dropped entirely (contributes ""); every other interpolation collapses to `{}`. */
function collapseOazapftsTemplate(inner) {
  const pieces = parseTemplatePieces(inner);
  let lastExprIdx = -1;
  for (let k = 0; k < pieces.length; k++) {
    if (pieces[k].type === 'expr') lastExprIdx = k;
  }
  const trailingQs =
    lastExprIdx !== -1 &&
    pieces[pieces.length - 1].type === 'text' &&
    pieces[pieces.length - 1].value === '' &&
    pieces[lastExprIdx].value.startsWith('QS.');
  let out = '';
  for (let k = 0; k < pieces.length; k++) {
    const p = pieces[k];
    if (p.type === 'text') {
      out += p.value;
      continue;
    }
    if (k === lastExprIdx && trailingQs) continue; // dropped, not `{}`
    out += '{}';
  }
  return out;
}

// A whole argument that is exactly one string OR template literal: `'...'` / `"..."` / `` `...` ``,
// nothing else around it. Captures the quote char and the raw inner text (escapes are kept raw,
// never resolved).
const LITERAL_RE = /^(['"`])([\s\S]*)\1$/;

/** Resolves a call's first-argument text to a raw URL path, or `null` when it is not a bare string/
 * template literal — only emit from visible literals, never guess. */
function resolveUrlArg(argText) {
  const m = LITERAL_RE.exec(argText.trim());
  if (!m) return null;
  const [, quote, inner] = m;
  return quote === '`' ? collapseOazapftsTemplate(inner) : inner;
}

// Any `oazapfts.<helper>({ ... })` wrapper call — receiver matched, not a helper allowlist.
const OAZAPFTS_WRAPPER_RE = /^oazapfts\s*\.\s*[A-Za-z_$][\w$]*\s*\(/;

/** Reads a literal `method: "..."` property from a 2nd-arg options object — either directly, or from
 * the object literal passed to an `oazapfts.<helper>(...)` wrapper. Returns the raw method string as
 * written (the caller upper-cases it), or `null` when no literal `method:` property is visible (a
 * `...spread` never resolves one — never guessed). */
function methodFromOptionsArg(argText) {
  const objText = objectLiteralFromOptionsArg(argText);
  if (objText === null) return null;
  return readStringProp(objText, 'method');
}

/** Extracts the object-literal text `{ ... }` an options argument resolves to: the argument itself, if
 * it already IS an object literal, or the sole argument of an `oazapfts.<helper>(...)` wrapper call
 * around one. Returns `null` when neither shape applies (e.g. a bare identifier like `opts`). */
function objectLiteralFromOptionsArg(argText) {
  const trimmed = argText.trim();
  if (trimmed.startsWith('{') && trimmed.endsWith('}')) return trimmed;
  const wrapperMatch = OAZAPFTS_WRAPPER_RE.exec(trimmed);
  if (!wrapperMatch) return null;
  const openIdx = wrapperMatch[0].length - 1; // index of the wrapper call's own `(`
  const closeIdx = findMatchingClose(trimmed, openIdx);
  if (closeIdx === -1) return null;
  const inner = trimmed.slice(openIdx + 1, closeIdx).trim();
  return inner.startsWith('{') && inner.endsWith('}') ? inner : null;
}

/** Reads a literal `<name>: "..."` string property from object-literal text `{ ... }`, at the top
 * level only (a `...spread` or a same-named nested property one level deeper never matches). */
function readStringProp(objText, name) {
  const inner = objText.slice(1, -1);
  for (const rawProp of splitTopLevelArgs(inner)) {
    const prop = rawProp.trim();
    const re = new RegExp(`^${name}\\s*:\\s*(['"])((?:\\\\.|(?!\\1).)*)\\1$`);
    const m = re.exec(prop);
    if (m) return m[2];
  }
  return null;
}

// `oazapfts.fetchJson` / `oazapfts.fetchText` / `oazapfts.fetchBlob` — receiver matched EXACTLY (the
// lookbehind rejects `client.fetchJson(` / `myoazapfts.fetchJson(`).
const CALL_RE = /(?<![.\w$])oazapfts\.(fetchJson|fetchText|fetchBlob)\b/g;

/** Skips an optional generic type-argument list (`<{ status: 200 }>`) between a method name and its
 * call's opening paren. Depth-counts `<`/`>` only — sufficient for the type-literal shapes oazapfts'
 * generated clients emit; no string/template content inside the generic is modeled. */
function skipGenericsAndWhitespace(text, idx) {
  let i = idx;
  while (i < text.length && /\s/.test(text[i])) i++;
  if (text[i] !== '<') return i;
  let depth = 0;
  while (i < text.length) {
    if (text[i] === '<') depth++;
    else if (text[i] === '>') {
      depth--;
      if (depth === 0) {
        i++;
        break;
      }
    }
    i++;
  }
  while (i < text.length && /\s/.test(text[i])) i++;
  return i;
}

function lineOf(text, index) {
  let line = 1;
  for (let i = 0; i < index; i++) {
    if (text[i] === '\n') line++;
  }
  return line;
}

/** Finds every recognized oazapfts call site in `text`, returning `{ line, key }` for each one whose URL
 * resolved to a key (unresolvable URLs are counted in `skipped`, never emitted — never guess). */
function scanFile(text, counters) {
  const consumes = [];
  CALL_RE.lastIndex = 0;
  let m;
  while ((m = CALL_RE.exec(text))) {
    counters.calls++;
    const afterName = m.index + m[0].length;
    const parenIdx = skipGenericsAndWhitespace(text, afterName);
    if (text[parenIdx] !== '(') continue; // not actually a call (e.g. a bare reference) — skip
    const closeIdx = findMatchingClose(text, parenIdx);
    if (closeIdx === -1) continue; // truncated/unbalanced — skip, never guess
    const args = splitTopLevelArgs(text.slice(parenIdx + 1, closeIdx));
    if (args.length === 0 || args[0].length === 0) continue;
    const rawUrl = resolveUrlArg(args[0]);
    if (rawUrl === null) {
      counters.skipped++;
      continue;
    }
    const method = (args.length > 1 ? methodFromOptionsArg(args[1]) : null) || 'GET';
    const key = resolveConsumeKey(method.toUpperCase(), rawUrl);
    if (key === null) {
      counters.skipped++;
      continue;
    }
    consumes.push({ key, line: lineOf(text, m.index) });
  }
  return consumes;
}

const builder = new EnvelopeBuilder({ parser: 'oazapfts-adapter/1', source });
const counters = { calls: 0, skipped: 0 };
let fileCount = 0;
for (const rel of walk(treeRoot, {
  include: ['ts', 'tsx', 'js', 'jsx', 'mjs'],
  excludeFile: /\.(spec|test)\.[tj]sx?$/,
})) {
  const text = readFileSync(path.join(treeRoot, rel), 'utf8');
  if (!text.includes('oazapfts')) continue;
  const consumes = scanFile(text, counters);
  if (consumes.length === 0) continue;
  builder.addFile(rel, { loc: text.split('\n').length });
  for (const c of consumes) builder.addConsume(rel, { kind: 'http', key: c.key, line: c.line });
  fileCount++;
}

const envelope = builder.toEnvelope();
// `client: 'oazapfts'` on every emitted consume — `EnvelopeBuilder.addConsume` has no `client` option
// (see CONTRACT LIMITATIONS above), so it is attached by post-processing the built envelope. Safe
// unconditionally: every consume this adapter can emit came from an `oazapfts.*` call site.
for (const file of envelope.files) {
  for (const consume of file.io.consumes) consume.client = 'oazapfts';
}

process.stderr.write(
  `[oazapfts-adapter] ${fileCount} file(s), ${counters.calls} call site(s) matched, ` +
    `${counters.calls - counters.skipped} keyed, ${counters.skipped} skipped\n`
);
process.stdout.write(JSON.stringify(envelope));
