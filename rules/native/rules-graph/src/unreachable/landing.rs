//! The §27 landing clause `unreachable` splices AHEAD of its own imperative, in its own module so the one
//! spelling stays the one spelling — the position pin in `tests.rs` compares an index against this
//! constant, and a second copy would make that comparison meaningless.
//!
//! ## Why this is a constant of its own and not the sibling's sentence
//!
//! `dead_candidates` already ships a long clause about frameworks that load a file from its own path and
//! about a repository holding half a product, and reusing it here would be the "nearest sibling is the
//! most dangerous reuse" trap `rule-quality.md` §37 records. Two facts separate them, and both are
//! mechanical rather than editorial:
//!
//! 1. **Cardinality.** That rule reports a file with ZERO importers and its prescription deletes ONE file.
//!    This rule reports a file with one or more importers whose whole component is unreachable, and its
//!    prescription — "delete the island" — deletes every file in that component. The finding names one.
//! 2. **A different entrypoint set, which is the part a reader cannot possibly guess.** `dead-candidates`
//!    is handed `entries::collect(...)`: package-manifest fields, adapter overlays, tool-config declared
//!    entries and framework directory conventions. THIS rule is handed only the Cargo workspace's target
//!    roots plus the prescan / asset-URL / auto-import targets (`assemble::rules`), on top of its own
//!    filename conventions, test files and zero-fan-in files. So a manifest entry that makes
//!    `dead-candidates` stand down starts no walk here at all — and everything downstream of it becomes a
//!    closed island by construction. That asymmetry is what the sentence is for.
//!
//! Deliberately silent on how COMMON any of that is: unmeasured, and a message that teaches a reader to
//! doubt true findings costs more than the one it saves (rule-quality.md §30/§32). It states what the walk
//! started from, which is a fact about this code.

/// How the `unreachable` prescription LANDS — spliced AHEAD of the imperative (rule-quality.md §27), and
/// behind the rule's own disqualifier (§38: whether the finding is TRUE is a separate question).
///
/// File-independent on purpose: the path belongs to the finding's anchor and the imperative, so the
/// position pin has ONE spelling to compare an index against.
pub(super) const ISLAND_DELETION_LANDING: &str = "COUNT THE FILES IN THE ISLAND BEFORE YOU DELETE IT, AND \
     READ THE ENTRYPOINT LIST BEFORE YOU COUNT: this finding names ONE file, but the prescription removes \
     the whole closed component it sits in, and this line does not list the other members. What decides \
     whether that component is really dead is the entrypoint set this walk started from, and that set is a \
     list of conventions rather than a fact — conventional entry FILENAMES, test files, files nothing \
     imports at all, Cargo target paths, and the files a pre-scan, asset-URL or auto-import mechanism \
     reaches. The entry fields of a package manifest (main, module, bin, exports), an entry a build \
     tool's own configuration file declares, and a framework's directory convention start NO walk here, \
     so a published library's own entry point, a \
     plugin loaded by name at runtime, or a half of the product built from a repository that is not on \
     disk leaves everything downstream of it looking closed. Find the loader before you count the files.";
