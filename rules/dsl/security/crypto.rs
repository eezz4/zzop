use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

// --- weak-password-hash (call-scan since the 2026-08-03 hash-call migration) ---
//
// The trigger is now a PROJECTED digest construction whose algorithm the parser read; the credential
// word stays a lexical same-line co-occurrence. Every positive below therefore has to construct a real
// digest through a resolvable platform API, and the two shapes that used to satisfy the old bare-word
// arms are pinned as NEGATIVES — they are the false-positive class this migration bought.

#[test]
fn a_projected_md5_construction_on_a_password_line_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import { createHash } from 'crypto';\nexport function hash(password: string) {\n  return createHash('md5').update(password).digest('hex');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-password-hash");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn a_bare_helper_named_md5_no_longer_fires() {
    // RED before the migration, silent after, and that is the POINT: `md5` here is a project's own
    // declared function, and the old arm could not tell it from a digest construction. The same
    // silence covers a variable, a parameter or an error string named `md5`.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const password: string;\ndeclare function md5(s: string): string;\nexport const hash = md5(password);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-password-hash").is_empty(),
        "a helper NAMED md5 is not a witnessed digest construction: {:?}",
        out.findings
    );
}

#[test]
fn an_algorithm_passed_to_an_unknown_helper_no_longer_fires() {
    // The other retired arm (`(password|pwd)[^;]*\bsha-?1\b`): `hashWith` is nobody's platform API, so
    // no site exists and the algorithm string is just a string. Disclosed recall cost, not a bug.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const password: string;\ndeclare function hashWith(s: string, algo: string): string;\nexport const h = hashWith(password, \"SHA1\");\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-password-hash").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_python_hashlib_md5_on_a_password_line_is_flagged() {
    // The migration's WIDENING half: one rule, six languages, no per-language regex copy.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.py",
        "import hashlib\n\ndef hash_password(password):\n    return hashlib.md5(password.encode()).hexdigest()\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-password-hash");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn a_dynamic_algorithm_is_silent_rather_than_guessed() {
    // THE never-guess pin at rule level: the construction is witnessed, the algorithm is not spelled,
    // so `algorithm_pattern` cannot match and the rule says nothing — never an approximation.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import { createHash } from 'crypto';\nexport function hash(password: string, algo: string) {\n  return createHash(algo).update(password).digest('hex');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-password-hash").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_weak_digest_with_no_credential_word_on_its_line_is_not_this_rule() {
    // The lexical residual, pinned: the co-occurrence half survived the migration unchanged, so a
    // file-checksum md5 is out of scope HERE — and since 2026-08-09 it is exactly what
    // `security/weak-crypto` judges (the general rule, six languages on the same `hash-call`
    // channel). The two rules split on the credential word: this fixture must fire under
    // weak-crypto and stay silent under weak-password-hash — the split, pinned from both sides.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import { createHash } from 'crypto';\nexport function checksum(buf: Uint8Array) {\n  return createHash('md5').update(buf).digest('hex');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-password-hash").is_empty(),
        "{:?}",
        out.findings
    );
    assert_eq!(hits(&out, "weak-crypto").len(), 1, "{:?}", out.findings);
}

#[test]
fn sha256_is_not_flagged() {
    // Silent under BOTH hash rules: a strong algorithm is the negative that keeps either rule from
    // being satisfiable by "fires on every digest call".
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import { createHash } from 'crypto';\nexport function hash(password: string) {\n  return createHash('sha256').update(password).digest('hex');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-password-hash").is_empty(),
        "{:?}",
        out.findings
    );
    assert!(hits(&out, "weak-crypto").is_empty(), "{:?}", out.findings);
}

#[test]
fn severity_is_warning_not_critical() {
    // Demoted 2026-08-02 and kept demoted through the structural migration: the TRIGGER is now proof
    // (a witnessed construction of a named algorithm), but the credential half is still one-line
    // co-occurrence, so the rule still cannot claim that this digest hashes that password.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import { createHash } from 'crypto';\nexport function hash(password: string) {\n  return createHash('md5').update(password).digest('hex');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-password-hash");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(
        h[0].severity,
        zzop_core::Severity::Warning,
        "weak-password-hash must report at warning — its credential half is co-occurrence: {:?}",
        h[0]
    );
}

