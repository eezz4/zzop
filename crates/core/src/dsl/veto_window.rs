//! The TEXT `CallScan::line_exclude_pattern` and `MethodScan::trigger_call_exclude_pattern` read — the
//! call's own parentheses rather than its first line.
//!
//! Two ENTRY POINTS over one window: [`call_window`] anchors on a projected `CallSite`'s callee text,
//! [`trigger_call_excluded`] on a regex match's offset. Everything below the anchor — the depth count,
//! the string mask, the comment terminator, the cap, and the rule that every refusal leaves the site
//! FIRING — is shared, and the two anchors' different failure modes are argued at each one's own doc.
//!
//! That field exists because the `regex` crate has no lookaround, so "this line must NOT say X" cannot
//! be spelled inside the positive pattern. It was then evaluated against ONE LINE, which made it blind
//! to the shape a formatter produces most often: `security/weak-crypto` fired on a `hashlib.md5(` whose
//! `usedforsecurity=False` sat on the next line — 3 of the 5 uses in getredash/redash @ `ca79fe98`, and
//! the rule's own message named that residual rather than fixing it.
//!
//! ## Why the window is the CALL's, and what happens when it cannot be placed
//!
//! Deriving the window from the LINE's net parenthesis depth is what the first version of this module
//! did, and it is wrong in the one direction a veto must never be wrong in. Three measured shapes, all
//! of which waived a real finding using text that belonged to something else:
//!
//! ```text
//! key   = hashlib.md5(f"{a}(").hexdigest()        # a `(` inside a STRING leaves net +1
//! other = hashlib.sha1(b, usedforsecurity=False)  # ...so this line waives the md5 above
//!
//! digest = hashlib.md5(payload)  # legacy hash (see PROJ-123   <- a `(` in a COMMENT, net +1
//!
//! log.info("d %s", hashlib.md5(x).hexdigest(),    # the ENCLOSING call leaves net +1
//!          extra=dict(usedforsecurity=False))     # ...so an unrelated dict waives it
//! ```
//!
//! So the window is anchored on the SITE's own callee: the depth count starts at the `(` that follows
//! the callee text and stops when that call closes. All three shapes above then yield the line alone.
//! When the callee text appears more than once on the line, this pass cannot tell which occurrence the
//! site is, and it declines rather than guessing — the window is the line, the same text the field read
//! before 2026-08-21. Declining costs a waiver; guessing costs a finding, and this field only ever
//! SUPPRESSES, so absence of evidence must never become evidence of a waiver.
//!
//! String literals are masked before the depth is counted (never before the MATCH — a veto keyword is
//! code, and the pattern must still see whatever the source wrote).
//!
//! ## Comments TERMINATE the window rather than being masked out of it
//!
//! A `(` in a comment is not a real open paren, and on a MULTI-LINE call it used to inflate the depth so
//! the window ran past the call's own `)` and read text belonging to the NEXT call — a real finding
//! waived by a declaration that was never this call's. The callee anchor does not save that case: it
//! only bounds where counting STARTS, and the early return only fires once a `)` has balanced the count,
//! which the inflated depth prevents.
//!
//! Masking the comment instead is what one would reach for first, and it is not safe here, because this
//! pass is deliberately language-blind: `//` opens a comment in JS/Rust and is FLOOR DIVISION in Python
//! (`(a // b)`), `#` opens one in Python and is a PRIVATE FIELD in JS (`obj.#x)`) and a directive in C#,
//! and `--` opens one in SQL and is DECREMENT in JS (`(i--)`). Masking a tail that is not a comment
//! deletes real `)` characters, which inflates the depth exactly like the bug being fixed.
//!
//! So a possible comment leader ENDS the window at the line carrying it: that line is included, nothing
//! after it is. Every misdetection then fails in one direction — the window is too SMALL, a suppression
//! is missed, and the site keeps firing — which is the direction the rest of this module already takes
//! for the ambiguous-callee and cap cases. The cost is stated in the residual below rather than hidden.
//!
//! ## The residual, which the consuming rule must state
//!
//! [`MAX_CALL_WINDOW_LINES`] caps the reach, so a declaration inside a call's own parentheses further
//! down than that still fires; and a declaration OUTSIDE those parentheses — set on a preceding
//! statement, or decided by a wrapper — is out of reach by construction. A comment anywhere inside the
//! argument list ends the window there, so a declaration written BELOW such a comment is out of reach
//! too, and the site keeps firing.
//!
//! ## The UPWARD twin lives next door
//!
//! [`enclosing_window`] answers the mirror question — "which still-open call/array/object openers sit
//! ABOVE this line" — for `LineScan::enclosing_call_exclude_pattern`. It shares this module's masker
//! and its comment-leader predicate and its `None`-means-no-suppression convention, and nothing else:
//! the direction inverts every other mechanic (see `enclosing.rs`'s own doc).

