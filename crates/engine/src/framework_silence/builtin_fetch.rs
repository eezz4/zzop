//! S5: builtin-fetch lexical census tripwire (consume side).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::app_buckets::nearest_app_root;
use super::controller_silence::MIN_PROVIDES_FLOOR;
use super::egress_intent::{arg_region, region_is_internal_intent, ArgSpan};

/// Word-boundary `fetch(`-style call-token matcher: `\bfetch\s*\(` covers bare `fetch(`,
/// `window.fetch(`, and `globalThis.fetch(` alike (`.` is a non-word char, so `\b` sits between it
/// and the `f`). DELIBERATELY simple and honest — no receiver classification, no attempt to exclude
/// `.fetch(` on non-window receivers: the call-site-count threshold below does the noise control, and
/// a heuristic exclusion list would be its own silent blindness. Same "independent of the extractor's
/// own vocabulary" stance as S1's decorator regex: this tripwire exists to catch what extraction
/// missed, so it must not share extraction's judgment. `pub(super)` (not just private) so S7
/// (`fetch_wrapper`) can reuse the exact same matcher for its own PASS 1 "does this file call builtin
/// `fetch(` at all" check, rather than re-deriving a second, possibly-diverging regex for the same idiom.
pub(super) fn fetch_call_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bfetch\s*\(").unwrap())
}

/// Minimum lexical `fetch(` call tokens tree-wide before the S5 self-report fires — enough call-site
/// mass that a wrapper idiom (many fetch calls, near-none keyed) is plausible rather than one or two
/// stray dynamic calls. The live tree this tripwire was built from carried ~10 call sites with 1
/// extracted consume; a micro-FE with 1-4 genuinely-dynamic fetch calls stays below this floor (its
/// blindness is a resolution gap, already `cross-layer/unresolved-consume-ratio`'s job).
///
/// Policy tier T3 (coincidental axis, do not unify): counts total token OCCURRENCES tree-wide — a
/// different tuning axis from S1's `MIN_FILES` (distinct decorator-matching FILES) and from
/// `MIN_PROVIDES_FLOOR` (extracted-fact floor); each moves without re-justifying the others.
pub(super) const FETCH_CALL_SITES_MIN: usize = 5;

/// Sample cap for the example-file list in the warning text — presentation bound, same role (and
/// value) as `controller_silence`'s `MAX_SAMPLES` (T3: a display cap, not a firing threshold; free to
/// diverge).
const MAX_SAMPLES: usize = 3;

/// Whether `rel` is a js/ts-family source file — the same extension set `dispatch.rs` routes to the
/// TypeScript frontend (`ts|tsx|js|jsx|mjs|cjs|mts|cts`); builtin `fetch` is a js-runtime global, so
/// only these files can carry the call idiom this census counts. Note the raw-text census's
/// documented over-disclosure tolerance includes `.d.ts` ambient declarations (`declare function
/// fetch(` — `.d.ts` has extension `ts`) and comment/string occurrences — a types-only tree with 5+
/// such lines and zero keyed consumes fires a warning it does not strictly merit; over-disclosure
/// is safe, silence is fatal (the coverage self-report principle). Unlike S1's decorator scan — which
/// anchors on a line-LEADING match to discount comment/string mentions — a `fetch(` call is commonly
/// mid-line (`await fetch(`), so there is no comparable anchor here; the raw-text tolerance stays,
/// mitigated instead by the `region_is_internal_intent` first-arg filter. `pub(super)` so S7
/// (`fetch_wrapper`) walks the identical file-family filter over the identical candidate list — one
/// extension policy for "can this file carry a js-runtime `fetch` idiom", not two.
pub(super) fn is_js_ts_family(rel: &str) -> bool {
    Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts"
            )
        })
}

