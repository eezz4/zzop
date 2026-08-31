//! `call_window`'s contract, including the three shapes that made the first version of it waive real
//! findings using text belonging to a different call.

use super::call_window;

fn win(src: &str, line_idx: usize, callee: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    call_window(&lines, line_idx, callee).expect("the line exists")
}

/// The floor: an ordinary single-line call yields exactly its line, byte for byte. This is what the
/// field read before it had a window at all, so every rule using it keeps its old behaviour here.
#[test]
fn a_single_line_call_is_the_line_and_nothing_else() {
    let src = "import hashlib\nh = hashlib.md5(payload).hexdigest()\nother = 1\n";
    assert_eq!(
        win(src, 1, "hashlib.md5"),
        "h = hashlib.md5(payload).hexdigest()"
    );
}

/// The case the window exists for: a formatter wrapped the call, so the declaration sits on a
/// continuation line INSIDE the call's own parentheses.
#[test]
fn a_wrapped_call_reaches_its_own_continuation_lines() {
    let src = "h = hashlib.md5(\n    data, usedforsecurity=False\n).hexdigest()\nx = 1\n";
    let w = win(src, 0, "hashlib.md5");
    assert!(w.contains("usedforsecurity=False"), "{w}");
    assert!(
        !w.contains("x = 1"),
        "the window must stop when the call closes: {w}"
    );
}

/// A `(` inside a STRING must not open the window — the first version counted it and let the NEXT
/// line's unrelated declaration waive this call.
#[test]
fn a_paren_inside_a_string_literal_does_not_open_the_window() {
    let src = "key = hashlib.md5(f\"{a}(\").hexdigest()\nother = hashlib.sha1(b, usedforsecurity=False)\n";
    let w = win(src, 0, "hashlib.md5");
    assert!(!w.contains("usedforsecurity"), "{w}");
}

/// An ENCLOSING call's open paren must not open the window either: the anchor is the site's own callee,
/// so the count starts after `hashlib.md5` and ends when THAT call closes.
#[test]
fn an_enclosing_calls_paren_does_not_open_the_window() {
    let src =
        "log.info(\"d %s\", hashlib.md5(x).hexdigest(),\n         extra=dict(usedforsecurity=False))\n";
    let w = win(src, 0, "hashlib.md5");
    assert!(!w.contains("usedforsecurity"), "{w}");
}

/// A comment's paren is the third shape of the same defect, and the callee anchor closes it too.
#[test]
fn a_paren_in_a_trailing_comment_does_not_open_the_window() {
    let src = "digest = hashlib.md5(payload)  # legacy hash (see PROJ-123\nx = 1\ny = 2\nsalted = hashlib.md5(z, usedforsecurity=False)\n";
    let w = win(src, 0, "hashlib.md5");
    assert!(!w.contains("usedforsecurity"), "{w}");
}

/// Two calls to the SAME callee on one line: this pass cannot tell which occurrence the site is, so it
/// declines and reads the line alone. Declining costs a waiver; guessing would cost a finding.
#[test]
fn two_calls_to_one_callee_on_a_line_fall_back_to_the_line() {
    let src = "return hashlib.md5(payload).hexdigest() + hashlib.md5(\n    salt, usedforsecurity=False\n).hexdigest()\n";
    let w = win(src, 0, "hashlib.md5");
    assert_eq!(w, "return hashlib.md5(payload).hexdigest() + hashlib.md5(");
    assert!(!w.contains("usedforsecurity"), "{w}");
}

/// A callee the line does not carry (an envelope run supplies no source text, or the callee is spelled
/// differently from the source) also falls back to the line rather than guessing.
#[test]
fn a_callee_the_line_does_not_carry_falls_back_to_the_line() {
    let src = "some_other_call(\n    usedforsecurity=False\n)\n";
    assert_eq!(win(src, 0, "hashlib.md5"), "some_other_call(");
}