/// POSITION, not presence (rule-quality.md §27). The test above pins the SEVERITY that the
/// co-occurrence limit justifies; this one pins where the reader is told about it. The clause saying
/// this finding may not be a password hash at all — that the credential half is lexical, that it
/// proves only that the two words share a line, and that a line REJECTING a weak algorithm reads the
/// same to it — shipped at the very END of a 3140-byte message, behind three imperatives, the first
/// of which (`Plan the cutover before the edit`) starts a migration of the LOGIN path. That is the
/// most expensive wasted edit this pack prescribes, and a reader who acts on the first instruction
/// never reaches the sentence that would have stopped them.
///
/// This was the last §27-ⓐ residual the DDL prescription census left behind: that census closed the
/// prescription-BREAKING bridge for this rule (`algorithm_cutover_landing.rs` pins the cutover
/// landing ahead of `Use bcrypt`) and left the ORDER bridge open, because a landing that says what
/// the edit strands is a different sentence from a disqualifier that says the finding may be wrong.
///
/// The repair was a pure MOVE: the message is 3140 bytes before and after, character-multiset
/// identical, and the clause travelled from byte 2831 to byte 265. So a length assertion cannot see
/// it and neither can `contains` — the invalidation probe is to put the clause back at the tail,
/// where every token below stays present and spelled exactly once and this test must go red on order
/// alone.
#[test]
fn the_co_occurrence_disqualifier_precedes_the_first_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import { createHash } from 'crypto';\nexport function hash(password: string) {\n  return createHash('md5').update(password).digest('hex');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-password-hash");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;

    // The FIRST imperative, not the loudest one. `Use bcrypt` is what a reader quotes back, but
    // `Plan the cutover before the edit` is what they reach first, so it is the index that decides
    // whether the disqualifier arrived in time.
    let first_imperative = "Plan the cutover before the edit";
    assert_eq!(
        m.matches(first_imperative).count(),
        1,
        "the first imperative must be spelled ONCE, or an index comparison against it means \
         nothing: {m}"
    );
    let imperative = m.find(first_imperative).expect("asserted above");

    for clause in [
        "stays a CO-OCCURRENCE",
        "not that this digest hashes that password",
        "`warning` and not `critical`",
        "REJECTS a weak algorithm while naming a credential reads the same to it",
    ] {
        assert_eq!(
            m.matches(clause).count(),
            1,
            "clause {clause:?} must be spelled ONCE, or the offset below is an arbitrary copy: {m}"
        );
        let at = m
            .find(clause)
            .expect("asserted above — reordering is not rewriting");
        assert!(
            at < imperative,
            "security/weak-password-hash: the clause that disqualifies this finding sits at byte \
             {at}, BEHIND the first imperative at byte {imperative} — a reader who starts planning \
             a login-path cutover on that instruction never learns the finding may not be a \
             password hash. Move the clause, do not rewrite it — {clause:?} in: {m}"
        );
    }
}

#[test]
fn negative_guard_line_rejecting_md5_is_not_flagged() {
    // The false-positive class that forced the 2026-08-02 demotion. It is now silent for a STRONGER
    // reason than the old `;` statement-boundary heuristic: a guard comparing a string to 'md5'
    // constructs no digest, so there is no site at all and the lexical boundary never has to be
    // trusted. The corpus twin lives in `cases/trees/decoy/lib/security.weak-password-hash.decoy.ts`.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "export function assertStrongHash(algo: string): void {\n  if (algo === 'md5' || algo === 'sha1') throw new Error('weak digest rejected; use bcrypt for password hashing');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-password-hash").is_empty(),
        "a guard line rejecting md5 must not be reported as using it: {:?}",
        out.findings
    );
}