/// Returns a ready-to-push `warnings` entry when the tree's js/ts sources carry at least
/// `FETCH_CALL_SITES_MIN` lexical `fetch(` call tokens while `http_consumes_keyed_count` sits below
/// `MIN_PROVIDES_FLOOR` — the builtin-fetch dual of S4's import tripwire: builtin `fetch` is a global,
/// not a module specifier, so there is no package import for S4 to anchor on (its own doc names this
/// gap), and a tree whose HTTP calls ride a hand-rolled wrapper over `fetch` goes consume-silent with
/// no channel firing at all.
///
/// Gate substrate: KEYED `http` consumes only — deliberately narrower than S4's keyed-AND-unresolved
/// count. `fetch(` is itself a recognized extraction shape, so a direct dynamic call still produces an
/// UNRESOLVED consume record; counting those here would silence the tripwire on exactly the trees it
/// targets (many visible fetch tokens, near-nothing the JOIN can use). Keyed is what joins — this
/// census guards join visibility of fetch-style egress, the same keyed substrate S3 gates on.
///
/// Cheap on the success path: the keyed-count gate short-circuits before any disk read (the same
/// convention as S1/S3); only a consume-near-silent tree pays the re-read, and only over its
/// js/ts-family files.
///
/// Determinism: `candidate_rels` must already be sorted by the caller (`analyze::assemble`, the same
/// convention S1/S3 rely on) — the example-file sample is therefore deterministic without a re-sort.
pub fn builtin_fetch_lexical_warning(
    root: &Path,
    candidate_rels: &[String],
    http_consumes_keyed_count: usize,
) -> Option<String> {
    if http_consumes_keyed_count >= MIN_PROVIDES_FLOOR {
        return None;
    }
    let (total, matched_files) = count_internal_fetch_sites(root, candidate_rels);
    if total < FETCH_CALL_SITES_MIN {
        return None;
    }
    let refs: Vec<&str> = matched_files.iter().map(String::as_str).collect();
    Some(format_tree_wide(total, http_consumes_keyed_count, &refs))
}

/// The internal-intent funnel tail shared verbatim by the tree-wide and per-app S5 warnings (and the
/// tree-wide fallback the census may emit) — the adapter-creation on-ramp (D9): a Mode B partial
/// envelope over just the consume channel, plus the embedded-contract pointer. Split into a `const` so
/// the tree-wide wording stays BYTE-IDENTICAL to its pre-census form while the new app-scoped wording
/// reuses the exact same tail.
const FUNNEL_TAIL: &str = "builtin fetch has no package import for the http-client tripwire to anchor on, and the call idiom is likely a hand-rolled wrapper whose computed URLs this extraction pass cannot key; cross-layer joins will be near-silent from this tree's consume side — project this tree's consumes with a Mode B overlay adapter (see the adapter examples) to restore cross-layer visibility: a partial envelope covering just the consume channel is enough; contract: `zzop contract envelope-guide` on MCP hosts, docs/NORMALIZED_AST.md in the repo.";

/// Counts INTERNAL-INTENT builtin `fetch(` call sites (per [`region_is_internal_intent`], classifying
/// each match's first-argument region via [`ArgSpan::First`] so a later options-object literal cannot
/// mark a bare-const external `fetch(CONST, …)` as internal) across `rels`'s js/ts-family files, and
/// returns `(total token count, matched-file rels in input order)`. Skips files that don't read (cheap:
/// no read at all for a non-js/ts rel). The intent filter is the discriminator that keeps absolute-URL
/// / bare-const egress (a CDN, a third-party API — nothing internal to join) OUT of the census.
fn count_internal_fetch_sites(root: &Path, rels: &[String]) -> (usize, Vec<String>) {
    let re = fetch_call_re();
    let mut total = 0usize;
    let mut matched_files: Vec<String> = Vec::new();
    for rel in rels.iter().filter(|rel| is_js_ts_family(rel)) {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        let count = re
            .find_iter(&text)
            .filter(|m| region_is_internal_intent(arg_region(&text, m.end(), ArgSpan::First)))
            .count();
        if count > 0 {
            total += count;
            matched_files.push(rel.clone());
        }
    }
    (total, matched_files)
}

/// The `MAX_SAMPLES`-capped, `, +N more`-suffixed example-file list shared by every S5 warning shape.
fn sample_of(matched_files: &[&str]) -> String {
    let sample: Vec<&str> = matched_files.iter().take(MAX_SAMPLES).copied().collect();
    let mut sample_str = sample.join(", ");
    if matched_files.len() > MAX_SAMPLES {
        sample_str.push_str(&format!(", +{} more", matched_files.len() - MAX_SAMPLES));
    }
    sample_str
}

/// The tree-wide S5 warning string — BYTE-IDENTICAL to the pre-census wording. Reused both by
/// [`builtin_fetch_lexical_warning`] (single-package / tests) and by the census's tree-wide FALLBACK
/// (an internal-fetch mass that split across packages and fell below every per-app floor, but whose
/// aggregate still clears [`FETCH_CALL_SITES_MIN`]).
fn format_tree_wide(total: usize, keyed: usize, matched_files: &[&str]) -> String {
    let sample_str = sample_of(matched_files);
    format!(
        "{total} builtin `fetch(` call site(s) appear lexically across {} js/ts file(s) but only \
{keyed} keyed http consume(s) were extracted tree-wide (e.g. {sample_str}) — {FUNNEL_TAIL}",
        matched_files.len()
    )
}

