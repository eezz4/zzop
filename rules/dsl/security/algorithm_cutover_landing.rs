//! §27 POSITION PINS for the three rules that tell a reader to swap a broken ALGORITHM —
//! `weak-password-hash`, `weak-cipher` and `weak-crypto`.
//!
//! The prescription-breaking veto these close (`1.architecture/rules/rule-quality.md` §27): changing
//! the construction changes what the code WRITES from the next deploy and nothing at all about what
//! the old algorithm already produced. Every stored digest and every stored ciphertext was produced by
//! the broken one and is readable only by it, so a straight in-place swap leaves those values
//! unverifiable — and unlike a schema migration the break does not arrive at deploy time. It arrives
//! at READ time, on live traffic: `weak-password-hash`'s reader turns MD5 into bcrypt and every stored
//! account fails login, `weak-cipher`'s turns DES into AES-GCM and every stored record fails to
//! decrypt. Measured over that PAIR's two shipped messages on 2026-08-27, `existing`, `stored`,
//! `migrat`, `rehash`/`re-hash`, `re-encrypt` and `dual` occurred ZERO times in BOTH. The canary has
//! to be a per-rule one because the two vocabularies barely overlap: `digest` occurred 7 times in
//! `weak-password-hash` and `cipher` 7 times in `weak-cipher` over the same pass, so the counter was
//! reading the strings. (`algorithm` is NOT a usable canary for the pair — 6 in the first message and
//! 0 in the second, which is why it is named here rather than relied on.)
//!
//! ONE constant for all three, byte-identical. The dangerous property belongs to an algorithm cutover
//! as such, not to what any one rule detects, and they diverge only in their EXIT — which is each
//! rule's own imperative. Same split `ROTATION_LANDING` uses one file over (`7c3d386`) and
//! `DATA_LOSS_LANDING` uses in `rules/native/rules-schema` (`cd891eb`), and the same reason: a
//! position pin needs ONE spelling to index.
//!
//! WHY `security/weak-crypto` JOINED ON 2026-08-27, having been left out when this file was written.
//! It is the third sibling of the pair above — `weak-password-hash`'s own message names it and says
//! the two CO-FIRE on one line by design — and its imperative is the same shape, `Use SHA-256 or
//! stronger`, so the same edit strands the same class of value. What the omission cost is measurable:
//! cal.com derives a TOTP secret with `createHash("md5")` at `verifyEmail.ts:105` (which GENERATES an
//! emailed verification code) and again at `verifyCodeUnAuthenticated.ts:17` (which CHECKS it), and
//! this rule reports the two as two separate findings in two separate files. Both were shipped a
//! message that said nothing at all about values the old algorithm had already produced.
//!
//! ITS EXIT CARRIES A DIFFERENCE THE CONSTANT DOES NOT STATE, which is why the exit needles below are
//! not the pair's. The constant's noun is a STORED value — a row that has to be migrated and recorded.
//! A per-call derived secret is recomputed rather than persisted, so nothing is stored to migrate and
//! there is no column to record; what outlives the edit is what is already IN FLIGHT, and the
//! read-only path is therefore a CLOCK (one validity window, self-retiring) rather than a migration
//! project. The constant is still correct for this rule — a weak digest is also used for content
//! addressing and cache keys, where the stored-value reading is exactly right — so it is spliced
//! unchanged and the divergence is written into the exit, where per-rule divergence belongs. The
//! second half of that exit has no analogue in either sibling: the derivation is normally spelled out
//! at BOTH the issuing and the checking site, so a reader who fixes only the finding they were shown
//! splits one secret into two, and the failure surfaces as an invalid-code error rather than as
//! anything traceable.
//!
//! WHY NOT `security/jwt-sign-literal-secret`, whose live tokens are also artifacts bound to an old
//! value: its imperative is `rotate the exposed key`, byte-for-byte the verb the seven rotation rules
//! carry, so the landing that guards that verb already exists and it carries that one instead. What is
//! jwt-specific there — the overlapping window sits on the VERIFY side, because a token is not a
//! consumer you migrate — lives in that rule's exit, where per-rule divergence belongs.
//!
//! WHY NOT CONDITIONING THE IMPERATIVE (§27's third and cheapest form): the premise is not a check.
//! "Is anything already stored under the old algorithm?" cannot be answered from the flagged line, or
//! from the source at all — it is a question about a deployed database — so for any reader who cannot
//! answer it, `IF ...: use bcrypt` collapses back to the bare imperative, which is the harmful text.
//! That is the third of the three grounds `7c3d386` recorded for rejecting the cheap form.
//!
//! POSITION, not presence. A reader who acts on the first instruction never reaches a caveat placed
//! behind it, so `contains` proves nothing. The invalidation probe for every test below is to move
//! `ALGORITHM_CUTOVER_LANDING` to the tail of the message: every token stays present and spelled
//! exactly once, and each must go red on ORDER alone.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