mod enclosing;

pub(super) use enclosing::enclosing_window;

/// How many lines past the site's own line a call window may reach. A declaration a formatter pushed
/// onto the next line is the case this exists for; a match ten lines down is far likelier to belong to
/// something else, and this field only ever SUPPRESSES, so the cap is the conservative direction.
const MAX_CALL_WINDOW_LINES: usize = 8;

/// The call's own line plus the continuation lines ITS parentheses hold open, for
/// `CallScan::line_exclude_pattern`. `callee` is the site's callee exactly as written
/// (`zzop_core::CallSite::callee`), which is what anchors the depth count — see the module doc.
///
/// `None` means the file text has no such line at all, which the caller must read as NO suppression.
pub(super) fn call_window(lines: &[&str], idx: usize, callee: &str) -> Option<String> {
    let start = lines.get(idx).and_then(|f| call_open_paren(f, callee));
    window_from(lines, idx, start)
}

/// Does `veto` match the same downward window, anchored on a REGEX MATCH instead of a callee string?
/// For `MethodScan::trigger_call_exclude_pattern`, whose site is a trigger-pattern match on a line
/// rather than a projected `CallSite`. `scan` is the exact text the matcher tested (string-masked or
/// not, per `strip_string_literals`); it is byte-length-identical to `lines[idx]`, so an offset taken
/// from one indexes the other.
///
/// ## Why the offset is STRICTLY more precise than the callee anchor, and where the ambiguity moves to
///
/// [`call_open_paren`] has to FIND its site — it searches the line for the callee text and declines
/// when that text occurs twice, because it cannot tell which occurrence the projected call site is. A
/// match offset does not need finding: it already names one occurrence, so that decline arm has nothing
/// to decline and its absence weakens nothing.
///
/// The ambiguity does not vanish, though; it moves one level UP, and this function still answers it the
/// same way. `MethodScan` decides per LINE, not per occurrence: a line carrying TWO trigger matches
/// yields one finding, so there is no "which one" for the caller to have meant either. Guessing the
/// first would let a mitigator on the first call waive a second, unmitigated call on the same line — a
/// finding lost to text that was never that call's, which is the exact failure the callee anchor exists
/// to prevent. So a multi-match line declines here, and declining means NO suppression: the site keeps
/// firing. Every other refusal below runs the same direction.
///
/// `veto` is `None` when the rule does not set the field, which is always FALSE — no field, no waiver.
pub(super) fn trigger_call_excluded(
    veto: Option<&regex::Regex>,
    trigger: &regex::Regex,
    lines: &[&str],
    idx: usize,
    scan: &str,
) -> bool {
    let Some(veto) = veto else { return false };
    let mut hits = trigger.find_iter(scan);
    let Some(m) = hits.next() else {
        return false; // no trigger match on this line — nothing to anchor, nothing to waive
    };
    if hits.next().is_some() {
        return false; // ambiguous: two trigger calls share this line, see the doc above
    }
    let start = lines
        .get(idx)
        .and_then(|f| match_open_paren(f, m.start(), m.end()));
    window_from(lines, idx, start).is_some_and(|w| veto.is_match(&w))
}

