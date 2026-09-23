//! The reply's compact `architecture` object — the ONLY channel by which a shipped CLI/MCP surface
//! publishes any part of the facade's `health`/`recommendations`/`critical` computation.
//!
//! Split out of `shape.rs` on 2026-08-20 for the repo's per-file line cap. The seam is the one the
//! two files' own names draw: `shape` decides WHICH fields a reply carries, this decides what the
//! architecture object SAYS — including the legends (`painMeaning`, `criticalTopMeaning`,
//! `topRecommendationMeaning`) that exist because words in it are read as words from a different part
//! of the same reply. Recount them: `grep -c 'Meaning\".to_string()' crates/summary/src/analyze/architecture.rs`.
//! This line used to say "the two legends" and then name three — one commit added the third and the
//! count stayed, which is why the number is a command now rather than a word.
//!
//! Two of the three are FOLDED (2026-09-14, ledger V237): their text lives in
//! `crate::output::architecture_legends` and ships once from the `reply-legends` contract document.
//! `painMeaning` is not, and `output::architecture_legends`' own doc owns the measurement that
//! separates them.

/// Builds the reply's compact `architecture` object from the facade output's `health`/
/// `recommendations`/`critical` fields — `None` (never `serde_json::Value::Null`) when `health`
/// itself is absent or JSON `null` (git signals did not run this tree), so the reply OMITS the key
/// entirely rather than growing a null `architecture` field on every git-less run. Deliberately
/// capped to ~10 lines of JSON: `pain` (the health scalar), the top-ROI `recommendations[0]`
/// (`{id, severity, topItem}`, null-safe when there are no recommendations or the top one has no
/// items), and up to 3 paths off the engine's own SIZE-WEIGHTED `critical` list (`blast_radius * ln(loc+2)`, NOT blast radius alone — re-sorting by `blastRadius` does not reproduce these three) — named
/// `criticalTop`, NOT "hotspot": the engine's `hotspotScore` is a DIFFERENT metric (churn
/// `changeCount x loc`, `nodes[].hotspotScore`), and reusing that word here would invite joining two
/// non-matching rankings. The full arrays never
/// ride this summary (see analyze_repo's own description: they are the direct `zzop-facade`
/// embedding lane's job).
pub(super) fn architecture_summary(output_view: &serde_json::Value) -> Option<serde_json::Value> {
    let health = output_view.get("health")?.as_object()?;
    let pain = health.get("pain")?.clone();
    // `pain`'s DENOMINATOR travels with it, always (2026-08-08). This summary is the only place the CLI
    // and MCP surfaces publish any score at all — the full `scores` object rides the direct
    // `zzop-facade` embedding lane and never reaches here — so before this, `pain` was a single folded
    // scalar with no way to tell how much of the structure it actually described. That is precisely the
    // shape `zzop_facade::query_coverage` forbids ("there is deliberately NO single score field, and one
    // must never be added"), and `pain` was sitting one crate away from the prohibition.
    //
    // `measuredWeight / totalWeight` is the fraction of the weighted metric table that had a population
    // to score over; `pain: null` with `measuredWeight: 0` is the honest "nothing was measurable" state,
    // which used to serialize as a confident `pain: 0`. Forwarded by name and `.get()`-defensive, the
    // same degradation contract every other field in this shaper uses.
    let measured_weight = health.get("measuredWeight").cloned();
    let total_weight = health.get("totalWeight").cloned();
    // `pain`'s AXIS SPLIT travels with it for the same reason its denominator does, and the omission was
    // worse: measured 2026-08-12, 80.6% of the weight table is structural OPINION and rule findings
    // contribute NOTHING — adding 20 `$queryRawUnsafe` files to a tree moved `critical` 1 -> 21 and left
    // `pain` byte-identical. A reader was being handed one number that looks like a verdict on the code
    // and is mostly a verdict on the code's STYLE. Forwarded whole (`defect`/`opinion`/`history`, each on
    // `pain`'s own scale and summing to it) rather than as a second scalar, so no consumer has to know
    // which axes exist to read it.
    let axis_pain = health.get("axisPain").cloned();
    // `pain`'s POPULATION travels with it, for exactly the reason its denominator and its axis split
    // already do — and this one is the only qualifier that can make two `pain` values over the SAME
    // BYTES disagree. Measured on the fixed `corpus/frameworks/express` checkout:
    // `scores.excludeTestFilesFromFileMetrics` off -> 30.0, on -> 11.5, while `findings.total` stayed
    // 58 and `byRule` stayed byte-identical (re-measured 2026-09-23; it read 56 until then — the
    // unchanged-ness is the claim and the absolute count is not, which is why the recount command
    // lives once, with the field, in `crates/facade/src/request/scores.rs`). Reproduce: add
    // `"scores": {"excludeTestFilesFromFileMetrics": true}` to a copy of that tree's
    // `zzop.config.jsonc` beside the tree, and diff `zzop analyze --config <copy> --limit 0`.
    // Nothing else on the wire says which of the two a number is.
    //
    // `.get()`-defensive and defaulting to the WIDE reading, the same degradation contract as every
    // other field here: an output written before `testFilesExcluded` existed was produced by a build
    // that could not narrow anything, so `false` is its true value rather than a guess.
    let test_files_excluded = health
        .get("testFilesExcluded")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let top_recommendation = output_view["recommendations"]
        .as_array()
        .and_then(|recs| recs.first())
        .map(|rec| {
            let top_item = rec["items"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(|item| item["path"].as_str());
            // `idMeaning` rides WITH the id, the shape `verdict`/`verdictMeaning` already uses
            // (review ledger V214). Before this the reply called `id` "one of a closed set", which
            // invited a consumer to hard-code the seven spellings — a promise `VERSIONING.md` does not
            // make, since field VALUES sit outside the freeze and the set may gain a member in a MINOR
            // release. Shipping the meaning removes the need for the promise instead of enlarging it.
            //
            // FORWARDED, not resolved here: the sentence is minted by `zzop_metrics::RecId::meaning`
            // beside the `serde` spelling that produces the id, so the two cannot drift. This crate is
            // forbidden a shipped dependency on that one (`Cargo.toml` states the layering), which is
            // exactly why the producer puts it on the wire rather than each reader looking it up.
            serde_json::json!({
                "id": rec["id"],
                "idMeaning": rec["idMeaning"],
                "severity": rec["severity"],
                "topItem": top_item,
            })
        });
    let critical_top: Vec<&str> = output_view["critical"]
        .as_array()
        .map(|files| {
            files
                .iter()
                .take(3)
                .filter_map(|f| f["path"].as_str())
                .collect()
        })
        .unwrap_or_default();
    let mut architecture = serde_json::Map::new();
    architecture.insert("pain".to_string(), pain);
    if let Some(measured_weight) = measured_weight {
        architecture.insert("painMeasuredWeight".to_string(), measured_weight);
    }
    if let Some(total_weight) = total_weight {
        architecture.insert("painTotalWeight".to_string(), total_weight);
    }
    if let Some(axis_pain) = axis_pain {
        architecture.insert("painByAxis".to_string(), axis_pain);
    }
    // The POPULATION clause below is the one part of this legend computed per RUN. It leads with a
    // fixed `POPULATION:` token so a reader diffing two replies, or grepping one, finds it without
    // reading the paragraph — and so that the two spellings differ at the first word rather than
    // somewhere inside a 1.2 KB block of prose.
    let population_clause = if test_files_excluded {
        "POPULATION: NARROWED — this run set `scores.excludeTestFilesFromFileMetrics`, so test files \
         left the denominator of the ten FILE-keyed metrics behind this number. The four keyed on a \
         DIRECTORY rollup (`sdp`, `mainSequence`, `modularity`, `cohesion`) still scored the whole \
         tree, and `criticalTop`/`topRecommendation` are not scores and were not narrowed at all — so \
         a test module can still be named beside a number that excluded them. NOT comparable with a \
         `pain` from a run without that key."
    } else {
        "POPULATION: every analyzed file, test files INCLUDED — so part of this number judges code \
         that never ships. `scores.excludeTestFilesFromFileMetrics` narrows it; this run did not set \
         it. Two `pain` values are comparable only when this sentence reads the same in both."
    };
    architecture.insert(
        "painMeaning".to_string(),
        serde_json::json!(format!(
            "Composite structural debt over the metrics that HAD something to measure, renormalized \
             onto the full weight table (0 = clean, higher = worse, ~186 = every weighted metric at \
             its worst). THIS NUMBER CONTAINS NO RULE FINDINGS: it is computed from structural scores \
             alone, so a tree full of SQL injection scores exactly what the same tree scores with none \
             — read `findings` and `findings.bySeverity` for defects, never this. Most of it is not \
             even a defect claim about structure: `painByAxis` splits it into `defect` (import cycles \
             only), `opinion` (barrel discipline, FSD layering, Robert Martin's SDP/Main Sequence, \
             Newman modularity, LOC ceilings — a project that deliberately does the opposite is not \
             wrong, it scores low) and `history` (rename churn, bus factor), each on this same scale \
             and summing to `pain`. `painMeasuredWeight` / `painTotalWeight` is how much of the table \
             was actually measurable on this tree: read a low ratio as \"this number describes a \
             minority of the structure\", not as a better score. `pain: null` means NO metric had a \
             population — absence of data, never 0. Renormalizing is what stops an unmeasurable axis (a \
             metric defined over a convention this tree never adopted) from making the repo look \
             healthier by silently scoring 100. The per-metric populations behind it ride `scores.*` in \
             the direct zzop-facade output. {population_clause}"
        )),
    );
    architecture.insert("topRecommendation".to_string(), top_recommendation.into());
    // The VOCABULARY of the field above, FOLDED since 2026-09-14 — the text and the four measured
    // misreads that shaped it live with the constant, in
    // `crate::output::architecture_legends::TOP_RECOMMENDATION_MEANING`.
    architecture.insert(
        "topRecommendationMeaning".to_string(),
        serde_json::json!(crate::output::legends::folded_string(
            "architecture.topRecommendationMeaning",
        )),
    );
    architecture.insert("criticalTop".to_string(), critical_top.into());
    // The POPULATION rides with the list, and this legend is FOLDED — its text plus the full
    // record of why each clause is load-bearing live with the constant, in
    // `crate::output::architecture_legends::CRITICAL_TOP_MEANING`. Not restated here: a rationale
    // left beside an insert that no longer holds the sentence is the copy that goes stale first.
    architecture.insert(
        "criticalTopMeaning".to_string(),
        serde_json::json!(crate::output::legends::folded_string(
            "architecture.criticalTopMeaning",
        )),
    );
    Some(serde_json::Value::Object(architecture))
}

#[cfg(test)]
mod tests {
    use super::architecture_summary;

    /// A minimal facade output view carrying just what `architecture_summary` reads, with
    /// `health.testFilesExcluded` set to `flag` (or the key omitted entirely when `flag` is `None`).
    fn view(flag: Option<bool>) -> serde_json::Value {
        let mut health = serde_json::json!({
            "pain": 42.0,
            "measuredWeight": 15.1,
            "totalWeight": 18.6,
            "axisPain": [],
        });
        if let Some(flag) = flag {
            health["testFilesExcluded"] = serde_json::json!(flag);
        }
        serde_json::json!({
            "health": health,
            "recommendations": [],
            "critical": [{ "path": "test/support/utils.js" }],
        })
    }

    fn pain_meaning(flag: Option<bool>) -> String {
        architecture_summary(&view(flag)).expect("architecture object")["painMeaning"]
            .as_str()
            .expect("painMeaning is a string")
            .to_string()
    }

    /// `pain` is re-based by `scores.excludeTestFilesFromFileMetrics`, so the legend that ships beside
    /// it must say WHICH population this run used — otherwise two replies over the same bytes carry
    /// two different numbers and nothing distinguishes them.
    ///
    /// Asserted as a DIFFERENCE between the two states rather than as a substring of one: a pin on
    /// the narrowed sentence alone would still pass if both states printed it.
    #[test]
    fn pain_meaning_states_the_live_population_and_the_two_states_differ() {
        let wide = pain_meaning(Some(false));
        let narrowed = pain_meaning(Some(true));
        assert_ne!(
            wide, narrowed,
            "a reader comparing two runs cannot tell a narrowed population from a wide one"
        );
        assert!(wide.contains("POPULATION: every analyzed file"), "{wide}");
        assert!(narrowed.contains("POPULATION: NARROWED"), "{narrowed}");
        // Each state names the key, so a reader who wants the other one knows what to set or unset.
        for text in [&wide, &narrowed] {
            assert!(
                text.contains("scores.excludeTestFilesFromFileMetrics"),
                "the clause must name the key that moves it: {text}"
            );
        }
        // The narrowed sentence must not be readable as "no test file influenced anything here":
        // four of the fourteen contributors are keyed on a directory rollup and still scored wide.
        assert!(narrowed.contains("cohesion"), "{narrowed}");
    }

    /// Output written by a build from before the flag existed has no `testFilesExcluded` key, and such
    /// a build could not narrow anything — so the absent key must read as the WIDE population, never
    /// as an unknown state or a panic.
    #[test]
    fn an_absent_population_flag_reads_as_the_wide_population() {
        assert_eq!(pain_meaning(None), pain_meaning(Some(false)));
    }

    /// `criticalTop` is computed by `compute_criticality`, which takes no scores config and therefore
    /// cannot be narrowed by that key — so its legend is the same sentence in both states. This pins
    /// the property the legend now ASSERTS out loud, so wiring the key into criticality later without
    /// updating the sentence turns this red instead of shipping a lie.
    #[test]
    fn the_critical_top_legend_is_identical_whether_or_not_the_scores_were_narrowed() {
        let wide = architecture_summary(&view(Some(false))).expect("architecture");
        let narrowed = architecture_summary(&view(Some(true))).expect("architecture");
        assert_eq!(wide["criticalTopMeaning"], narrowed["criticalTopMeaning"]);
        assert_eq!(wide["criticalTop"], narrowed["criticalTop"]);
        assert!(wide["criticalTopMeaning"]
            .as_str()
            .expect("string")
            .contains("This holds in EVERY run"));
    }
}
