#!/usr/bin/env bash
# Markdown code-span guard — a public document must not leave an inline code span unclosed.
#
# ## The failure this exists to end
# One line of `docs/rules/catalog.md` carried FIVE backticks, because the command it published has a
# backtick inside it (`grep -c '^| `cross-layer/' …`) and that cannot be written in a single-backtick
# span. CommonMark then paired the runs the other way round: the command rendered as two fragments,
# forty characters of ordinary prose rendered as CODE, and the last run never closed — which spilled
# into the following paragraphs and left SEVEN more spans mispaired after it. One typo, eight broken
# paragraphs, and the published recount command unrunnable.
#
# It was not a local defect either: `docs/rules/catalog.md` is `include_str!`'d into both binaries, so
# the same bytes ship from `zzop contract rule-catalog` and the `zzop://contract/rule-catalog` MCP
# resource. A reader who copies the command gets a shell error; a reader who trusts the rendering
# reads prose as a command.
#
# ## Why the check is stream-scoped and not line-scoped
# A code span may legally cross a newline, and most of this repo's inline spans do. A line-at-a-time
# tokenizer reads the CLOSING backtick of a continued span as an unclosed OPENER — measured: the
# first version of this check reported 17 hits across 8 files and every one of them was that false
# positive. So the non-fence text of each file is concatenated into one stream, tokenized once, and
# positions are mapped back to lines.
#
# Two things are reported:
#   * an opening run with no later run of the SAME length — the span never closes;
#   * a span whose body contains a BLANK LINE — CommonMark ends inline parsing at a paragraph break,
#     so such a "span" is not one, and the backticks render literally on both sides.
#
# Fenced blocks (``` and ~~~) are skipped entirely: backticks inside them are content.
#
# ## What it does NOT check
# That a span contains something sensible, or that a published command runs. The sibling defect found
# in the same round — a `find` command published with unescaped `(` — is invisible here, because it
# is lexically a perfectly good code span. Nothing mechanical reads published commands yet.
set -euo pipefail

cd "$(dirname "$0")/.."

node - <<'NODE'
const fs = require('node:fs');
const { execFileSync } = require('node:child_process');

// Every tracked markdown file. The private notes tree is a separate repository and never appears
// in `git ls-files` here, so no filter is needed for it — and the filter that WAS here matched an
// unanchored '.claude' prefix, which excluded nothing it meant to and silently dropped the tracked,
// published `.claude-plugin/commands/update.md` from the population. A prefix is not a path segment;
// this repo's own `scripts/lib/tracked-grep.sh` says so in those words.
const files = execFileSync('git', ['ls-files', '*.md'], { encoding: 'utf8' })
  .trim()
  .split('\n')
  .filter((f) => f);

// The floor is PER CLASS, not a total. A magic total has to be re-tuned on every commit that adds
// a page, which is how a ratchet becomes a number people bump without reading — `scripts/lib/
// shell-subjects.sh` says exactly that and is the pattern copied here. This repo cannot be in a
// state with no docs/, no root documents and no package READMEs, so a zero from any one of them is
// a broken needle and never a fact about the tree.
const CLASSES = [
  ['docs', (f) => f.startsWith('docs/')],
  ['root documents', (f) => !f.includes('/')],
  ['package READMEs', (f) => f.startsWith('packages/')],
];

let lines = 0;
const bad = [];

for (const f of files) {
  const L = fs.readFileSync(f, 'utf8').split('\n');
  let fence = false;
  let stream = '';
  const lineOf = [];
  L.forEach((l, i) => {
    if (/^\s*(```|~~~)/.test(l)) { fence = !fence; return; }
    if (fence) return;
    lines += 1;
    for (let k = 0; k < l.length; k++) lineOf.push(i + 1);
    stream += l;
    lineOf.push(i + 1);
    stream += '\n';
  });

  const runs = [...stream.matchAll(/`+/g)].map((m) => ({ n: m[0].length, i: m.index }));
  let k = 0;
  while (k < runs.length) {
    const open = runs[k];
    let close = -1;
    for (let j = k + 1; j < runs.length; j++) {
      if (runs[j].n === open.n) { close = j; break; }
    }
    if (close < 0) {
      bad.push(`${f}:${lineOf[open.i]}: a run of ${open.n} backtick(s) never closes — ` +
        `${JSON.stringify(stream.slice(open.i, open.i + 70).split('\n')[0])}`);
      k += 1;
      continue;
    }
    if (/\n[ \t]*\n/.test(stream.slice(open.i + open.n, runs[close].i))) {
      bad.push(`${f}:${lineOf[open.i]}: this span's body crosses a blank line, so it is not a span — ` +
        `${JSON.stringify(stream.slice(open.i, open.i + 70).split('\n')[0])}`);
    }
    k = close + 1;
  }
}

// Floor before the verdict: a subject set that collapsed must fail, not pass.
const empty = CLASSES.filter(([, pick]) => !files.some(pick)).map(([name]) => name);
if (empty.length) {
  console.error('check-markdown-code-spans: REFUSING TO JUDGE — a pathspec class came back empty: ' +
    `${empty.join(', ')}. Of ${files.length} tracked .md file(s), none is in that class, which is a ` +
    'broken listing rather than a fact about the tree. A clean report here would vouch for pages ' +
    'nothing read.');
  process.exit(1);
}

if (bad.length) {
  console.error('check-markdown-code-spans: FAILED');
  for (const b of bad) console.error(`  ${b}`);
  console.error('\nA command containing a backtick needs a DOUBLE-backtick span with a space on each ' +
    'side: `` grep -c \'^| `x\' file `` . One mispaired run re-pairs every run after it, so the ' +
    'damage is never local to the line you wrote.');
  process.exit(1);
}
const perClass = CLASSES.map(([name, pick]) => `${name} ${files.filter(pick).length}`).join(', ');
console.log(`check-markdown-code-spans: clean (${files.length} public .md file(s) — ${perClass}; ${lines} non-fenced ` +
  'line(s); every inline code span opens, closes, and stays inside its paragraph).');
NODE
