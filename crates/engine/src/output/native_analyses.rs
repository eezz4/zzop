//! `AnalyzeOutput::native_analyses` — the NATIVE half of "which rules could this reply's `findings`
//! have come from", and the answer to a blank that had three unrelated causes wearing one face.
//!
//! # The defect this closes
//! A reply expresses its verdicts as `findings.byRule`, a map keyed by the rules that FIRED. A rule
//! that could not fire has no key, and an absent key is exactly what a rule that ran clean also
//! produces. For the DSL half that ambiguity is already closed one field over: `packsLoaded` marks a
//! pack the config switched off (`PackNotRun`) and lists the rules whose own path gates admitted no
//! file (`PackLoaded::zero_admission_rules`). The NATIVE analyses had no such channel at all — this
//! reply named none of them, ever.
//!
//! Measured on the nine-tree corpus before this field existed: `zzop analyze --config <one tree>`
//! reported **zero** `cross-layer/*` keys on all nine, while `zzop cross` over the SAME config file,
//! the same binary and the same tree reported **975 cross-layer findings across 9/9 trees** (cal.com
//! 255 over 11 rules, nocodb 324 over 4, immich 199 over 6, mall 162 over 3, fe-axios/fe-vite 12
//! each, eShop 9, koel/fe-redux 1 each). Not one of those nine blanks was an honest zero, and no
//! adjacent channel said so: `configWarnings` `[]`, `coverageGaps.extensions` `[]`, `degraded` `[]`,
//! `coverage.joinContributionZero` `false`.
//!
//! # The three causes are kept apart because the remedies are opposite
//! * [`NativeAnalyses::reported_in_cross_layer_findings`] — structural. These analyses judge the
//!   cross-tree JOIN and report into its own `crossLayerFindings` channel, which no per-tree output
//!   has. Remedy: run the cross-layer join.
//! * [`NativeAnalyses::disabled`] — the run's own config switched them off by id. Remedy: turn them
//!   back on.
//! * everything else — EXCEPT the two registration classes [`NativeAnalyses::registered`] names, which
//!   key no `findings.byRule` entry under their own id at all — it ran, and its absence from
//!   `findings.byRule` is a real zero. Remedy: none.
//!
//! Folded into one number they would say nothing, which is the state this field replaces.
//!
//! # Derived, never listed
//! Both lists come out of the registrations themselves — [`crate::register_all_native`] for the
//! population and `zzop_rules_cross_layer`'s own `register_native_analyses` for the cross-layer
//! subset — so a native analysis added, moved between crates or renamed is carried without an edit
//! here, and no count in this file can go stale. The floor that keeps a broken derivation from
//! reading as "everything was evaluated" is `rule_contracts::native_analyses`, which requires the
//! cross-layer subset to be a non-empty PROPER subset of the registry.

use zzop_core::{is_enabled, RuleConfig};

