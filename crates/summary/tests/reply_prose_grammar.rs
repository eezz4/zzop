//! Grammatical integrity of the LEGEND PROSE a reply ships, pinned against a REAL reply.
//!
//! # The defect this closes
//! `coverageGaps.meaning` is 3,246 characters of shipped prose that rides on every `analyze` reply.
//! `5498b13` inserted a new paragraph INTO its opening parenthetical, closed that parenthetical early
//! and left the original tail behind, so the sentence shipped as
//!
//! > `... Line distribution is not evidence about whether anything read the files). filetype look
//! > principal on its own; a parsed extension is judged against >=10% of the files ... instead).`
//!
//! Two `)` with no opener and a subjectless lowercase fragment, on a user-visible surface this release
//! is the FIRST to emit. Every gate in the workspace was green: these strings are `const &str`
//! assembled from backslash-continued source lines, so no reviewer reads the rendered sentence and no
//! compiler can. The repo already guards published prose in `*.md`/`*.html`
//! (`scripts/check-overclaim-prose.sh`); nothing guarded the prose compiled into the binary.
//!
//! # Why a rendered-reply test and not a source grep
//! A source grep sees `... the files). \` and `filetype look principal ...` on two different lines with
//! a line continuation between them; it cannot tell a wrapped sentence from a broken one. Rendering the
//! reply and reading the resulting STRING is the only place the sentence exists as a sentence. That is
//! the same argument `reply_keyset_parity.rs` makes for keys, one layer down: text cannot see what
//! serialization builds.
//!
//! # The two checks, and why only these two
//! Both are MECHANICAL properties of English prose with no judgment in them, because a guard that needs
//! judgment gets a false positive and then gets deleted (`check-overclaim-prose.sh`'s own design note).
//!
//! 1. **Delimiters nest.** `(`/`)` and `[`/`]` must balance and must never close before they open.
//!    A stranded `)` is the exact residue an edit like `5498b13`'s leaves.
//! 2. **A sentence starts with a capital.** After `. ` / `! ` / `? `, the next word must not begin with
//!    a lowercase letter — the other half of the same residue. Identifiers are exempt and they are what
//!    make this checkable at all: a token that is backticked, or that carries a `.`/`_`/`-`/`/`/`:` or
//!    an inner capital, is a NAME rather than a word ("`zzop coverage`'s table", `kind`, `io-scan`), and
//!    a bare lowercase English word after a full stop is never anything but a break. ABBREVIATIONS are
//!    the other exemption and they are why this needed a second pass: the cross reply's own
//!    tree-discovery warning says "unrelated repos (e.g. a parallel demo implementation)", and `e.g.` is
//!    a full stop that ends no sentence. A token carrying an inner `.`, a single-letter token, and a
//!    short closed list ([`ABBREVIATIONS`]) are skipped for that reason.
//!
//! Deliberately NOT checked: quote balance (`'` is an apostrophe far more often than a quote),
//! double-space, or anything about meaning.
//!
//! # Scan surface
//! Every string in a real `analyze` reply and a real `cross` reply, at any depth, at least
//! [`PROSE_FLOOR`] characters long. The floor is what separates a LEGEND from data — a path, a rule id
//! and a finding anchor are not sentences and have no grammar to check. Both replies are built from
//! throwaway trees the way the sibling reply tests build theirs, so the subject is the shipped string
//! and not a fixture copy of it.

use std::fs;

/// Shortest string this treats as prose. A legend sentence in this repo runs to hundreds of
/// characters; the longest non-prose strings a reply carries are paths, rule ids and short messages.
/// Set where it separates those two populations rather than tuned to today's contents.
const PROSE_FLOOR: usize = 120;

fn default_filters() -> zzop_summary::FindingFilters {
    zzop_summary::FindingFilters::new(None, None, None).expect("no-filter view always constructs")
}

fn tmp_tree(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("zzop-prose-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("zzop.config.jsonc"),
        zzop_config::template::CONFIG_TEMPLATE_JSONC,
    )
    .unwrap();
    dir
}

/// Every string at any depth, paired with the dotted path it sits at, so a failure names the FIELD
/// rather than only the sentence.
fn strings(v: &serde_json::Value, at: &str, out: &mut Vec<(String, String)>) {
    match v {
        serde_json::Value::String(s) => out.push((at.to_string(), s.clone())),
        serde_json::Value::Array(a) => {
            for (i, item) in a.iter().enumerate() {
                strings(item, &format!("{at}[{i}]"), out);
            }
        }
        serde_json::Value::Object(o) => {
            for (k, item) in o {
                strings(item, &format!("{at}.{k}"), out);
            }
        }
        _ => {}
    }
}

/// The first unbalanced delimiter, as `(index, character)` — either a closer with no opener, or an
/// opener never closed (reported at its own index so the message can point at it).
fn unbalanced(s: &str) -> Option<(usize, char)> {
    let mut stack: Vec<(usize, char)> = Vec::new();
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => stack.push((i, c)),
            ')' | ']' => {
                let want = if c == ')' { '(' } else { '[' };
                match stack.pop() {
                    Some((_, open)) if open == want => {}
                    _ => return Some((i, c)),
                }
            }
            _ => {}
        }
    }
    stack.first().copied()
}

