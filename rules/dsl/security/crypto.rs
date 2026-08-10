use crate::{hits, scan, TempDir};

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
