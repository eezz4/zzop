//! Contract 9: bare-word anchoring — a keyword-shaped regex must not match prose/string-literal text.

use zzop_core::{Matcher, RuleDef};

use crate::load_all_packs;

/// English words that read as ordinary prose (a JSDoc example, a log message, a string literal like
/// `"logged in to do this"` or `"waiting for ${x}"`) but are also meaningful loop/SQL keywords when they
/// appear as real syntax. A DSL pattern that matches one of these as a bare `\bword\b` with no adjacent
/// syntax anchor fires on prose too — exactly the defect class two shipped rules had (`perf/api-in-loop`
/// matched bare `\bdo\b`; `be-security/sql-taint` matched bare `UPDATE`), both fixed in the same commit
/// that added this contract. Deliberately a small, curated list (not "every English word that's also a
/// keyword") — these are the words shipped rules have actually tripped over in practice; extend this list
/// only once real usage finds a new one, the same "fix the whole class, not the one sampled rule"
/// discipline `docs/rules/authoring-guide.md`'s checklist item 4 documents.
const DANGEROUS_BARE_WORDS: &[&str] = &["do", "for", "while", "update", "delete", "select"];

/// A character that, sitting close to a dangerous word in the pattern's own SOURCE TEXT, is good textual
/// evidence the word is anchored to real syntax rather than free-standing prose: an open paren/brace (a
/// call or block following the word), a quote (a string-literal boundary wrapping it), or a `+`/`.`/`[`
/// (a concatenation/member-access/character-class token adjacent to it). Deliberately NOT `)`/`}`/`|` —
/// alternation pipes and closing delimiters sit next to EVERY word in a bare `\bfor\b|\bwhile\b|\bdo\b`
/// alternation (the exact shipped bug this contract targets), so treating them as anchors would make the
/// heuristic accept the very defect it exists to catch.
const ANCHOR_CHARS: &[char] = &['(', '{', '"', '\'', '+', '.', '['];

/// How far (in bytes) from a dangerous word's own start/end — or from its innermost enclosing regex
/// group's open/close paren, see `enclosing_group` — this contract looks for an `ANCHOR_CHARS` hit.
/// Deliberately small relative to a whole pattern (patterns here run 80-250+ characters): a real anchor in
/// every shipped rule sits within a handful of bytes of the word or its group boundary (`\bdo\s*\{`'s `{`
/// is 4 bytes after `do`; `(?:get|post|...|delete)\s*\(`'s `(` is ~4 bytes after the enclosing group's own
/// `)`) — a window this size cannot mistake "some quote/paren exists somewhere in this 200-byte regex" for
/// "this specific word is anchored".
const ANCHOR_WINDOW: usize = 12;

/// Every regex-bearing field capable of hiding a bare dangerous word against real scanned source text —
/// deliberately NOT `file_pattern`/`require_file`/`require_file_all`/`require_file_absent`/`exclude_pattern`/
/// `file_exclude_pattern`: those gate which FILES get scanned or veto an otherwise-matched line, they never
/// themselves shape a finding's matched text the way `line_pattern`/`any`/`patterns`/`absent` do (a bare
/// `\b(?:SELECT|INSERT|UPDATE|DELETE|MERGE)\b` in `be-security/sql-taint`'s own `require_file` only widens
/// which files reach the real `line_pattern` check below it — intentionally bare, not the latent bug its
/// `line_pattern` was).
fn regex_bearing_texts(rule: &RuleDef) -> Vec<(&'static str, &str)> {
    match &rule.matcher {
        Matcher::LineScan(m) => {
            let mut out = Vec::new();
            if let Some(p) = &m.line_pattern {
                out.push(("line_pattern", p.as_str()));
            }
            if let Some(alts) = &m.any {
                for lp in alts {
                    out.push(("any[].pattern", lp.pattern.as_str()));
                }
            }
            out
        }
        Matcher::MethodScan(m) => {
            let mut out = Vec::new();
            for lp in &m.patterns {
                out.push(("patterns[].pattern", lp.pattern.as_str()));
            }
            for lp in &m.absent {
                out.push(("absent[].pattern", lp.pattern.as_str()));
            }
            out
        }
        Matcher::SymbolScan(m) => m
            .name_pattern
            .as_deref()
            .map(|p| vec![("name_pattern", p)])
            .unwrap_or_default(),
        Matcher::IoScan(m) => m
            .key_pattern
            .as_deref()
            .map(|p| vec![("key_pattern", p)])
            .unwrap_or_default(),
    }
}

