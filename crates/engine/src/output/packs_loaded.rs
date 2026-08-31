//! `AnalyzeOutput::packs_loaded`'s element type and its one constructor — the POSITIVE pack-load
//! confirmation on the wire.
//!
//! Split out of `output.rs` on 2026-08-20 for the repo's per-file line cap, and the seam is the one
//! the two names already draw: `output.rs` is the roster of what one `analyze_tree` returns, this is
//! what a single loaded pack SAYS about itself. Nothing here reads a finding, a score or an IR node;
//! its whole input is `EngineConfig` plus the applicability census
//! (`analyze::diagnostics::pack_scope`), which is the module family this type is the wire face of.
//! The move itself changed nothing; `rule_ids`'s sort, made in the same batch, is recorded on that
//! field.

use zzop_core::{is_pack_enabled, RuleConfig};

use crate::{EngineConfig, PackSource};

/// Why a LOADED pack was never evaluated this run — `PackLoaded::did_not_run`'s payload, and the one
/// thing this struct says about GATING rather than loading.
///
/// The two variants are the two pack-level levers, and they are the whole of that axis:
/// `zzop_core::is_pack_enabled` is the single gate every pack-level call site goes through, and it
/// consults exactly these two lists. Per-RULE gating (`disabled_rules` holding a `"<pack>/<rule>"` id)
/// is deliberately NOT here — such a pack does run, and `rule_ids` minus
/// `RuleOverridesApplied::disabled` already answers that question one level down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackNotRun {
    /// The pack's own id sits in `RuleConfig::disabled_rules` (config dialect `packs.disabled`, or a
    /// `rules` entry set to a disabling severity — both fill the same vec; embedders: `disabledRules`).
    Disabled,
    /// A pack ALLOWLIST is in force (`RuleConfig::only_packs`; config `packs.only`, embedders
    /// `packsOnly`) and does not name this pack.
    NotAllowlisted,
}

impl PackNotRun {
    /// The wire string `AnalyzeOutput::packs_loaded` serializes: `"disabled"` | `"notAllowlisted"`.
    pub fn as_str(self) -> &'static str {
        match self {
            PackNotRun::Disabled => "disabled",
            PackNotRun::NotAllowlisted => "notAllowlisted",
        }
    }
}

/// Mirrors [`is_pack_enabled`] — it is CALLED here rather than reimplemented, so this can never
/// disagree with the gate about whether a pack ran; only about which of the two lists to blame, and
/// that attribution repeats `is_pack_enabled`'s own order (allowlist first). A pack both outside an
/// allowlist and explicitly disabled reports `NotAllowlisted`, which is the check that stopped it.
fn pack_not_run(rule_config: &RuleConfig, pack_id: &str) -> Option<PackNotRun> {
    if is_pack_enabled(rule_config, pack_id) {
        return None;
    }
    let allowlist_excluded =
        !rule_config.only_packs.is_empty() && !rule_config.only_packs.iter().any(|p| p == pack_id);
    Some(if allowlist_excluded {
        PackNotRun::NotAllowlisted
    } else {
        PackNotRun::Disabled
    })
}

