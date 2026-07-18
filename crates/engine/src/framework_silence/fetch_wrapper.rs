//! S7: fetch-wrapper call-site census tripwire (consume side) — the wrapper-indirection dual of S5.
//! S5 catches many DIRECT lexical `fetch(` call sites with near-zero keyed consumes; this tripwire
//! catches the shape S5 itself cannot: a tree that funnels its ENTIRE http-egress surface through one
//! hand-rolled wrapper module (a single `fetch(` call site inside the wrapper, then 20+ cross-file
//! calls to the wrapper's exported `get`/`post`/`put`/`del` bindings) — S5's own tree-wide `fetch(`
//! token count stays near the floor even though the tree's real call-site surface is large, because
//! only the wrapper file itself contains the literal token.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::builtin_fetch::{fetch_call_re, is_js_ts_family, FETCH_CALL_SITES_MIN};
use super::controller_silence::MIN_PROVIDES_FLOOR;
use super::egress_intent::{arg_region, region_is_internal_intent, ArgSpan};

/// "http-verby" or generic-sender exported binding names — a file that exports one of these AND itself
/// calls builtin `fetch(` looks like a hand-rolled http wrapper module (round-10 blind-field test R10's
/// fe-svelte `src/lib/api.js`: `export function get/post/put/del`, each delegating to one internal
/// `fetch(` call). Deliberately a tight, exact-match vocabulary (not a fuzzy/substring match): each entry
/// names either an HTTP verb-shaped sender (`get`, `post`, `put`, `del`, `delete_`, `patch`, `request`,
/// `send`) or a generic wrapper-object name (`api`, `http`, `client`) real-world wrapper modules export
/// under (e.g. `export default { get, post }` style access as `api.get(...)`, matched via the exported
/// default-object accessor name itself). Census-tracked — see `scripts/policy-census.txt`.
const WRAPPER_EXPORT_NAMES: &[&str] = &[
    "get", "post", "put", "del", "delete_", "patch", "request", "send", "api", "http", "client",
];

/// Sample cap for the example-file list in the warning text — same "up to 3 example paths" convention
/// `orm_schema_silence`'s own `MAX_EXAMPLES` documents (T3: a display cap, not a firing threshold; free
/// to diverge from every sibling's own copy of this same value).
const MAX_EXAMPLES: usize = 3;

fn export_function_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bexport\s+(?:async\s+)?function\s+([A-Za-z_$][\w$]*)").unwrap())
}

fn export_const_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bexport\s+const\s+([A-Za-z_$][\w$]*)\s*=").unwrap())
}

/// `export { a, b as c }` list form — deliberately not distinguished from a re-export
/// (`export { a } from './y'`) at this lexical grain: a re-export still means the name is reachable
/// through this file, so counting it as a locally-defined wrapper export is accepted over-disclosure,
/// not a bug (this module's governing stance, same as every sibling S1-S6 tripwire's).
fn export_list_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bexport\s*\{([^}]*)\}").unwrap())
}

