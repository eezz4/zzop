//! `security/open-redirect`'s MITIGATOR VETO — the `trigger_call_exclude_pattern` arm.
//!
//! A separate module from `request_targets.rs` (which owns the rule's base positives/negatives) for the
//! repo's 300-line-per-file cap, not because the rule is two rules.
//!
//! What these cases pin, measured on `cal.com` @ the corpus snapshot: 37 `res.redirect(` sites there
//! have their argument opened by `getSafeRedirectUrl(...)`, cal.com's ORIGIN ALLOWLIST
//! (`packages/lib/getSafeRedirectUrl.ts` — a non-listed origin is discarded and replaced with
//! `WEBAPP_URL`). ONE has the mitigator on the anchor line; the other THIRTY-SIX have it on a CONTINUATION
//! line, inside the still-open `redirect(` — which is why the veto reads the call's downward WINDOW
//! (`zzop_core::dsl::veto_window`) and not the anchor line. The veto suppresses 17 TRIGGER MATCHES,
//! which is a different count from the 37 sites because a span reports once. A line-local veto would
//! have cleared 1 of
//! the rule's 34 corpus findings; this one takes cal.com from 28 to 15 (16 anchors cleared, 3 findings
//! RE-ANCHORED onto a second, unmitigated `redirect(` in the same function) and leaves express's 4 and
//! nocodb's 2 untouched.
//!
//! THE OTHER HALF, which cost nothing on the corpus and is the reason half these cases exist: the veto
//! is a WHOLE-VALUE claim, not a position. A pattern that stopped at the mitigator's own `(` proved
//! only that the helper opened the argument, and waived `redirect(getSafeRedirectUrl(base) + req.query.q)`
//! — a real open redirect, since `q = "@evil.com"` makes the browser read the allowlisted prefix as
//! userinfo. So the pattern also spells the value's closure: `??`/`||` fallbacks that are themselves
//! calls or quoted literals, or a template literal whose text after the helper begins with `?`/`#`
//! (both terminate a URL's authority), and then the call's own `)` or `,`. Measured: cal.com stays at
//! the same 15 findings, by (file,line), with the closure added.

use crate::{hits, scan, TempDir};

/// The 1-of-17 shape: mitigator and anchor on one line. `stripepayment/api/callback.ts:17` verbatim.
#[test]
fn a_mitigator_wrapping_the_whole_redirect_value_on_the_anchor_line_is_vetoed() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/callback.ts",
        "declare const res: any;\ndeclare function getSafeRedirectUrl(u: unknown): string | undefined;\nexport function handleCallback(req: any) {\n  const state = req.query.state;\n  return res.redirect(getSafeRedirectUrl(state) ?? \"/apps/installed/payment\");\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "open-redirect").is_empty(), "{:?}", out.findings);
}

/// THE case that decides a line-local veto against a call-window one: the 15-of-17 shape, where a
/// formatter pushed the mitigator onto the next line and the `redirect(` above it is still open.
/// `googlecalendar`/`dub`/`office365*`/`zoho*`/`webex`/... all carry exactly this.
#[test]
fn a_mitigator_on_the_next_line_inside_the_still_open_redirect_call_is_vetoed() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/callback.ts",
        "declare const res: any;\ndeclare function getSafeRedirectUrl(u: unknown): string | undefined;\nexport function handleCallback(req: any) {\n  const state = req.query.state;\n  res.redirect(\n    getSafeRedirectUrl(state) ??\n      \"/apps/installed\"\n  );\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "open-redirect").is_empty(), "{:?}", out.findings);
}

/// The raw-value-FIRST half of the closure: a request value concatenated AHEAD of the mitigator leaves
/// the site firing. Its mirror — a raw value after the mitigator — is the test below, and the two
/// together are what keep the veto from degrading into "a safe-sounding name appears near the call".
#[test]
fn a_mitigator_concatenated_after_a_raw_value_still_fires() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/callback.ts",
        "declare const res: any;\ndeclare function getSafeRedirectUrl(u: unknown): string;\nexport function handleCallback(req: any) {\n  res.redirect(req.query.base + getSafeRedirectUrl(\"/apps/installed\"));\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "open-redirect");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