/// One `AnalyzeOutput::packs_loaded` entry — a loaded DSL rule pack's id, its rule count as loaded
/// (before `disabled_rules` gating), its provenance (`PackSource::as_str`: `"dir"` | `"inline"`),
/// how many of this tree's analyzed files fall in scope of >=1 of its rules' `file_pattern`s,
/// which of its rules' own path gates admitted zero files, and — since 2026-08-26 — whether the pack
/// was evaluated at all.
///
/// # Loading is not running, and until 2026-08-26 the wire could not tell you which
/// Every other field here is a statement about the pack and the tree, computed identically whether or
/// not a single rule of the pack was ever evaluated. An outside auditor measured what that costs: with
/// `packs: { disabled: ["security", "browser"] }` a reply's findings fell 22 -> 2 while this array
/// still carried `security  rules=51  filesInScope=9` and 51 `ruleIds`, with the only contrary signal
/// on a DIFFERENT top-level key (`RuleOverridesApplied::disabled`). Joined the obvious way, that reads
/// as "the security pack scanned 9 files and found nothing" — "not analyzed" served as "analyzed and
/// safe". [`Self::did_not_run`] is the field that answers it, and the reason
/// [`Self::files_in_scope`]'s wire key is now conditional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackLoaded {
    pub id: String,
    pub rules: usize,
    pub source: String,
    /// `Some` when this pack LOADED but was never evaluated — see [`PackNotRun`] for the two levers.
    /// `None` is the ordinary case: the pack ran.
    ///
    /// This is the field that qualifies every other one. When it is `Some`, `files_in_scope` and
    /// `zero_admission_rules` are still computed (the census is enablement-blind by construction, and
    /// deliberately so — it is a property of patterns and paths, not of a run), but they describe a
    /// scan that did not happen; the JSON view republishes `files_in_scope` under a counterfactual
    /// name and drops the admission list entirely, so that no wire key ever claims a scan that never
    /// occurred. In-process consumers reading this struct have both facts side by side and need no
    /// such split.
    pub did_not_run: Option<PackNotRun>,
    /// The per-pack applicability signal (D16 follow-up): analyzed files matching at least one of this
    /// pack's rule `file_pattern`s (`analyze::diagnostics::compute_dsl_scope`'s census — exact per-file
    /// counts, shared with the tree-wide zero-applicability warning). `0` on a loaded pack is the
    /// per-pack "never applicable here" disclosure: `typescript: 12 rules` on a pure-Go tree reads
    /// `filesInScope: 0`, so zero findings from that pack means "out of scope", not "clean".
    pub files_in_scope: usize,
    /// The RULE-granularity half of the same census: sorted ids of this pack's rules whose own path
    /// gates (`file_pattern` AND `file_exclude_pattern` — the definition's owner is
    /// `analyze::diagnostics::pack_scope::rule_admission`'s module doc) admit ZERO analyzed files. A
    /// rule listed here could not have read a single byte of this tree, so its zero findings are
    /// scope, never a clean bill — the distinction `files_in_scope` cannot make one level down (a
    /// pack with 100 in-scope files can still carry a rule whose own gates match nothing here).
    /// Derived from the walked rel list, never from execution, so it is byte-identical on warm
    /// (cache-replayed) and cold runs. In ENVELOPE mode the census additionally lists every rule
    /// whose matcher kind that mode never evaluates (only `SymbolScan`/`IoScan` run there — see
    /// `analyze::diagnostics::compute_dsl_scope_filtered`): such a rule read nothing however many
    /// files its path gates match, so its green is vacuous too. Empty when every rule admits >=1
    /// file, and DELIBERATELY empty
    /// for a pack whose `files_in_scope` is 0 (admission is a subset of pattern candidacy, so "all of
    /// them" is already said by the pack-level zero) — which also covers the empty tree. Same
    /// loaded-not-gated convention as the rest of this struct: a disabled rule still counts by its
    /// gates. On the wire this is `zeroAdmissionRules`, serialized only when non-empty (the
    /// `testPaths` additive-disclosure precedent).
    pub zero_admission_rules: Vec<String>,
    /// Every rule id this pack loaded with, SORTED — the LIST behind the `rules` COUNT above, and the
    /// reason both can be published without either going stale: they are two projections of one `Vec`
    /// taken in one expression (see [`PackLoaded::from_config`]).
    ///
    /// Sorted rather than left in the pack's own declaration order, which is what it carried for one
    /// review cycle. Two id lists sit inside ONE `packsLoaded` entry — this one and
    /// [`Self::zero_admission_rules`], which has always been sorted — and the second is a SUBSET of
    /// the first, so a consumer wanting "which of this pack's rules did admit files" is doing a set
    /// difference. Under two different orders it cannot do it as one, and nothing on the wire told it
    /// the orders differ; declaration order bought nothing against that, since every reader of this
    /// field (`zzop_summary`'s unknown-rule-filter warning, the CLI's `--rule` refusal) asks it for
    /// membership. Both lists sorted is one fact fewer to publish, not one more.
    ///
    /// # Why a count was not enough
    /// The reply's only statement about what this run could report was a number, so every consumer
    /// asking "does this run carry a rule named X" had to guess from the pack prefix. The CLI's
    /// `--rule` refusal did exactly that and the asymmetry was measured on 2026-08-20:
    /// `--rule zzz/qqq` and `--rule totallyBogus` correctly exited 2, while
    /// `--rule security/no-such-rule` — a typo inside a pack that IS loaded — exited 0 with an empty
    /// stderr and `shown: 0`, which `zzop analyze --help` promises can never happen. The shared
    /// warning channel (`zzop_summary`'s unknown-rule-filter) had the same blind spot and says so in
    /// its own doc: "the reply's `packsLoaded` carries each pack's id, rule COUNT and source, never
    /// its rule ids".
    ///
    /// The alternative was to validate against the catalog compiled into the binary, which would
    /// falsely refuse a rule from a user pack loaded out of `<tree>/zzop/rules/` — the exact defect
    /// fixed for native ids earlier the same day. Publishing the run's own ids is the only answer that
    /// is right for both pack sources, because it is measured from the packs that actually loaded
    /// rather than from any list of what usually loads.
    ///
    /// Weighed before adding, and the two numbers are not interchangeable — the SERIALIZATION decides
    /// which one a reader pays. Measured 2026-08-20 over 23 trees, the bundled 118 ids across 11 packs
    /// cost a CONSTANT **2,752 bytes compact** and **3,979 bytes as the CLI's pretty-printed output**;
    /// the cost is a property of the pack set, not of the tree, so only the DENOMINATOR moves. Against
    /// a counts-only reply (`--limit 0`) that pretty payload is 14.0%–24.2% of it, and against the same
    /// tree's default reply with its findings list it is 6.9%–9.0%. Recount:
    /// `zzop analyze <tree> > r.json` then
    /// `node -e 'const v=require("./r.json");for(const p of v.packsLoaded)delete p.ruleIds;
    /// console.log(require("fs").statSync("r.json").size - JSON.stringify(v,null,2).length)'`.
    /// Paid unconditionally rather than only when `--rule` is passed, because the field is a statement
    /// about the RUN (any consumer can now read an empty findings list against what could have
    /// produced one) and a field present only on some invocations is a field consumers cannot rely on.
    /// Always serialized, empty pack included — an absent list would be indistinguishable from a pack
    /// whose ids this build declines to name, and a validator cannot tell "no such rule" from "no data"
    /// without that distinction.
    pub rule_ids: Vec<String>,
}

