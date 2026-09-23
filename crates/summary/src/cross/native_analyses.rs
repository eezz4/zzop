//! `nativeAnalyses` + `nativeAnalysesMeaning` for the JOIN reply — the same roster the per-tree reply
//! carries, re-derived for the run the join actually gated, and read with the OPPOSITE action.
//!
//! # The defect this closes
//! `9272ee0` gave the per-tree `analyze` reply a native-analysis roster, because a `cross-layer/*` rule
//! that had nowhere to report looked exactly like one that ran clean. Its own argument was general:
//! *"a channel populated by what it found reports nothing on a clean run in bytes indistinguishable
//! from one that never ran"*. That argument holds here word for word and this lane was not fixed.
//! Measured on the nine-repo corpus before this module existed: `zzop cross` carried `nativeAnalyses`
//! NOWHERE — not at the reply root, not on any of the nine `sources[]` rows — while its own
//! `crossLayerFindings.byRule` held 18 keys out of a 27-analysis roster. The nine silent analyses
//! (`ambiguous-consume`, `body-field-drift`, `external-base-url-drift`,
//! `external-host-in-multiple-sources`, `external-ip-literal`, `path-near-miss`,
//! `retrying-write-no-idempotency`, `route-shadowing`, `version-skew`) had no population standing next
//! to them, so "ran and found nothing" and "never ran" were the same absence — in the ONE reply where
//! those analyses are the whole subject.
//!
//! # 🔴 The same key, the inverted action — which is why the legend forks and the field name does not
//! On a per-tree reply `reportedInCrossLayerFindings` means *"not evaluable here; run the join"*. In
//! THIS reply those analyses are the channel the reader is already looking at, so that sentence would
//! read *"run the thing you just ran"*. What the key CLAIMS is lane-invariant — these analyses report
//! into `crossLayerFindings` — and only the reader's next action inverts, so the name stays and the
//! legend splits. Keeping one name across both lanes is deliberate: a reader who moves between the two
//! replies learns one vocabulary, and the two lists are the same derivation from the same
//! registrations.
//!
//! Because the join reply is the one place where the population is REAL, it earns the row the per-tree
//! reply cannot have: [`ZERO_KEY`] publishes the members of that population which keyed nothing in
//! `crossLayerFindings.byRule` this run. That is a zero turned into a row (a zero is invisible when it
//! is an absent map key), and what it always means is that the analysis was ENABLED and the join RAN
//! it — never "not measured".
//!
//! # 🔴 What it does NOT mean, and the run that proved the legend wrong
//! It used to also say "these are MEASURED zeros", full stop. A zero-keyed row has three causes, and
//! only one of them is a clean bill of health. On the nine-repo dogfood join this legend stood over
//! `cross-layer/ambiguous-consume` and `cross-layer/unprovided-mutation-call` while a fired
//! `cross-layer/all-consumes-unjoined` had REPLACED 76 and 17 of their findings respectively —
//! switching that one aggregate off took the run from 226 findings to 314. Worse, the legend's own
//! verification pointer sent the reader the wrong way: it named `buckets`/`sources[].coverage` as
//! where "an empty input channel is visible", and both buckets were populated (114 and 24), so
//! following the instruction produced "it ran, it had input, it found nothing — clean".
//!
//! The fix is a MEASUREMENT rather than a standing caveat. A rule that subsumes another's findings
//! declares the ids it stands in for in its own `data.replaces`; [`join_roster`] intersects that
//! declaration with the zero list and the legend names the result, so the sentence is true of the run
//! in front of the reader and this crate never learns a rule id of its own.
//!
//! # Derived, never listed, and derived from the SAME numbers the join gated on
//! Nothing here is hand-written and no count appears in this file. Every input is a per-tree
//! `nativeAnalyses` object the engine already derived from `register_all_native` /
//! `zzop_rules_cross_layer::register_native_analyses` / `zzop_core::is_enabled`, and this module only
//! recombines them the way the join itself does:
//!
//! * `disabled` = the UNION over trees. `zzop_engine`'s `cross_layer_findings::merge_config` gates the
//!   join on the union of every tree's `disabled_rules` (EXCLUDE-ONLY: a joint verdict no single tree
//!   owns, so any tree's opt-out opts the run out), and `zzop_core::is_enabled` consults nothing but
//!   `disabled_rules`. So the union of "disabled in tree i" IS the join gate's disabled set — an
//!   identity, not an approximation. If `is_enabled` ever grows a second input, that identity breaks;
//!   [`crate::cross_test`] pins it against a real reply so the break is loud.
//! * the population = that union of the trees' own cross-layer lists, MINUS `disabled`. Written as a
//!   subtraction rather than an intersection because the subtraction is what `merge_config` does; the
//!   two agree today and only the subtraction stays right if a tree stops publishing a list.
//! * the zero list = the population minus the rule ids that keyed the RAW `crossLayerFindings` array.
//!   Raw, never the shaped view: `--severity`/`--rule` narrow `shown`, and a roster that moved with a
//!   display filter would report "this analysis found nothing" about an analysis the filter hid.
//!
//! # The floor
//! A collapsed derivation publishes an empty population, which reads as *"no analysis reports into
//! `crossLayerFindings`"* — a confident falsehood standing beside a `crossLayerFindings` map that has
//! entries, and byte-identical to a healthy run of a build with no cross-layer rules. So the invariants
//! are asserted where they are free (`debug_assert`) and pinned where they are load-bearing
//! (`crate::cross_test`), exactly as `zzop_engine::NativeAnalyses` does for its own derivation:
//! the population is a non-empty PROPER subset of `registered`, and every id that FIRED is in it.
//! Absence of the whole object means one thing only — no tree published a roster (an older engine) —
//! and the key is then omitted rather than written as `null`, the same degradation `packsLoadedMeaning`
//! takes one field over.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use serde_json::Value;

