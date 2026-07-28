//! `cache-lane-file-read` — a symbol the project declared as a CACHED LANE reaches, through the call
//! graph, a callee whose name is in the declared filesystem-read vocabulary.
//!
//! # The invariant, stated as the user's problem rather than ours
//! Every incremental build system has the same bug class: a memoized unit reads an input that its cache
//! KEY does not cover, so the cache serves a stale answer forever and nothing looks broken. The read is
//! correct, the key is correct, and the pair is wrong. Nobody sees it because the failure is silence.
//!
//! Textual guards cannot answer this. A scan for `std::fs` inside the lane's own file catches the direct
//! call and misses the one that matters — the helper one hop away, in another file, that reads a shared
//! manifest. Following the call path is exactly what a call graph is for, so this is a rule rather than
//! another `scripts/check-*.sh`.
//!
//! # Shape: `mutating-route-no-auth` with the polarity flipped
//! Same three parts — anchor symbols, a BFS over the whole-repo `SymbolGraph`, and a name vocabulary at
//! the far end. The difference is only the direction of the verdict: there, reaching the vocabulary
//! CLEARS a route; here, reaching it IS the finding. Everything the sibling rule learned about not
//! guessing carries over unchanged.
//!
//! # Why the sink is a per-symbol fact and not a graph edge
//! The obvious design — BFS until a node's id looks like `read_to_string` — cannot work, and the reason
//! is worth writing down because it would otherwise be rediscovered as a mystery. `std::fs::read` is an
//! EXTERNAL crate head; `zzop_engine`'s resolver returns `None` for it and the edge is dropped rather
//! than guessed (the same discipline that keeps every other cross-crate edge honest). So the sink never
//! becomes a node and a node-predicate BFS is structurally blind to it.
//!
//! The fix is to ask the question one level down, exactly as `non-idempotent-write` does with
//! `SourceSymbol::write_sites`: walk the graph over RESOLVED edges to find which symbols are reachable,
//! then at each reachable symbol check the raw callee NAMES it spelled. Reachability needs resolution;
//! the sink does not — a callee name is text observed at a real call site whether or not anything
//! resolved it. [`CacheLaneCallSites`] is that per-symbol fact, and the engine builds it from the same
//! `RawCall`s it already gathers for the graph, so no parser learns anything new.
//!
//! # Both vocabularies are DECLARED, and an undeclared one means silence
//! What a project calls its cached lane is a convention (`compute_fresh_artifact`), and so is what counts
//! as reading the world (`read_to_string`, `read_dir`, a project's own `load_manifest`). Neither is a
//! fact this engine can know, so both are `vocabulary.*` keys and an undeclared one makes the judgment
//! NOT — the rule emits nothing rather than inventing an anchor set. That is the D14 rule applied
//! literally, and here it is also the only safe direction: a guessed anchor would report a function that
//! was never promised to be pure.
//!
//! # Declared limits
//! - **Single hop past an unresolved edge.** Inherited from the engine's call-graph wiring: a target id
//!   nothing else has outgoing edges from ends the walk. A read three files away behind two unresolved
//!   specifiers is not found. The rule under-reports; it does not invent.
//! - **A path-qualified call to a non-imported module is not an edge**, measured on this repo rather
//!   than reasoned about. `zzop_core::callgraph::resolve_method` resolves a receiver through the calling
//!   file's `ImportMap` or its own local symbols, and a Rust `super::artifact::probe(..)` offers
//!   neither — `artifact` is a sibling MODULE, not an imported binding or a declared symbol — so the
//!   edge is dropped rather than guessed. What DOES resolve, and what the measurement below used, is the
//!   idiomatic majority: a call to a name the file imported (`use super::parsers::lexical_loc;` then
//!   `lexical_loc(text)`), which resolves through the real Rust module resolver.
//!
//!   Measured on zzop itself, anchor `^compute_fresh_artifact$`: a read planted IN the lane reports at
//!   depth 0; a read planted in an imported cross-file callee reports at depth 1 naming that callee; a
//!   read behind `super::artifact::probe(..)` reports nothing. That third case is the honest edge of this
//!   rule's reach today, and closing it belongs to the resolver, not here.
//! - **A name is not a proof.** `read_to_string` is a `Read` trait method, not only `std::fs`'s free
//!   function, so a project that reads an in-memory buffer through it can be reported. The vocabulary is
//!   the project's to narrow, and the finding names the exact symbol and callee so the judgment is
//!   checkable in one jump.
//! - **Macro bodies and dynamic dispatch are invisible**, per each parser's own declared blindness.

use zzop_core::callgraph::{bfs_reachable, SymbolGraph};
use zzop_core::{Finding, Severity, SourceSymbol};

/// Built-in default for `vocabulary.fileReadCallees` — standard-library spellings that read the
/// filesystem in the ecosystems this engine parses, NOT names a project picks. That asymmetry is why
/// this half has a default and the anchor half deliberately does not: everyone's `read_to_string` is
/// the same function, while everyone's cache lane has a different name.
///
/// Rust (`std::fs`, `tokio::fs`) and Node (`node:fs`) both appear because the graph is language-neutral;
/// a name that means nothing in a given tree simply never matches. Method-shaped names only — the
/// receiver (`fs`, `File`, `fs.promises`) is dropped by every `RawCall` producer, so matching on
/// `fs::read` would match nothing anywhere.
pub const DEFAULT_FILE_READ_CALLEES: &[&str] = &[
    "read",
    "read_to_string",
    "read_to_end",
    "read_dir",
    "read_link",
    "open",
    "metadata",
    "canonicalize",
    "readFile",
    "readFileSync",
    "readdir",
    "readdirSync",
    "existsSync",
    "statSync",
];