/// The cap is real and it is the conservative direction: a declaration further inside the call than
/// `MAX_CALL_WINDOW_LINES` is out of reach, so the site keeps firing.
#[test]
fn the_window_is_capped_and_the_residual_is_that_the_site_keeps_firing() {
    let mut src = String::from("h = hashlib.md5(\n");
    for i in 0..9 {
        src.push_str(&format!("    arg{i},\n"));
    }
    src.push_str("    usedforsecurity=False\n)\n");
    let w = win(&src, 0, "hashlib.md5");
    assert!(!w.contains("usedforsecurity"), "{w}");
    assert_eq!(
        w.lines().count(),
        9,
        "site line plus the eight-line cap: {w}"
    );
}

/// Out of range is `None`, which the caller must read as NO suppression.
#[test]
fn a_line_the_text_does_not_have_is_none() {
    let lines = vec!["a", "b"];
    assert!(call_window(&lines, 9, "x").is_none());
}

/// The multi-line half of the comment-paren defect, which the callee anchor does NOT close: an
/// unbalanced `(` in a comment used to inflate the depth so the window ran past this call's own `)` and
/// read the NEXT call's declaration. Reproduced on a real tree before the fix — `weak-crypto` reported
/// 0 where it must report the md5.
#[test]
fn an_unbalanced_paren_in_a_comment_cannot_extend_the_window_past_the_call() {
    let src = "digest = hashlib.md5(  # not for auth (see PROJ-1\n    payload\n).hexdigest()\nother = hashlib.sha1(payload, usedforsecurity=False)\n";
    let w = win(src, 0, "hashlib.md5");
    assert!(!w.contains("usedforsecurity"), "{w}");
    assert_eq!(w.lines().count(), 1, "the window ends at the comment: {w}");
}

/// Same defect one line in: the comment sits on a CONTINUATION line, so the window must include that
/// line and stop, never reaching the following call.
#[test]
fn a_comment_on_a_continuation_line_ends_the_window_there() {
    let src = "h = hashlib.md5(\n    payload,  # TODO(bob: this is legacy\n    salt\n).hexdigest()\nother = hashlib.sha1(b, usedforsecurity=False)\n";
    let w = win(src, 0, "hashlib.md5");
    assert!(!w.contains("usedforsecurity"), "{w}");
    assert!(w.contains("TODO"), "the carrying line is included: {w}");
    assert_eq!(w.lines().count(), 2, "{w}");
}

/// The language-blind cost, pinned so it is a decision and not a surprise: Python's floor division and
/// JS's decrement/private-field spellings are read as possible comment leaders, so a call wrapped across
/// lines containing one loses its suppression and the site keeps firing. That is the safe direction.
#[test]
fn a_non_comment_leader_shrinks_the_window_rather_than_growing_it() {
    for line in ["    a // b,", "    i--,", "    obj.#x,"] {
        let src = format!("h = hashlib.md5(\n{line}\n    usedforsecurity=False\n)\n");
        let w = win(&src, 0, "hashlib.md5");
        assert!(
            !w.contains("usedforsecurity"),
            "must fail toward firing: {w}"
        );
    }
}

// --- `enclosing_window`: the UPWARD walk (`LineScan::enclosing_call_exclude_pattern`) ---

use super::enclosing_window;

fn enc(src: &str, line_idx: usize) -> String {
    let lines: Vec<&str> = src.lines().collect();
    enclosing_window(&lines, line_idx).expect("the walk placed a window")
}

/// `None` and "no opener found" both mean NO suppression at the caller, so the negative assertions
/// flatten them into the same empty text rather than pinning which one a shape produces.
fn enc_or_empty(src: &str, line_idx: usize) -> String {
    let lines: Vec<&str> = src.lines().collect();
    enclosing_window(&lines, line_idx).unwrap_or_default()
}

/// U1 — the floor: an element directly under the opener sees the opener.
#[test]
fn an_element_one_line_under_the_opener_sees_it() {
    let src = "await prisma.$transaction([\n  ctx.user.create({ data }),\n]);\n";
    let w = enc(src, 1);
    assert!(w.contains("$transaction(["), "{w}");
}

/// U2 — the test that must exist or the fix looks done while half-working: the SECOND element, whose
/// previous line is the first element's `}),`. A walk that does not consume a closed group here either
/// reports `ctx.a.create({` as still open or gives up before reaching the array.
#[test]
fn the_second_element_after_a_closing_brace_paren_still_sees_the_opener() {
    let src = "await prisma.$transaction([\n  ctx.a.create({\n    data: { id },\n  }),\n  ctx.b.create({ data }),\n]);\n";
    let w = enc(src, 4);
    assert!(w.contains("$transaction(["), "{w}");
    assert!(
        !w.contains("ctx.a.create"),
        "a group closed by `}}),` must not be reported as an opener: {w}"
    );
}