/// The algorithm-cutover landing, spliced ahead of the swap imperative in all three messages. Not a
/// `convention` value — no project declares it; it is zzop's own sentence about what the reader's own
/// edit does to data that already exists.
const ALGORITHM_CUTOVER_LANDING: &str = "CHANGING THE ALGORITHM DOES NOT CHANGE WHAT THE OLD ONE ALREADY PRODUCED: every value already stored was produced by the broken algorithm and is readable only by it, so swapping the construction in place leaves those values unverifiable, and the break arrives at READ time, on live traffic, rather than at deploy. Plan the cutover before the edit — write with the new algorithm from the first deploy, keep the old one on a read-only path, record which of the two produced each stored value, and retire the old path once nothing it produced is left.";

/// `weak-password-hash` is the arm where the old values CANNOT be recomputed: a digest's plaintext is
/// gone, so the cutover has to ride the login path. The bulk-upgrade trap is pinned separately because
/// it is the plausible wrong move — `bcrypt(md5(pw))` verifies correctly forever and permanently pins
/// the account to MD5's input space, so a reader who "fixed" every row in one batch job has frozen
/// exactly the accounts the finding was raised to protect.
#[test]
fn weak_password_hash_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "import { createHash } from 'crypto';\nexport function hash(password: string) {\n  return createHash('md5').update(password).digest('hex');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-password-hash");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "weak-password-hash",
        &h[0].message,
        ALGORITHM_CUTOVER_LANDING,
        "Use bcrypt",
    );
    for needle in [
        "cannot be recomputed",
        "at its next successful login",
        "Do NOT bulk-upgrade",
        "`bcrypt(md5(pw))`",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/weak-password-hash: the exit lost {needle:?}: {}",
            h[0].message
        );
    }
}

/// `weak-cipher` is the mirror arm: the data IS recoverable, so the read-only path is a decrypt path
/// and the cutover is a re-encryption rather than a lazy upgrade. Two facts are pinned that the hash
/// rule has no equivalent of — a peer that decrypts the ciphertext has to accept the new form BEFORE
/// it is emitted, and re-encrypting does not un-leak what ECB or DES already exposed to a holder of an
/// old copy, which is the sentence that hands this rule's reader over to the rotation family without
/// restating its landing.
#[test]
fn weak_cipher_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "D.java",
        "import javax.crypto.Cipher;\npublic class D {\n  void run() throws Exception {\n    Cipher.getInstance(\"DES/CBC/PKCS5Padding\");\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-cipher");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "weak-cipher",
        &h[0].message,
        ALGORITHM_CUTOVER_LANDING,
        "Use AES-GCM",
    );
    for needle in [
        "this data IS recoverable",
        "a boundary you do not own",
        "does not un-leak",
        "rotate whatever it protected",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/weak-cipher: the exit lost {needle:?}: {}",
            h[0].message
        );
    }
}

/// `weak-crypto` is the general sibling of the pair above, and the arm where the stranded artifact is
/// often not a stored row at all. Its exit needles are the ones the constant does NOT say: that the
/// derived-secret case has nothing to migrate, that the read-only path is a validity window rather
/// than a migration, and that the same derivation normally sits at two sites this rule reports
/// separately — so fixing one of them is what actually breaks the flow.
#[test]
fn weak_crypto_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/verify.ts",
        "import { createHash } from 'crypto';\nexport function derive(email: string, key: string) {\n  return createHash('md5').update(email + key).digest('hex');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-crypto");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "weak-crypto",
        &h[0].message,
        ALGORITHM_CUTOVER_LANDING,
        "Use SHA-256 or stronger",
    );
    for needle in [
        "THE ISSUING SIDE OFTEN HAS NONE",
        "recomputed, not persisted",
        "a CLOCK rather than a migration",
        "two findings in two files",
        "in ONE commit",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/weak-crypto: the exit lost {needle:?} — the constant does not say it, so \
             nothing else in this file would catch its removal: {}",
            h[0].message
        );
    }
}

/// The landing is one constant for three rules, so the thing that can silently rot is a COPY that
/// drifts. This asserts the shipped pack carries it byte-identically in exactly those three and
/// NOWHERE else — a rule that acquires an algorithm-swap imperative later and paraphrases the landing
/// fails here rather than shipping a second spelling that the position pins above would each still
/// pass.
#[test]
fn the_landing_is_byte_identical_in_exactly_the_three_algorithm_swap_rules() {
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("security.json")).expect("security.json parses");
    let mut carriers: Vec<&str> = pack["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .filter(|r| {
            r["message"]
                .as_str()
                .is_some_and(|m| m.contains(ALGORITHM_CUTOVER_LANDING))
        })
        .map(|r| r["id"].as_str().expect("rule id"))
        .collect();
    carriers.sort_unstable();
    assert_eq!(
        carriers,
        ["weak-cipher", "weak-crypto", "weak-password-hash"],
        "the algorithm-cutover landing is carried by a different rule set than the three it was \
         measured for"
    );
}
