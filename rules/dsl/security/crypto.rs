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
    // file-checksum md5 is still out of scope here (`security/weak-crypto` is where a Java digest is
    // judged without a credential word).
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
}

#[test]
fn sha256_is_not_flagged() {
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
fn the_salt_first_bcrypt_family_is_silent_and_the_message_says_so() {
    // The gap this rule's message now discloses, pinned from the silent side so the disclosure cannot
    // drift away from the behavior (a sentence about a check is not the check — working-agreements
    // §4.5). The `line_pattern` requires the digit to sit after another argument, so every call whose
    // cost is the FIRST argument is structurally unmatchable — POSITION decides, not argument count
    // (`genSaltSync(saltRounds, 4)` fires; `genSalt(4, cb)` does not). bcryptjs's documented two-step
    // API and its nested one-liner both live on the silent side. A future batch that widens the pattern
    // should delete this test and the message clause together — that is the point of pinning them as a
    // pair.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const bcrypt: any;\ndeclare const password: string;\ndeclare const cb: any;\nexport const s = bcrypt.genSaltSync(4);\nexport const s2 = bcrypt.genSalt(4, cb);\nexport const h = bcrypt.hashSync(password, bcrypt.genSaltSync(4));\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "bcrypt-cost-too-low").is_empty(),
        "{:?}",
        out.findings
    );
    let msg = crate::security_pack()
        .rules
        .iter()
        .find(|r| r.id == "bcrypt-cost-too-low")
        .expect("bcrypt-cost-too-low is a shipped security rule")
        .message
        .clone();
    for form in ["genSalt(4, cb)", "genSaltSync(4)"] {
        assert!(
            msg.contains(form),
            "the message must name the silent form `{form}` it cannot match — it is the only way a \
             reader learns this rule's 0 findings are not a clean bill of health"
        );
    }
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
