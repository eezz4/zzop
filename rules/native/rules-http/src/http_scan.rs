//! `scan_unsafe_read_endpoint` + `scan_non_idempotent_write` — the two native whole-graph rules in this
//! crate that need call-graph BFS. `apiChurn` (needs git-history joins) and `feBeSpecDrift` (cross-service
//! type drift) are out of scope: both need capabilities beyond a single-repo call graph.
//!
//! Both scanners resolve a method-gated `ApiEndpoint`'s handler to a symbol id, then BFS downstream over
//! the whole-repo `SymbolGraph` (`zzop_core::callgraph::bfs_reachable`) until a symbol carrying a
//! qualifying write site is found (lowest depth wins; ties break by symbol id ascending). Write-site
//! detection itself is NOT done here: it is a structural attribute computed once at TS parse time
//! (`zzop_parser_typescript::write_sites_for_symbol`, feeding `SourceSymbol::write_sites`) rather than a
//! regex re-scan of each BFS-reached symbol's raw text on every analysis run — see that function's module
//! doc for the detection rules (vocabulary, SQL-vs-ORM precedence, the `unsafe-read-endpoint`-specific
//! counter-site exclusion) and their two narrowing consequences, both unchanged by the move: a nested
//! function's body is included in its outer symbol's scanned span, so a write inside it attributes to the
//! outer symbol; and a raw-SQL label truncates at the first newline, so a multi-line statement's label can
//! be incomplete.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use zzop_core::SourceSymbol;

mod non_idempotent;
mod unsafe_read;

pub use non_idempotent::{scan_non_idempotent_write, ScanNonIdempotentWriteInput};
pub use unsafe_read::{scan_unsafe_read_endpoint, ScanUnsafeReadEndpointInput};

/// SIZE of the `idempotent-ok` marker window, in lines, shared by both scanners. The window ENDS at the
/// handler's body-start line and extends upward, so `4` means "the body-start line plus the 3 lines above
/// it" — NOT "the 4 lines above the handler", which would reach one line higher than the scan ever does.
/// [`scan_marker_window`] is the definition; [`OK_MARKER_LOOKBACK_ABOVE`] is the "above" count the messages
/// quote, derived here so the two can never drift.
const OK_MARKER_LOOKBACK_LINES: u32 = 4;

/// How many lines ABOVE the body-start line the window reaches — the number user-facing text quotes.
///
/// POLICY VALUE, T2: this number is ALSO spelled by hand, in English prose, in three files outside this
/// crate — `docs/getting-started.md`, `site/rules.html`, `site/usage.html` — because a Markdown/HTML page
/// cannot reference a Rust constant. Do not edit it alone: the phrase
/// [`marker_window_phrase`] builds is the mirrored thing, and
/// `the_marker_window_phrase_is_identical_in_the_finding_and_the_published_docs` (this module's `tests`)
/// asserts the finding message and all three pages carry the SAME rendered phrase.
const OK_MARKER_LOOKBACK_ABOVE: u32 = OK_MARKER_LOOKBACK_LINES - 1;

/// The one rendering of the marker window that every user-facing surface must use verbatim — the finding
/// message below splices it in, and the published docs are pinned against it. Sealing a whole PHRASE
/// rather than the bare number is deliberate: `3` alone appears in prose for a dozen unrelated reasons,
/// so a bare-number pin would be satisfied by an unrelated sentence and prove nothing.
fn marker_window_phrase() -> String {
    format!("body-start line or up to {OK_MARKER_LOOKBACK_ABOVE} lines above")
}

const SAFE_METHODS: [&str; 2] = ["GET", "HEAD"];
/// The write-verb vocabulary this crate gates on, and the OWNER of that set for everything downstream.
///
/// `pub(crate)` until 2026-07-28, which is what let four respellings accumulate outside this crate,
/// each doc claiming to be pinned to it — a claim nothing could honour, since nothing outside could
/// read the symbol. Downstream now either imports it (T1) or pins equality against it in
/// `crates/engine/tests/rule_contracts/policy_pins.rs` (T2, and see that test for the measurement).
pub const WRITE_HTTP_METHODS: [&str; 4] = ["PUT", "DELETE", "POST", "PATCH"];

