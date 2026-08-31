//! Prisma schema structural rules — source-agnostic checks over the schema IR (`zzop_core::schema`).
//! IR types (`SchemaModel` etc.) live in `zzop-core`; the rule bodies that operate on them live here.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use zzop_core::{SchemaModel, Severity};

/// Version token for what this crate's rules EMIT, folded into the ruleset cache fingerprint so a stale
/// cache doesn't keep serving old `schema/*` findings. Restamp with the current `CARGO_PKG_VERSION`
/// (2026-07-22 version reform: cache-bust tokens are package-version stamps).
///
/// TWO LANES, ONE TOKEN — decide a bump on the lane, not on the crate:
/// - CACHED: `structural.rs`'s rules run in the fused per-file pass (`engine`'s `pipeline::schema_findings`)
///   and their findings are WRITTEN to and served verbatim from the per-file findings cache entry. Any change
///   that alters what they emit — a rule body, a threshold, or the shared MESSAGE text in `message.rs` —
///   needs a bump here, or a warm cache keeps serving the old finding for byte-identical source.
/// - NOT CACHED: `usage.rs`'s rules run from `analyze::assemble`'s whole-tree stage
///   (`pipeline::schema_usage_findings`) and are recomputed every run, so a usage-only change needs no bump.
///
/// The trap was `message.rs`: it is shared by both lanes, so "usage isn't cached, no bump" does NOT
/// generalize to it. The 0.22.0 -> 0.24.0 bump was exactly that case — `family_disable_hint` became
/// `issue_disable_hint`, changing the message text of every cached STRUCTURAL finding, and nothing but
/// an author's memory connected the two.
///
/// **Since 2026-07-29 this const no longer has to be right.** `crates/engine/build.rs` hashes this whole
/// crate's dependency closure into the cache key alongside this string, so the `message.rs` case — and
/// every case like it — invalidates on its own. What survives here is the human-readable half: a version
/// a person can read in a cache path. Bump it when you want to SAY something changed; correctness no
/// longer depends on you noticing. The lane split above is still worth reading — it explains which
/// changes have a cache consequence at all — but it is now an explanation, not an obligation.
pub const STRUCTURAL_RULES_VERSION: &str = "0.33.0";

/// A structural schema issue (source-agnostic; from a single model/field). `camelCase` here matches
/// every other output-facing type, since this struct serializes verbatim into `Finding.data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaIssue {
    pub rule: String,
    pub severity: Severity,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Rule-specific auxiliary parameters (god-model fieldCount, missing-timestamps missing[], ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// Not `#[serde(rename_all = "camelCase")]`: `analyze_schema` is only used by this crate's own tests and
/// never crosses the JSON wire boundary, so `model_risk` stays as declared. Add the attribute if that changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchemaAnalysis {
    pub models: Vec<SchemaModel>,
    pub issues: Vec<SchemaIssue>,
    /// modelName -> risk score (sum of severity points).
    pub model_risk: HashMap<String, i64>,
}

pub(crate) fn severity_points(s: Severity) -> i64 {
    match s {
        Severity::Critical => 5,
        Severity::Warning => 2,
        Severity::Info => 1,
    }
}

/// Field count at or above which `god-model` reports. **A CONVENTION, NOT A MEASUREMENT** — written
/// down here because this number decides the rule's entire output and, until 2026-08-29, nothing in
/// the tree said where it came from.
///
/// What IS measured (2026-08-29, the only Prisma schema in the 9-tree corpus — calcom/cal.com,
/// 100 models; two clean release builds of the same commit, one constant apart): 15 reports **27**
/// models, 14 reports **32**, 16 reports **23**. One field moves the count by 4-5, so the number
/// dominates the result rather than trimming its edges.
///
/// And it does not cut where the schema's own shape suggests. The field-count distribution runs
/// 3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,21,22,24,25,26 and then jumps to 35,36,56,71,95,106 —
/// exactly ONE gap in the whole range, at 26 -> 35, with six models above it (EventType 106, User 95,
/// Team 71, Booking 56, SelectedCalendar 36, MonthlyProration 35). At 15 the rule reports those six
/// plus 21 ordinary domain models of 15-26 fields: the line sits in the DENSE BODY of the
/// distribution, not at its outlier boundary. That is the honest reading of the message's own
/// "field count is a design opinion" — the opinion is where this constant sits.
///
/// **Not raised to the gap here on purpose.** Moving it to ~27 would delete 21 findings on the one
/// schema anyone has measured, and deleting findings is the direction that leaves no trace
/// (`.claude` rule-quality §24). That is a user's call with a second Prisma tree in hand, not a
/// comment's. n=1: the corpus contains no other Prisma schema, so the numbers above are one data
/// point, not a distribution — the next tree that has one should re-measure before anyone moves this.
///
/// POLICY VALUE, T2: also spelled by hand, in English prose, in `docs/rules/catalog.md` and
/// `site/rules.html` (a Markdown/HTML page cannot reference a Rust constant) — pinned by
/// `crate::message::tests::the_god_model_threshold_is_identical_in_the_constant_and_the_published_docs`,
/// which compares both pages against [`god_model_threshold_claim`] and [`GOD_MODEL_MEASUREMENT`].
/// The FINDING's own message is T1 rather than T2: it renders this constant and that sentence
/// directly, so nothing stands between them to drift. `scripts/policy-census.txt` deliberately
/// carries NO copy of the numbers above — that guard compares key and axis and never reads a tail,
/// so a copy parked there is unguardable by construction (it held a stale one until 2026-08-31).
pub(crate) const GOD_THRESHOLD: usize = 15;

