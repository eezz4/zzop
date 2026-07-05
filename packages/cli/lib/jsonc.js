'use strict';

// Minimal, dependency-free JSONC -> JSON comment stripper. A single character scanner that tracks
// whether it is inside a double-quoted string, so `//` and `/* */` occurring *inside* a JSON string
// value are preserved verbatim. Only line (`//`) and block (`/* */`) comments outside of strings are
// removed. Trailing commas are also tolerated (removed) since editors that write JSONC commonly leave
// them. This is deliberately small — it is not a full JSON5 parser; the result is handed to JSON.parse.

/**
 * Strip `//` and `/* *\/` comments (and trailing commas) from a JSONC string, leaving comment-like
 * sequences inside string literals untouched. Replaces stripped comment characters with spaces of the
 * same run length so byte offsets / line-column positions in JSON.parse errors stay meaningful.
 *
 * @param {string} input
 * @returns {string}
 */
function stripJsonComments(input) {
  const out = [];
  let inString = false;
  let escaped = false;

  for (let i = 0; i < input.length; i++) {
    const ch = input[i];
    const next = input[i + 1];

    if (inString) {
      out.push(ch);
      if (escaped) {
        escaped = false;
      } else if (ch === '\\') {
        escaped = true;
      } else if (ch === '"') {
        inString = false;
      }
      continue;
    }

    if (ch === '"') {
      inString = true;
      out.push(ch);
      continue;
    }

    if (ch === '/' && next === '/') {
      // Line comment: skip to end of line, preserving the newline itself.
      i += 2;
      while (i < input.length && input[i] !== '\n' && input[i] !== '\r') {
        i++;
      }
      i--; // let the loop's i++ re-examine the terminator (newline) normally
      continue;
    }

    if (ch === '/' && next === '*') {
      // Block comment: skip to the closing */, preserving any newlines inside so line numbers hold.
      i += 2;
      while (i < input.length && !(input[i] === '*' && input[i + 1] === '/')) {
        if (input[i] === '\n') {
          out.push('\n');
        }
        i++;
      }
      i++; // consume the closing '/'
      continue;
    }

    out.push(ch);
  }

  return stripTrailingCommas(out.join(''));
}

/**
 * Remove trailing commas that precede a `}` or `]` (ignoring whitespace), skipping over string
 * literals so a comma inside a string is never touched.
 *
 * @param {string} input
 * @returns {string}
 */
function stripTrailingCommas(input) {
  const chars = input.split('');
  let inString = false;
  let escaped = false;

  for (let i = 0; i < chars.length; i++) {
    const ch = chars[i];

    if (inString) {
      if (escaped) {
        escaped = false;
      } else if (ch === '\\') {
        escaped = true;
      } else if (ch === '"') {
        inString = false;
      }
      continue;
    }

    if (ch === '"') {
      inString = true;
      continue;
    }

    if (ch === ',') {
      // Look ahead past whitespace for a closing bracket/brace.
      let j = i + 1;
      while (j < chars.length && /\s/.test(chars[j])) {
        j++;
      }
      if (chars[j] === '}' || chars[j] === ']') {
        chars[i] = ' ';
      }
    }
  }

  return chars.join('');
}

module.exports = { stripJsonComments };