/// Finds the byte offsets of every `\b<word>\b` (case-insensitive) occurrence of any `DANGEROUS_BARE_WORDS`
/// entry inside `pattern`'s own SOURCE TEXT — i.e. this scans the regex STRING itself as plain text, not
/// scanned source code. Reuses the same `\b` word-boundary semantics the shipped rules themselves rely on,
/// so "does word X appear as a standalone word in this regex" is answered the same way "does word X appear
/// as a standalone word in a source file" would be (e.g. `update` inside `updateMany` never matches, since
/// there is no word boundary between `e` and `M`).
fn dangerous_word_occurrences(pattern: &str) -> Vec<(usize, usize, &'static str)> {
    let mut out = Vec::new();
    for &word in DANGEROUS_BARE_WORDS {
        let re = regex::Regex::new(&format!(r"(?i)\b{word}\b")).expect("static word regex");
        for m in re.find_iter(pattern) {
            out.push((m.start(), m.end(), word));
        }
    }
    out
}

/// Whether an unescaped paren (`(` or `)` not immediately preceded by a single `\`) sits at byte offset `i`
/// in `bytes` — an escaped `\(`/`\)` is a LITERAL character the pattern matches in scanned text (e.g.
/// `\bfor\s*(?:\(|await\b)`'s `\(` matches a real `(` in source code), not a grouping metacharacter, so the
/// enclosing-group scan below must not count it as one. Pragmatic single-backslash lookback (does not
/// handle a doubled `\\(` escaped-backslash-then-paren edge case) — consistent with every other heuristic in
/// this file being a textual proxy, not a real regex parser.
fn is_unescaped_paren(bytes: &[u8], i: usize) -> bool {
    matches!(bytes[i], b'(' | b')') && (i == 0 || bytes[i - 1] != b'\\')
}

/// The innermost enclosing `(...)`/`(?:...)` group's open- and close-paren byte offsets for the span
/// `[start, end)`, found by a plain paren-depth scan outward from the span in both directions (ignoring
/// escaped parens, see `is_unescaped_paren`) — NOT a real regex parser (no awareness of character classes,
/// where an unescaped `(` inside `[...]` is a literal character, not a group; no pattern this contract
/// currently scans puts a paren inside a character class, so this gap has never mattered in practice, but
/// it is a real gap in what this function can prove). Returns `None` when the span is not inside any group
/// at all (e.g. a bare `\bfor\b` sitting directly in a top-level alternation with no wrapping `(...)`).
fn enclosing_group(pattern: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let bytes = pattern.as_bytes();

    let mut depth = 0i32;
    let mut open = None;
    let mut i = start;
    while i > 0 {
        i -= 1;
        if !is_unescaped_paren(bytes, i) {
            continue;
        }
        if bytes[i] == b')' {
            depth += 1;
        } else if depth == 0 {
            open = Some(i);
            break;
        } else {
            depth -= 1;
        }
    }
    let open = open?;

    let mut depth = 0i32;
    let mut close = None;
    let mut j = end;
    while j < bytes.len() {
        if is_unescaped_paren(bytes, j) {
            if bytes[j] == b'(' {
                depth += 1;
            } else if depth == 0 {
                close = Some(j);
                break;
            } else {
                depth -= 1;
            }
        }
        j += 1;
    }
    let close = close?;
    Some((open, close))
}

