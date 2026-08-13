//! `unprovided-consume` — an `IoConsume` (`kind == "http"`, `key` resolved `Some`) with no matching
//! `IoProvide` key in the same analysis. A single-tree, narrower cousin of
//! `zzop_core::link_cross_layer_io`'s `unprovided_consumes` set.
//!
//! ## The zero-provides veto
//! A pure front-end tree with ZERO `http` provides of its own legitimately consumes routes served by a
//! remote backend outside this analysis's scope, so this rule only runs when the tree has at least one
//! `http` provide itself — an unmatched consume there is more likely a typo'd path than "someone else's API".
//!
//! ## Single-tree vs multi-tree
//! This runs at the single-tree level (`analyze::assemble`), so it never sees a sibling tree's provides.
//! `MultiAnalyzeOutput::cross_layer.unprovided_consumes` matches every consume against every tree's provides
//! unconditionally (provides from ANY tree already cover the remote-backend case, so no zero-provides veto
//! is needed there) — prefer that cross-tree join for a split FE/BE repo pair.
//!
//! ## Static-asset veto
//! Static-asset fetches (`public/` JSON, `.svg` icons, ...) are not API consumption, so the veto has two
//! tiers — both vocabularies and the whole gate rationale live in [`vocab`], next to the values.
//!
//! A related residual gap: raw-Worker manual dispatch (`export default { fetch }` comparing `url.pathname`
//! against literals) IS extracted by the evidence-gated `pathname_dispatch` adapter, but shapes outside its
//! never-guess gate (dynamic/`startsWith` paths, const-indirected literals, functions without Request
//! evidence) stay invisible on the provide side. [`Severity::Info`] absorbs both this and the tradeoff above.
//!
//! ## Structural gates — one shared sequence, not local copies
//! Everything decidable from the KEY or the FILE alone belongs to the STRUCTURAL layer, whose single
//! definition is the linker's, and this rule CALLS it rather than restating it: a structural gate must
//! exist on BOTH axes, built once in core and called — never hand-copied.
//!
//! - **No key / declared-host re-key / absolute-URL egress** — [`zzop_core::classify_consume_join`], the
//!   linker's own sequence *including its order*. The order is the contract: the re-key runs BEFORE the
//!   `://` gate, so a tree that declares `hosts` gets its own gateway calls re-keyed to internal paths
//!   and matched instead of vetoed as egress. Copying the linker's PREDICATE without that preceding
//!   TRANSFORM is exactly how this rule once regenerated the defect class the predicate fix had just
//!   closed; one function carrying both makes that misuse unexpressible. A key that still carries a
//!   scheme afterwards is third-party egress by the linker's written contract
//!   (`zzop_core::CrossLayerResult::external_consumes`: "never counted as `unprovidedConsumes`, since an
//!   unmatched absolute-URL consume is expected ... not drift") and could never string-match an
//!   extension-free `provided_keys` entry anyway — which ended the field-measured contradiction
//!   (`GET https://api.sunrise-sunset.org/json` read as drift here, `externalConsumes` there). A
//!   localhost/loopback dev self-reference is a strict SUBSET, carrying no vocabulary of its own;
//!   host-stripping to force a join stays rejected — a fabricated join can mask a real mismatch. On a
//!   re-key the finding reports the internal join key and keeps the absolute spelling in `data.rawKey`,
//!   mirroring the linker's own bucket invariant; declaring no hosts leaves behavior byte-identical.
//! - **Route identity** — [`zzop_core::key_carries_route_identity`], asked on BOTH axes only of a key
//!   that matched nothing (hence not part of the sequence above: a key with no route identity that
//!   actually HITS a provide is a join, not a guess). An all-`{}` key (`GET /{}`) names no route — the
//!   head-drop artifact of an interpolation the extractor could not resolve (`` fetch(`${BASE}/${id}`) ``)
//!   — so its failure to match proves nothing about this tree's routes, and reporting it would fabricate
//!   an internal contract out of an extraction gap. The multi-tree linker buckets the same key as
//!   `unresolvedConsumes`; this rule has nowhere to move it, so it vetoes.
//! - **Wildcard route partition** — [`zzop_core::wildcard_route_covers`], asked (like route identity) only
//!   of a key that matched nothing. A route whose path is an ANT PATTERN (`GET /files/**`) is not an exact
//!   key: comparing it literally made a live catch-all read as a dead route AND made every call beneath it
//!   read as a missing one. The verb must still match, so a `POST` under a `@GetMapping("/files/**")` keeps
//!   firing. Same predicate as the multi-tree linker's, which partitions such a route out of the join
//!   entirely — this rule has no bucket to move it to, so it vetoes, exactly as it does for route identity.
//! - **Test-file classification** — `zzop_core::is_test_file`, the same predicate the cross-tree join's
//!   input filter (`filter_join_io`, D11) applies; see the consume loop's own comment.
//!
//! What stays local is exactly what the layer split says must: the VOCABULARY vetoes (static assets, API
//! segments) and the single-tree utterance shaping (zero-provides veto, foreign fold) below.
//!
//! ## Foreign-vs-overlapping fold (partial-provider trees)
//! Field measurement (a monorepo analyzed as ONE tree): one app contributed a handful of `http` provides,
//! which opened the zero-provides veto above, while keyed consumes from SIBLING apps — served outside this
//! analysis's scope, matching no provided key — each fired an individual [`Severity::Info`] finding. That's
//! tone noise, not signal: this tree is only a *partial* provider, so a wall of independently-worded "no
//! route provides this" findings reads as N broken routes when it's really one root cause.
//!
//! [`unprovided_consume_findings`] therefore splits unmatched consumes by FIRST PATH SEGMENT overlap with
//! the tree's own provided key space ([`first_path_segment`]). "Overlapping" (first segment IS one of the
//! tree's own provided first segments) keeps today's individual finding unchanged — still plausibly a typo'd
//! or removed route under a family this tree actually serves. "Foreign" (first segment is NOT in that space)
//! folds into ONE aggregate finding once [`MIN_FOREIGN_UNPROVIDED_GROUP`] or more accumulate, under the same
//! replace-not-silently-suppress contract as `cross-layer/prefix-drift` (that rule's own module): the
//! aggregate enumerates every folded key in `data.routes` and the message body, so nothing is lost, only N
//! findings replaced by one. Below the fold threshold, foreign consumes stay individual — 1-2 could be
//! coincidence, not a pattern.
//!
//! ### A veto can RAISE the finding count (fold interaction)
//! Every veto above is applied BEFORE this split, so the threshold counts surviving consumes. Dropping one
//! sibling can therefore take a foreign group from 3 to 2 and replace ONE aggregate finding with TWO
//! individual ones — at anchors the aggregate never used, since it anchored at a single file:line. So
//! "vetoes only remove" is true of KEYS REPORTED and false of FINDING COUNT and ANCHORS; a run comparing
//! counts across a veto change must expect that. Deliberately not "fixed" by deciding the fold on the
//! PRE-veto population: a vetoed consume (vendor egress, a static asset) is not evidence that this tree is
//! a partial provider, so letting it push a 2-key group into an aggregate would fire the fold on exactly
//! the evidence the vetoes just judged irrelevant, and would make an aggregate claim a threshold its own
//! enumerated keys contradict. Disclosed in the individual finding's message instead.
//!
//! [`Severity::Info`]: zzop_core::Severity::Info

