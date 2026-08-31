//! The UPWARD twin of [`super::call_window`]: the still-unclosed OPENER LINES above a matched line, for
//! `LineScan::enclosing_call_exclude_pattern`.
//!
//! ## Why upward, and why a new walk rather than the existing one
//!
//! `call_window` walks `idx + 1 + taken` — strictly downward from a site whose callee it holds in hand.
//! The question here is the mirror: not "what does this call's own argument list say" but "what
//! expression is this line an ARGUMENT OF". Nothing of the downward machinery survives the flip. There
//! is no callee to anchor on (the enclosing call's name is what we are looking FOR), the paren count has
//! to run right-to-left with a pending-CLOSER stack instead of a depth counter, and its early exit means
//! the opposite thing: downward, an early exit means "this call closed"; upward, it means "found an
//! opener nobody closed". What DOES survive is the masker, the comment-leader predicate, and — load
//! bearing — the `Option<String>` convention: every ambiguity comes back as `None`, the caller reads
//! that as NO suppression, and the site keeps firing.
//!
//! ## The window is opener LINES, never the text between them
//!
//! This is what makes a 40-line reach safe where `next_line_exclude_pattern`'s doc says an unbounded
//! walk is how a veto goes body-scoped. The window is not a span of source: a line enters it only by
//! carrying a bracket that is STILL OPEN at the site. Every intervening statement is dropped, and so is
//! every group the walk watches close — a `])` or `})` seen on the way up consumes the matching opener
//! above it, so the walk can never climb out of one statement into the previous one. A veto keyword
//! cannot hide in text that never enters the window.
//!
//! EVERY still-unclosed opener is collected, not just the innermost one: the immediately enclosing
//! opener of a `create({` element inside `$transaction([ ... ])` is the array `[`, and a walk reporting
//! only that would never see the callee the veto is written against. They are joined with `\n` and the
//! field is compiled with a `(?m)` prefix, so an author's `^`/`$` still anchors per opener line.
//!
//! ## What the walk refuses to do (all four refusals fail toward the site FIRING)
//!
//! 1. **Multi-line template literals.** `string_mask`'s own contract leaves a string opened but not
//!    closed on a line UNMASKED to end-of-line, so the interior lines of a `` ` ``…`` ` `` SQL template
//!    arrive as raw code and a `WHERE id IN (` reads as an unclosed opener. Backtick parity is tracked
//!    across the whole walk and the moment a line leaves it odd, the walk declines.
//! 2. **Comments.** Locating where a leader actually starts is exactly what [`super::has_comment_leader`]
//!    refuses to do (`//` is Python floor division, `#` a JS private field, `--` a JS decrement), and
//!    upward the leader sits mid-line with CODE IN FRONT of it, so the downward trick of "include this
//!    line and stop" cannot apply. Blanket-declining costs real suppressions (2 of cal.com's 13
//!    `$transaction` elements sit under a prose comment), so there is exactly one carve-out: a line whose
//!    `trim_start` begins with a leader AND which carries no `(){}[]` at all contributes nothing and is
//!    stepped over. Any other leader-bearing line declines.
//! 3. **Mismatched brackets.** A closer that does not match the opener the walk meets means the text the
//!    masker produced is not something this pass can read as nesting (a regex literal's stray quote
//!    swallowing real code is the measured way to get there), so it declines rather than guessing.
//! 4. **The cap.** [`MAX_ENCLOSING_WALK_LINES`] bounds the reach; an opener further up is out of reach
//!    and the site keeps firing. That residual belongs in the consuming rule's message.
//!
//! One inherited residual is NOT fixed here and is disclosed instead: `string_mask`'s regex-literal
//! over-masking can blank a real `(`. The direction of that error is "an opener disappears", i.e. no
//! suppression — the same direction as every refusal above.

/// How far above the site the walk may look. Headroom over the farthest opener→finding distance
/// measured on cal.com @ `b25beb5` (23 lines, `tokens.repository.ts:100 -> :123`) and far short of a
/// function body, which is the distance at which a lexical veto starts speaking for code it has no
/// claim on. The risk this number carries is bounded by construction rather than by its size: only
/// OPENER lines enter the window (see the module doc), so widening the reach adds candidate lines, not
/// candidate TEXT.
const MAX_ENCLOSING_WALK_LINES: usize = 40;

/// The still-unclosed opener lines above `lines[idx]`, outermost first, joined with `\n`.
///
/// `None` means the walk DECLINED — no such line, nothing above it, or one of the four refusals in the
/// module doc — and the caller must read it as NO suppression. `Some("")` is the honest answer for a
/// site the walk reached the top (or the cap) of without finding any opener still open.
pub(in crate::dsl) fn enclosing_window(lines: &[&str], idx: usize) -> Option<String> {
    let _ = lines.get(idx)?;
    // Start at `idx - 1`: the matched line ends with its OWN unclosed openers (`x.create({`), which
    // describe the site rather than what encloses it.
    let mut at = idx.checked_sub(1)?;
    // Closers seen but not yet matched, innermost last. Carried ACROSS lines — a `}),` on one line is
    // what consumes the `x.create({` two lines above it.
    let mut pending: Vec<u8> = Vec::new();
    let mut openers: Vec<&str> = Vec::new();
    let mut backticks = 0usize;
    for _ in 0..MAX_ENCLOSING_WALK_LINES {
        let raw = lines[at];
        let masked = super::mask(raw);
        // Backtick parity first: inside a multi-line template every other reading of this line is
        // fiction. A closing `` ` `` leaves the running count odd, which is the flip this catches.
        backticks += masked.bytes().filter(|b| *b == b'`').count();
        if backticks % 2 == 1 {
            return None;
        }
        if super::has_comment_leader(&masked) {
            if !is_bracket_free_comment_line(&masked) {
                return None;
            }
        } else if scan_line_upward(&masked, &mut pending)? {
            openers.push(raw);
        }
        let Some(next) = at.checked_sub(1) else {
            break; // top of file reached within the cap
        };
        at = next;
    }
    openers.reverse(); // outermost first, so the joined window reads top-to-bottom like the source
    Some(openers.join("\n"))
}

/// A line that is ENTIRELY a comment and carries no bracket at all: it can neither open nor close a
/// group, so the walk steps over it. The `trim_start` test is what keeps this narrow — a leader with
/// code in front of it is the ambiguous case the module doc declines on.
fn is_bracket_free_comment_line(masked: &str) -> bool {
    let t = masked.trim_start();
    ["//", "#", "--", "/*"].iter().any(|l| t.starts_with(l))
        && !t.contains(['(', ')', '[', ']', '{', '}'])
}

/// Scans one masked line RIGHT TO LEFT, matching closers against openers through `pending`, and reports
/// whether this line carries at least one opener still unclosed at the site.
///
/// `None` on a mismatched pair — refusal 3 in the module doc.
fn scan_line_upward(masked: &str, pending: &mut Vec<u8>) -> Option<bool> {
    let mut opens_here = false;
    for b in masked.bytes().rev() {
        match b {
            b')' | b']' | b'}' => pending.push(b),
            b'(' | b'[' | b'{' => match pending.pop() {
                None => opens_here = true, // nothing below closed it -> still open at the site
                Some(closer) => {
                    if closer != matching_closer(b) {
                        return None;
                    }
                }
            },
            _ => {}
        }
    }
    Some(opens_here)
}

fn matching_closer(open: u8) -> u8 {
    match open {
        b'(' => b')',
        b'[' => b']',
        _ => b'}',
    }
}
