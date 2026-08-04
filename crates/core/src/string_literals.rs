//! The BOUND-STRING-LITERAL channel ([`BoundStringLiteral`]) — "this file binds the name N to a string
//! literal at line L, whose value hashes to H and carries E bits of Shannon entropy".
//!
//! One fact for the class of evidence `security/hardcoded-secret`'s message spent a paragraph admitting
//! it could not reach: a line-scan regex can see that SOME value follows a secret-named binding, but it
//! cannot compare the value to the binding name (regex has no cross-group backreference between name and
//! value), and it cannot compute entropy at all — which is why a passphrase-style credential
//! (`"correct-horse-battery-staple"`) was ALWAYS silent there, and a random base64url token was silent
//! whenever it drew no digits. This channel carries both judgments pre-computed at extraction time, so a
//! rule can make them without the value ever being stored.
//!
//! # The value is NEVER carried — hash + entropy only, and why that is the design
//!
//! A field holding the literal's raw text would write every candidate SECRET into `.zzop/cache`'s
//! plain-text JSON on every cold run — the channel would leak the exact class of data it exists to
//! protect. So the value is reduced AT EXTRACTION to the two judgments the consuming rule needs:
//!
//! - [`BoundStringLiteral::value_hash`] — [`value_hash_hex`], FNV-1a 64 over the value's UTF-8 bytes,
//!   16 lowercase hex chars. NOT cryptographic (FNV has no preimage resistance; a short or dictionary
//!   value can be brute-forced from its hash) — which is exactly why this channel also stays OFF the
//!   external wire (`docs/NORMALIZED_AST.md` has no counterpart field; see
//!   `zzop_engine::envelope::file_pass`'s call-site note for the shared boundary). Inside the local
//!   cache it answers the one equality question the rule asks: "is the value literally its own binding
//!   name?" (`refresh_token = "refresh_token"` — a sentinel, not a secret), by comparing against
//!   `value_hash_hex(name)`.
//! - [`BoundStringLiteral::entropy`] — [`shannon_entropy_bits`], TOTAL Shannon entropy in bits over the
//!   value's UTF-8 byte distribution: `H_total = -n * Σ p_i·log2(p_i)` where `p_i` is byte `i`'s
//!   frequency among the `n` bytes. Total (not per-byte) so length and diversity land in ONE number a
//!   threshold can gate: `"test-key"` ≈ 20 bits, `"correct-horse-battery-staple"` ≈ 97.9 bits, a
//!   24-char base64url token ≥ 85 bits (measured floor over 20k no-digit samples). Quantized to 1/8 bit
//!   (see [`shannon_entropy_bits`]) so the f32 is byte-stable under serialization. "The value" is
//!   load-bearing: producers whose CST hands them the source SPELLING rather than the cooked value
//!   (Java/C#/Go escaped literals) stay SILENT on any literal containing a `\` — spelling-entropy is a
//!   different quantity than value-entropy (measured: cooked 59.0 vs raw 87.1 on one tab-joined
//!   constant), so emitting it would gate the threshold on the wrong number. Each producer's module doc
//!   discloses that silence.
//!
//! # Never-guess
//!
//! A literal with no resolvable binding NAME emits nothing — not an entry with an empty or approximated
//! name. A destructuring pattern, a positional argument, an array element, a concatenation, an
//! interpolated/template string: all silent. Same consequence as `call_sites`: the channel under-reports
//! on purpose, so a rule reading it must treat silence as "no evidence", never as "no violation".
//!
//! Per-language capture scope (which binding forms emit, and each language's deliberate silences) is the
//! PRODUCER's to own in its module doc (`zzop_parser_*::extract_string_literals`); the per-environment
//! SSOT is `crates/engine/tests/rule_contracts/capability_matrix.rs`'s declared table.

use serde::{Deserialize, Serialize};