/// The MARKUP-FREE threshold claim the published pages must carry, rendered from
/// [`GOD_THRESHOLD`] so the one part most likely to go stale cannot — the same arrangement
/// `crate::message::sightline::field_usage_sightline_claim` uses for its extension list.
pub(crate) fn god_model_threshold_claim() -> String {
    format!("at least {GOD_THRESHOLD} fields")
}

/// The MEASUREMENT behind [`GOD_THRESHOLD`], in one markup-free sentence with ONE owner. The
/// finding's message splices it and both published pages are pinned against it, because these four
/// numbers were hand-copied into three prose surfaces and no two of them could be compared.
///
/// It states the threshold it was TAKEN at rather than interpolating [`GOD_THRESHOLD`], and that is
/// the honest form: move the constant and 27/32/23 do not move with it — they become a reading about
/// a line the rule no longer draws. The pin's `GOD_THRESHOLD == 15` assertion is what turns that into
/// a red test instead of a quietly stale sentence, and re-measuring is the only way past it.
pub(crate) const GOD_MODEL_MEASUREMENT: &str = "on the corpus's only Prisma schema (cal.com, 100 \
     models), a threshold of 15 reports 27 of them, 14 reports 32 and 16 reports 23, so one field \
     moves the count by 4-5";

/// Field-name tokens denoting a whole monetary amount (matched as a case-insensitive substring).
pub const MONEY_TOKENS: &[&str] = &[
    "price",
    "amount",
    "cost",
    "total",
    "subtotal",
    "balance",
    "salary",
    "wage",
    "payment",
    "payout",
    "payable",
    "receivable",
    "refund",
    "rebate",
    "fee",
    "fare",
    "tariff",
    "surcharge",
    "deposit",
    "revenue",
    "income",
    "expense",
    "budget",
    "profit",
    "tax",
    "discount",
    "charge",
    "credit",
    "debit",
    "commission",
    "currency",
    "money",
    "cash",
    "invoice",
    "billing",
    "premium",
    "allowance",
    "bonus",
];

/// Analyze schema models -> issues + per-model risk. Structural-only path (usage rules require a code scan).
pub fn analyze_schema(models: Vec<SchemaModel>) -> SchemaAnalysis {
    let issues = apply_schema_rules(&models, MONEY_TOKENS);
    let mut model_risk: HashMap<String, i64> = models.iter().map(|m| (m.name.clone(), 0)).collect();
    for issue in &issues {
        *model_risk.entry(issue.model.clone()).or_insert(0) += severity_points(issue.severity);
    }
    SchemaAnalysis {
        models,
        issues,
        model_risk,
    }
}

/// `money_tokens` is the run's declared `vocabulary.moneyTokens` — what a project calls its money
/// columns is its own convention, so `float-money` judges against a declared list ([`MONEY_TOKENS`] is
/// only its default).
pub fn apply_schema_rules(models: &[SchemaModel], money_tokens: &[&str]) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    for model in models {
        rule_god_model(model, &mut issues);
        rule_missing_timestamps(model, &mut issues);
        rule_redundant_index(model, &mut issues);
        for field in &model.fields {
            rule_float_money(model, field, money_tokens, &mut issues);
            rule_stale_updated_at(model, field, &mut issues);
            rule_temporal_as_string(model, field, &mut issues);
            if !is_fk_candidate(field) {
                continue;
            }
            rule_fk_no_index(model, field, &mut issues);
            rule_nullable_fk(model, field, &mut issues);
            rule_implicit_fk(model, field, &mut issues);
        }
    }
    issues
}

mod rules;
#[cfg(test)]
mod tests;

use rules::{
    is_fk_candidate, rule_fk_no_index, rule_float_money, rule_god_model, rule_implicit_fk,
    rule_missing_timestamps, rule_nullable_fk, rule_redundant_index, rule_stale_updated_at,
    rule_temporal_as_string,
};