/// Whether any `ANCHOR_CHARS` byte appears in `bytes[lo..hi]` (`hi` clamped to `bytes.len()`, `lo >= hi`
/// meaning an empty/invalid window yields no anchor). Operates on raw bytes (not a `str` slice) so it can
/// never panic on a non-UTF-8-boundary offset — every `ANCHOR_CHARS` entry is single-byte ASCII, so a raw
/// byte comparison is exact regardless of where `lo`/`hi` land relative to any multi-byte character
/// elsewhere in the pattern.
fn window_has_anchor(bytes: &[u8], lo: usize, hi: usize) -> bool {
    let hi = hi.min(bytes.len());
    if lo >= hi {
        return false;
    }
    bytes[lo..hi]
        .iter()
        .any(|&b| ANCHOR_CHARS.contains(&char::from(b)))
}

/// Whether the dangerous-word occurrence at `[start, end)` in `pattern` is anchored to real syntax. Anchored
/// when EITHER: (a) an `ANCHOR_CHARS` byte sits within `ANCHOR_WINDOW` bytes immediately before `start` or
/// immediately after `end` in the pattern's own text, or (b) the word sits inside a `(...)`/`(?:...)` group
/// (`enclosing_group`) and an `ANCHOR_CHARS` byte sits within `ANCHOR_WINDOW` bytes immediately before that
/// group's own open paren or immediately after its close paren — the shape every real
/// alternation-of-keywords rule in the shipped packs uses (the anchor lives just outside the group wrapping
/// the whole alternative list, not next to any one word inside it — e.g.
/// `(?:get|post|put|patch|delete|head|options|request|fetch|send|query)\s*\(`'s `(` sits right after the
/// group's own `)`, dozens of bytes from `delete` itself, so only the group-boundary check (b), not the
/// immediate-proximity check (a), anchors that occurrence).
fn is_anchored(pattern: &str, start: usize, end: usize) -> bool {
    let bytes = pattern.as_bytes();
    let immediate = window_has_anchor(bytes, start.saturating_sub(ANCHOR_WINDOW), start)
        || window_has_anchor(bytes, end, end + ANCHOR_WINDOW);
    if immediate {
        return true;
    }
    if let Some((open, close)) = enclosing_group(pattern, start, end) {
        return window_has_anchor(bytes, open.saturating_sub(ANCHOR_WINDOW), open)
            || window_has_anchor(bytes, close + 1, close + 1 + ANCHOR_WINDOW);
    }
    false
}

/// Contract #9 — no shipped DSL rule matches a `DANGEROUS_BARE_WORDS` entry as free-standing prose. See the
/// module doc's contract #9 entry and `ANCHOR_CHARS`/`ANCHOR_WINDOW`/`is_anchored`'s own docs for exactly
/// what "anchored" means and what this heuristic can and cannot prove: it is a textual-proximity check on
/// the regex pattern's own SOURCE STRING, not a regex semantics engine — it cannot understand alternation
/// grouping beyond simple paren-depth counting, so a sufficiently contrived pattern (e.g. a real anchor
/// sitting outside even the word's own enclosing group, further out than this contract's innermost-group
/// check reaches) could still evade it. It exists to catch the concrete, real defect class two shipped
/// rules had (`perf/api-in-loop` matched bare `\bdo\b` inside prose string literals like `"logged in to do
/// this"`; `be-security/sql-taint` matched bare `UPDATE` inside prose), not to be a sound regex analyzer —
/// a human reviewing a new rule's pattern by eye remains the real backstop for a pattern this heuristic
/// doesn't flag.
#[test]
fn dangerous_bare_words_are_syntax_anchored_not_bare_prose_matches() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        for rule in &pack.rules {
            for (field, text) in regex_bearing_texts(rule) {
                for (start, end, word) in dangerous_word_occurrences(text) {
                    if !is_anchored(text, start, end) {
                        offenders.push(format!(
                            "{}/{} ({field}): bare `{word}` at byte {start}..{end} in {text:?} has no \
                             adjacent syntax anchor ({ANCHOR_CHARS:?} within {ANCHOR_WINDOW} bytes, or \
                             just outside its enclosing regex group) — it will match \"{word}\" inside \
                             ordinary prose/string-literal text, not just real syntax",
                            pack.id, rule.id
                        ));
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "DSL rule patterns match a dangerous bare word with no syntax anchor — see this test's own doc \
         comment for what \"anchored\" means: {offenders:#?}"
    );
}
