//! The server-level `initialize` orientation string — see [`ORIENTATION`].
//!
//! # What belongs here and what does not — ROUTING, not reply shape (review ledger V151)
//!
//! These instructions own the ONE fact no single tool can state: WHICH tool a given tree shape
//! calls. A per-tool description can only speak for its own side of that choice, so stating the
//! routing rule there would mean stating it in two descriptions, and two owners of one fact is how
//! this repo has been bitten repeatedly.
//!
//! The mirror rule is what this section had to be trimmed for: WHAT A REPLY CONTAINS is owned by
//! that tool's own description, never here. This file used to add that pointing the single-tree
//! tool at a parent directory does not produce the join — which analyze_repo's description already
//! says, more precisely, about its own reply ("this tool reports this tree's own per-tree findings
//! only"). Two statements of one fact, and the shorter one here would have been the one to rot.

/// Server-level orientation, shipped on every `initialize` under `instructions` (the spec's slot for
/// text a host may hand the model, like a system prompt).
///
/// Scope rule for this string: it states only what is true BEFORE any tool runs and cannot be said by
/// a tool description. Per-tool behaviour stays in the tool descriptions, which are this repo's most
/// honest surface — each names what it cannot do first — and duplicating any of it here would create
/// a second owner that drifts. What they structurally cannot say is that a config must exist at all,
/// which tree-shape picks which tool, and that a zero here is not a proof; those three are exactly
/// what an agent otherwise learns by failing.
///
/// 🔴 The first sentence is the PRODUCT's identity and had drifted out of sync with it (review ledger
/// V68). It read "zzop analyzes a repo's cross-layer contracts" — the identity the README carried until
/// the headline moved to reading a whole codebase deterministically. Three surfaces stated three things,
/// and this is the one an AGENT actually reads, which is the audience the product now names first. The
/// join is not demoted: it is the second half of the same sentence, and point 2 below still routes to it.
/// Keep this sentence in step with `README.md`'s headline; they are one claim with two spellings, and the
/// one that ships inside the binary is the one nobody re-reads.
pub(super) const ORIENTATION: &str = "zzop reads a whole repository and answers the same way every \
    time: one tree, or a frontend and a backend joined so you can see which calls reach which routes \
    and which reach nothing. Three things to know before calling anything:\n\
    1. EVERY tree-rooted analysis lane needs a config file (`zzop.config.jsonc`) in the tree — there \
    is no zero-config mode for a tree, and the one exception is analyze_envelope, which analyzes \
    envelope text with no tree and so has no config to require. If a call fails asking for one, read \
    the `zzop://contract/config-template` resource and write that file; do not guess its shape.\n\
    2. ONE tree -> the analyze_repo tool. TWO OR MORE trees (a frontend and a backend, a monorepo's \
    packages) -> the cross_repo tool. The cross-layer join only exists in the multi-tree lane, \
    which is what makes this choice matter.\n\
    3. An empty findings list is NOT a proof of correctness. Detection is total by default, but what \
    this build can see depends on the languages and frameworks it recognizes — before reporting a \
    clean result, read the reply's own coverage/disclosure fields where present (the names differ per \
    tool; check_file's signals are `verdict` and `warnings`).";