/// `from '<specifier>'` / `from "<specifier>"` — deliberately matched without anchoring on a leading
/// `import`/`export` keyword: this catches plain `import ... from '<spec>'`, multi-line named-import
/// lists, and re-export forms (`export { x } from '<spec>'`) alike with one pattern, the same
/// "keep it simple, document the looseness" choice `exported_names`' own doc makes for the export side.
fn from_specifier_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"\bfrom\s*['"]([^'"]+)['"]"#).unwrap())
}

/// Every lexically-exported binding name in `text`, via the three shapes documented on
/// `WRAPPER_EXPORT_NAMES`'s own doc: `export function <name>`/`export async function <name>`,
/// `export const <name> =`, and an `export { a, b as c }` list (the list arm takes the EXTERNAL name —
/// the part after `as` when present, since that is the name a caller actually imports).
fn exported_names(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for cap in export_function_re().captures_iter(text) {
        names.insert(cap[1].to_string());
    }
    for cap in export_const_re().captures_iter(text) {
        names.insert(cap[1].to_string());
    }
    for cap in export_list_re().captures_iter(text) {
        for item in cap[1].split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let name = match item.split_once(" as ") {
                Some((_, external)) => external.trim(),
                None => item,
            };
            if !name.is_empty() {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// The subset of `WRAPPER_EXPORT_NAMES` that `text` lexically exports, in vocab order (so the warning's
/// name list is deterministic regardless of the source file's own declaration order).
fn matched_wrapper_export_names(text: &str) -> Vec<&'static str> {
    let names = exported_names(text);
    WRAPPER_EXPORT_NAMES
        .iter()
        .copied()
        .filter(|vocab| names.contains(*vocab))
        .collect()
}

/// Strips one known js/ts source extension from `name`, if present — a mechanical normalization (not a
/// policy vocabulary; deliberately an inline array literal, not a `const`, so it stays outside
/// `check-policy-census.sh`'s tracked shapes, same choice `is_js_ts_family`'s own extension `matches!`
/// makes) used only to compare an import specifier's last path segment against a wrapper file's stem.
fn strip_known_js_ext(name: &str) -> &str {
    for ext in [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts"] {
        if let Some(stripped) = name.strip_suffix(ext) {
            return stripped;
        }
    }
    name
}

/// Whether `importer_text` imports from a specifier that lexically resolves to `wrapper_stem` (the
/// wrapper file's basename with its extension stripped), via "loose-but-honest" suffix matching: the
/// specifier's LAST path segment, extension stripped, equals the wrapper stem exactly. Deliberately loose
/// — no module-resolution algorithm, no bundler-alias table (e.g. SvelteKit's `$lib/api` alias resolves
/// to `src/lib/api.js` at build time; this function has no such table and instead matches on the shared
/// trailing segment `api`). The false-negative risk this buys (an unrelated same-stemmed module import,
/// e.g. `'./api'` pointing somewhere else entirely) is the accepted looseness — the same over-disclosure-
/// is-safe stance every sibling S1-S6 tripwire's own doc states.
fn imports_the_wrapper(importer_text: &str, wrapper_stem: &str) -> bool {
    from_specifier_re().captures_iter(importer_text).any(|cap| {
        let specifier = &cap[1];
        let last_segment = specifier.rsplit('/').next().unwrap_or(specifier);
        strip_known_js_ext(last_segment) == wrapper_stem
    })
}

/// Word-boundary call-token count of `<name>(` in `text` — matches both the bare-call shape (`get(...)`,
/// if the wrapper's exports were imported by name/namespace-destructure) and the member-call shape
/// (`api.get(...)`, `client.post(...)`) alike in ONE pass, the identical `\b`-boundary mechanism
/// `fetch_call_re` uses for the same reason (`.` is a non-word char, so `\b` sits between it and the
/// name) — this avoids double-counting a single `api.get(` call site under two separate patterns.
///
/// Counts ONLY internal-intent call sites: each `<name>(` match's WHOLE argument region ([`ArgSpan::All`]
/// — a wrapper call carries its path in a later positional arg, e.g. `request("GET", "/api/x")`) is
/// classified by [`region_is_internal_intent`], so a wrapper export called with only absolute-URL /
/// bare-const args (nothing internal to join) does not inflate the census.
fn call_site_count(text: &str, name: &str) -> usize {
    let pattern = format!(r"\b{}\s*\(", regex::escape(name));
    Regex::new(&pattern)
        .map(|re| {
            re.find_iter(text)
                .filter(|m| region_is_internal_intent(arg_region(text, m.end(), ArgSpan::All)))
                .count()
        })
        .unwrap_or(0)
}

/// Returns a ready-to-push `warnings` entry when the tree carries a fetch-wrapper module (PASS 1: a
/// js/ts file that both calls builtin `fetch(` and lexically exports at least one
/// [`WRAPPER_EXPORT_NAMES`] binding) whose exported names are called at least [`FETCH_CALL_SITES_MIN`]
/// times, tree-wide, from OTHER js/ts files that import it (PASS 2) — while `http_consumes_keyed_count`
/// sits below [`MIN_PROVIDES_FLOOR`]. Shares S5's exact gate/floor and reuses S5's own
/// `FETCH_CALL_SITES_MIN` call-site-mass floor (same policy value, same tuning axis: total call-token
/// OCCURRENCES, here summed across the wrapper's cross-file callers instead of tree-wide bare `fetch(`).
///
/// Cheap on the success path: short-circuits before any disk read once `http_consumes_keyed_count`
/// clears the floor, same convention as every sibling S1-S6 tripwire.
///
/// Determinism: `candidate_rels` must already be sorted by the caller (`analyze::assemble`) — PASS 1
/// picks the FIRST (sorted-order) qualifying wrapper file when more than one exists in a tree, and PASS
/// 2's example-file sample is built in that same sorted order, so both are deterministic without any
/// re-sort here.
pub fn fetch_wrapper_call_site_warning(
    root: &Path,
    candidate_rels: &[String],
    http_consumes_keyed_count: usize,
) -> Option<String> {
    if http_consumes_keyed_count >= MIN_PROVIDES_FLOOR {
        return None;
    }
    let js_rels: Vec<&str> = candidate_rels
        .iter()
        .map(String::as_str)
        .filter(|rel| is_js_ts_family(rel))
        .collect();

    let (wrapper_rel, matched_names) = find_wrapper(root, &js_rels)?;
    let wrapper_stem = wrapper_stem_of(wrapper_rel);

    let mut total = 0usize;
    let mut matched_files: Vec<&str> = Vec::new();
    for &rel in &js_rels {
        if rel == wrapper_rel {
            continue;
        }
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        if !imports_the_wrapper(&text, wrapper_stem) {
            continue;
        }
        let count: usize = matched_names
            .iter()
            .map(|name| call_site_count(&text, name))
            .sum();
        if count > 0 {
            total += count;
            matched_files.push(rel);
        }
    }
    if total < FETCH_CALL_SITES_MIN {
        return None;
    }
    let name_list = matched_names.join("/");
    Some(format_tree_wide_s7(
        wrapper_rel,
        &name_list,
        total,
        http_consumes_keyed_count,
        &matched_files,
    ))
}

/// The wrapper-indirection funnel tail shared verbatim by the tree-wide and per-app S7 warnings (and
/// the tree-wide fallback the census may emit) — same adapter-creation on-ramp (D9) as S5's, worded for
/// the wrapper case. Split into a `const` so the tree-wide wording stays BYTE-IDENTICAL to its
/// pre-census form while the new app-scoped wording reuses the exact same tail.
const WRAPPER_FUNNEL_TAIL: &str = "wrapper indirection over builtin fetch is not followed by this extraction pass, so cross-layer joins will be near-silent from this tree's consume side — project this tree's consumes with a Mode B overlay adapter (see the adapter examples) to restore cross-layer visibility: a partial envelope covering just the consume channel is enough; contract: `zzop-mcp contract envelope-guide` on MCP hosts, docs/NORMALIZED_AST.md in the repo.";

/// PASS 1, TREE-WIDE and unchanged from the pre-census behavior: the FIRST (sorted-order) js/ts file
/// that both calls builtin `fetch(` and lexically exports a [`WRAPPER_EXPORT_NAMES`] binding, with the
/// vocab-ordered subset of names it exports. Deliberately NOT intent-filtered on the wrapper's own
/// internal `fetch(` — the wrapper's URL is the internal call sites' concern (PASS 2), not PASS 1's.
fn find_wrapper<'a>(root: &Path, js_rels: &[&'a str]) -> Option<(&'a str, Vec<&'static str>)> {
    for &rel in js_rels {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        if !fetch_call_re().is_match(&text) {
            continue;
        }
        let matched = matched_wrapper_export_names(&text);
        if !matched.is_empty() {
            return Some((rel, matched));
        }
    }
    None
}

/// The wrapper file's basename, extension stripped — the stem [`imports_the_wrapper`] matches on.
fn wrapper_stem_of(wrapper_rel: &str) -> &str {
    strip_known_js_ext(
        Path::new(wrapper_rel)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(wrapper_rel),
    )
}

/// The `MAX_EXAMPLES`-capped, `, +N more`-suffixed example-file list shared by every S7 warning shape.
fn sample_of(matched_files: &[&str]) -> String {
    let sample: Vec<&str> = matched_files.iter().take(MAX_EXAMPLES).copied().collect();
    let mut sample_str = sample.join(", ");
    if matched_files.len() > MAX_EXAMPLES {
        sample_str.push_str(&format!(", +{} more", matched_files.len() - MAX_EXAMPLES));
    }
    sample_str
}

/// The tree-wide S7 warning string — BYTE-IDENTICAL to the pre-census wording. Reused both by
/// [`fetch_wrapper_call_site_warning`] (single-package / tests) and by the census's tree-wide FALLBACK.
fn format_tree_wide_s7(
    wrapper_rel: &str,
    name_list: &str,
    total: usize,
    keyed: usize,
    matched_files: &[&str],
) -> String {
    let sample_str = sample_of(matched_files);
    format!(
        "{wrapper_rel} exports a fetch-wrapper idiom ({name_list}) with {total} cross-file call site(s) \
across {} file(s) that import it (e.g. {sample_str}) but only {keyed} keyed http \
consume(s) were extracted tree-wide — {WRAPPER_FUNNEL_TAIL}",
        matched_files.len()
    )
}

/// PASS 1's wrapper detection + PASS 2's per-importer internal-intent call-site count, wired into the
/// per-app + fallback composition. Split into a submodule to keep this root file under the source
/// line-length cap; it reuses this module's private PASS 1/PASS 2 helpers (`find_wrapper`,
/// `wrapper_stem_of`, `imports_the_wrapper`, `call_site_count`) and formatting (`format_tree_wide_s7`,
/// `sample_of`, `WRAPPER_FUNNEL_TAIL`) via `super::`.
mod census;
pub use census::fetch_wrapper_census;
