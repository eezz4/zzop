//! The CST-depth cap: the one number three tree-sitter frontends refuse a parse tree by.
//!
//! ## Why this exists as a SECOND gate, when a pre-parse one already does
//!
//! `crates/engine/src/pipeline/fresh/recursion.rs` refuses pathological input BEFORE a parser runs,
//! and it reads two needles off the raw text: bracket nesting, and the longest binary-operator chain.
//! Both are proxies for the same thing — how deep a recursive walk will go — and a needle that names
//! shapes is only ever as complete as the list someone thought of.
//!
//! 📏 **Measured on 2026-09-08 (review ledger V129), four shapes walked straight through both needles
//! into an exit-127 abort** — zero findings, 53 bytes of stderr, every other file in the tree
//! unreported:
//!
//! | input (C#, one file) | size | outcome |
//! |---|---|---|
//! | `b ? 1 : b ? 1 : …` × 70,000 | 560 KB | abort |
//! | `!!!!…b` × 70,000 | **70 KB** | abort |
//! | `a.b.b.b…` × 70,000 | 140 KB | abort |
//! | `- - - b` × 70,000 | 140 KB | abort |
//!
//! The conditional and member chains contain no bracket and no counted operator at all. The last one
//! is the sharpest: every `-` in it IS in the operator set, and the scan's "a multi-byte operator
//! counts once" rule — correct for `&&` and `<<` — collapses the whole chain to a run of ONE.
//!
//! ## Why the tree and not a third text needle
//!
//! The obvious shape-independent needle is "significant tokens in the longest statement", since what
//! all four shapes share is that the file is one enormous statement. 📏 It was built and censused
//! across 58,919 files: the longest real statement is **12,734** tokens (a vendored yarn bundle),
//! against **70,001** for the cheapest killer above. So it CAN separate them.
//!
//! 🔴 **This paragraph said the opposite when this module first shipped, and the correction is the
//! reason to read it twice.** That census ran on a scanner that counted identifier BYTES where it
//! meant identifier TOKENS — an arm spliced into the wrong `match` — so every real-code number came
//! out three to six times too large (76,356 and 38,070 where the truth is 12,734), and the
//! populations appeared to overlap when they do not. A census is only as good as what it counts,
//! and a needle that names shapes is not the only thing that can be wrong about them.
//!
//! What survives the correction is a preference, not an impossibility, and the reason is the SIZE of
//! the window. A token count is a proxy for depth; measured on the finished tree, depth IS the
//! answer. The proxy's window is 12,734 against a 45,000 abort — about 2x on one side, 1.8x on the
//! other, and it moves whenever someone vendors a bigger generated file. The tree's window is 645
//! against roughly 60,000: 6.3x and ~15x, and it moves only when real code genuinely nests deeper.
//!
//! So the needle is kept for exactly the one frontend that can never have a tree to measure —
//! `parser-rust`, where the overflow happens inside the parse itself — and is scoped to it by
//! `MAX_RUST_STATEMENT_TOKENS`, whose own doc carries that census.
//!
//! ## What this measures instead
//!
//! The tree, not the text. tree-sitter's own parse is iterative and survives every input above; what
//! recurses is the per-language visitors that walk the result afterwards. So the honest place to ask
//! "how deep will they recurse" is the finished tree, where the answer is a fact rather than a proxy,
//! and the check itself is an iterative cursor walk that adds no stack of its own.
//!
//! 📏 **The census that fixes the number**, over 25,106 real files under `corpus/`, `cases/`,
//! `crates/`, `parser/`, `rules/`, `packages/` and `site-src/`:
//!
//! | language | files | p50 | p99 | max |
//! |---|---|---|---|---|
//! | C# | 10,662 | 15 | 36 | **54** |
//! | Go | 9,192 | 18 | 49 | **645** |
//! | Java | 5,252 | 13 | 28 | **111** |
//!
//! The deepest real file in any of them is a generated protobuf binding
//! (`terraform/internal/rpcapi/terraform1/stacks/stacks.pb.go`, depth 645) — generated code, but code
//! someone really analyzes, so the cap has to clear it.
//!
//! Re-derive the lower bound by re-running that walk over a corpus; the upper bound by bisecting one
//! of the reproductions in the table above until the exit code turns 127.

/// CST depth past which a parse tree is refused and the file comes back `degraded` instead of taking
/// the process down. See the module doc for the two measurements that bracket it.
///
/// 4,096 sits **6.3×** above the deepest real file measured in any of the three languages, and roughly
/// **15×** below the lowest abort floor measured (C#, between 50,000 and 70,000 for the shapes above).
/// It is not a taste call and must not be moved without re-taking both bounds.
pub const MAX_CST_DEPTH: usize = 4_096;