/// THE CLOSURE, in the direction a positional-only veto gets wrong — and the reason the pattern does
/// not stop at the mitigator's own `(`. All four of these are REAL open redirects: the allowlisted
/// origin is only the PREFIX of the value, and a suffix that does not begin with `?` or `#` can finish
/// the authority the prefix started. With `q = "@evil.com"` everything before the `@` is read as
/// USERINFO and the browser's authority becomes `evil.com`; with `q = ".evil.com"` the appended text
/// completes a hostname instead. A veto proving only that the mitigator OPENS the argument cleared all
/// four (measured against the shipped pattern before 2026-08-22), which is a veto-induced FALSE
/// NEGATIVE — the one direction a suppression must never fail in.
///
/// One file per shape, so a shape that starts being silenced names itself instead of moving a count.
#[test]
fn a_raw_value_after_the_mitigator_in_the_same_target_still_fires() {
    let head = "declare const res: any;\ndeclare function getSafeRedirectUrl(u: unknown): string;\nexport function handleCallback(req: any) {\n  const base = \"/apps/installed\";\n  res.redirect(";
    let shapes = [
        ("concat.ts", "getSafeRedirectUrl(base) + req.query.q"),
        (
            "concat-const.ts",
            "getSafeRedirectUrl(base) + \"?next=\" + req.query.q",
        ),
        (
            "template.ts",
            "`${getSafeRedirectUrl(base)}${req.query.next}`",
        ),
        ("nullish.ts", "getSafeRedirectUrl(base) ?? req.query.next"),
    ];
    let dir = TempDir::new("zzop-be-sec");
    for (name, target) in shapes {
        dir.write(name, &[head, target, ");\n}\n"].concat());
    }
    let out = scan(&dir);
    let mut fired: Vec<&str> = hits(&out, "open-redirect")
        .iter()
        .map(|f| f.file.as_str())
        .collect();
    fired.sort_unstable();
    let mut want: Vec<&str> = shapes.iter().map(|(n, _)| *n).collect();
    want.sort_unstable();
    assert_eq!(fired, want);
}

/// The one suffix that IS safe, and why it is a rule rather than a corpus observation: `?` (and `#`)
/// TERMINATE the authority component of a URL, so no text after one can move the origin, whatever it
/// was built from. `googlecalendar/api/callback.ts:140` verbatim — the mitigator is interpolated at the
/// START of a template literal and a `?error=` query string follows it. The RAW half of the same shape
/// — a request value opening the template — is not mitigated, and still fires.
#[test]
fn a_mitigator_opening_a_template_literal_target_is_vetoed_but_a_raw_value_there_is_not() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/mitigated.ts",
        "declare const res: any;\ndeclare function getSafeRedirectUrl(u: unknown): string | undefined;\nexport function handleCallback(req: any) {\n  const state = req.query.state;\n  res.redirect(\n    `${\n      getSafeRedirectUrl(state) ?? \"/apps/installed\"\n    }?error=denied`\n  );\n}\n",
    );
    dir.write(
        "api/raw.ts",
        "declare const res: any;\nexport function handleCallback(req: any) {\n  res.redirect(\n    `${\n      req.query.next\n    }?error=denied`\n  );\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "open-redirect");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].file, "api/raw.ts");
}

/// The two names MEASURED to be false vetoes, kept out by two DIFFERENT halves of the design — which is
/// why they share a test.
///
/// `revalidatePath` (Next.js App Router; 17 corpus uses, 6 of them in cal.com `actions.ts` files that
/// also call `redirect()`) is kept out by the VOCABULARY: it carries the URL/path half and none of the
/// safety half. Requiring only the safety half would be as bad in the other direction — `escapePath`
/// alone is 629 corpus uses.
///
/// `sanitizeUrlForLog` carries BOTH halves and is still not a redirect mitigator (it makes a URL safe to
/// LOG). It is kept out by the WINDOW: it sits on its own statement in the body, outside the redirect
/// call's parentheses, where a body-scoped `absent` veto would have seen it.
#[test]
fn a_name_in_the_family_that_is_not_a_redirect_mitigator_does_not_veto() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/revalidate.ts",
        "declare const res: any;\ndeclare function revalidatePath(p: string): string;\nexport function handleCallback(req: any) {\n  const next = req.query.next;\n  res.redirect(revalidatePath(next));\n}\n",
    );
    dir.write(
        "api/logging.ts",
        "declare const res: any;\ndeclare const logger: any;\ndeclare function sanitizeUrlForLog(u: string): string;\nexport function handleCallback(req: any) {\n  const next = req.query.next;\n  logger.info(sanitizeUrlForLog(next));\n  res.redirect(next);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "open-redirect").len(), 2, "{:?}", out.findings);
}

