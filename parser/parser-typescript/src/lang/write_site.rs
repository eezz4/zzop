//! Per-symbol write-site detection — a pure `(symbol body-span text, constant vocab) -> Vec<WriteSite>`
//! regex scan, moved here from `zzop_rules_graph::http_scan` so it runs ONCE at parse time (feeding
//! `SourceSymbol::write_sites`) instead of once per BFS-reached symbol on every analysis run. Detects two
//! independent shapes within a symbol's body span (`body_start..=body_end`, 1-based, inclusive):
//! - a raw-SQL write string (`INSERT`/`UPDATE ...`/`DELETE FROM`/`REPLACE INTO`, case-insensitive, never a
//!   SELECT) — covers stacks issuing SQL directly (Cloudflare D1, better-sqlite3, `pg`) rather than
//!   through a store-method call;
//! - an ORM/store-like method call (`base(.sub)?.method(`, receiver matched against
//!   [`DEFAULT_ORM_RECEIVER_PATTERN`]) whose method either belongs to the generic write vocabulary
//!   ([`DEFAULT_WRITE_METHODS`]) or classifies as a [`zzop_core::NonIdempotentKind`] (create /
//!   atomic-accumulate / counter).
//!
//! Both shapes are merged into one position-sorted `Vec<zzop_core::WriteSite>` per symbol, so a single
//! list serves both call-graph scanners in `zzop_rules_graph::http_scan`:
//! - `unsafe-read-endpoint` wants "any write site" — every entry EXCEPT a pure counter-bump
//!   (`kind == Some(Counter)`) qualifies, since [`DEFAULT_WRITE_METHODS`] (the vocabulary
//!   `unsafe-read-endpoint` always used) never included the counter vocabulary
//!   (`incr`/`incrby`/`decr`/`decrby`) — excluding `Counter` sites here reproduces that vocabulary gap
//!   exactly rather than widening it now that both scans share one list.
//! - `non-idempotent-write` wants every entry whose `kind` is set and allowed for the endpoint's HTTP
//!   method — unaffected by the `Counter` exclusion above, which is `unsafe-read-endpoint`-specific.
//!
//! Vocabulary is never config-overridden (the engine always calls this with the default vocab baked in),
//! so pre-computing it at parse time is behavior-neutral. A nested function's body is included in its
//! outer symbol's scanned span (`body_start`/`body_end` attribute a nested closure/function's writes to
//! the enclosing symbol — an existing, unchanged narrowing), and a raw-SQL sink label truncates at the
//! first newline (a multi-line statement's label can be incomplete) — both carried over verbatim from the
//! regex scan this replaces.

use std::sync::OnceLock;

use regex::Regex;
use zzop_core::{NonIdempotentKind, SourceSymbol, WriteSite};

/// Default ORM-receiver pattern: a Repository/Store suffix, or a bare prisma/db/orm/tx/trx identifier.
pub const DEFAULT_ORM_RECEIVER_PATTERN: &str = r"Repository$|Store$|^prisma$|^db$|^orm$|^tx$|^trx$";

/// Default write-method vocabulary underlying `unsafe-read-endpoint`'s "any write" check.
pub const DEFAULT_WRITE_METHODS: &[&str] = &[
    "create",
    "createMany",
    "update",
    "updateMany",
    "delete",
    "deleteMany",
    "upsert",
    "insert",
    "save",
    "remove",
];

const WRITE_LABEL_TOKEN_COUNT: usize = 3;

const CREATE_METHODS: [&str; 3] = ["create", "createMany", "insert"];
const UPDATE_METHODS: [&str; 3] = ["update", "updateMany", "upsert"];
const COUNTER_METHODS: [&str; 4] = ["incr", "incrby", "decr", "decrby"];
const ATOMIC_OP_KEYS: [&str; 4] = ["increment", "decrement", "push", "multiply"];

/// Raw-SQL write in a string literal. Never matches a SELECT.
fn sql_write_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:INSERT\s+(?:OR\s+\w+\s+)?INTO|UPDATE\s+\w|DELETE\s+FROM|REPLACE\s+INTO)\b",
        )
        .unwrap()
    })
}

fn atomic_op_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(&format!(r"\b(?:{})\s*:", ATOMIC_OP_KEYS.join("|"))).unwrap())
}

/// A store-shaped method call, receiver/method captured generically (classified in `classify`).
fn method_call_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"\b([A-Za-z_$][\w$]*)(?:\.[A-Za-z_$][\w$]*)?\.([A-Za-z_$][\w$]*)\s*\(").unwrap()
    })
}

fn orm_receiver_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(DEFAULT_ORM_RECEIVER_PATTERN).unwrap())
}