/// U3 — the measured extremes: cal.com's farthest opener->finding distance is 23 lines (its
/// `tokens.repository.ts:100 -> :123` shape), and one line past the cap is the disclosed residual.
#[test]
fn the_walk_reaches_the_measured_far_opener_and_stops_one_line_past_the_cap() {
    let far = |gap: usize| {
        let mut s = String::from("await prisma.$transaction([\n");
        for i in 0..gap - 1 {
            s.push_str(&format!("  arg{i},\n"));
        }
        s.push_str("  ctx.b.create({ data }),\n");
        s
    };
    assert!(
        enc(&far(23), 23).contains("$transaction(["),
        "23 is measured, not chosen"
    );
    let past = enc_or_empty(&far(41), 41);
    assert!(
        !past.contains("$transaction("),
        "past MAX_ENCLOSING_WALK_LINES the opener is out of reach and the site keeps firing: {past}"
    );
}

/// U4 — a whole-line, bracket-free comment contributes nothing and is stepped over. Blanket-declining
/// on every leader-bearing line costs 2 of cal.com's 13 `$transaction` elements.
#[test]
fn a_whole_line_bracket_free_comment_does_not_end_the_walk() {
    let src = "await prisma.$transaction([\n  // Simply remove this update when we remove the field\n  ctx.b.update({ data }),\n]);\n";
    assert!(enc(src, 2).contains("$transaction(["));
}

/// U5 — the other half of the same rule: a MID-LINE comment sits with code in front of it, and finding
/// where the leader really starts is exactly what `has_comment_leader`'s module refuses to do. An
/// unbalanced `(` inside such a tail would open a phantom opener, so the walk declines.
#[test]
fn a_mid_line_comment_carrying_an_unbalanced_paren_declines() {
    let src = "await prisma.$transaction([\n  first, // see PROJ-1 (legacy\n  ctx.b.update({ data }),\n]);\n";
    let lines: Vec<&str> = src.lines().collect();
    assert!(enclosing_window(&lines, 2).is_none());
}

/// U6 — a bracket inside a CLOSED string literal is masked and is not an opener.
#[test]
fn a_bracket_inside_a_closed_string_literal_is_not_an_opener() {
    let src = "await outer(\n  log(\"array [ here\"),\n  ctx.b.create({ data }),\n";
    let w = enc(src, 2);
    assert!(w.contains("await outer("), "{w}");
    assert_eq!(
        w.lines().count(),
        1,
        "the string's `[` must not enter the window: {w}"
    );
}

/// U7 — the refusal `string_mask` forces: a string opened but not closed on a line is left UNMASKED to
/// end-of-line, so a multi-line template's interior arrives as raw code and a `WHERE id IN (` reads as
/// an unclosed opener. Backtick parity catches the closing line and the whole walk declines.
#[test]
fn a_multi_line_template_literal_declines_rather_than_reading_its_interior_as_code() {
    let src = "const sql = `\n  SELECT * FROM t\n  WHERE id IN (\n`;\nprisma.x.create({ data });\n";
    let lines: Vec<&str> = src.lines().collect();
    assert!(enclosing_window(&lines, 4).is_none());
}

/// U8 — the immediately enclosing opener of a `create({` element is the inner `[`, not `$transaction(`.
/// A walk reporting only the innermost opener would never see the callee the veto is written against,
/// so EVERY still-unclosed opener line enters the window.
#[test]
fn every_still_open_opener_line_enters_the_window_not_just_the_innermost() {
    let src = "await prisma.$transaction([\n  [\n    ctx.a.create({ data }),\n  ],\n]);\n";
    let w = enc(src, 2);
    assert!(
        w.contains("$transaction(["),
        "the OUTER opener is the one the veto reads: {w}"
    );
    assert_eq!(w.lines().count(), 2, "both unclosed openers: {w}");
}

