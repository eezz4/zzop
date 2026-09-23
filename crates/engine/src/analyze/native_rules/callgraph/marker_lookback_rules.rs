//! The two http rules that read SOURCE TEXT: `unsafe-read-endpoint` and `non-idempotent-write`.
//!
//! They sit here rather than inline in [`super::run_callgraph_rules`] because what they share is not
//! their subject (both judge a route's handler, as `mutating-route-no-auth` does too) but their
//! **input port**: each one wants the few lines above a handler declaration, to see whether an
//! `// idempotent-ok:` marker suppresses it. Every other rule in this pass judges facts already in
//! memory — `RawCall`s, symbols, the graph — and needs no file at all.
//!
//! 🔴 That port is a READER, and it is the reason this module was cut out (2026-09-06, external review
//! lane B). These two used to be handed a `HashMap<String, String>` holding every TypeScript file in
//! the tree, which the caller filled during its parse loop and kept alive to the end of the pass. Peak
//! RSS then tracked tree bytes at close to 1:1. Measured on a 156.3 MB synthetic TS tree
//! (400 files + one route), warm, three runs each:
//!
//! | | peak working set |
//! |---|---|
//! | map of every file's text | 177.3 / 177.3 / 178.2 MB |
//! | reader (this module) | 26.2 / 26.1 / 27.1 MB |
//!
//! Output is byte-identical (checked warm on `corpus/frameworks/grafana`, 86,971 bytes), because the
//! rules read exactly the same lines — they just no longer ask for them all up front.
//!
//! **Do not reintroduce a text map here as a "cache".** The reader costs one `fs::read` per candidate
//! FINDING, not per file, and findings are rare by construction: a tree with 8,993 TypeScript files can
//! produce a handful of these. Trading an unbounded term for a bounded one was the point.

use std::time::Instant;

use zzop_core::callgraph::SymbolGraph;
use zzop_core::{ApiEndpoint, Finding, SourceSymbol};

use crate::analyze::record_native_timing;

/// Everything the two scans judge. Grouped into a struct because the alternative is an eight-argument
/// function whose adjacent `bool`s mean opposite things.
pub(super) struct Args<'a> {
    /// Tree root. The reader resolves rel paths against it; nothing else here touches the disk.
    pub root: &'a std::path::Path,
    pub api_endpoints: &'a [ApiEndpoint],
    pub all_symbols: &'a [SourceSymbol],
    pub symbol_graph: &'a SymbolGraph,
    pub run_unsafe_read: bool,
    pub run_non_idempotent: bool,
    pub profile: bool,
}

/// Runs whichever of the two rules is enabled, appending to `global_findings`.
///
/// Both are skipped wholesale when neither is on, and the reader is built either way — it is a closure
/// over `root`, so building it allocates nothing and reads nothing.
pub(super) fn run(
    args: &Args<'_>,
    rule_time: &mut std::collections::HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
) {
    // One file at a time, on demand, and never retained past the line scan that asked for it.
    //
    // `from_utf8_lossy` matches how the caller's parse loop read the same bytes, so a marker in a file
    // with invalid UTF-8 is seen identically by both. A file that cannot be read yields `None`, which
    // suppresses nothing: the scan skips that declaration rather than treating unreadable as marked.
    let read_file = |rel: &str| -> Option<String> {
        std::fs::read(args.root.join(rel))
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    if args.run_unsafe_read {
        let t0 = args.profile.then(Instant::now);
        let found = zzop_rules_http::scan_unsafe_read_endpoint(
            &zzop_rules_http::ScanUnsafeReadEndpointInput {
                api_endpoints: args.api_endpoints,
                symbols: args.all_symbols,
                symbol_graph: args.symbol_graph,
                read_file: &read_file,
            },
        );
        record_native_timing(rule_time, t0, "unsafe-read-endpoint", found.len());
        global_findings.extend(found);
    }
    if args.run_non_idempotent {
        let t0 = args.profile.then(Instant::now);
        let found = zzop_rules_http::scan_non_idempotent_write(
            &zzop_rules_http::ScanNonIdempotentWriteInput {
                api_endpoints: args.api_endpoints,
                symbols: args.all_symbols,
                symbol_graph: args.symbol_graph,
                read_file: &read_file,
            },
        );
        record_native_timing(rule_time, t0, "non-idempotent-write", found.len());
        global_findings.extend(found);
    }
}