/// The window itself, once its anchor has been placed (or declined). `start` is the byte offset of the
/// `(` the depth count begins at; `None` means the anchor could not be placed and the window is the
/// LINE alone — the text these fields read before either anchor existed.
fn window_from(lines: &[&str], idx: usize, start: Option<usize>) -> Option<String> {
    let first = *lines.get(idx)?;
    let Some(start) = start else {
        return Some(first.to_string());
    };
    let mut out = String::from(first);
    let first_masked = mask(first);
    if has_comment_leader(&first_masked[start..]) {
        return Some(out);
    }
    let mut depth = paren_delta(0, &first_masked[start..]);
    let mut taken = 0usize;
    while depth > 0 && taken < MAX_CALL_WINDOW_LINES {
        let Some(next) = lines.get(idx + 1 + taken) else {
            break;
        };
        out.push('\n');
        out.push_str(next);
        let next_masked = mask(next);
        if has_comment_leader(&next_masked) {
            return Some(out);
        }
        depth = paren_delta(depth, &next_masked);
        taken += 1;
    }
    Some(out)
}

/// Byte offset of the `(` that opens THIS site's argument list, or `None` when the line does not place
/// the call unambiguously — no occurrence of the callee (an envelope line, a callee spelled differently
/// from the source), more than one occurrence (two calls to the same function on one line), or no `(`
/// after it. Every `None` means "read the line alone", which is what this field did before it had a
/// window at all.
///
/// Matched on the MASKED line so a callee name quoted inside a string cannot be mistaken for the site.
fn call_open_paren(line: &str, callee: &str) -> Option<usize> {
    if callee.is_empty() {
        return None;
    }
    let masked = mask(line);
    let mut hits = masked.match_indices(callee);
    let (at, _) = hits.next()?;
    if hits.next().is_some() {
        return None; // ambiguous: two calls to the same callee share this line
    }
    let after = at + callee.len();
    let rest = masked.get(after..)?;
    let open = rest.find('(')?;
    // Only whitespace may sit between the callee and its own `(`; anything else means the `(` belongs
    // to a different expression (`md5.hexdigest, other(` on one line).
    if !rest[..open].chars().all(char::is_whitespace) {
        return None;
    }
    Some(after + open)
}

/// [`call_open_paren`]'s offset twin: the `(` opening the argument list of the call a regex matched at
/// `start..end` on this line. `None` — read as "the line alone" — when there is no `(` at or after the
/// match, or when something other than whitespace sits between the match's end and that `(`, which
/// means the `(` belongs to a different expression (`req.query, other(` under a bare-token trigger).
///
/// A trigger regex that includes its own `(` (`\bredirect\s*\(`) puts the paren INSIDE `start..end`, so
/// the gap is empty and the check passes trivially — which is why the search starts at `start` and not
/// at `end`. Scanned on the MASKED line for [`call_open_paren`]'s reason: a paren inside a string
/// literal is not this call's.
fn match_open_paren(line: &str, start: usize, end: usize) -> Option<usize> {
    let masked = mask(line);
    let open = start + masked.get(start..)?.find('(')?;
    let gap = masked.get(end.min(open)..open)?;
    if !gap.chars().all(char::is_whitespace) {
        return None;
    }
    Some(open)
}

/// Does this (string-masked) text carry anything that COULD open a comment in one of the languages this
/// pass runs over? Deliberately over-eager — see the module doc: a false hit shrinks the window, which
/// costs a suppression, while a miss grows it, which costs a finding. `/*` is included because a block
/// comment's parens count the same way; its close is not tracked, since the window ends at the open.
fn has_comment_leader(text: &str) -> bool {
    ["//", "#", "--", "/*"].iter().any(|l| text.contains(l))
}

/// The line with every closed string literal's interior blanked — used for the depth count, the callee
/// search, and [`enclosing_window`]'s bracket scan, never for the MATCH itself. Same masker
/// `LineScan::strip_string_literals` uses, so "what counts as a string" has one owner.
fn mask(line: &str) -> String {
    super::string_mask::mask_string_literals(line)
}

/// Depth of THIS call after `text`, starting from `depth`, stopping the instant the call closes.
///
/// The early return is what makes the count the call's rather than the line's: everything after the
/// matching `)` — a trailing comment carrying a `(`, a chained `.hexdigest()`, a second call — belongs
/// to something else and must not reopen a window this call already closed. That was the third of the
/// three measured shapes in the module doc.
fn paren_delta(depth: i32, text: &str) -> i32 {
    let mut d = depth;
    for c in text.chars() {
        match c {
            '(' => d += 1,
            ')' => {
                d -= 1;
                if d <= 0 {
                    return 0;
                }
            }
            _ => {}
        }
    }
    d
}

#[cfg(test)]
mod tests;