/// The deepest CST any real file reaches, over the census the module doc describes.
///
/// Kept beside the cap so the two move together: re-taking the census means updating this, and the
/// assertion below then says whether the cap still clears it. Same shape as the pre-parse gate's own
/// `DEEPEST_REAL_FILE_MEASURED`, and for the same reason.
pub const DEEPEST_REAL_CST_DEPTH_MEASURED: usize = 645;

// The cap must sit above the deepest real file, or this refuses code someone generated on purpose.
// A `const` assertion rather than a test: both sides are constants, so this fails the BUILD the moment
// someone lowers the cap below the census — a test would only fail when someone ran it.
const _: () = assert!(
    MAX_CST_DEPTH > DEEPEST_REAL_CST_DEPTH_MEASURED,
    "the CST-depth cap dropped to or below the deepest real file measured -- re-derive BOTH bounds \
     (the module doc says how) before moving either"
);

/// The three cursor moves a depth walk needs, and the reason this trait exists at all.
///
/// 🔴 The walk used to be COPIED into each tree-sitter frontend — three byte-identical bodies, with
/// a doc in each explaining that `zzop-core` owns the number but cannot own the walk because it does
/// not depend on `tree-sitter` and `scripts/check-dep-closure.sh` exists to keep it that way. That
/// reason was true and the conclusion did not follow (review ledger V132).
///
/// 📏 The dependency objection is real: `zzop-core` appears nowhere in
/// `scripts/dep-closure-baseline.txt`, and nearly every workspace crate depends on core — so a
/// `tree-sitter` edge there would put a parser in the closure of the whole workspace, which is the
/// exact property those guards were built to hold.
///
/// 🔵 But a walk over a cursor does not need the cursor's TYPE, only its three moves. Behind this
/// trait the walk has ONE owner and core stays parser-free — the objection is satisfied and the
/// duplication is gone, rather than one being traded for the other.
///
/// ⚠ Why the duplication was worth removing rather than tolerating: three copies diverge silently.
/// A cap edited in one frontend and missed in another leaves that language uncovered while its own
/// tests stay green, because each copy was tested against itself. That is not hypothetical here —
/// V130 was exactly a frontend nobody had covered, found four commits later by an outside reader.
pub trait CstCursor {
    /// Move to the first child; `false` when there is none (the cursor must not move).
    fn goto_first_child(&mut self) -> bool;
    /// Move to the next sibling; `false` when there is none.
    fn goto_next_sibling(&mut self) -> bool;
    /// Move to the parent; `false` at the root.
    fn goto_parent(&mut self) -> bool;
}

/// Whether the tree under `cursor` is deeper than [`MAX_CST_DEPTH`].
///
/// ITERATIVE, and that is load-bearing rather than a style note: a recursive depth check would
/// overflow the stack on exactly the inputs it exists to refuse.
///
/// The cursor is left where it started only when the answer is `false`; a `true` answer short-
/// circuits and the cursor is abandoned, which every caller does anyway (it refuses the tree).
pub fn exceeds_max_depth<C: CstCursor>(cursor: &mut C) -> bool {
    let mut depth = 0usize;
    loop {
        while cursor.goto_first_child() {
            depth += 1;
            if depth > MAX_CST_DEPTH {
                return true;
            }
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return false;
            }
            depth -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cursor over a plain nested-vector tree, so the walk is tested without any parser present —
    /// which is the whole point of it living here.
    struct Chain {
        depth: usize,
        at: usize,
    }
    impl CstCursor for Chain {
        fn goto_first_child(&mut self) -> bool {
            if self.at >= self.depth {
                return false;
            }
            self.at += 1;
            true
        }
        fn goto_next_sibling(&mut self) -> bool {
            false
        }
        fn goto_parent(&mut self) -> bool {
            if self.at == 0 {
                return false;
            }
            self.at -= 1;
            true
        }
    }

    #[test]
    fn a_chain_past_the_cap_is_refused_and_one_at_the_cap_is_not() {
        assert!(exceeds_max_depth(&mut Chain {
            depth: MAX_CST_DEPTH + 1,
            at: 0
        }));
        assert!(!exceeds_max_depth(&mut Chain {
            depth: MAX_CST_DEPTH,
            at: 0
        }));
    }

    /// The walk must not recurse — it is refusing inputs that would overflow a recursive one.
    #[test]
    fn the_walk_itself_does_not_recurse() {
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                assert!(exceeds_max_depth(&mut Chain {
                    depth: 5_000_000,
                    at: 0
                }))
            })
            .expect("spawn")
            .join()
            .expect("a 256 KB stack must be enough for a five-million-deep walk");
    }
}