/// The per-app S5 warning string — names the below-floor app bucket whose internal-relative `fetch(`
/// mass a healthy sibling app would otherwise MASK under a tree-wide gate. Same [`FUNNEL_TAIL`] as the
/// tree-wide wording.
fn format_app_scoped(
    app_root: &str,
    total: usize,
    keyed: usize,
    matched_files: &[String],
) -> String {
    let refs: Vec<&str> = matched_files.iter().map(String::as_str).collect();
    let sample_str = sample_of(&refs);
    format!(
        "{total} builtin `fetch(` call site(s) with internal-relative URLs appear lexically across {} \
js/ts file(s) within app `{app_root}` but only {keyed} keyed http consume(s) were extracted for that \
app (e.g. {sample_str}) — {FUNNEL_TAIL}",
        matched_files.len()
    )
}

/// The per-app builtin-`fetch(` internal-intent census — the de-masking dual of
/// [`builtin_fetch_lexical_warning`]. A monorepo tree gates a healthy sibling app's keyed consumes over
/// the WHOLE tree, so a dark app's silent FE<->BE contract hides under the tree-wide floor; this census
/// gates per app-root instead and NAMES the dark app.
///
/// - Single-package (`app_roots == [""]`): reduces exactly to the tree-wide path — healthy => empty,
///   else delegate to [`builtin_fetch_lexical_warning`] (a 0/1-element vec).
/// - Multi-package: partition the js/ts rels by [`nearest_app_root`], read ONLY below-floor buckets'
///   files (healthy buckets are never touched), and accumulate each bucket's internal-intent total.
///   Per-app fire: every below-floor bucket clearing [`FETCH_CALL_SITES_MIN`] pushes one app-scoped
///   warning (iterated in sorted `app_roots` order). Tree fallback: if NOTHING fired per-app, yet the
///   tree-wide keyed sum is below floor AND the below-floor buckets' aggregate internal total clears
///   [`FETCH_CALL_SITES_MIN`], push ONE tree-wide-worded warning — recovering an internal-fetch mass
///   that split across packages and slipped below every per-app floor.
pub fn builtin_fetch_census(
    root: &Path,
    all_rels: &[String],
    keyed_by_root: &BTreeMap<String, usize>,
    app_roots: &[String],
) -> Vec<String> {
    if app_roots.len() == 1 && app_roots[0].is_empty() {
        let keyed = keyed_by_root.get("").copied().unwrap_or(0);
        if keyed >= MIN_PROVIDES_FLOOR {
            return Vec::new();
        }
        return builtin_fetch_lexical_warning(root, all_rels, keyed)
            .into_iter()
            .collect();
    }

    let re = fetch_call_re();
    let mut per_bucket: BTreeMap<&str, (usize, Vec<String>)> = BTreeMap::new();
    for rel in all_rels.iter().filter(|rel| is_js_ts_family(rel)) {
        let bucket = nearest_app_root(rel, app_roots);
        if keyed_by_root.get(bucket).copied().unwrap_or(0) >= MIN_PROVIDES_FLOOR {
            continue; // healthy bucket: never read
        }
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        let count = re
            .find_iter(&text)
            .filter(|m| region_is_internal_intent(arg_region(&text, m.end(), ArgSpan::First)))
            .count();
        if count > 0 {
            let entry = per_bucket.entry(bucket).or_default();
            entry.0 += count;
            entry.1.push(rel.clone());
        }
    }

    let mut warnings = Vec::new();
    let mut any_fired = false;
    for bucket in app_roots {
        let keyed = keyed_by_root.get(bucket).copied().unwrap_or(0);
        if keyed >= MIN_PROVIDES_FLOOR {
            continue;
        }
        if let Some((total, files)) = per_bucket.get(bucket.as_str()) {
            if *total >= FETCH_CALL_SITES_MIN {
                warnings.push(format_app_scoped(bucket, *total, keyed, files));
                any_fired = true;
            }
        }
    }

    if !any_fired {
        let tree_keyed: usize = keyed_by_root.values().sum();
        let agg_total: usize = per_bucket.values().map(|(t, _)| *t).sum();
        if tree_keyed < MIN_PROVIDES_FLOOR && agg_total >= FETCH_CALL_SITES_MIN {
            let mut all_files: Vec<&str> = Vec::new();
            for bucket in app_roots {
                if let Some((_, files)) = per_bucket.get(bucket.as_str()) {
                    all_files.extend(files.iter().map(String::as_str));
                }
            }
            warnings.push(format_tree_wide(agg_total, tree_keyed, &all_files));
        }
    }

    warnings
}