/// The native-analysis roster for one run: how many are registered in this build, and which of them
/// could not have contributed a `findings.byRule` key — split by CAUSE, because the reader's next
/// action differs per cause (see the module doc).
///
/// A registered id that appears in neither list ran — EXCEPT for the two registration classes
/// [`NativeAnalyses::registered`] names, which key no `findings` entry under their own id by
/// construction: an id that gates a score computation emits no finding at all, and an umbrella
/// registration reports under the finer `schema/<label>` ids, so look for those rather than for the
/// umbrella id. For every OTHER id in neither list, absence from `findings` is a measured zero. That is
/// the only claim this type makes about it — like `PackLoaded::zero_admission_rules`, it says nothing
/// about whether the analysis then had anything to judge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeAnalyses {
    /// Every native analysis id `register_all_native` composes into this build, counted. The
    /// DENOMINATOR the two lists below are read against — without it "27 not evaluated" is a
    /// magnitude with no scale, the failure `score-population-empty` names one layer over.
    ///
    /// Counts REGISTRATIONS, which is the id space the config gates and `RuleOverridesApplied` speaks
    /// — not the set of ids a finding can carry. The two differ in both directions and deliberately:
    /// the ids `zzop_metrics` registers gate score computations that emit no finding at all, and
    /// `schema-structural`/`schema-usage` are umbrella registrations whose findings carry the
    /// finer `schema/<label>` ids built at finding time. Publishing the gate space is what makes this
    /// number joinable with `ruleOverridesApplied`, which is where a reader goes next.
    ///
    /// # The two classes are NAMED, never COUNTED
    /// This sentence said "`zzop_metrics`' five ids" until 2026-08-29. The module doc and this type's
    /// own doc both defer to this one for the description of the two classes they carve out, so a stale
    /// number here would have been load-bearing in three places at once.
    ///
    /// The same hand count was judged in `crates/facade/src/output/native_analyses_legend.rs`, and that
    /// verdict is NOT inherited here, because the dependency graph is not the same: `zzop-engine` has a
    /// direct edge to BOTH `zzop-metrics` and `zzop-rules-schema`, where the facade has one to the first
    /// and none to the second. Re-judged from here it lands the same way. The score-gating ids ARE
    /// countable at runtime from this crate — but this is a doc comment, which cannot carry a runtime
    /// value at all. The umbrella subset is not countable from anywhere:
    /// `zzop_rules_schema::register_native_analyses` registers SEVENTEEN ids (the family gates, its join
    /// rules, and the `schema/<label>` ids derived from its exported label lists), and the family gates
    /// are spelled only inside private items of that crate — so the obvious derivation, counting that
    /// crate's registrations, returns 17 and would ship a false number with more confidence than the
    /// hand stamp it replaced. What a reader acts on is WHICH classes exist, not how many ids each
    /// holds: the action is "look under `schema/<label>` rather than the umbrella id", and no count
    /// changes it.
    pub registered: usize,
    /// Registered ids this run's config switched off, sorted. Read through [`is_enabled`] — the same
    /// gate the evaluator consults — rather than by re-reading `disabled_rules`, so this cannot
    /// disagree with the gate about what ran.
    ///
    /// Overlaps `RuleOverridesApplied::disabled` on purpose, and is not redundant with it: that field
    /// answers *which entries of the caller's own request took effect* (a typo never appears there,
    /// and it is `None` when the caller requested nothing), while this one answers *which of this
    /// build's native analyses were not evaluated* — the question a reader of an empty `findings` has.
    /// The same non-redundancy `PackNotRun` has against that field, one id space over.
    pub disabled: Vec<String>,
    /// Registered ids that are STILL ENABLED and yet can never key this reply's `findings`: they judge
    /// the cross-tree join and report into `crossLayerFindings`, a channel only the multi-tree reply
    /// shape has. Sorted.
    ///
    /// Disabled ids are excluded rather than listed twice — this list carries an implicit remedy
    /// ("run the join and you will see these"), and that remedy is false for an analysis the config
    /// switched off. `disabled` is the honest home for those, and the two lists are therefore
    /// disjoint by construction.
    ///
    /// Unconditional, not gated on "did the join run": it is a statement about where these analyses
    /// REPORT, which is true of every per-tree output in every command. A per-tree row inside a
    /// multi-tree reply carries it too, and correctly so — the findings are in that reply's top-level
    /// `crossLayerFindings`, not in the row's own `findings`.
    pub reported_in_cross_layer_findings: Vec<String>,
}

impl NativeAnalyses {
    /// Builds the roster from the run's gate config alone — no walk, no findings, no IR, so it is
    /// identical on warm (cache-replayed) and cold runs and available on every construction path
    /// (`analyze::assemble` and `envelope::ingest` alike).
    pub fn of(rule_config: &RuleConfig) -> Self {
        let mut all = zzop_core::RuleRegistry::new();
        crate::register_all_native(&mut all);

        let mut cross = zzop_core::RuleRegistry::new();
        zzop_rules_cross_layer::register_native_analyses(&mut cross);

        let mut disabled: Vec<String> = all
            .ids()
            .iter()
            .filter(|id| !is_enabled(rule_config, id))
            .cloned()
            .collect();
        disabled.sort();

        let mut reported_in_cross_layer_findings: Vec<String> = cross
            .ids()
            .iter()
            .filter(|id| is_enabled(rule_config, id))
            .cloned()
            .collect();
        reported_in_cross_layer_findings.sort();

        // The derivation's own floor. An empty cross-layer registration would publish an empty list,
        // which reads as "every native analysis reports into `findings`" — the false all-clear this
        // whole field exists to remove, and it would look identical to a healthy run. The release
        // build cannot afford a panic here, so the load-bearing check is
        // `rule_contracts::native_analyses`; this is the same assertion where it is free.
        debug_assert!(
            !cross.ids().is_empty(),
            "zzop_rules_cross_layer registered nothing — the cross-layer disclosure would be silently \
             vacuous"
        );

        Self {
            registered: all.ids().len(),
            disabled,
            reported_in_cross_layer_findings,
        }
    }
}