/// Is `word` a NAME rather than an English word? Identifiers are the only reason a lowercase letter may
/// legitimately open a sentence, and every one of them is marked: backticked, dotted/underscored/
/// hyphenated/slashed/colon-bearing, or carrying an inner capital.
fn is_identifier(word: &str) -> bool {
    let bare = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '`');
    bare.starts_with('`')
        || bare.contains('.')
        || bare.contains('_')
        || bare.contains('-')
        || bare.contains('/')
        || bare.contains(':')
        || bare.chars().skip(1).any(|c| c.is_ascii_uppercase())
}

/// Words whose trailing `.` is part of the word. The inner-dot and single-letter rules below cover
/// `e.g.`, `i.e.` and initials; this list is for the ones that carry no inner dot.
const ABBREVIATIONS: [&str; 6] = ["etc", "vs", "cf", "approx", "al", "no"];

/// Is the token ending at `s[..stop]` an abbreviation rather than a sentence's last word?
fn ends_an_abbreviation(before: &str) -> bool {
    let token = before.rsplit(char::is_whitespace).next().unwrap_or(before);
    let bare = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
    bare.contains('.')
        || bare.chars().filter(char::is_ascii_alphanumeric).count() == 1
        || ABBREVIATIONS.contains(&bare.to_ascii_lowercase().as_str())
}

/// The first sentence in `s` that opens with a lowercase English word, as `(index, word)`.
fn lowercase_sentence_start(s: &str) -> Option<(usize, String)> {
    let bytes = s.as_bytes();
    for (i, w) in s.char_indices() {
        // A sentence boundary is a terminator followed by exactly one space. Two spaces, a newline or a
        // closing bracket after the stop are all shapes this does not judge.
        if !matches!(w, '.' | '!' | '?') {
            continue;
        }
        let next = i + w.len_utf8();
        if bytes.get(next) != Some(&b' ') {
            continue;
        }
        // A decimal point, an ellipsis or an abbreviation is not a full stop.
        if s[..i].ends_with(|c: char| c.is_ascii_digit())
            || s[..i].ends_with('.')
            || ends_an_abbreviation(&s[..i])
        {
            continue;
        }
        let rest = &s[next + 1..];
        let word: String = rest
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        if word.is_empty() || is_identifier(&word) {
            continue;
        }
        if word.starts_with(|c: char| c.is_ascii_lowercase()) {
            return Some((next + 1, word));
        }
    }
    None
}

fn assert_prose_is_well_formed(reply: &str, surface: &str) {
    let v: serde_json::Value = serde_json::from_str(reply).expect("a reply is a JSON object");
    let mut found = Vec::new();
    strings(&v, surface, &mut found);
    let prose: Vec<(String, String)> = found
        .into_iter()
        .filter(|(_, s)| s.chars().count() >= PROSE_FLOOR)
        .collect();
    assert!(
        !prose.is_empty(),
        "{surface}: no string reached the {PROSE_FLOOR}-character prose floor, so this guard checked \
         NOTHING. Either the reply lost its legends or the floor is wrong — an empty scan passing is \
         the failure mode every prose guard has."
    );
    for (at, s) in &prose {
        if let Some((i, c)) = unbalanced(s) {
            panic!(
                "{at}: shipped prose has an unbalanced `{c}` at byte {i}. A `)` with no opener is what \
                 an edit leaves behind when it deletes a parenthetical's first half; these strings are \
                 backslash-continued `const &str`s, so nothing else in this workspace reads them as \
                 sentences.\n... {} ...",
                &s[i.saturating_sub(160)..(i + 160).min(s.len())]
            );
        }
        if let Some((i, word)) = lowercase_sentence_start(s) {
            panic!(
                "{at}: shipped prose starts a sentence with the lowercase word `{word}` at byte {i} — \
                 the residue of a deleted clause. Identifiers are exempt (backticked, or carrying \
                 `.`/`_`/`-`/`/`/`:` or an inner capital); a bare English word here is a break.\n... {} \
                 ...",
                &s[i.saturating_sub(160)..(i + 160).min(s.len())]
            );
        }
    }
}

/// 🔴 The `analyze` reply — `coverageGaps.meaning`'s home, and the surface the defect shipped on.
#[test]
fn the_analyze_reply_ships_no_broken_sentence() {
    let dir = tmp_tree("analyze");
    fs::write(
        dir.join("a.ts"),
        "import { b } from './b';\nexport const a = b;\n",
    )
    .unwrap();
    fs::write(dir.join("b.ts"), "export const b = 1;\n").unwrap();
    let reply =
        zzop_summary::analyze_summary(Some(&dir.display().to_string()), None, &default_filters())
            .expect("analyze must succeed on a configured tree");
    assert_prose_is_well_formed(&reply, "analyze");
}

/// The `cross` reply — a different legend set (`nativeAnalysesMeaning`, `bucketMeaning`) built by a
/// different module, and therefore a second population rather than a second run of the same one.
#[test]
fn the_cross_reply_ships_no_broken_sentence() {
    let fe = tmp_tree("cross-fe");
    fs::write(
        fe.join("api.ts"),
        "export const load = () => fetch('/api/users');\n",
    )
    .unwrap();
    let be = tmp_tree("cross-be");
    fs::write(
        be.join("server.ts"),
        "import express from 'express';\nconst app = express();\napp.get('/api/orders', (_req, res) => res.json([]));\n",
    )
    .unwrap();
    let reply = zzop_summary::cross_summary(
        &[fe.display().to_string(), be.display().to_string()],
        None,
        &default_filters(),
    )
    .expect("cross must succeed on two configured trees");
    assert_prose_is_well_formed(&reply, "cross");
}
