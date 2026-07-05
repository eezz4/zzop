//! Content-addressing hash — FNV-1a 64, doubled to a 128-bit digest.
//!
//! **Not a cryptographic hash.** FNV-1a has no resistance to a deliberately crafted collision. For
//! this crate's use — content-addressing source bytes and cache-key strings within one repo's
//! working tree — the risk that matters is *accidental* collision, and doubling FNV-1a 64 into a
//! 128-bit digest pushes that probability down to the birthday bound of a 128-bit space, far below
//! any realistic corpus size.
//!
//! Defense in depth: every cache entry re-stores its full key inside its JSON payload, and every
//! read compares the stored key against the requested key by exact string equality (see `store.rs`).
//! The digest here is only used to shard entries into a flat directory, never compared for equality
//! on its own, so a digest collision in a *filename* can at worst cause one entry to overwrite
//! another; the next lookup for either key's contents finds a mismatch and treats it as a miss,
//! never a wrong hit.
//!
//! What this cannot protect against: two different file contents producing the same `content_hash`
//! digest itself. That risk is accepted by design (the cache key is a content hash, not a byte
//! compare) — if a cryptographic digest is ever needed, `digest128` can switch to one (e.g. `sha2`)
//! without changing its `&[u8] -> String` interface.

const FNV_OFFSET_A: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME_A: u64 = 0x0000_0100_0000_01b3;
// Second, independent (offset, prime) pair for the other half of the digest — chosen only to
// decorrelate the two halves so a collision in one is unlikely to also collide in the other.
const FNV_OFFSET_B: u64 = 0x8422_2325_cbf2_9ce4;
const FNV_PRIME_B: u64 = 0x0000_01b3_1000_0001;

fn fnv1a64(bytes: &[u8], offset: u64, prime: u64) -> u64 {
    let mut hash = offset;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(prime);
    }
    hash
}

/// 128-bit (32 hex char) content-addressing digest of `bytes`. Deterministic across process runs
/// and platforms (no `Hasher`/`RandomState` seeding, unlike `std::hash`).
pub fn digest128(bytes: &[u8]) -> String {
    let a = fnv1a64(bytes, FNV_OFFSET_A, FNV_PRIME_A);
    let b = fnv1a64(bytes, FNV_OFFSET_B, FNV_PRIME_B);
    format!("{a:016x}{b:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        assert_eq!(digest128(b"hello"), digest128(b"hello"));
    }

    #[test]
    fn distinguishes_different_input() {
        assert_ne!(digest128(b"hello"), digest128(b"hellp"));
    }

    #[test]
    fn distinguishes_empty_from_nonempty() {
        assert_ne!(digest128(b""), digest128(b"a"));
    }

    #[test]
    fn is_32_hex_chars() {
        let d = digest128(b"some file content\nwith multiple lines\n");
        assert_eq!(d.len(), 32);
        assert!(d.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn distinguishes_concatenation_ambiguity() {
        // This crate always joins fields with a NUL separator (see store.rs), not concatenation —
        // the hash must still tell adjacent byte patterns apart for that scheme to be meaningful.
        assert_ne!(digest128(b"ab\0c"), digest128(b"a\0bc"));
    }
}
