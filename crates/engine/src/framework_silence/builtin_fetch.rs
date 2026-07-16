//! S5: builtin-fetch lexical census tripwire (consume side).

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::controller_silence::MIN_PROVIDES_FLOOR;

/// Word-boundary `fetch(`-style call-token matcher: `\bfetch\s*\(` covers bare `fetch(`,
/// `window.fetch(`, and `globalThis.fetch(` alike (`.` is a non-word char, so `\b` sits between it
/// and the `f`). DELIBERATELY simple and honest — no receiver classification, no attempt to exclude
/// `.fetch(` on non-window receivers: the call-site-count threshold below does the noise control, and
/// a heuristic exclusion list would be its own silent blindness. Same "independent of the extractor's
/// own vocabulary" stance as S1's decorator regex: this tripwire exists to catch what extraction
/// missed, so it must not share extraction's judgment.
fn fetch_call_re() -> &'static Regex {
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
/// is safe, silence is fatal (same stance as S1's decorator line-scan).
fn is_js_ts_family(rel: &str) -> bool {
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
    let re = fetch_call_re();
    let mut total = 0usize;
    let mut matched_files: Vec<&str> = Vec::new();
    for rel in candidate_rels.iter().filter(|rel| is_js_ts_family(rel)) {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        let count = re.find_iter(&text).count();
        if count > 0 {
            total += count;
            matched_files.push(rel.as_str());
        }
    }
    if total < FETCH_CALL_SITES_MIN {
        return None;
    }
    let sample: Vec<&str> = matched_files.iter().take(MAX_SAMPLES).copied().collect();
    let mut sample_str = sample.join(", ");
    if matched_files.len() > MAX_SAMPLES {
        sample_str.push_str(&format!(", +{} more", matched_files.len() - MAX_SAMPLES));
    }
    Some(format!(
        "{total} builtin `fetch(` call site(s) appear lexically across {} js/ts file(s) but only \
{http_consumes_keyed_count} keyed http consume(s) were extracted tree-wide (e.g. {sample_str}) — builtin \
fetch has no package import for the http-client tripwire to anchor on, and the call idiom is likely a \
hand-rolled wrapper whose computed URLs this extraction pass cannot key; cross-layer joins will be \
near-silent from this tree's consume side — project this tree's consumes with a Mode B overlay adapter \
(see the adapter examples) to restore cross-layer visibility: a partial envelope covering just the \
consume channel is enough; contract: `zzop-mcp contract envelope-guide` on MCP hosts, \
docs/NORMALIZED_AST.md in the repo.",
        matched_files.len()
    ))
}
