//! `cross-layer/all-consumes-unjoined` (info) — ONE finding per source tree whose internal http consumes
//! were all extracted successfully and NONE of them produced a join edge. The tree talks to something; this
//! run cannot say to what.
//!
//! ## Why this rule exists — the measured wall it replaces
//! Measured 2026-07-29 over the 17-tree dogfood join, template vocabulary on: **zero cross-source http
//! edges run-wide**, and the per-consume rules that fire in that vacuum produced the bulk of the output —
//! 76 `cross-layer/ambiguous-consume` (19 apiece from `fe-angular`/`fe-axios`/`fe-vite`/`fe-vue`) plus 17
//! `cross-layer/unprovided-mutation-call` from `be-fastapi-fs`, i.e. **93 of 210 findings**. Every one of
//! those 93 restates a single per-tree fact — this tree's calls did not join — once per call site. Reading
//! 19 of them teaches nothing the first one did not.
//!
//! The two shapes behind it are both a base/prefix that static analysis refuses to guess, on opposite sides
//! of the join:
//! - CONSUME side (`fe-axios`): `axios.defaults.baseURL = settings.baseApiUrl` — a cross-file constant, so
//!   the call site keeps `GET /articles` while the backend serves `GET /api/articles`.
//!   `parser-typescript`'s `client_base` adapter names this exact expression as the shape it refuses.
//! - PROVIDE side (`be-fastapi-fs`): `app.include_router(api_router, prefix=settings.API_V1_STR)` — the
//!   mirror image. The routes key as `GET /items` while the tree's own generated SDK calls
//!   `GET /api/v1/items`.
//!
//! Both are the never-guess IO convention working as designed. What was NOT working as designed is the
//! output: a refusal to guess was being paid for in N per-call-site findings, several of which
//! (`unprovided-mutation-call` = "no provider anywhere") are outright FALSE when the provider is sitting in
//! the join behind an unresolved prefix.
//!
//! ## Replacement, not suppression (`output-philosophy.md` §0/§1)
//! The orchestrator drops the per-consume `cross-layer/ambiguous-consume` and
//! `cross-layer/unprovided-mutation-call` findings anchored in a tree this rule fired for, via
//! [`subsumed_sources`](AllConsumesUnjoinedOutput::subsumed_sources). Nothing is lost silently: this
//! finding states the count, the bucket split, and a sample of the affected keys, exactly as
//! `prefix_drift` (the precedent this follows) enumerates every route it folds.
//!
//! AGGREGATES ARE NEVER SUBSUMED. `cross-layer/prefix-drift` and `cross-layer/unresolved-consume-ratio` are
//! already one-per-cause findings and are strictly more actionable than this one when they fire — drift
//! names the actual prefix. Folding them into this would trade a specific diagnosis for a generic one.
//!
//! ## Info severity, and the knobs the message names
//! Info: "this tree did not join" is a coverage statement about the analysis, not a defect claim about the
//! code — the same reasoning that keeps `prefix-drift` at info. Per `output-philosophy.md` §9 the message
//! only names knobs that EXIST. It used to say, truthfully, that the CALLING side had no declarative
//! repair at all; `trees[].topology.clientBase` (2026-07-29) is the knob that made that sentence false,
//! and this rule's message is the reason it was built — a disclosure that can only say "rewrite your
//! source" is a diagnosis with no remedy for the exact shape it diagnoses.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::{disable_hint, CrossLayerResult, Finding, Severity};

/// Minimum internal, keyed http consumes a tree must contribute before a total join failure is worth one
/// finding. Below this the per-consume findings ARE the readable form — folding 2 into 1 that says
/// "2 findings were folded" is pure ceremony.
///
/// The value equals `MIN_PREFIX_DRIFT_GROUP` but is a SEPARATE constant on purpose (a T3 divergence, not a
/// missing T1 share): both answer "pattern or coincidence?", but over DIFFERENT populations — prefix-drift
/// counts near-misses that share one prefix against one target tree, this counts calls no rule could
/// explain at all. Nothing keeps them equal and nothing should; tying them would make a change to either
/// rule's sensitivity silently move the other. Policy value — pinned by `fires_at_threshold_not_below`.
/// (rule-quality.md §6 inventory.)
pub const MIN_UNJOINED_CONSUMES: usize = 3;

/// How many affected consume keys the message and `data.sampleKeys` list before truncating. The finding
/// always states the honest TOTAL (`consumeCount`) alongside, so a truncated sample can never read as the
/// whole set.
const MAX_SAMPLE_KEYS: usize = 8;

/// [`all_consumes_unjoined_findings`]'s return: the per-tree findings, plus the source ids whose
/// per-consume findings the orchestrator now drops as replaced.
pub struct AllConsumesUnjoinedOutput {
    pub findings: Vec<Finding>,
    /// Source ids a finding fired for. The orchestrator drops `cross-layer/ambiguous-consume` and
    /// `cross-layer/unprovided-mutation-call` findings whose consume anchor belongs to one of these trees —
    /// see [`retain_non_subsumed_sources`].
    pub subsumed_sources: BTreeSet<String>,
}

