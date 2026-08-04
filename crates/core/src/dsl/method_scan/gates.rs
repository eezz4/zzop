//! The two per-SPAN decisions `super::eval_method_scan` makes before it scans a symbol body's lines:
//! which symbols are candidates at all (innermost-span priority), and whether a span carrying a
//! `require_call_kind` gate has the projected witness that gate demands.
//!
//! Split out of `super` purely for the repo's per-file line cap, and along the seam that makes that
//! split honest: everything here is a pure function of the file's projected facts, with no matcher
//! state, no regex, and no finding construction. Anything that needs the scan's running state stays
//! in `super`.

use crate::dsl::source::SourceFile;

/// Which of `f.symbols` must be SKIPPED because a nested candidate span exists inside them.
///
/// Innermost-span priority: when spans overlap (a class symbol's span contains its methods' spans),
/// the outer one is dropped so one violation is not counted twice — the leaf is what a rule means by
/// "in this method". Returns a per-symbol flag vector indexed exactly like `f.symbols`, so the caller
/// can keep iterating symbols in projection order.
pub(super) fn drop_outer_spans(f: &SourceFile) -> Vec<bool> {
    let spans: Vec<(usize, u32, u32)> = f
        .symbols
        .iter()
        .enumerate()
        .filter_map(|(idx, sym)| {
            let (Some(s), Some(e)) = (sym.body_start, sym.body_end) else {
                return None;
            };
            (s != 0 && e >= s).then_some((idx, s, e))
        })
        .collect();
    let mut drop_symbol = vec![false; f.symbols.len()];
    for &(idx_a, s_a, e_a) in &spans {
        for &(idx_b, s_b, e_b) in &spans {
            if idx_a != idx_b && s_a <= s_b && e_a >= e_b && (s_a, e_a) != (s_b, e_b) {
                drop_symbol[idx_a] = true;
                break;
            }
        }
    }
    drop_symbol
}

/// Does this span carry the projected call-site witness a `require_call_kind` gate demands — i.e. at
/// least one `SourceFile::call_sites` entry of exactly `kind` whose own line falls inside
/// `body_start..=body_end`?
///
/// `None` (no gate) is vacuously satisfied. A gate with an EMPTY channel is not: the gate ALLOWS on
/// evidence, so absence silences the rule for this span rather than degrading to the lexical
/// co-occurrence it replaced — the direction `MethodScan::require_call_kind`'s own doc states, and the
/// reason a rule setting it must disclose the trade in its message.
pub(super) fn call_kind_witnessed(
    f: &SourceFile,
    kind: Option<&String>,
    body_start: u32,
    body_end: u32,
) -> bool {
    let Some(kind) = kind else { return true };
    f.call_sites
        .iter()
        .any(|s| s.kind == *kind && body_start <= s.line && s.line <= body_end)
}