/// U9 — a site on the first line of a file has nothing above it. No panic, no wrap.
#[test]
fn a_site_on_the_first_line_of_a_file_has_nothing_above_it() {
    let lines = vec!["prisma.x.create({ data });"];
    assert!(enclosing_window(&lines, 0).is_none());
}

/// U10 — out of range is `None` for the upward walk too, which the caller must read as NO suppression.
#[test]
fn a_line_the_text_does_not_have_is_none_for_the_upward_walk_too() {
    let lines = vec!["a", "b"];
    assert!(enclosing_window(&lines, 9).is_none());
}

// --- `trigger_call_excluded`: the OFFSET anchor (`MethodScan::trigger_call_exclude_pattern`) ---

use super::trigger_call_excluded;

/// The caller's exact call shape: `scan` is byte-length-identical to `lines[idx]`, and the veto is
/// compiled MULTILINE the way `Diagnostics::compile_opt_multiline` compiles it.
fn tce(src: &str, idx: usize, trigger: &str, veto: &str) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    let t = regex::Regex::new(trigger).expect("test trigger compiles");
    let v = regex::Regex::new(&format!("(?m){veto}")).expect("test veto compiles");
    trigger_call_excluded(Some(&v), &t, &lines, idx, lines[idx])
}

/// T1 — the OFFSET anchor's twin of `two_calls_to_one_callee_on_a_line_fall_back_to_the_line`, and the
/// arm that has to be pinned or the whole field degrades quietly: a line carrying TWO trigger matches
/// yields ONE finding, so there is no "which one" the caller could have meant, and guessing the first
/// would let the FIRST call's mitigator waive the SECOND, unmitigated call. That is not hypothetical —
/// commit `addfb6a` shipped exactly that mutant on a neighbouring veto, where one call's declaration
/// waived the next call's finding. Declining means NO suppression: both sites keep firing.
///
/// The control below is what makes the assertion about the DECLINE rather than about the veto simply
/// not matching: the same veto text, on a line with ONE trigger match, does suppress.
#[test]
fn two_trigger_matches_on_a_line_decline_the_window_and_leave_the_site_firing() {
    let trigger = r"\bredirect\s*\(";
    let veto = r"safeRedirectUrl\s*\(";
    assert!(
        !tce(
            "res.redirect(safeRedirectUrl(a)); res.redirect(rawTarget);\n",
            0,
            trigger,
            veto
        ),
        "a second, unmitigated redirect on the line must not be waived by the first one's mitigator"
    );
    assert!(
        tce("res.redirect(safeRedirectUrl(a));\n", 0, trigger, veto),
        "control: one trigger match, same veto text, suppresses"
    );
}

/// T2 — `match_open_paren`'s non-whitespace-gap decline, the offset twin of `call_open_paren`'s (pinned
/// at the top of this file). A trigger that carries no `(` of its own can have the NEXT call's paren
/// sitting after it on the line; opening the window there would count a window that belongs to a
/// different expression and let its text waive this site. Only whitespace may separate the match from
/// its own `(`, so this shape reads the line alone — and the mitigator two lines down stays out of
/// reach, leaving the site firing.
#[test]
fn a_non_whitespace_gap_between_the_match_and_a_paren_declines_the_window() {
    let trigger = r"req\.query";
    let veto = r"safeRedirectUrl\s*\(";
    assert!(
        !tce(
            "res.redirect(req.query.next, other(\n  safeRedirectUrl(x)\n));\n",
            0,
            trigger,
            veto
        ),
        "`, other(` is a different expression's paren and must not open this site's window"
    );
    assert!(
        tce(
            "res.redirect(req.query (\n  safeRedirectUrl(x)\n));\n",
            0,
            trigger,
            veto
        ),
        "control: whitespace-only gap, same text, opens the window and suppresses"
    );
}

/// T3 — the unset field. `None` is always FALSE: no field, no waiver.
#[test]
fn an_unset_veto_never_suppresses() {
    let lines = vec!["res.redirect(safeRedirectUrl(a));"];
    let t = regex::Regex::new(r"\bredirect\s*\(").expect("test trigger compiles");
    assert!(!trigger_call_excluded(None, &t, &lines, 0, lines[0]));
}