/// Extensions whose parser actually PRODUCES the `SourceSymbol::write_sites` evidence both scanners in
/// this module consume — see [`write_site_sightline`] for why this must be published in the findings.
///
/// POLICY VALUE, T2: the same list is also spelled by hand, in English prose, in `docs/rules/catalog.md`,
/// `site/rules.html` and `docs/getting-started.md` (a Markdown/HTML page cannot reference a Rust
/// constant), and it duplicates `zzop_engine::dead_exports::is_ts_source_ext`'s match arm — this crate
/// depends on `zzop_core` only, so it cannot import that predicate. Both duplications are pinned:
/// `zzop_engine`'s `call_graph_covered_extensions_pin` checks the engine side, and this module's
/// `tests::the_write_site_sightline_is_identical_in_the_finding_and_the_published_docs` checks the prose.
///
/// Deliberately NARROWER than `mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS`, which is this list
/// PLUS `"java"`, `"py"`/`"pyi"` and `"rs"`: those parsers feed the shared `SymbolGraph` real call edges,
/// so the auth-guard BFS can walk their handlers — but no Java (or Python/Go/Rust/C#) parser fills
/// `write_sites`, so the two scanners here have no write EVIDENCE to find at the end of that walk. That
/// asymmetry is also what makes the Rust extractor-guard edges (`zzop_parser_rust::
/// parse_extractor_guards`) inert for these two rules: extra edges out of a Rust handler can only ever
/// reach the auth vocabulary, never a write site that does not exist. A tree can therefore show
/// `mutating-route-no-auth` findings while these two are structurally zero on the very same routes.
pub const WRITE_SITE_COVERED_EXTENSIONS: &[&str] =
    &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"];

/// The MARKUP-FREE claim every user-facing surface must carry verbatim: both findings splice it in,
/// and `docs/rules/catalog.md` / `site/rules.html` / `docs/getting-started.md` are pinned against it by
/// `tests::the_write_site_sightline_is_identical_in_the_finding_and_the_published_docs`.
///
/// Markup-free on purpose (no backticks, no quotes): an HTML page would have to escape them, and an
/// escaped copy is no longer byte-comparable — the pin would then be unenforceable exactly where the
/// drift is easiest. The extension list inside it comes from [`WRITE_SITE_COVERED_EXTENSIONS`], so the
/// one thing most likely to go stale is the one thing that cannot.
fn write_site_sightline_claim() -> String {
    format!(
        "needs store-write evidence that only the TypeScript parser produces ({})",
        WRITE_SITE_COVERED_EXTENSIONS.join("/")
    )
}

/// The full sightline sentence both findings append — [`write_site_sightline_claim`] plus the "what
/// silence means" reading.
///
/// Why a finding has to carry it: neither scanner emits anything when it finds no write site, and
/// `write_sites` is filled by the TypeScript parser alone. So for a Python/Go/Rust/C#/Java route the
/// BFS predicate is false BY CONSTRUCTION, and the rule's silence carries no information about the
/// route at all. Same silent-failure class `mutating_route_no_auth`'s LANGUAGE SIGHTLINE closes, and
/// the same fix: quote the covered set from the constant so the published sightline can never drift.
///
/// Known reach limit (the reason the docs surfaces below matter more than this one): a message only
/// ships ON a finding, and the repos this sightline is about produce ZERO findings — so for the reader
/// who most needs it, this sentence never renders. It is here for the mixed-language repo (TS findings
/// present, Java routes silently unchecked); the catalog/site/getting-started copies are what reach
/// the all-Java reader, and `zzop_engine::disclosure`'s `rule-evidence-language-gap` class is what
/// reaches a machine consumer on a zero-finding run.
fn write_site_sightline() -> String {
    format!(
        "LANGUAGE SIGHTLINE: this check {claim} — `SourceSymbol::write_sites`, which \
         parser-python-3/go/rust/csharp/java-21 all leave empty, so a handler in those languages has \
         no write site the call-graph BFS could reach and this rule cannot fire there at all. Easiest \
         to misread: the sibling `mutating-route-no-auth` rule DOES walk Java, so a Java repo can show \
         that rule's findings while this rule stayed dark on the very same routes. ZERO findings of \
         this rule outside {exts} therefore means NOT ANALYZED, never \"no risky write on these \
         routes\".",
        claim = write_site_sightline_claim(),
        exts = WRITE_SITE_COVERED_EXTENSIONS.join("/")
    )
}

