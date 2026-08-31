//! `security/open-redirect`'s AUTHORITY GATE — the second arm of `trigger_call_exclude_pattern`,
//! beside the mitigator arm that `open_redirect_veto.rs` owns. Same field, same window, same anchor;
//! a separate module because the two arms are argued from different evidence and the repo pairs one
//! test file to one argument.
//!
//! WHAT THE ARM CLAIMS, and why it is a rule rather than a corpus observation: the rule's own message
//! used to admit that "a handler that reads a query parameter for anything at all while redirecting to
//! a CONSTANT PATH fires the same way" — a residual it described and did not implement. It is
//! implementable without reading a single value, because a URL's AUTHORITY is decided by the FIRST
//! characters of the reference and by nothing after them. So the discriminator is not "is this target
//! free of request-derived reads" (which is unspellable here — the `regex` crate has no lookaround and
//! the field is exclude-only — and would be a deleting-direction inference besides). It is: DOES THE
//! LITERAL FRAGMENT THAT OPENS THE TARGET PIN THE AUTHORITY?
//!
//! Two fragments do, and the declaration is the fragment's own SHAPE, spelled in the scanned source:
//!   * it starts `/` and its SECOND character is neither `/` nor `\` — an absolute-path reference,
//!     resolved against the current origin whatever is appended to it;
//!   * it already carries `?` or `#`, both of which TERMINATE the authority component, so no text
//!     after one can reach back into it.
//!
//! The `//` case is the exception that makes this subtle and it has its own test below: `//host` is a
//! PROTOCOL-RELATIVE reference and moves the origin, so `'/' + input` — one character, closing quote,
//! concatenation — must keep firing. So must `'/\' + input`, since a leading `/\` is read as `//` by
//! browsers. Both are the reason the arm counts the second character rather than testing `starts_with`.
//!
//! WHAT IT DELIBERATELY DOES NOT REACH: a fragment held in a NAME rather than written into the call.
//! Resolving `const base = "/orders/"` to its value is local const-resolution — a one-site dataflow
//! inference this layer does not do — so that site keeps firing, and the rule message says so. The
//! test at the bottom pins that silence as a CHOICE.
//!
//! THE PIN THAT IS NOT HERE: `open_redirect_veto.rs`'s `paymentCallback` regression test is what
//! guards this arm from the other direction. That site's target is `callbackUrl.toString()` — a call,
//! not a literal — and an arm that reached it would take a TRUE POSITIVE with it. It is red with this
//! arm and red without, so it is left where it is rather than duplicated.

use crate::{hits, scan, TempDir};

/// The shape the corpus writes most: an absolute-path template whose first fragment is a whole path
/// AND carries `?`, so both halves of the rule agree. One file per shape so a shape that stops being
/// vetoed names itself instead of moving a count.
#[test]
fn a_target_opening_with_a_literal_that_pins_the_authority_is_vetoed() {
    let dir = TempDir::new("zzop-be-sec");
    let head = "declare const res: any;\nexport function handleCallback(req: any) {\n  const body = req.query.state;\n  res.redirect(";
    let shapes = [
        // template, path fragment then a `?` then an interpolation
        ("tpl-path-query.ts", "`/settings/profile?error=${body}`"),
        // template, path fragment ALONE — the authority is pinned before the first `${`
        ("tpl-path-only.ts", "`/orders/${body}`"),
        // template with no interpolation at all
        ("tpl-constant.ts", "`/settings/profile`"),
        // quoted literal concatenated with a request value: the `+` is this shape's interpolation
        ("concat-single.ts", "'/user/' + req.query.id"),
        ("concat-double.ts", "\"/user/\" + req.query.id"),
        // quoted constant
        ("quoted-constant.ts", "\"/settings/profile\""),
        // no leading `/`, but the fragment has already passed a `?`
        ("query-first.ts", "'?next=' + req.query.next"),
        // an ABSOLUTE url whose authority the `?` has already closed
        (
            "absolute-past-query.ts",
            "'https://example.invalid/x?next=' + req.query.next",
        ),
    ];
    for (name, target) in shapes {
        dir.write(name, &[head, target, ");\n}\n"].concat());
    }
    let out = scan(&dir);
    assert!(hits(&out, "open-redirect").is_empty(), "{:?}", out.findings);
}