/// What this tree's unjoined consumes were, per bucket — the split the message reports so the reader can
/// tell "nothing provides this key" (drift) from "several trees do" (ambiguity), which need different fixes.
#[derive(Default)]
struct Tally {
    unprovided: usize,
    ambiguous: usize,
    /// First `(file, line)` seen, in sorted order — the finding's anchor.
    sites: BTreeSet<(String, u32)>,
    keys: BTreeSet<String>,
}

impl Tally {
    fn total(&self) -> usize {
        self.unprovided + self.ambiguous
    }
}

/// Emits one finding per source whose internal keyed http consumes number at least
/// [`MIN_UNJOINED_CONSUMES`] and produced ZERO http join edges, provided this run actually has http routes
/// to join against.
///
/// ## What counts, and what deliberately does not
/// - `unprovided_consumes` and `ambiguous_consumes` COUNT: both are keys the extractor resolved and the
///   linker compared. They are the population a base/prefix fix would move into `edges`.
/// - `unresolved_consumes` does NOT count. Those are extractor blindness (`key: None`, or an all-`{}` path)
///   and already have their own per-tree aggregate, `cross-layer/unresolved-consume-ratio`. Counting them
///   here would let a tree that extracted nothing look like a tree whose join failed, which is a different
///   defect with a different fix.
/// - `external_consumes` does NOT count. An absolute-URL call to a third party is SUPPOSED not to join; a
///   tree that only talks to Stripe is healthy, and including it here would fire on every such tree forever.
/// - Only `kind == "http"`. `db-table` joins are intra-tree by nature and say nothing about route topology.
///
/// ## The run-has-providers precondition
/// A run with no http provides anywhere (a front end analyzed alone) cannot join by construction, and
/// saying "your consumes did not join" there blames the user for the shape of their own invocation. The
/// gate is checked once, run-wide, from the same `cross_layer` value.
///
/// ## `blind_sources` — joining the blind-spot partition instead of becoming its third co-firer
/// `blind_sources` is [`super::majority_unresolved_http_sources`] — the shipped predicate for "this tree's
/// http consumes are mostly unextractable". A tree in that set is skipped outright. It has already
/// self-reported through `cross-layer/unresolved-consume-ratio`, whose diagnosis (the EXTRACTOR could not
/// key these call sites) is both different from and more accurate than this rule's (the keys were fine, the
/// BASE was not) — and `MIN_TOTAL_CONSUMES`' own doc records that `unresolved-consume-ratio` and
/// `untraced-client-import-no-visible-consume` were deliberately built to partition the space and never
/// co-fire on one tree. This rule is the third member of that family and holds the same line.
///
/// Measured: the `unresolved` benchmark fixture — a tree whose URLs are assembled from a variable `base` —
/// fired both until this gate existed. "Likely one unresolved base path" was not even wrong there, which is
/// exactly why the tie has to break toward the rule that names the real mechanism.
///
/// ## `diagnosed` — the precondition that keeps this from being a second, worse near-miss rule
/// `diagnosed` carries the `(source, file, line)` anchor of every consume some SPECIFIC cross-layer rule
/// already explained: `method-mismatch`, `version-skew`, `path-near-miss`, `route-near-miss`,
/// `prefix-drift`. Those consumes do NOT count toward the floor, because each one is proof the join is
/// working — the key WAS compared against real provides and came close. A tree whose calls each break in
/// their own way is not a tree with one unresolved base path, and saying so would be a false diagnosis
/// stated more confidently than the per-call findings it replaced.
///
/// This is not hypothetical tuning: the 2026-07-29 detection benchmark caught exactly that. Two fixture
/// trees fired before this precondition existed — `xlayer-fe`, which plants one of EVERY cross-layer defect
/// (a method mismatch, a version skew, a path near-miss, ...) in five adjacent lines, and `unresolved`,
/// whose keyed minority is a deliberate method mismatch. Both were true statements ("nothing joined") and
/// both were the wrong story. With `diagnosed` excluded each drops to 2 undiagnosed calls, below the floor,
/// and stays silent — while the four real dogfood front ends (19-23 undiagnosed calls apiece) still fold.
pub fn all_consumes_unjoined_findings(
    cross_layer: &CrossLayerResult,
    diagnosed: &BTreeSet<(String, String, u32)>,
    blind_sources: &BTreeSet<String>,
) -> AllConsumesUnjoinedOutput {
    let mut out = AllConsumesUnjoinedOutput {
        findings: Vec::new(),
        subsumed_sources: BTreeSet::new(),
    };

    if !run_has_http_provides(cross_layer) {
        return out;
    }

    // Sources that DID join at least one http consume are disqualified up front: the join works for them,
    // so whatever else is wrong is not "this tree cannot reach the join".
    let joined: BTreeSet<&str> = cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http")
        .map(|e| e.from.source.as_str())
        .collect();

    let is_diagnosed = |source: &str, file: &str, line: u32| {
        diagnosed.contains(&(source.to_string(), file.to_string(), line))
    };

    let mut tallies: BTreeMap<&str, Tally> = BTreeMap::new();
    for c in &cross_layer.unprovided_consumes {
        if c.consume.kind != "http" || is_diagnosed(&c.source, &c.consume.file, c.consume.line) {
            continue;
        }
        let t = tallies.entry(c.source.as_str()).or_default();
        t.unprovided += 1;
        t.sites.insert((c.consume.file.clone(), c.consume.line));
        if let Some(k) = &c.consume.key {
            t.keys.insert(k.clone());
        }
    }
    for a in &cross_layer.ambiguous_consumes {
        if a.consume.kind != "http" || is_diagnosed(&a.source, &a.consume.file, a.consume.line) {
            continue;
        }
        let t = tallies.entry(a.source.as_str()).or_default();
        t.ambiguous += 1;
        t.sites.insert((a.consume.file.clone(), a.consume.line));
        if let Some(k) = &a.consume.key {
            t.keys.insert(k.clone());
        }
    }

    for (source, tally) in tallies {
        if joined.contains(source)
            || blind_sources.contains(source)
            || tally.total() < MIN_UNJOINED_CONSUMES
        {
            continue;
        }
        let Some((file, line)) = tally.sites.iter().next().cloned() else {
            continue;
        };
        out.subsumed_sources.insert(source.to_string());

        let n = tally.total();
        let sample: Vec<&str> = tally
            .keys
            .iter()
            .take(MAX_SAMPLE_KEYS)
            .map(String::as_str)
            .collect();
        let more = tally.keys.len().saturating_sub(sample.len());
        let sample_tail = if more > 0 {
            format!(", +{more} more")
        } else {
            String::new()
        };
        let split = format!(
            "{} with no provider anywhere, {} matching 2+ trees",
            tally.unprovided, tally.ambiguous
        );

        let message = format!(
            "not one internal http call extracted from `{source}` reached a provider, and {n} of them \
             ({split}) got no more specific explanation from any other cross-layer rule — no near-miss, no \
             method or version mismatch. This run does have routes to join against, so the likely cause is \
             ONE unresolved base path, not {n} independent problems. This engine refuses to guess a base it \
             cannot read \
             statically (a `baseURL` assigned from a cross-file constant on the calling side, a router \
             mounted under a computed prefix on the serving side), which is why the keys never lined up. \
             Four repairs that work today: make the base a string literal at its assignment; declare THIS \
             tree's own outbound base in zzop.config.jsonc (`trees[].topology.clientBase` — the calling \
             side's knob, and the one to reach for when the base is a cross-file constant); declare the \
             SERVING side's mount (`trees[].topology.mountedAt`, `trees[].topology.mounts`, \
             `trees[].topology.hosts`); or supply the effective keys through an adapter overlay \
             (`overlays: [...]`, see `zzop contract adapter-guide` with the CLI binary, or MCP resource \
             `zzop://contract/adapter-guide`). This replaces \
             the per-call `cross-layer/ambiguous-consume` and `cross-layer/unprovided-mutation-call` \
             findings for this tree, whose \"no provider\" verdicts cannot be trusted while the join is \
             dark; affected keys: {}{sample_tail}. If a `cross-layer/prefix-drift` finding also fired for \
             this tree it names the CONCRETE prefix on the routes it could compare — read that one first; \
             this finding is what tells you the same cause covers all {n} calls rather than only those. {}",
            sample.join(", "),
            disable_hint("cross-layer/all-consumes-unjoined"),
        );

        out.findings.push(Finding {
            rule_id: "cross-layer/all-consumes-unjoined".to_string(),
            severity: Severity::Info,
            file,
            line,
            message,
            evidence_paths: Vec::new(),
            data: Some(serde_json::json!({
                "consumeSource": source,
                "consumeCount": n,
                "unprovidedCount": tally.unprovided,
                "ambiguousCount": tally.ambiguous,
                "distinctKeyCount": tally.keys.len(),
                "sampleKeys": sample,
            })),
        });
    }

    out.findings
        .sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

/// Does this run contain any http provide at all? True when an http edge landed, when an unconsumed provide
/// is http, or when an ambiguous consume names http candidates — the three places a provide can survive
/// into `CrossLayerResult`. Without one, no consume in the run COULD have joined and the rule stays silent.
fn run_has_http_provides(cross_layer: &CrossLayerResult) -> bool {
    cross_layer.edges.iter().any(|e| e.kind == "http")
        || cross_layer
            .unconsumed_provides
            .iter()
            .any(|p| p.provide.kind == "http")
        || cross_layer
            .ambiguous_consumes
            .iter()
            .any(|a| a.consume.kind == "http" && !a.candidates.is_empty())
}

mod subsume;
pub use subsume::retain_non_subsumed_sources;

#[cfg(test)]
mod tests;
