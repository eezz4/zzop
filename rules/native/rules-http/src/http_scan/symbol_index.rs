//! Name index and handler resolution — the seam where a handler REFERENCE ("delete", "ctrl.list",
//! "rateLimit(fn)") becomes a `SourceSymbol` id, plus the id->symbol map the two write-site scanners
//! read.
//!
//! Split out of `http_scan.rs` on 2026-08-11 when that file crossed the 300-line cap. The split line is
//! not arbitrary: everything here exists because **`SourceSymbol.id` is `file#name` and is therefore NOT
//! unique** — a language distinguishes overloads by parameter list and this id cannot. Both functions
//! below had to learn that separately, in the same week, from the same class of silent false negative,
//! so they belong in one module where the next reader meets the constraint before either consumer.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use zzop_core::SourceSymbol;

pub(super) fn ident_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[A-Za-z_$][\w$]*").unwrap())
}

/// Tail name (after the last `.`) -> DISTINCT symbol ids (`"file#name"`). `pub(crate)`: also used by
/// `mutating_route_no_auth`.
///
/// The dedup is load-bearing, not tidiness. `SourceSymbol.id` is `file#name` and is **not unique** — a
/// language distinguishes overloads by their parameter list and this id cannot (measured on the
/// reference corpus: 12 colliding groups over 28 symbols, in Java, C# and TypeScript). Without the
/// dedup, one id pushed twice reads as two candidates downstream, `resolve_handler_scoped`'s
/// unique-or-nothing rule refuses to guess, and the route's handler goes unresolved — so a rule that
/// judges the handler falls silent.
///
/// Measured 2026-08-11: a Spring controller with a single `@PostMapping create(String)` is flagged by
/// `mutating-route-no-auth`; adding an ordinary UNANNOTATED overload `create(int, String)` in the same
/// file silences it. The route is still one route and still unguarded; two entries of one string were
/// the entire cause, and nothing in `warnings`, `blindSpots` or the disclosure classes said so.
///
/// What this does NOT do is resolve genuine ambiguity: two DIFFERENT ids under one tail name still
/// yield `None`, in-file tie-break included. Only the duplicate-of-itself case changes.
pub(crate) fn build_name_index(symbols: &[SourceSymbol]) -> HashMap<String, Vec<String>> {
    let mut idx: HashMap<String, Vec<String>> = HashMap::new();
    for s in symbols {
        let tail = s.name.rsplit('.').next().unwrap_or(&s.name).to_string();
        let ids = idx.entry(tail).or_default();
        if !ids.iter().any(|id| id == &s.id) {
            ids.push(s.id.clone());
        }
    }
    idx
}

/// Symbol id -> the symbol that id should answer with, for the two rules that look a handler's write
/// sites up by id. ONE owner, because both of them had `.map(|s| (s.id.as_str(), s)).collect()` written
/// inline and `HashMap::collect` keeps the LAST entry for a repeated key.
///
/// That silently mattered, because `SourceSymbol.id` is not unique (see [`build_name_index`]). In
/// TypeScript a declaration-merged `interface helper {}` written AFTER `function helper()` shares the
/// function's id and carries no write sites, so last-wins replaced the real symbol with the empty one
/// and the finding vanished. Measured 2026-08-11: `unsafe-read-endpoint` fires on the fixture, stays
/// firing when the interface is declared BEFORE the function, and disappears when it is declared after.
/// Order of declaration is not a property this rule may depend on.
///
/// The tie-break is "the entry that actually carries write sites wins", which is the only axis these
/// two consumers read. It is deliberately not "merge the sites of both": two same-id symbols are two
/// DIFFERENT declarations, and unioning their sites would attribute one declaration's write to another.
pub(crate) fn symbols_by_id(symbols: &[SourceSymbol]) -> HashMap<&str, &SourceSymbol> {
    let mut by_id: HashMap<&str, &SourceSymbol> = HashMap::new();
    for s in symbols {
        by_id
            .entry(s.id.as_str())
            .and_modify(|held| {
                if held.write_sites.is_empty() && !s.write_sites.is_empty() {
                    *held = s;
                }
            })
            .or_insert(s);
    }
    by_id
}

/// Resolves a handler reference string to a unique symbol id, stripping wrapper calls (`rateLimit(fn)`) and
/// member access (`ctrl.list`). `None` when unknown or ambiguous (defined in multiple files) — never guessed.
pub(crate) fn resolve_handler(handler: &str, idx: &HashMap<String, Vec<String>>) -> Option<String> {
    resolve_handler_scoped(handler, idx, None)
}

/// [`resolve_handler`] with an optional route-FILE tie-break. When a handler name is ambiguous repo-wide
/// (defined in multiple files), a `Some(route_file)` disambiguates to the candidate declared in that file:
/// a decorator-routed handler (NestJS `@Delete() delete()`) is a METHOD of the controller class in the
/// file its route `IoProvide` points at, so a bare method name colliding across controllers (`delete` in
/// four controllers) still resolves uniquely once scoped to the route's own file. Only a UNIQUE in-file
/// candidate resolves; two `delete`s in one file, or none, still yields `None` (do-not-guess). With
/// `None` the behavior is identical to the original repo-wide-unique-or-nothing rule.
pub(crate) fn resolve_handler_scoped(
    handler: &str,
    idx: &HashMap<String, Vec<String>>,
    route_file: Option<&str>,
) -> Option<String> {
    let ids: Vec<&str> = ident_re().find_iter(handler).map(|m| m.as_str()).collect();
    for ident in ids.iter().rev() {
        match idx.get(*ident) {
            Some(candidates) if candidates.len() == 1 => return Some(candidates[0].clone()),
            Some(candidates) => {
                // Ambiguous repo-wide. Tie-break to the route's own file, if one was given.
                if let Some(file) = route_file {
                    let mut in_file = candidates
                        .iter()
                        .filter(|id| id.split('#').next() == Some(file));
                    if let (Some(one), None) = (in_file.next(), in_file.next()) {
                        return Some(one.clone());
                    }
                }
                return None; // still ambiguous — do not guess
            }
            None => continue,
        }
    }
    None
}