/// Per-symbol callee names, the sink half of the check — see module doc "Why the sink is a per-symbol
/// fact". Keyed by the CALLER's symbol id, holding the raw `callee_name`s it spelled, resolved or not.
pub type CacheLaneCallSites<'a> =
    std::collections::HashMap<&'a str, std::collections::BTreeSet<&'a str>>;

/// Input for [`scan_cache_lane_file_read`].
pub struct ScanCacheLaneFileReadInput<'a> {
    /// Every symbol this run extracted — the anchor set is selected out of it by name.
    pub symbols: &'a [SourceSymbol],
    /// Whole-repo resolved call graph, used for REACHABILITY only.
    pub symbol_graph: &'a SymbolGraph,
    /// Raw callee names per calling symbol — the sink evidence. See [`CacheLaneCallSites`].
    pub call_sites: &'a CacheLaneCallSites<'a>,
    /// How this project spells the entry point of a cached per-file lane, matched against a symbol's
    /// NAME. `None` — undeclared — means this rule makes no judgment at all and returns empty.
    pub cache_lane_anchor_pattern: Option<&'a str>,
    /// Callee names that count as reading the filesystem. EMPTY means no name can prove a read, which is
    /// again the whole judgment not being made.
    pub file_read_callees: &'a [&'a str],
}

/// One finding per anchor symbol that reaches a declared file-read callee. Empty when either vocabulary
/// is undeclared, which is the "not judged" state rather than a clean bill of health.
pub fn scan_cache_lane_file_read(input: &ScanCacheLaneFileReadInput) -> Vec<Finding> {
    let Some(anchor_src) = input.cache_lane_anchor_pattern.filter(|p| !p.is_empty()) else {
        return Vec::new();
    };
    if input.file_read_callees.is_empty() {
        return Vec::new();
    }
    // A vocabulary that does not compile is a declaration this run cannot honor. Returning empty (rather
    // than falling back to a built-in) keeps "undeclared" and "unusable" in the same honest state; the
    // engine's own config layer is what reports a bad pattern to the author.
    let Ok(anchor_re) = regex::Regex::new(anchor_src) else {
        return Vec::new();
    };

    let anchors: Vec<&SourceSymbol> = input
        .symbols
        .iter()
        .filter(|s| anchor_re.is_match(&s.name))
        .collect();
    if anchors.is_empty() {
        return Vec::new();
    }

    let reads_a_file = |id: &str| -> Option<&str> {
        let names = input.call_sites.get(id)?;
        input
            .file_read_callees
            .iter()
            .find(|c| names.contains(**c))
            .copied()
    };

    let mut out = Vec::new();
    for anchor in anchors {
        let Some((reached, depth)) = bfs_reachable(input.symbol_graph, &anchor.id, |id| {
            reads_a_file(id).is_some()
        }) else {
            continue;
        };
        // The predicate just proved this is `Some`; re-reading it is how the CALLEE gets into the
        // message, which is what makes the finding checkable in one jump rather than a hunt.
        let callee = reads_a_file(&reached).unwrap_or("");
        let hint = hint_for(&anchor.name, &reached, callee, depth);
        out.push(Finding {
            rule_id: "cache-lane-file-read".to_string(),
            severity: Severity::Warning,
            file: anchor.file.clone(),
            line: anchor.line,
            message: hint.clone(),
            data: Some(serde_json::json!({
                "anchor": anchor.name,
                "anchorSymbol": anchor.id,
                "reachedSymbol": reached,
                "callee": callee,
                "depth": depth,
                "hint": hint,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

/// The finding's whole message. Says what was found, why it is a defect rather than a style note, and
/// the two ways out — including "the read is fine, put it in the key", which is often the right answer
/// and which a message that only said "remove the read" would hide.
fn hint_for(anchor: &str, reached: &str, callee: &str, depth: u32) -> String {
    let path = if depth == 0 {
        format!("`{anchor}` itself calls `{callee}`")
    } else {
        format!("`{anchor}` reaches `{reached}` ({depth} hop(s) away), which calls `{callee}`")
    };
    format!(
        "Cache-lane escape: {path}. This symbol was declared a cached per-file lane \
         (`vocabulary.cacheLaneAnchorPattern`), which means its output is stored and replayed under a \
         key computed from its declared inputs. A filesystem read reachable from it is an input the key \
         almost certainly does not cover, so once the file it reads changes, every warm entry keeps \
         serving the old answer and nothing looks broken — the failure is silence, not an error. Two \
         ways out, and the first is often the right one: (1) keep the read but make what it read part of \
         the key, so a change invalidates the entry; (2) hoist the read to the lane's caller and pass \
         the value in, which is what makes the lane closed by construction. If this callee does not \
         actually touch the filesystem, narrow `vocabulary.fileReadCallees` — the vocabulary is yours. \
         {}",
        zzop_core::disable_hint("cache-lane-file-read"),
    )
}

#[cfg(test)]
mod tests;