/// THE PIN. `packages/app-store/stripepayment/api/paymentCallback.ts` (lines 16-27 and 90 verbatim) is
/// the auditor's only TRUE POSITIVE in the corpus: the request `callbackUrl` reaches `res.redirect`
/// through a zod `querySchema.parse(req.query)` whose `.transform()` passes an ABSOLUTE `http(s)://`
/// URL through UNCHANGED — it only prefixes `WEBAPP_URL` onto RELATIVE ones.
///
/// The sister rule `browser/location-assign-dynamic` advertises that `security/taint-flow` waives on
/// "`escape*`/`sanitize*`/`validate*` plus zod-shaped parses". Borrowing that zod arm here would silence
/// exactly this finding, which is why `.parse(`, `zod` and `querySchema` are absent from this rule's
/// vocabulary. This test is RED with the veto and RED without it — that is what makes it a guard on the
/// vocabulary rather than a fixture for it.
#[test]
fn a_zod_parsed_query_url_that_passes_absolute_urls_through_is_still_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/paymentCallback.ts",
        "import { z } from \"zod\";\ndeclare const res: any;\ndeclare const WEBAPP_URL: string;\nconst querySchema = z.object({\n  callbackUrl: z.string().transform((url) => {\n    if (url.search(/^https?:/) === -1) {\n      url = `${WEBAPP_URL}${url}`;\n    }\n    return new URL(url);\n  }),\n  checkoutSessionId: z.string(),\n});\nexport async function getHandler(req: any) {\n  const { callbackUrl } = querySchema.parse(req.query);\n  callbackUrl.searchParams.set(\"paymentStatus\", \"unpaid\");\n  return res.redirect(callbackUrl.toString()).end();\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "open-redirect");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 16);
}