fn ok_marker_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"//\s*idempotent-ok:").unwrap())
}

/// The honored marker, spelled the way the messages tell an author to write it.
const OK_MARKER_SPELLING: &str = "// idempotent-ok: <reason>";

/// Detector 2 — THIS marker's own stem, misspelled or mispunctuated (`// idempotent-okay:`,
/// `// idempotent-ok - reason`). The shared marker shape (detector 1, see [`near_miss_res`]) cannot see
/// these: `-okay` has no `-ok` tail, and a detached reason breaks the terminator. This detector exists
/// because [`ok_marker_re`] is stricter than its DSL counterpart (it REQUIRES the trailing colon), so this
/// surface has a whole silent-failure family the shared shape was never designed for. Cost, stated plainly:
/// a comment that merely says the words "idempotent-ok" in prose above a handler is also reported. That is
/// a purely additive sentence on a finding that fires either way — never a new finding — and the
/// alternative is the silence being fixed.
const NEAR_MISS_STEM_PATTERN: &str = r"//[^\n]*?(\bidempotent-ok[a-z0-9+-]*)";

/// The two near-miss detectors, compiled once.
///
/// Detector 1 — a token SHAPED like some OTHER rule's suppress marker (`// non-idempotent-write-ok`).
/// Its shape is the DSL interpreter's own, imported (`zzop_core::NEAR_MISS_MARKER_TOKEN_PATTERN`) rather
/// than re-spelled, so a near-miss reads identically on both surfaces by construction. Only `//` leaders
/// are prepended — [`ok_marker_re`] honors nothing else, and a leader that could never have suppressed
/// must never be blamed for failing to.
///
/// Detector 2 — [`NEAR_MISS_STEM_PATTERN`], this surface's own extra family.
fn near_miss_res() -> &'static [Regex; 2] {
    static R: OnceLock<[Regex; 2]> = OnceLock::new();
    R.get_or_init(|| {
        [
            format!(r"//\s*{}", zzop_core::NEAR_MISS_MARKER_TOKEN_PATTERN),
            NEAR_MISS_STEM_PATTERN.to_string(),
        ]
        .map(|p| Regex::new(&p).expect("compile-time constant regex"))
    })
}

// --- Shared helpers (name index / handler resolution / whitelist) ---

fn ident_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[A-Za-z_$][\w$]*").unwrap())
}

/// Tail name (after the last `.`) -> symbol ids (`"file#name"`). `pub(crate)`: also used by `mutating_route_no_auth`.
pub(crate) fn build_name_index(symbols: &[SourceSymbol]) -> HashMap<String, Vec<String>> {
    let mut idx: HashMap<String, Vec<String>> = HashMap::new();
    for s in symbols {
        let tail = s.name.rsplit('.').next().unwrap_or(&s.name).to_string();
        idx.entry(tail).or_default().push(s.id.clone());
    }
    idx
}

/// Resolves a handler reference string to a unique symbol id, stripping wrapper calls (`rateLimit(fn)`) and
/// member access (`ctrl.list`). `None` when unknown or ambiguous (defined in multiple files) — never guessed.
pub(crate) fn resolve_handler(handler: &str, idx: &HashMap<String, Vec<String>>) -> Option<String> {
    resolve_handler_scoped(handler, idx, None)
}

/// [`resolve_handler`] with an optional route-FILE tie-break. When a handler name is ambiguous repo-wide
/// (defined in multiple files), a `Some(route_file)` disambiguates to the candidate declared in that file:
/// a decorator-routed handler (NestJS `@Delete() delete()`) is a METHOD of the controller class in the
/// file its route `IoProvide` points at, so a bare method name colliding across controllers (`delete` in
/// four controllers) still resolves uniquely once scoped to the route's own file. Only a UNIQUE in-file
/// candidate resolves; two `delete`s in one file, or none, still yields `None` (do-not-guess). With
/// `None` the behavior is identical to the original repo-wide-unique-or-nothing rule.
pub(crate) fn resolve_handler_scoped(
    handler: &str,
    idx: &HashMap<String, Vec<String>>,
    route_file: Option<&str>,
) -> Option<String> {
    let ids: Vec<&str> = ident_re().find_iter(handler).map(|m| m.as_str()).collect();
    for ident in ids.iter().rev() {
        match idx.get(*ident) {
            Some(candidates) if candidates.len() == 1 => return Some(candidates[0].clone()),
            Some(candidates) => {
                // Ambiguous repo-wide. Tie-break to the route's own file, if one was given.
                if let Some(file) = route_file {
                    let mut in_file = candidates
                        .iter()
                        .filter(|id| id.split('#').next() == Some(file));
                    if let (Some(one), None) = (in_file.next(), in_file.next()) {
                        return Some(one.clone());
                    }
                }
                return None; // still ambiguous — do not guess
            }
            None => continue,
        }
    }
    None
}