#[test]
fn weak_hash_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import { createHash } from 'crypto';\nexport function hash(password: string) {\n  // zzop-weak-password-hash-ok: legacy checksum for cache-busting, not used for auth\n  return createHash('md5').update(password).digest('hex');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-password-hash").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- bcrypt-cost-too-low (split out of weak-password-hash by the same migration) ---

#[test]
fn bcrypt_with_single_digit_cost_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const bcrypt: any;\ndeclare const password: string;\nexport const hash = bcrypt.hashSync(password, 4);\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "bcrypt-cost-too-low").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn bcrypt_with_double_digit_cost_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const bcrypt: any;\ndeclare const password: string;\nexport const hash = bcrypt.hashSync(password, 12);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "bcrypt-cost-too-low").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn the_salt_first_bcrypt_family_now_fires() {
    // This test used to assert the OPPOSITE, and it was right to: the single `line_pattern` required
    // the digit to sit after another argument, so every call whose cost was the FIRST argument was
    // structurally unmatchable, and the rule's message disclosed that silence by name. The old test
    // pinned the silence and the disclosure together so neither could drift from the other, and its own
    // comment said a batch that widened the pattern must delete both — this is that batch, so the pin
    // is INVERTED rather than deleted: the three forms the message used to apologize for are now the
    // three the pattern must catch.
    //
    // All three are one shape, `cost-as-first-argument`, added as a second alternation branch beside the
    // original cost-after-a-value branch: bcryptjs's two-step `genSalt`/`genSaltSync` API and the nested
    // one-liner its README documents.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const bcrypt: any;\ndeclare const password: string;\ndeclare const cb: any;\nexport const s = bcrypt.genSaltSync(4);\nexport const s2 = bcrypt.genSalt(4, cb);\nexport const h = bcrypt.hashSync(password, bcrypt.genSaltSync(4));\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "bcrypt-cost-too-low").len(),
        3,
        "every salt-first form must fire — genSaltSync(4), genSalt(4, cb), and the nested \
         hashSync(pw, genSaltSync(4)): {:?}",
        out.findings
    );
    // And the message must no longer apologize for a silence that ended. A disclosure outliving the
    // gap it disclosed is the same defect as a gap with no disclosure — it teaches a reader to distrust
    // a number that is now correct.
    let msg = crate::security_pack()
        .rules
        .iter()
        .find(|r| r.id == "bcrypt-cost-too-low")
        .expect("bcrypt-cost-too-low is a shipped security rule")
        .message
        .clone();
    assert!(
        !msg.contains("stays silent"),
        "the salt-first silence clause must go with the silence: {msg}"
    );
}

#[test]
fn a_two_digit_cost_in_the_salt_first_position_is_not_flagged() {
    // The widened branch must keep the rule's one numeric claim: SINGLE digit only. `genSalt(10)` is
    // the recommended floor, and a pattern that fired on it would turn the correct call into noise —
    // the fastest way to get a security rule switched off.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const bcrypt: any;\nexport const s = bcrypt.genSaltSync(10);\nexport const s2 = bcrypt.genSalt(12, () => {});\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "bcrypt-cost-too-low").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_low_bcrypt_cost_no_longer_reports_under_the_hash_rule() {
    // The split, pinned from the losing side: the same source that used to produce a
    // `weak-password-hash` finding now produces one under its own id, so a consumer keyed on the old
    // id sees a rename rather than a silent disappearance.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const bcrypt: any;\ndeclare const password: string;\nexport const hash = bcrypt.hashSync(password, 4);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-password-hash").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- weak-crypto: CPython's own `usedforsecurity=False` declaration ---
//
// `hashlib.md5(data, usedforsecurity=False)` (CPython 3.9+) is the LANGUAGE's way of stating that a
// digest is not a security primitive; bandit honours it as B324. It is a DECLARATION in the code, not
// an inference about intent, which is what makes it admissible in the suppression direction.

#[test]
fn a_python_digest_declared_not_for_security_is_excluded_while_a_bare_one_beside_it_still_fires() {
    // The exclusion and its production CONTROL in ONE assertion: the same file constructs two MD5
    // digests, one carrying the declaration and one carrying nothing. A green fixture therefore
    // proves the exclusion fired, rather than proving the scan was inert — if the whole rule went
    // dark, the second assertion (the bare construction, still reported, on its own line) fails.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/hashes.py",
        "import hashlib\n\n\ndef avatar(email):\n    return hashlib.md5(email.encode(), usedforsecurity=False).hexdigest()\n\n\ndef signature(payload):\n    return hashlib.md5(payload.encode()).hexdigest()\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-crypto");
    assert_eq!(
        h.len(),
        1,
        "the declared non-security digest must be excluded and the bare one must survive: {:?}",
        out.findings
    );
    assert_eq!(
        h[0].line, 9,
        "the surviving finding must be the BARE construction, not the declared one: {:?}",
        out.findings
    );
}

