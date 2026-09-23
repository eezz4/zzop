//! §37 LANDING for `security/timing-unsafe-compare` — the constant-time comparison the finding asks
//! for THROWS on the exact input shape the rule flags, rather than answering `false`.
//!
//! WHY (`1.architecture/rules/rule-quality.md` §27 leg 3). `crypto.timingSafeEqual` raises
//! `RangeError [ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH]` when the two buffers differ in byte length, and
//! `TypeError [ERR_INVALID_ARG_TYPE]` when handed plain strings. The shape this rule reports — a
//! request-supplied value against a stored secret — is precisely the shape whose lengths differ, so the
//! literal swap replaces a failed auth check with a 500 on every wrong guess. `String.length` is not the
//! guard the reader will reach for either: it counts UTF-16 code units, so one accented character makes
//! a five-character string six bytes and two equal-`.length` strings still throw. The exit is in the
//! message and it is not "check the length": hash both sides to a fixed width and compare the digests,
//! which is 32 bytes on both sides for any input.
//!
//! WHY IT TAKES ITS OWN CONSTANT. Nothing else in this tree describes a remedy whose failure mode is an
//! exception on well-formed input; the neighbouring crypto landings are about values that outlive the
//! change (`ALGORITHM_CUTOVER_LANDING`, `ROTATION_LANDING`) or about a drop-in generator swap
//! (`CSPRNG_SWAP_LANDING`), and none of those is true here — the comparison is local, nothing already
//! issued is affected, and the swap is not a drop-in.
//!
//! WHAT THE CONSTANT DELIBERATELY EXCLUDES. The message's question (1) — "are both sides secret values
//! someone is trying to guess?" — ends in "If not, this line is already correct", which is a statement
//! that the FINDING is wrong. That is the other axis (§38), and it stays out of these bytes so the two
//! claims do not travel as one. This rule's §27-a pin in `timing_compare.rs` already carries it.
//!
//! POSITION, not presence. The invalidation probe is to move this constant to the tail of the message:
//! every token stays present and spelled exactly once, and the pin must go red on ORDER alone.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

/// The length-mismatch landing, spliced ahead of the hash-then-compare imperative.
const TIMING_SAFE_EQUAL_LENGTH_LANDING: &str = "(2) CAN THE TWO SIDES DIFFER IN BYTE LENGTH? `crypto.timingSafeEqual` throws `RangeError [ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH]` on unequal byte lengths and `TypeError [ERR_INVALID_ARG_TYPE]` when handed plain strings, so passing it a request-supplied value and a stored secret — the exact shape this rule flags — replaces a failed auth check with a 500 on every wrong guess, and `String.length` is not the guard: it counts UTF-16 code units, so one accented character makes a five-character string six bytes and two equal-`.length` strings still throw.";

/// One constant, one carrier — asserted against the shipped pack so a sibling that grows a
/// constant-time-comparison remedy and pastes this sentence fails here rather than shipping a second
/// spelling.
#[test]
fn the_timing_safe_equal_length_landing_is_carried_by_exactly_one_rule() {
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("security.json")).expect("security.json parses");
    let mut carriers: Vec<&str> = pack["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .filter(|r| {
            r["message"]
                .as_str()
                .is_some_and(|m| m.contains(TIMING_SAFE_EQUAL_LENGTH_LANDING))
        })
        .map(|r| r["id"].as_str().expect("rule id"))
        .collect();
    carriers.sort_unstable();
    assert_eq!(
        carriers,
        ["timing-unsafe-compare"],
        "the timing-safe-equal length landing is carried by a different rule set than the one it was \
         written for"
    );
}

/// The position pin, on a DELIVERED finding. Separate `#[test]` from this rule's §27-a pin in
/// `timing_compare.rs`, which plants the "TWO QUESTIONS" summary — that summary opens a block whose
/// first half is a disqualifier and whose second half is this landing, and one needle cannot hold both.
#[test]
fn timing_unsafe_compare_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const token: string;\ndeclare const expectedToken: string;\nexport function checkToken() {\n  return token === expectedToken;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "timing-unsafe-compare");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "timing-unsafe-compare",
        &h[0].message,
        TIMING_SAFE_EQUAL_LENGTH_LANDING,
        "hash each side to a fixed width and compare the digests",
    );
}
