//! The server-level `initialize` orientation string — see [`ORIENTATION`].

/// Server-level orientation, shipped on every `initialize` under `instructions` (the spec's slot for
/// text a host may hand the model, like a system prompt).
///
/// Scope rule for this string: it states only what is true BEFORE any tool runs and cannot be said by
/// a tool description. Per-tool behaviour stays in the tool descriptions, which are this repo's most
/// honest surface — each names what it cannot do first — and duplicating any of it here would create
/// a second owner that drifts. What they structurally cannot say is that a config must exist at all,
/// which tree-shape picks which tool, and that a zero here is not a proof; those three are exactly
/// what an agent otherwise learns by failing.
pub(super) const ORIENTATION: &str = "zzop analyzes a repo's cross-layer contracts (which frontend calls reach \
    which backend endpoints, and which do not). Three things to know before calling anything:\n\
    1. EVERY tree-rooted analysis lane needs a config file (`zzop.config.jsonc`) in the tree — there \
    is no zero-config mode for a tree, and the one exception is analyze_envelope, which analyzes \
    envelope text with no tree and so has no config to require. If a call fails asking for one, read \
    the `zzop://contract/config-template` resource and write that file; do not guess its shape.\n\
    2. ONE tree -> the analyze_repo tool. TWO OR MORE trees (a frontend and a backend, a monorepo's \
    packages) -> the cross_repo tool. The cross-layer join is the point of this server, and it only \
    exists in the multi-tree lane; pointing the single-tree tool at a parent directory does not \
    produce it.\n\
    3. An empty findings list is NOT a proof of correctness. Detection is total by default, but what \
    this build can see depends on the languages and frameworks it recognizes — before reporting a \
    clean result, read the reply's own coverage/disclosure fields where present (the names differ per \
    tool; check_file's signals are `verdict` and `warnings`).";