/// Joins `text`'s lines `body_start..=body_end` (1-based) into one block, plus `body_start` for `line_at`.
fn symbol_span_text(text: &str, body_start: u32, body_end: u32) -> (String, u32) {
    let lines: Vec<&str> = text.split('\n').collect();
    let start_idx = (body_start.saturating_sub(1)) as usize;
    let end_idx = (body_end as usize).min(lines.len());
    if start_idx >= end_idx {
        return (String::new(), body_start);
    }
    (lines[start_idx..end_idx].join("\n"), body_start)
}

/// Absolute 1-based line for a byte `offset` into a `symbol_span_text` block.
fn line_at(block: &str, first_line: u32, offset: usize) -> u32 {
    let capped = offset.min(block.len());
    first_line + block[..capped].matches('\n').count() as u32
}

/// The bracket-balanced argument text after a call's `(` (`open_after` = byte offset right after it, at depth 1).
fn call_args_span(block: &str, open_after: usize) -> &str {
    let mut depth = 1i32;
    let mut pos = open_after;
    let mut end = block.len();
    for c in block[open_after..].chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = pos;
                    break;
                }
            }
            _ => {}
        }
        pos += c.len_utf8();
    }
    &block[open_after..end]
}

fn args_have_atomic_op(args: &str) -> bool {
    atomic_op_re().is_match(args)
}

fn write_sink_label(matched: &str) -> String {
    matched
        .trim_end()
        .trim_end_matches('(')
        .trim_end()
        .to_string()
}

/// A short "verb + next token(s)" label for a raw-SQL write match, truncated at the first newline.
fn sql_label(rest_from_match_start: &str) -> String {
    rest_from_match_start
        .chars()
        .take_while(|&c| c != '\n')
        .collect::<String>()
        .replace(['"', '\'', '`'], "")
        .split_whitespace()
        .take(WRITE_LABEL_TOKEN_COUNT)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Classifies a store-like method call by its (case-sensitive) method name and, for an UPDATE-family
/// method, whether its call args carry an atomic-op key — the same precedence `zzop_rules_graph`'s old
/// `symbol_bad_sites` used: create-family first, then counter (case-insensitive), then update-family
/// gated on an atomic op being present.
fn classify(method: &str, block: &str, args_start: usize) -> Option<NonIdempotentKind> {
    if CREATE_METHODS.contains(&method) {
        return Some(NonIdempotentKind::Create);
    }
    if COUNTER_METHODS.contains(&method.to_lowercase().as_str()) {
        return Some(NonIdempotentKind::Counter);
    }
    if UPDATE_METHODS.contains(&method) {
        let args = call_args_span(block, args_start);
        return args_have_atomic_op(args).then_some(NonIdempotentKind::AtomicAccumulate);
    }
    None
}

/// Computes `sym`'s `write_sites`: every raw-SQL write plus every ORM/store-like write call in its body
/// span, position-sorted (SQL wins an exact-position tie, matching the old scan's `<=` precedence). Empty
/// when `sym` has no body span (type/interface symbols, or a degraded parse) or the span is empty.
pub fn write_sites_for_symbol(sym: &SourceSymbol, text: &str) -> Vec<WriteSite> {
    let (Some(start), Some(end)) = (sym.body_start, sym.body_end) else {
        return Vec::new();
    };
    let (block, first_line) = symbol_span_text(text, start, end);
    if block.is_empty() {
        return Vec::new();
    }
    let orm_re = orm_receiver_re();

    let mut sites: Vec<(usize, WriteSite)> = Vec::new();

    for m in sql_write_re().find_iter(&block) {
        sites.push((
            m.start(),
            WriteSite {
                file: sym.file.clone(),
                line: line_at(&block, first_line, m.start()),
                sink: sql_label(&block[m.start()..]),
                kind: None,
            },
        ));
    }

    for caps in method_call_re().captures_iter(&block) {
        let base = &caps[1];
        if !orm_re.is_match(base) {
            continue;
        }
        let method = &caps[2];
        let m0 = caps.get(0).unwrap();
        let kind = classify(method, &block, m0.end());
        let in_default_vocab = DEFAULT_WRITE_METHODS.contains(&method);
        if kind.is_none() && !in_default_vocab {
            continue; // neither the generic write vocab nor a non-idempotent classification — not a write
        }
        sites.push((
            m0.start(),
            WriteSite {
                file: sym.file.clone(),
                line: line_at(&block, first_line, m0.start()),
                sink: write_sink_label(m0.as_str()),
                kind,
            },
        ));
    }

    sites.sort_by_key(|(pos, _)| *pos);
    sites.into_iter().map(|(_, s)| s).collect()
}