impl PackLoaded {
    /// Builds `AnalyzeOutput::packs_loaded` from `config.packs` + `config.pack_sources`, sorted by pack
    /// id (deterministic regardless of load order). A pack id with no `pack_sources` entry reports
    /// `"inline"` — see `EngineConfig::pack_sources`. `scope` is the ONE `compute_dsl_scope` census the
    /// caller already computed over the same `config.packs`: its per-pack vectors are parallel to
    /// `config.packs` ORDER (the pairing happens before the id sort), one entry per pack; a missing
    /// entry (never happens from the two real call sites) degrades to `0`/empty. Shared by
    /// `analyze::assemble` and `envelope::analyze_envelope`, so both entry points confirm the identical
    /// pack set.
    pub(crate) fn from_config(
        config: &EngineConfig,
        scope: &crate::analyze::DslScope,
    ) -> Vec<PackLoaded> {
        let mut loaded: Vec<PackLoaded> = config
            .packs
            .iter()
            .enumerate()
            .map(|(i, pack)| PackLoaded {
                id: pack.id.clone(),
                // The count and the list below are the same `Vec` read twice, three lines apart, so
                // "118 rules" and a 117-entry list is not a state this struct can be built into.
                rules: pack.rules.len(),
                // Sorted to match `zero_admission_rules`, its own subset in this same entry — see
                // `PackLoaded::rule_ids`. Sorting after the map keeps the count/list pairing above
                // intact (a sort moves elements, never drops one).
                rule_ids: {
                    let mut ids: Vec<String> = pack.rules.iter().map(|r| r.id.clone()).collect();
                    ids.sort_unstable();
                    ids
                },
                source: config
                    .pack_sources
                    .get(&pack.id)
                    .copied()
                    .unwrap_or(PackSource::Inline)
                    .as_str()
                    .to_string(),
                // Reads the SAME gate the pipeline reads, so the row cannot claim a pack ran that
                // did not (or the reverse) — see `pack_not_run`.
                did_not_run: pack_not_run(&config.rule_config, &pack.id),
                files_in_scope: scope.files_in_scope_by_pack.get(i).copied().unwrap_or(0),
                zero_admission_rules: scope
                    .zero_admission_rules_by_pack
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect();
        loaded.sort_by(|a, b| a.id.cmp(&b.id));
        loaded
    }
}