/// Runs `pick` over every line of the handler's marker lookback window, ascending, returning the first
/// `Some`. THE window definition, for both the honored-marker check and the near-miss disclosure below —
/// a disclosure that scanned different lines than suppression could blame a comment that never had a
/// chance to suppress.
///
/// The window is `OK_MARKER_LOOKBACK_LINES` lines wide and ENDS at the body-start line: `body_start` is
/// 1-based, so the half-open index range `(decl-4)..decl` reads source lines `decl-3 ..= decl` — the
/// body-start line ITSELF plus [`OK_MARKER_LOOKBACK_ABOVE`] lines above it. A marker
/// `OK_MARKER_LOOKBACK_LINES` lines above the body start is OUTSIDE it and never read; user-facing text
/// must say "body-start line or up to 3 lines above", never "the 4 lines above".
fn scan_marker_window<T>(
    handler_symbol: &str,
    symbols: &[SourceSymbol],
    files: &HashMap<String, String>,
    mut pick: impl FnMut(&str) -> Option<T>,
) -> Option<T> {
    let sym = symbols.iter().find(|s| s.id == handler_symbol)?;
    let text = files.get(&sym.file)?;
    let lines: Vec<&str> = text.split('\n').collect();
    let decl_line = sym.body_start.unwrap_or(sym.line);
    (decl_line.saturating_sub(OK_MARKER_LOOKBACK_LINES)..decl_line)
        .filter_map(|i| lines.get(i as usize).copied())
        .find_map(&mut pick)
}

/// A `// idempotent-ok: <reason>` comment in that window suppresses the finding.
fn is_whitelisted(
    handler_symbol: &str,
    symbols: &[SourceSymbol],
    files: &HashMap<String, String>,
) -> bool {
    scan_marker_window(handler_symbol, symbols, files, |l| {
        ok_marker_re().is_match(l).then_some(())
    })
    .is_some()
}

/// `base` with one disclosure sentence appended when the handler's lookback window carries a
/// marker-shaped token that did NOT suppress — the author wrote a suppression comment in good faith and
/// it does nothing, so the finding says so instead of firing mutely. Same contract as the DSL
/// interpreter's `message_with_near_miss`: purely additive to the message, changes no gate, so the set of
/// findings is untouched.
///
/// Only ever called for a finding about to be emitted, i.e. after [`is_whitelisted`] already returned
/// false — so ANY marker-shaped token still standing in that window is by construction non-suppressing,
/// and no `token != honored` filter is needed (the DSL sibling needs one because its honored regex accepts
/// both the bare and the `:`-suffixed spelling). That difference is load-bearing here: [`ok_marker_re`]
/// REQUIRES the trailing colon, so a bare `// idempotent-ok` is itself a silent near-miss and is reported.
pub(crate) fn with_ok_marker_near_miss(
    base: String,
    handler_symbol: &str,
    symbols: &[SourceSymbol],
    files: &HashMap<String, String>,
) -> String {
    let found = scan_marker_window(handler_symbol, symbols, files, |l| {
        near_miss_res()
            .iter()
            .find_map(|re| re.captures(l))
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
    });
    match found {
        Some(found) => format!(
            "{base} Note: a comment on this handler's {window} it reads `{found}`, which does not \
             suppress this rule — the marker this rule honors is `{OK_MARKER_SPELLING}` (the trailing \
             colon is required), placed in that same window, so this finding still fires.",
            window = marker_window_phrase()
        ),
        None => base,
    }
}

#[cfg(test)]
mod tests;