#[test]
fn a_declaration_outside_the_calls_own_parentheses_never_waives_it() {
    // The boundary the message asserts, pinned rather than assumed. Three shapes, all of which the
    // FIRST version of the window waived by counting the LINE's parentheses instead of the call's: a
    // `(` inside a string literal, a `(` in a trailing comment, and an ENCLOSING call's open paren.
    // In each the `usedforsecurity=False` belongs to a DIFFERENT call, and letting it through would
    // drop a real finding with no trace — the one direction a veto must never fail in.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/hashes.py",
        concat!(
            "import hashlib\n",
            "import logging\n",
            "\n",
            "\n",
            "def stringy(a, b):\n",
            "    key = hashlib.md5(f\"{a}(\").hexdigest()\n",
            "    return hashlib.sha1(b, usedforsecurity=False), key\n",
            "\n",
            "\n",
            "def commented(payload):\n",
            "    digest = hashlib.md5(payload)  # legacy hash (see PROJ-123\n",
            "    salted = hashlib.md5(payload, usedforsecurity=False)\n",
            "    return digest, salted\n",
            "\n",
            "\n",
            "def logged(x):\n",
            "    logging.info(\"d %s\", hashlib.md5(x).hexdigest(),\n",
            "                 extra=dict(usedforsecurity=False))\n",
        ),
    );
    let out = scan(&dir);
    let mut lines: Vec<u32> = hits(&out, "weak-crypto").iter().map(|f| f.line).collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        vec![6, 11, 17],
        "each digest whose own parentheses carry no declaration must still fire: {:?}",
        out.findings
    );
}

#[test]
fn a_usedforsecurity_marker_on_a_continuation_line_is_honoured_and_a_bare_split_call_still_fires() {
    // The residual this pin used to SEAL, inverted on 2026-08-21 when the veto stopped reading one
    // line. Measured on getredash/redash @ ca79fe98 with "grep -rn usedforsecurity --include=*.py": 5
    // occurrences, 3 of them on a continuation line — those 3 fired until the veto's window became
    // the call's own parentheses instead of its first line.
    //
    // Both directions in ONE fixture, because a veto that widened is worth nothing if it also
    // swallowed the control: the first function declares the keyword on its continuation line and
    // must go silent, the second is split across lines exactly the same way and declares nothing, so
    // it must still fire. A rule gone dark fails the second assertion.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/hashes.py",
        "import hashlib\n\n\ndef identity(a, b):\n    return hashlib.md5(\n        \"{},{}\".format(a, b).encode(), usedforsecurity=False\n    ).hexdigest()\n\n\ndef signature(a, b):\n    return hashlib.md5(\n        \"{},{}\".format(a, b).encode()\n    ).hexdigest()\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-crypto");
    assert_eq!(
        h.len(),
        1,
        "the declared split call must be waived and the bare split call must survive: {:?}",
        out.findings
    );
    assert_eq!(
        h[0].line, 11,
        "the surviving finding must be the BARE split construction: {:?}",
        out.findings
    );
    // A message must not assert more than its matcher enforces, and the reverse too: what the window
    // now covers, and what it still does not, both have to be readable where the reader is.
    let msg = crate::security_pack()
        .rules
        .iter()
        .find(|r| r.id == "weak-crypto")
        .expect("weak-crypto is a shipped security rule")
        .message
        .clone();
    assert!(
        msg.contains("usedforsecurity") && msg.contains("WINDOW") && msg.contains("OUTSIDE"),
        "the rule must disclose both what the window covers and what stays out of reach: {msg}"
    );
}