use std::collections::{BTreeSet, HashSet};

use regex::Regex;

mod fold;
mod message;
pub use fold::MIN_FOREIGN_UNPROVIDED_GROUP;

/// First `/`-delimited non-empty path segment of a `"METHOD /path"` key — the unit "foreign-vs-overlapping"
/// grouping compares (module doc). `None` when the path carries no segment (`"GET /"`), which the caller
/// treats as foreign (nothing to overlap with).
fn first_path_segment(key: &str) -> Option<&str> {
    let path = key.split_once(' ').map(|(_, p)| p).unwrap_or(key);
    path.split('/').find(|segment| !segment.is_empty())
}

/// One unmatched (post-veto, no provided-key match) consume, carried through the foreign/overlapping split.
/// `key` is the JOIN key: for a declared-host absolute URL that is the re-keyed internal path, with the
/// original spelling kept in `raw` (module doc "Structural gates"), mirroring the linker's own
/// bucket invariant that nothing past the re-key ever carries a scheme.
pub(super) struct UnmatchedConsume<'a> {
    key: std::borrow::Cow<'a, str>,
    raw: Option<&'a str>,
    file: &'a str,
    line: u32,
}

/// `internal_hosts`: hosts this tree declares it owns (`EngineConfig::hosts`), threaded in from
/// `analyze::assemble` — see module doc "Structural gates". Pass `&[]` for no declared hosts.
/// `api_segment_pattern`: the declared API-segment vocabulary — see [`vocab::api_segment_re`] for `None`.
pub fn unprovided_consume_findings(
    io_provides: &[zzop_core::IoProvide],
    io_consumes: &[zzop_core::IoConsume],
    internal_hosts: &[String],
    api_segment_pattern: Option<&str>,
) -> Vec<zzop_core::Finding> {
    let has_http_provide = io_provides.iter().any(|p| p.kind == "http");
    if !has_http_provide {
        return Vec::new();
    }
    let provided_keys: HashSet<&str> = io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .map(|p| p.key.as_str())
        .collect();
    // See module doc "Foreign-vs-overlapping fold". A provide whose path is `/` has no segment
    // (`first_path_segment` returns `None`) and contributes nothing to this tree's provided key space.
    let provide_first_segments: BTreeSet<&str> = io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .filter_map(|p| first_path_segment(&p.key))
        .collect();
    let contributing_provide_count = io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .filter(|p| first_path_segment(&p.key).is_some())
        .count();

    // ANT wildcard routes never enter `provided_keys` above as usable exact keys, so they are collected
    // separately and asked as a PATTERN below — the same partition the multi-tree linker performs, via
    // the same shared predicate, so the two axes cannot answer differently (module doc "Structural gates").
    let wildcard_route_keys: Vec<&str> = io_provides
        .iter()
        .filter(|p| p.kind == "http" && zzop_core::wildcard_route_path(&p.key).is_some())
        .map(|p| p.key.as_str())
        .collect();

    let always_veto_re = Regex::new(vocab::ALWAYS_VETO_EXTENSION_PATTERN).unwrap();
    let asset_dir_gated_re = Regex::new(vocab::ASSET_DIR_GATED_EXTENSION_PATTERN).unwrap();
    let api_segment_re = vocab::api_segment_re(api_segment_pattern);

    let mut overlapping: Vec<UnmatchedConsume> = Vec::new();
    let mut foreign: Vec<UnmatchedConsume> = Vec::new();

    // Test-file consumes are not deployed surface — a `test_*.py`/`*.spec.ts` call to a route (often a
    // deliberately-wrong path exercising a 404, or an httpx/requests client fixture) is test scaffolding, not
    // app egress that should be matched against the app's routes. Skipping them mirrors the cross-tree join's
    // own `filter_join_io` test-drop (D11): the multi-tree path already excludes test-classified io before
    // matching, and this intra-app rule now agrees. Filtering only CONSUMES (not provides) is the safe
    // direction — it removes noise without creating a finding (a test-only provide can only suppress).
    for c in io_consumes
        .iter()
        .filter(|c| c.kind == "http" && !zzop_core::is_test_file(&c.file))
    {
        // The linker's whole STRUCTURAL gate sequence in one call — no-key, declared-host re-key, and
        // the `://` egress gate that must follow it — so neither the gates nor their ORDER can drift
        // from the multi-tree join (module doc "Structural gates"). Only `Joinable` reaches this rule's
        // own layer; `Unresolved`/`External` are the linker's non-drift buckets and are skipped here.
        let zzop_core::ConsumeJoin::Joinable { key, rekeyed_host } =
            zzop_core::classify_consume_join(c.key.as_deref(), internal_hosts)
        else {
            continue;
        };
        // The matched-host half is the linker's `host_rekey_counts` bookkeeping; this rule has no such
        // disclosure surface and uses it only to decide whether to carry the absolute spelling.
        let raw: Option<&str> = rekeyed_host.and(c.key.as_deref());
        let key_str: &str = &key;
        if provided_keys.contains(key_str) {
            continue;
        }
        if wildcard_route_keys
            .iter()
            .any(|r| zzop_core::wildcard_route_covers(r, key_str))
        {
            continue; // an ANT catch-all in THIS tree serves this call — module doc "Structural gates"
        }
        if !zzop_core::key_carries_route_identity(key_str) {
            continue; // all-`{}` path names no route — the linker's own gate, same predicate; module doc
        }
        if always_veto_re.is_match(key_str) {
            continue; // static-asset fetch, not API consumption — see module doc
        }
        let api_ish = api_segment_re
            .as_ref()
            .is_some_and(|re| re.is_match(key_str));
        if asset_dir_gated_re.is_match(key_str) && !api_ish {
            continue; // json/xml with no API-ish path segment — vetoed by default, see module doc
        }

        let is_foreign = match first_path_segment(key_str) {
            Some(segment) => !provide_first_segments.contains(segment),
            None => true, // no path segment at all — nothing to overlap with, see module doc
        };
        let item = UnmatchedConsume {
            key,
            raw,
            file: c.file.as_str(),
            line: c.line,
        };
        if is_foreign {
            foreign.push(item);
        } else {
            overlapping.push(item);
        }
    }

    // How the surviving set is SAID — one finding each, or one aggregate replacing N — is the fold's
    // job, not this function's (module doc "Foreign-vs-overlapping fold"; the split is in `fold`).
    fold::findings(
        &overlapping,
        &foreign,
        &provide_first_segments,
        contributing_provide_count,
    )
}

mod vocab;
pub use vocab::API_SEGMENT_PATTERN;

#[cfg(test)]
mod tests;