/// The line the whole arm turns on. Every one of these OPENS with a literal and NONE of them pins the
/// authority, so all of them are real open redirects and all of them must survive the gate.
///
/// `'/' + input` is the case a `starts_with('/')` reading gets wrong: `input = "/evil.example"` makes
/// the value `//evil.example`, a PROTOCOL-RELATIVE reference, and the browser's origin moves. `'/\'`
/// is the same hole spelled with the separator browsers normalise, and `'https://' + host` is the
/// plain concatenation of an authority. The fourth is the mirror of the vetoed template above: when
/// the interpolation comes FIRST there is no fragment at all to pin anything.
#[test]
fn a_literal_that_does_not_pin_the_authority_still_fires() {
    let dir = TempDir::new("zzop-be-sec");
    let head = "declare const res: any;\nexport function handleCallback(req: any) {\n  const next = req.query.next;\n  res.redirect(";
    let shapes = [
        ("slash-only.ts", "'/' + next"),
        ("protocol-relative.ts", "'//' + next"),
        ("tpl-protocol-relative.ts", "`//${next}/apps`"),
        ("backslash.ts", "'/\\\\' + next"),
        ("scheme.ts", "'https://' + next"),
        ("tpl-interpolation-first.ts", "`${next}?error=denied`"),
    ];
    let mut want: Vec<&str> = shapes.iter().map(|(n, _)| *n).collect();
    for (name, target) in shapes {
        dir.write(name, &[head, target, ");\n}\n"].concat());
    }
    let out = scan(&dir);
    let mut fired: Vec<&str> = hits(&out, "open-redirect")
        .iter()
        .map(|f| f.file.as_str())
        .collect();
    fired.sort_unstable();
    want.sort_unstable();
    assert_eq!(fired, want, "{:?}", out.findings);
}

/// A vetoed trigger is NOT A HIT rather than a suppressed finding, so a second `redirect(` in the same
/// body still supplies the finding's line. Measured on the corpus: of the sites this arm cleared, two
/// bodies RE-ANCHORED this way and kept their finding while six lost theirs entirely. Pinning it here
/// means a future arm that starts deleting whole bodies has to explain itself.
#[test]
fn a_body_whose_other_redirect_is_not_pinned_re_anchors_rather_than_going_silent() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/callback.ts",
        "declare const res: any;\nexport function handleCallback(req: any) {\n  const state = req.query.state;\n  if (!state) {\n    return res.redirect(`/settings/profile?error=${state}`);\n  }\n  res.redirect(state.returnTo);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "open-redirect");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(
        h[0].line, 7,
        "the finding must move to the UNPINNED call, not vanish"
    );
}

/// THE DISCLOSED LIMIT, pinned as a choice rather than left to be discovered. The fragment is the same
/// `/orders/` the test above vetoes; the only difference is that it is held in a name. Reading that
/// name's value is local const-resolution, and this layer does not do it — so the site fires, and the
/// rule message names the case. If this ever goes silent, the arm has started resolving identifiers
/// and the message's claim is stale.
#[test]
fn a_pinning_fragment_held_in_a_name_is_out_of_reach_and_still_fires() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/callback.ts",
        "declare const res: any;\nconst base = \"/orders/\";\nexport function handleCallback(req: any) {\n  res.redirect(base + req.query.uid);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "open-redirect");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

/// §27 (`.claude` decision `rule-quality.md`): the clause that DISQUALIFIES a finding has to arrive
/// BEFORE the imperative, because a reader who acts on the first instruction never reaches what is
/// placed behind it. This rule was on the measured 23-rule violation list — its co-occurrence
/// disclaimer sat at byte 1949 while "Validate the target against an allow-list" sat at byte 262.
///
/// POSITION, not presence. A `contains` loop stays GREEN with every clause shoved back behind the
/// remedy, which is how the ordering defect survives a green suite. THE INVALIDATION PROBE for the
/// assertions below is exactly that move: put the four clauses back after the imperative and this test
/// must go red with all four tokens, and the imperative, still present and still spelled exactly once.
#[test]
fn the_clauses_that_disqualify_a_finding_precede_the_remedy() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/callback.ts",
        "declare const res: any;\nexport function handleCallback(req: any) {\n  res.redirect(req.query.next);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "open-redirect");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;

    const IMPERATIVE: &str = "Validate the target against an allow-list of known internal origins";
    assert_eq!(
        m.matches(IMPERATIVE).count(),
        1,
        "the imperative must be spelled ONCE, or an index comparison against it means nothing: {m}"
    );
    let imperative = m
        .find(IMPERATIVE)
        .expect("the remedy must survive verbatim");

    for (role, clause) in [
        (
            "names the heuristic as a heuristic",
            "CO-OCCURRENCE heuristic",
        ),
        (
            "disqualifies the finding",
            "MAY HAVE NOTHING TO DO WITH the redirect's target",
        ),
        ("states the gate that now exists", "PINS THE AUTHORITY"),
        ("states the gate that now exists", "DOES NOT FIRE AT ALL"),
        (
            "states the gate that now exists",
            "HAS TERMINATED THE AUTHORITY",
        ),
    ] {
        let at = m
            .find(clause)
            .unwrap_or_else(|| panic!("clause {clause:?} left the message entirely: {m}"));
        assert!(
            at < imperative,
            "the clause that {role} sits at byte {at}, AFTER the imperative at byte {imperative}: \
             a reader who edits on the first instruction never reaches it. Move the clause, do not \
             rewrite it — {clause:?} in: {m}"
        );
    }

    // The gate exists now, so the message must no longer describe it as an unbuilt residual.
    assert!(
        !m.contains("while redirecting to a constant path fires the same way"),
        "the residual sentence outlived the gate that closed it: {m}"
    );
    // ...and the limit the gate really does have must be stated instead.
    assert!(
        m.contains("The authority gate has a limit of the same kind"),
        "the gate's own disclosed limit (a fragment held in a name) left the message: {m}"
    );
}