/// `MAX_CALL_WINDOW_LINES` (8, `crates/core/src/dsl/veto_window.rs`) pinned from BOTH sides, so the cap
/// is a contract rather than an assumption: the same mitigator, opening the same argument, vetoes at
/// exactly 8 continuation lines below the anchor and fires at 9. Blank filler rather than a `??` chain
/// on purpose — a comment leader would end the window at its own line and prove nothing about the cap.
///
/// The redirect's own `)` sits on the mitigator's line here, and that is load-bearing rather than
/// cosmetic: since the veto became a WHOLE-VALUE claim it has to READ the value's terminator, so the
/// cap now bounds the mitigator, its fallbacks AND that closing paren together. Push the `)` one line
/// further and the site fires — one more residual in the only direction this field is allowed to fail.
#[test]
fn a_mitigator_more_than_the_window_below_the_anchor_still_fires() {
    let head = "declare const res: any;\ndeclare function getSafeRedirectUrl(u: unknown): string;\nexport function handleCallback(req: any) {\n  const next = req.query.next;\n  res.redirect(\n";
    let tail = "    getSafeRedirectUrl(next));\n}\n";

    let inside = TempDir::new("zzop-be-sec");
    inside.write("api/at8.ts", &format!("{head}{}{tail}", "\n".repeat(7)));
    let out = scan(&inside);
    assert!(hits(&out, "open-redirect").is_empty(), "{:?}", out.findings);

    let outside = TempDir::new("zzop-be-sec");
    outside.write("api/at9.ts", &format!("{head}{}{tail}", "\n".repeat(8)));
    let out = scan(&outside);
    let h = hits(&out, "open-redirect");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

/// The `??` FALLBACK CHAIN, which is what 36 of cal.com's 37 mitigated sites actually write and what
/// forces the closure to be a small grammar rather than a terminator set. A fallback is not a suffix —
/// it is the value the redirect takes when the mitigator returns nullish.
///
/// THE PATTERN ADMITS A FALLBACK BY SHAPE, NOT BY SAFETY, and an earlier version of this doc claimed
/// otherwise. A CALL is admitted because it is a call: `getSafeRedirectUrl(base) ?? String(req.query.next)`
/// is VETOED and is an open redirect, as are `?? decodeURIComponent(req.query.next)`,
/// `?? req.query.next.toString()`, and a template atom whose BARE identifier was assigned from
/// `req.query` one line earlier. Measured across both corpora: 0 sites write a request read into a
/// fallback, against a control of 36 that use the arm at all — so removing the call atom would cost
/// nearly the whole veto and buy nothing this corpus can show. The cost is DISCLOSED in the rule
/// message instead of erased. `dub/api/callback.ts:20-24` and
/// `office365calendar/api/callback.ts:129` verbatim.
#[test]
fn a_chain_of_call_and_literal_fallbacks_is_still_the_whole_value() {
    let dir = TempDir::new("zzop-be-sec");
    let head = "declare const res: any;\ndeclare const WEBAPP_URL: string;\ndeclare function getSafeRedirectUrl(u: unknown): string | undefined;\ndeclare function getInstalledAppPath(o: unknown): string;\nexport function handleCallback(req: any) {\n  const state = req.query.state;\n";
    dir.write(
        "api/chain.ts",
        &[
            head,
            "  res.redirect(\n    getSafeRedirectUrl(state.onErrorReturnTo) ??\n      getSafeRedirectUrl(state?.returnTo) ??\n      `${WEBAPP_URL}/apps/installed`\n  );\n}\n",
        ]
        .concat(),
    );
    dir.write(
        "api/oneline.ts",
        &[
            head,
            "  res.redirect(\n    `${getSafeRedirectUrl(state?.onErrorReturnTo) ?? getInstalledAppPath({ variant: \"calendar\", slug: \"o365\" })}?error=denied`\n  );\n}\n",
        ]
        .concat(),
    );
    let out = scan(&dir);
    assert!(hits(&out, "open-redirect").is_empty(), "{:?}", out.findings);
}

/// A KNOWN FALSE VETO, pinned rather than discovered later. `sanitizeUrl` is the export of
/// `@braintree/sanitize-url`, and it carries BOTH halves of this vocabulary while doing a different
/// job: it strips `javascript:`/`data:`/`vbscript:` protocols for XSS and passes an absolute
/// `https://evil.com` through UNCHANGED. So it wraps the whole value — the closure above cannot help —
/// and this redirect is cleared when it should fire.
///
/// It survived the "0 false vetoes against the 34 corpus findings" measurement because neither corpus
/// imports it: grepping cal.com for `sanitizeUrl\|sanitize-url` returns ONE file, and it is
/// `packages/lib/ssrfProtection.ts`'s `sanitizeUrlForLog` — the name the test above already covers —
/// against a control (`getSafeRedirectUrl`) of 27. Excluding the one spelling is not honest either — `sanitizeURL` and
/// `sanitizeUri` would still be admitted and the exclusion would only look like a fix — so the RULE
/// MESSAGE names it instead, and this test exists so that the day the vocabulary can tell the two
/// apart, this assertion goes red and the residual leaves the message with it.
#[test]
fn a_protocol_sanitizer_wearing_the_mitigator_name_is_a_stated_false_veto() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/braintree.ts",
        "import { sanitizeUrl } from \"@braintree/sanitize-url\";\ndeclare const res: any;\nexport function handleCallback(req: any) {\n  res.redirect(sanitizeUrl(req.query.next));\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "open-redirect").is_empty(),
        "STATED RESIDUAL — if this now fires, delete the sanitizeUrl sentence from the rule message \
         in rules/dsl/security/security.json in the same change: {:?}",
        out.findings
    );
}