/// §33/§37 LANDING for `weak-token-random`, spliced ahead of the `crypto` imperative.
///
/// WHY. The message shipped at 286 characters: one sentence of observation and one of remedy, with no
/// room for a caveat and therefore none in it (§34's shape — short read as safe). The remedy is a
/// concrete edit and it costs three separate things. The replacement produces a DIFFERENT VALUE —
/// `randomBytes(n).toString(hex)` is 2n hex characters, `randomUUID()` is a fixed 36-character dashed
/// form — so a fixed-width column, a checksum or a six-digit OTP contract breaks on the first write
/// and not at the call. `crypto.randomBytes` is Node's, and this rule fires on a line-scan that reaches
/// browser and edge bundles where `node:crypto` does not resolve at all. And the edit reaches nothing
/// already issued: every token minted by the old generator stays guessable until it is expired.
///
/// WHY NOT `ALGORITHM_CUTOVER_LANDING`, the nearest sibling by topic (§37). That constant says stored
/// values are "readable only by" the old algorithm and that the break arrives at READ time — both
/// FALSE here. A weak random token reads back perfectly; it is merely predictable, and the break has
/// already happened rather than waiting for a read. A landing whose mechanism is wrong is worse than
/// none, because it is confidently wrong.
///
/// POSITION, not presence. The invalidation probe is to move this constant to the tail of the message.
const CSPRNG_SWAP_LANDING: &str = "THE SWAP CHANGES THE VALUE, NOT ONLY THE GENERATOR, AND IT DOES NOT REACH THE VALUES ALREADY ISSUED: `randomBytes(n).toString(hex)` yields 2n characters over a 16-symbol alphabet and `randomUUID()` yields a fixed 36-character form with dashes, so neither matches the length or the character set the `Math.random()` expression produced — a fixed-width column, a checksum, a display format or a six-digit OTP contract fails on the first write rather than at the call. Check where the value is stored and compared before you change how it is made, and size the replacement to the column rather than to the example. `crypto.randomBytes` is Node — in code that also builds for a browser, an edge runtime or a Worker there is no `node:crypto` to import, and the replacement there is `crypto.getRandomValues(new Uint8Array(n))` from the Web Crypto API. Every value minted before this edit stays predictable: changing the generator does not invalidate them, so the ones still accepted have to be expired or reissued separately.";

#[test]
fn weak_token_random_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/token.ts",
        "export function makeToken() {\n  const token = Math.random().toString(36);\n  return token;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-token-random");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "weak-token-random",
        &h[0].message,
        CSPRNG_SWAP_LANDING,
        "Use `crypto.randomBytes(n).toString('hex')`",
    );
    // Three distinct costs, three needles: a landing that keeps only the easy one (length) would read
    // as compliant while dropping the two a reader cannot infer.
    for needle in [
        "yields 2n characters over a 16-symbol alphabet",
        "there is no `node:crypto` to import",
        "changing the generator does not invalidate them",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/weak-token-random: the landing lost {needle:?}: {}",
            h[0].message
        );
    }
}

// --- weak-token-random ---

#[test]
fn math_random_with_token_keyword_before_it_on_the_line_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/token.ts",
        "export function makeToken() {\n  const token = Math.random().toString(36);\n  return token;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-token-random");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn math_random_with_secret_keyword_after_it_on_the_line_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/token.ts",
        "export function makeSecretSuffix() {\n  const value = Math.random().toString() + \"-secret\";\n  return value;\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "weak-token-random").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn math_random_with_no_security_keyword_on_the_line_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/token.ts",
        "export function randomDelay() {\n  const delay = Math.random() * 1000;\n  return delay;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-token-random").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn weak_random_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/token.ts",
        "export function makeToken() {\n  // zzop-weak-token-random-ok: non-security cache-busting value, not used for auth\n  const token = Math.random().toString(36);\n  return token;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "weak-token-random").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- the HMAC / counterparty-fixed disclosures (2026-08-23) ---