/// One string literal bound to a name. Category ② in the structural-fact projection contract (a
/// DSL-facing per-file fact — see `zzop_cache::FileIrSlice`'s module doc for what that membership
/// obligates): projected per file, cached per file, read directly by `crate::dsl::Matcher::LiteralScan`.
///
/// `#[serde(rename_all = "camelCase")]` for consistency with the sibling fact types — load-bearing here
/// (unlike `CallSite`, where it documents intent): `value_hash` serializes as `valueHash`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundStringLiteral {
    /// The binding name EXACTLY as written — a `const`/`let`/field/property name, not lowercased, not
    /// separator-normalized. A literal whose name cannot be resolved statically emits no entry at all
    /// (never-guess, module doc).
    pub name: String,
    /// 1-based source line of the literal. Same contract as `CallSite::line`: a producer that cannot
    /// place the line emits nothing; consumers may defensively skip a `0`.
    pub line: u32,
    /// [`value_hash_hex`] of the literal's value — 16 lowercase hex chars, never the value itself (see
    /// module doc for the no-plaintext contract and what the hash can and cannot promise).
    pub value_hash: String,
    /// [`shannon_entropy_bits`] of the literal's value — total bits, quantized to 1/8 bit.
    pub entropy: f32,
}

/// POLICY THRESHOLD (T2 measured, 2026-08-03) — the entropy floor `security/high-entropy-secret` ships
/// in its `entropy_min`, in TOTAL Shannon bits ([`shannon_entropy_bits`]). This constant exists for the
/// census + drift pin, NOT for the engine to read: the pack JSON is the value the engine loads (a pack
/// cannot reference a Rust constant), and `crates/engine/tests/rule_contracts/literal_scan_threshold.rs`
/// asserts the loaded pack equals this spelling, so the two cannot drift silently.
///
/// Measured (scratch command: `node entropy-measure.mjs cases/trees` — separation sweep over the FP
/// inventory + corpus-mined secret-named bindings vs 20k 4-word diceware passphrases and 20k/length
/// no-digit base64url tokens):
/// - **Below 80, must stay silent**: `"PlaceholderSecretValue"` 75.7 (the decoy tree's hardest
///   negative), `"a7Fk29QmZx41Lp08Wd"` 75.1, `"authorization_code"` 64.3, `"refresh_token"` 41.4,
///   `"adsbygoogle"` 34.1. That 4.3-bit gap is NOT a stable margin: total bits are LENGTH-dominated
///   (`n·H`), so one character flips it — `"PlaceholderSecretValues"` (one appended `s`, 23 chars)
///   measures 81.6 and fires, and an all-distinct-byte string clears the floor from n=19 on
///   (`n·log2(n)`: 75.0 at 18, 80.75 at 19). The floor separates the measured CLASSES (4-word
///   passphrases and long tokens vs short placeholders), never any individual value near the
///   boundary.
/// - **At or above 80, must fire**: `"correct-horse-battery-staple"` 97.9,
///   `"trombone_ravine_wallet_ember"` 99.5, 4-word diceware p5 = 82.98 (miss rate 2.86%), 24-char
///   no-digit base64url floor = 85.02 (miss rate 0%), 32/43-char floors 122.2/177.4.
/// - **Published misses**: 3-word phrases sit mostly below (88.5% missed at 80) — a 3-word diceware
///   phrase is itself weak; and an English-word identifier chain ≥ ~25 chars bound to a secret name
///   (`"mantine-DatePickerInput-input"` 107.4) is indistinguishable from a passphrase by any
///   content-free statistic, so above the floor the rule deliberately treats it as one.
pub const HIGH_ENTROPY_SECRET_MIN_BITS: f32 = 80.0;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64 of `value`'s UTF-8 bytes as 16 lowercase hex chars — [`BoundStringLiteral::value_hash`]'s
/// one producer, shared by every parser crate so a rule-side comparison (`value_hash_hex(name) ==
/// site.value_hash`) can never disagree with a producer about the function. Same algorithm family as
/// `zzop-cache`'s content digest (that crate doubles it to 128 bits; 64 suffices here — the comparison
/// is an equality convenience within one file, not content addressing), re-stated rather than imported
/// because `zzop-cache` depends on this crate, not the reverse.
///
/// Deterministic across platforms and runs: pure integer arithmetic, no seeding.
pub fn value_hash_hex(value: &str) -> String {
    let mut hash = FNV_OFFSET;
    for &b in value.as_bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

/// TOTAL Shannon entropy of `value`'s UTF-8 byte distribution, in bits: `H_total = n · H` where
/// `H = -Σ p_i·log2(p_i)` over the byte frequencies `p_i = count_i / n`. Total rather than per-byte so
/// one threshold gates length AND diversity together (module doc has the measured landmarks).
///
/// Quantized to 1/8 bit (`(h · 8).round() / 8`) before the f32 narrowing. Two reasons, both about
/// determinism: every multiple of 0.125 below 2²¹ is exactly representable in f32, so serialization is
/// byte-stable ("same value → same JSON bytes", pinned in this module's tests); and `log2` is the one
/// libm call in the pipeline whose last ULP is not guaranteed identical across platforms — 1/8-bit
/// buckets absorb that, so a cached entropy cannot flip a threshold comparison between machines unless
/// the pre-quantization total lands within a `log2` ULP of a bucket midpoint — possible in principle,
/// never observed, and bounded to one bucket either way. (The exact-representability guarantee lapses
/// above 2²¹ total bits — a ~500 KB bound literal — where f32 spacing exceeds 1/8; determinism still
/// holds there because the same f64 narrows the same way, only the "every 0.125 is exact" claim stops.)
/// The rule-side cost is nil: thresholds are measured in whole bits.
pub fn shannon_entropy_bits(value: &str) -> f32 {
    let bytes = value.as_bytes();
    let n = bytes.len();
    if n == 0 {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    for &b in bytes {
        counts[b as usize] += 1;
    }
    let n_f = n as f64;
    let mut h = 0.0f64;
    for &c in counts.iter().filter(|&&c| c > 0) {
        let p = f64::from(c) / n_f;
        h -= p * p.log2();
    }
    let total = h * n_f;
    ((total * 8.0).round() / 8.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a64_matches_the_published_test_vectors() {
        // Reference vectors for FNV-1a 64 (offset 0xcbf29ce484222325, prime 0x100000001b3):
        // the empty string hashes to the offset basis; "a" is the canonical one-byte vector.
        assert_eq!(value_hash_hex(""), "cbf29ce484222325");
        assert_eq!(value_hash_hex("a"), "af63dc4c8601ec8c");
    }

    #[test]
    fn entropy_landmarks_match_the_measured_values() {
        // The values the A17 threshold measurement recorded (scripts command in the rule's message /
        // catalog row): a passphrase clears the 80-bit threshold, the decoy placeholder does not.
        assert_eq!(shannon_entropy_bits(""), 0.0);
        assert_eq!(shannon_entropy_bits("aaaa"), 0.0); // one symbol, zero surprise
        let passphrase = shannon_entropy_bits("correct-horse-battery-staple");
        assert!(
            (97.8..98.0).contains(&passphrase),
            "measured 97.9 total bits, got {passphrase}"
        );
        let placeholder = shannon_entropy_bits("PlaceholderSecretValue");
        assert!(
            (75.6..75.8).contains(&placeholder),
            "measured 75.7 total bits, got {placeholder}"
        );
    }

    /// The f32 determinism pin: same input → same f32 bits → same serialized bytes, and the quantized
    /// value is EXACTLY representable (re-parsing the JSON yields bit-identical f32).
    #[test]
    fn entropy_serialization_is_byte_stable() {
        for value in [
            "correct-horse-battery-staple",
            // Split so no contiguous vendor-token prefix survives in raw source (the
            // check-vendor-token-literals convention — see rules/dsl/security/vendor_token_committed.rs).
            concat!("sk_", "live_0123456789abcdef"),
            "x",
            "ab-cd",
        ] {
            let a = shannon_entropy_bits(value);
            let b = shannon_entropy_bits(value);
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "{value}: non-deterministic entropy"
            );
            // 1/8-bit quantization → f32-exact → serde_json round-trips to the identical bit pattern.
            assert_eq!(
                a.fract() * 8.0,
                (a.fract() * 8.0).round(),
                "{value}: not on the 1/8 grid"
            );
            let json = serde_json::to_string(&a).unwrap();
            assert_eq!(serde_json::to_string(&b).unwrap(), json);
            let back: f32 = serde_json::from_str(&json).unwrap();
            assert_eq!(
                back.to_bits(),
                a.to_bits(),
                "{value}: JSON round trip moved the bits"
            );
        }
    }

    #[test]
    fn bound_literal_serializes_camel_case_and_round_trips() {
        let lit = BoundStringLiteral {
            name: "apiKey".to_string(),
            line: 3,
            value_hash: value_hash_hex("correct-horse-battery-staple"),
            entropy: shannon_entropy_bits("correct-horse-battery-staple"),
        };
        let json = serde_json::to_string(&lit).unwrap();
        assert!(
            json.contains("\"valueHash\""),
            "camelCase wire field: {json}"
        );
        let back: BoundStringLiteral = serde_json::from_str(&json).unwrap();
        assert_eq!(back, lit);
    }
}