/// The reply key for the population that produced nothing this run. Named as a subtraction from its
/// neighbour (`reportedInCrossLayerFindings` minus what fired) so the two keys read as one sentence.
const ZERO_KEY: &str = "zeroInCrossLayerFindings";

/// The join reply's `(nativeAnalyses, nativeAnalysesMeaning)`, or `None` when no tree published a
/// roster to recombine.
///
/// `trees` is `analyzeTrees`' own `trees` array (each entry's `output` is an `AnalyzeOutputView`);
/// `cross_layer_findings` is the RAW join findings array, before any display filter.
pub(super) fn join_roster(
    trees: &[Value],
    cross_layer_findings: &[Value],
) -> Option<(Value, Value)> {
    let rosters: Vec<&Value> = trees
        .iter()
        .filter_map(|t| t["output"].get("nativeAnalyses"))
        .collect();
    if rosters.is_empty() {
        return None;
    }

    // A build constant: every tree in one run is analyzed by one binary, so the trees cannot disagree
    // about how many analyses that binary registers. `max` rather than "first" only so a tree whose
    // field is malformed cannot silently shrink the denominator every other number is read against.
    let registered = rosters
        .iter()
        .filter_map(|n| n.get("registered").and_then(Value::as_u64))
        .max()?;

    let disabled = union(&rosters, "disabled");
    let population: BTreeSet<String> = union(&rosters, "reportedInCrossLayerFindings")
        .difference(&disabled)
        .cloned()
        .collect();
    let fired: BTreeSet<String> = cross_layer_findings
        .iter()
        .filter_map(|f| f.get("ruleId").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    let zero: Vec<String> = population.difference(&fired).cloned().collect();
    // The ids in `zero` that are there because something REPLACED them, read off the replacing
    // finding's own declaration rather than from a list this crate keeps. A rule that folds another
    // rule's per-call findings into one aggregate publishes `data.replaces`; anything else in `zero`
    // genuinely produced nothing. Generic on purpose: the next aggregate that subsumes gets this for
    // free, and this crate never learns a rule id it would then have to be kept in step with.
    // Keyed by the REPLACING rule id, because "switch it off to see what it stands for" is only an
    // instruction if the reader is told which knob — a legend that made them hunt for the aggregate
    // would be a second version of the pointer this whole repair is fixing.
    let mut replaced: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for f in cross_layer_findings {
        let (Some(by), Some(ids)) = (
            f.get("ruleId").and_then(Value::as_str),
            f.get("data")
                .and_then(|d| d.get("replaces"))
                .and_then(Value::as_array),
        ) else {
            continue;
        };
        for id in ids.iter().filter_map(Value::as_str) {
            if zero.iter().any(|z| z == id) {
                replaced.entry(by).or_default().insert(id);
            }
        }
    }

    // The derivation's own floor, asserted where it costs nothing. Each of these failing means the
    // published roster is a lie of a specific shape, and each lie is invisible in the output:
    //   - an empty population reads as "nothing reports here" beside a populated findings map;
    //   - a population that is not a PROPER subset of the registry means the cross-layer subset
    //     swallowed the whole registry, i.e. the two registrations were read as one;
    //   - a fired id outside the population means the roster and the findings disagree about what ran,
    //     which is the exact confusion this object exists to end.
    // The release build cannot afford a panic on a reply path, so the load-bearing checks are the
    // behavioural pins in `crate::cross_test` — the same split `zzop_engine::NativeAnalyses` makes.
    debug_assert!(
        !population.is_empty() || !disabled.is_empty(),
        "the join's cross-layer roster derived to EMPTY with nothing disabled — the per-tree rosters \
         were unreadable, and the reply would claim no analysis reports into crossLayerFindings"
    );
    debug_assert!(
        (population.len() as u64) < registered,
        "the cross-layer population ({}) is not a PROPER subset of the {registered} registered \
         analyses — the two registrations collapsed into one",
        population.len()
    );
    debug_assert!(
        fired.is_subset(&population),
        "crossLayerFindings carries rule ids the derived roster does not: {:?}",
        fired.difference(&population).collect::<Vec<_>>()
    );

    // Built key by key rather than through the `json!` macro because one key is a `const`, and
    // spelling "zeroInCrossLayerFindings" a second time here is exactly how a field and the legend
    // that explains it drift apart — the legend below is keyed by that same const.
    let mut roster = serde_json::Map::new();
    roster.insert("registered".to_string(), Value::from(registered));
    roster.insert(
        "disabled".to_string(),
        Value::from(disabled.into_iter().collect::<Vec<_>>()),
    );
    roster.insert(
        "reportedInCrossLayerFindings".to_string(),
        Value::from(population.into_iter().collect::<Vec<_>>()),
    );
    roster.insert(ZERO_KEY.to_string(), Value::from(zero));
    Some((Value::Object(roster), meaning(&replaced)))
}

/// The union of one list-valued key across every tree's roster, sorted and deduped by `BTreeSet`.
///
/// Union is the join's own combinator for both keys this is called with, and for one reason: the join
/// gate is EXCLUDE-ONLY (`merge_config::union_configs`), so anything any tree switched off is off for
/// the run. Applied to the cross-layer list it is the pre-subtraction population — every id at least
/// one tree still knows about — which the caller then narrows by that same union of exclusions.
fn union(rosters: &[&Value], key: &str) -> BTreeSet<String> {
    rosters
        .iter()
        .filter_map(|n| n.get(key).and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// The join reply's legend. Two sentences are read from `zzop_facade` because they describe the BUILD
/// and the GATE and mean the same thing in either lane; the rest are written here because they are
/// exactly what inverts. See this module's header for why the key name does not fork with them.
fn meaning(replaced: &BTreeMap<&str, BTreeSet<&str>>) -> Value {
    let mut m: BTreeMap<&'static str, String> = BTreeMap::new();
    m.insert(
        "registered",
        zzop_facade::NATIVE_ANALYSES_REGISTERED_MEANING.to_string(),
    );
    m.insert(
        "disabled",
        format!(
            "{} In a JOIN reply this is the RUN's gate rather than one tree's: a cross-layer analysis \
             is off for the whole join when ANY analyzed tree's config disables it, because the \
             verdict spans trees and no single tree owns it — so an id here may be switched off in \
             only one of the `sources[]` below.",
            zzop_facade::NATIVE_ANALYSES_DISABLED_MEANING
        ),
    );
    m.insert(
        "reportedInCrossLayerFindings",
        "the analyses whose verdicts THIS reply carries: they judge the cross-tree join and report \
         into `crossLayerFindings` above. That map is read against this list — it is the population, \
         and a key missing from it is a silence this list makes visible rather than one you have to \
         infer. The key is spelled the same on a per-tree `analyze` reply and derived the same way, \
         but the action it licenses there is the opposite one (`run the join`); here the join HAS run \
         and this is its roster."
            .to_string(),
    );
    // The REPLACED clause is computed per run, never a standing caveat. Until 2026-08-31 this legend
    // said only "These are MEASURED zeros", and on the nine-repo dogfood join that sentence stood over
    // `cross-layer/ambiguous-consume` and `cross-layer/unprovided-mutation-call` while a fired
    // `cross-layer/all-consumes-unjoined` had dropped 76 and 17 of their findings — switching that one
    // aggregate off moved the run from 226 findings to 314. The reply was not hiding it (the aggregate's
    // own message names what it replaces), but this roster is the channel built for exactly this
    // question, and it was answering it wrong. Naming the ids only when a finding DECLARED replacing
    // them keeps the legend a measurement rather than a disclaimer.
    let replacement = if replaced.is_empty() {
        "Nothing in this run declared a replacement, so every id above is the first kind."
            .to_string()
    } else {
        let clauses: Vec<String> = replaced
            .iter()
            .map(|(by, ids)| {
                format!(
                    "{by} (fired above) declares in its own `data.replaces` that it stands in for {} \
                     — one aggregate per affected tree instead of one finding per call site, and its \
                     message names the tree and the affected keys; set `\"{by}\": \"off\"` under \
                     `rules` in zzop.config.jsonc and re-run to see the findings it stood in for",
                    ids.iter().copied().collect::<Vec<&str>>().join(" and ")
                )
            })
            .collect();
        format!(
            "🔴 In THIS run the following are the SECOND kind and are NOT measured zeros: {}.",
            clauses.join("; ")
        )
    };
    m.insert(
        ZERO_KEY,
        format!(
            "the members of the list above that keyed nothing in `crossLayerFindings.byRule` this run \
             — `reportedInCrossLayerFindings` minus those keys, published as a list of its own because \
             a zero is invisible when it is an absent map key. Each of these analyses was ENABLED and \
             the join DID run it, so an id here never means 'not measured'. It does not follow that the \
             analysis found nothing, and there are three ways to land here, which take three different \
             readings. (1) MEASURED ZERO: it ran over a real population and judged it clean. \
             (2) REPLACED: it produced findings and an aggregate that also fired subsumed them, so the \
             verdict is in this reply under a different rule id rather than absent. (3) NOTHING TO \
             JUDGE: its input channel was empty — `buckets` and `sources[].coverage` above are where \
             that is visible, and note that a NON-empty bucket does not by itself make an id the first \
             kind, because (2) starts from a non-empty bucket too. {replacement} Computed over the FULL \
             findings set: the severity and rule filters narrow `shown` and never this list."
        ),
    );
    m.insert(
        "everythingElse",
        "a registered analysis in NEITHER list judges ONE TREE rather than the join, so this reply is \
         not where its verdict lives — and there are TWO reasons it might not be in the tree's reply \
         either, which this reply cannot tell apart. Usually it ran and reported into that tree's own \
         `findings`, which this reply does not carry: `sources[].findingCount` is a count of it and \
         nothing more. But some analyses SHIP OFF (unused-code hygiene — see `nativeAnalyses.shippedOff` \
         on a per-tree reply), and those were not evaluated at all unless that tree's config named them. \
         Either way the answer is in the tree's own reply, not here: analyze the source you care about \
         on its own, and read its roster. The join gate is EXCLUDE-ONLY and cannot turn anything on, so \
         nothing this reply says can settle it — which is why there is no `shippedOff` key at this root \
         and why one would be a guess: a config that opted in for one tree and not another would have \
         no honest entry, and a union or an intersection would each be wrong for half the trees. The \
         two exceptions `registered` names apply here too."
            .to_string(),
    );
    serde_json::to_value(m).unwrap_or(Value::Null)
}