//
// `createHmac` is in the TypeScript producer's `hash-call` family (Go's `crypto/hmac`, C#'s
// `HMACSHA1`, Rust's `hmac` and Python's `hmac` are each deliberately outside theirs), so an
// HMAC-SHA1 fires here and nowhere else. It keeps firing -- HMAC-SHA1 is deprecated for new
// protocols -- but the message may no longer read as if SHA-1's collision attacks broke it, and it
// must name the case where the algorithm is the counterparty's choice (cal.com
// apps/api/v2/src/vercel-webhook.guard.ts:44 verifies an `x-vercel-signature`, where "use SHA-256"
// rejects the genuine webhooks instead of hardening anything).

#[test]
fn an_hmac_sha1_construction_still_fires_and_says_it_is_not_a_bare_digest() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/webhook.ts",
        "import { createHmac } from 'crypto';\nexport function verify(body: string, secret: string) {\n  return createHmac('sha1', secret).update(body).digest('hex');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-crypto");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    let m = &h[0].message;
    for needle in [
        "AN HMAC IS NOT A BARE DIGEST",
        "do NOT break HMAC-SHA1",
        "THE ALGORITHM MAY NOT BE YOURS TO PICK",
        "it rejects the genuine requests instead",
    ] {
        assert!(
            m.contains(needle),
            "the delivered weak-crypto message no longer separates an HMAC from a bare digest, or no \
             longer discloses the counterparty-fixed case -- missing {needle:?} in: {m}"
        );
    }

    // POSITION, not presence. A reader who acts on the first instruction never reaches a caveat placed
    // after it, so every clause that disqualifies this finding has to arrive BEFORE the imperative.
    // Two blind auditors read this exact message at two cal.com sites (apps/api/v2/src/
    // vercel-webhook.guard.ts:44 and apps/web/app/api/sync/helpscout/route.ts:42), each picked the rule
    // as HARMFUL, and each withdrew after reaching the counterparty clause -- and each said,
    // independently, that it arrives too late to stop a hurried reader from shipping the outage.
    //
    // The `contains` loop above is everything this pin asserted until 2026-08-25, and on its own it
    // stays GREEN with every one of those clauses shoved back behind the remedy -- which is exactly
    // how the ordering defect survived a green test suite for as long as it did. A message pin that
    // checks only presence cannot see the failure a reader actually suffers. The invalidation probe
    // for the assertions below is that same move: put the clauses back after "Use SHA-256 or
    // stronger" and this test must go red with all six clause tokens, and the imperative, still
    // present and still spelled exactly once each.
    assert_eq!(
        m.matches("Use SHA-256 or stronger").count(),
        1,
        "the imperative must be spelled ONCE, or an index comparison against it means nothing: {m}"
    );
    let imperative = m
        .find("Use SHA-256 or stronger")
        .expect("the remedy must survive verbatim -- reordering is not rewriting");
    for (role, clause) in [
        (
            "de-escalates the security claim",
            "AN HMAC IS NOT A BARE DIGEST",
        ),
        ("de-escalates the security claim", "do NOT break HMAC-SHA1"),
        (
            "disqualifies the finding",
            "THE ALGORITHM MAY NOT BE YOURS TO PICK",
        ),
        (
            "disqualifies the finding",
            "it rejects the genuine requests instead",
        ),
        ("disqualifies the finding", "x-vercel-signature"),
        ("disqualifies the finding", "a length check fails first"),
    ] {
        let at = m
            .find(clause)
            .unwrap_or_else(|| panic!("clause {clause:?} left the message entirely: {m}"));
        assert!(
            at < imperative,
            "the clause that {role} sits at byte {at}, AFTER the imperative at byte \
             {imperative}: a reader who edits on the first instruction never reaches it. Move \
             the clause, do not rewrite it -- {clause:?} in: {m}"
        );
    }
}

/// The exemption list is THREE ITEMS and it used to close with "Everywhere else the remedy is the
/// algorithm itself." That closer is the defect, not the shortness of the list: a narrow enumeration
/// that ends by telling you to proceed everywhere else turns its own INCOMPLETENESS into a push,
/// promoting every unlisted legitimate site from "not covered" to "confirmed". The reader walks the
/// list, honestly matches nothing, and makes the dangerous edit BECAUSE they were diligent -- with no
/// exemption at all they would at least have hesitated. `32919f9` retired the same construction from
/// `mutating-route-no-auth`; this rule was the last place in the repo it survived.
///
/// The list is also lopsided in a way the closer hid: all three named shapes (inbound webhook, OAuth
/// 1.0a, legacy partner callback) are on the VERIFY side, where a counterparty picked the algorithm.
/// The ISSUING side has no entry at all, and it is not hypothetical -- cal.com derives a TOTP secret
/// with `createHash("md5")` at `verifyEmail.ts:105`, which GENERATES an emailed verification code, and
/// again at `verifyCodeUnAuthenticated.ts:17`, which CHECKS it. Neither site is anybody's callback, so
/// the exemption list matches neither, and changing one of them alone splits the shared secret and
/// invalidates every code already sitting in a user's inbox.
///
/// So the closer now says the list is EXAMPLES and hands over the discriminator instead: does this
/// position CHOOSE the algorithm or RECEIVE one, and is anything the old algorithm produced still
/// alive somewhere that has to keep accepting it. The imperative is untouched -- a genuine weak digest
/// still gets "Use SHA-256 or stronger".
///
/// Two things are pinned, and they fail for different reasons.
///
/// POSITION: the discriminator has to arrive BEFORE the imperative, same rule as the pin above, and
/// the invalidation probe is the same move -- relocate the four clauses below to the tail of the
/// message (after the parenthesis that closes the `weak-password-hash` cross-reference) and this test
/// must go red with every token still present and still spelled exactly once. Length is unchanged by
/// that move, so a length assertion would not see it.
///
/// ABSENCE: `contains` on the new clauses cannot see a RESTORED old closer, because a future edit can
/// satisfy every positive token and put the old sentence back beside them. The negative assertion is
/// the only thing that catches that, so it is spelled out separately -- the same shape `32919f9` used.
#[test]
fn the_exemption_list_says_it_is_examples_and_hands_over_a_discriminator() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/verify.ts",
        "import { createHash } from 'crypto';\nexport function derive(email: string, key: string) {\n  return createHash('md5').update(email + key).digest('hex');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-crypto");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let m = &h[0].message;

    assert!(
        !m.contains("Everywhere else"),
        "the closing sentence that promoted every unlisted site to a confirmed defect is BACK in \
         security/weak-crypto. An exemption list may not end by telling the reader to proceed \
         everywhere else -- say the list is examples and give the discriminator instead: {m}"
    );

    assert_eq!(
        m.matches("Use SHA-256 or stronger").count(),
        1,
        "the imperative must be spelled ONCE, or an index comparison against it means nothing: {m}"
    );
    let imperative = m
        .find("Use SHA-256 or stronger")
        .expect("the remedy must survive verbatim -- the prescription is not what was wrong");
    for (role, clause) in [
        ("says the list is examples", "NOT THE WHOLE LIST"),
        (
            "names the axis the list is lopsided on",
            "on the VERIFY side",
        ),
        (
            "gives the choose-or-receive discriminator",
            "CHOOSE the algorithm, or RECEIVE one somebody else already chose",
        ),
        (
            "gives the still-alive discriminator that reaches the issuing side",
            "still alive somewhere that has to keep accepting it",
        ),
    ] {
        assert_eq!(
            m.matches(clause).count(),
            1,
            "the clause that {role} must be spelled exactly ONCE, or the offset comparison below \
             compares against an arbitrary copy -- {clause:?} in: {m}"
        );
        let at = m
            .find(clause)
            .expect("asserted present exactly once just above");
        assert!(
            at < imperative,
            "the clause that {role} sits at byte {at}, AFTER the imperative at byte {imperative}: \
             a reader who edits on the first instruction never reaches it, which is the whole \
             failure this pin exists for. Move the clause, do not rewrite it -- {clause:?} in: {m}"
        );
    }
}
