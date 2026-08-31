//! The reply's compact `architecture` object — the ONLY channel by which a shipped CLI/MCP surface
//! publishes any part of the facade's `health`/`recommendations`/`critical` computation.
//!
//! Split out of `shape.rs` on 2026-08-20 for the repo's per-file line cap. The seam is the one the
//! two files' own names draw: `shape` decides WHICH fields a reply carries, this decides what the
//! architecture object SAYS — including the two legends (`painMeaning`, `criticalTopMeaning`,
//! `topRecommendationMeaning`) that exist because three of the words in it are read as words from a
//! different part of the same reply.

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
    let top_recommendation = output_view["recommendations"]
        .as_array()
        .and_then(|recs| recs.first())
        .map(|rec| {
            let top_item = rec["items"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(|item| item["path"].as_str());
            serde_json::json!({ "id": rec["id"], "severity": rec["severity"], "topItem": top_item })
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
    architecture.insert(
        "painMeaning".to_string(),
        serde_json::json!(
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
             the direct zzop-facade output."
        ),
    );
    architecture.insert("topRecommendation".to_string(), top_recommendation.into());
    // The VOCABULARY of the field above, and it needed one because the field reuses a word that already
    // means something else in this very reply. `severity` here is the ROI ranker's own priority band
    // (`zzop_metrics::recommendations`), which happens to be spelled with the same three tokens as a
    // finding severity and is computed from something else entirely — import cycles, fan-out, churn.
    // Measured 2026-08-20 on dotnet/eShop: `findings.bySeverity` was `{"info":8,"warning":16}` — zero
    // criticals anywhere in the run — beside `topRecommendation: {"id":"circular","severity":
    // "critical","topItem":"tests/Ordering.UnitTests/Domain/OrderAggregateTest.cs"}`, a file `zzop file`
    // reports `total: 0` findings on, while the run's ONE `circular` finding was a warning on a
    // different file. Two independent auditors read that as the reply contradicting itself. The
    // ranking is right; the word is load-bearing and was undefined. Renaming the field would be a
    // major bump under VERSIONING.md (CLI JSON field names and types are the compatibility surface),
    // so the token stays and the legend ships beside it — the same device `painMeaning` and
    // `criticalTopMeaning` already use, for the same failure.
    architecture.insert(
        "topRecommendationMeaning".to_string(),
        serde_json::json!(
            "`topRecommendation.severity` is a RECOMMENDATION PRIORITY BAND, not a finding severity, \
             and the two are computed from different things: this band comes from the ROI ranker's \
             structural rules (import cycles, fat fan-out, churn-per-LOC, hidden coupling), which \
             read no rule findings. So a `critical` here is routine on a tree whose \
             `findings.bySeverity` holds no critical at all, and `topItem` names a FILE that may have \
             zero findings of its own — read `findings.bySeverity` for the finding census and \
             `findings.shown` for what fired where; neither is summarized by this field. The one \
             place the two vocabularies touch: a recommendation whose file also carries a CRITICAL \
             rule finding is escalated into the `urgent-bug-risk` group, so `id: \"urgent-bug-risk\"` \
             is the only band value that implies a finding exists. `id` is one of a closed set the \
             `rule-catalog` contract document defines. Population caveat: same as `criticalTop`'s — \
             test files are not excluded, so a test module can be the `topItem`."
        ),
    );
    architecture.insert("criticalTop".to_string(), critical_top.into());
    // The POPULATION rides with the list, the same contract `painMeaning`/`verdictMeaning`/
    // `bucketMeaning` already carry: a ranking whose subject set is unstated gets read as a ranking
    // over the code that ships. It is not. `crates/metrics` never calls `zzop_core::is_test_file`
    // (the io and join lanes do, and disclose it), so test modules are scored exactly like
    // production ones — measured 2026-08-18 on this repo, where all three `criticalTop` slots were
    // test modules. Stated rather than silently fixed, because narrowing the population moves SIX
    // scores at once (`godFile`, `cohesion`, `busFactor`, `mainSequence`, `fileSizeCompliance`,
    // `health.pain`) and there is no baseline to tell a repair from a regression until this sentence
    // exists to be the baseline.
    architecture.insert(
        "criticalTopMeaning".to_string(),
        serde_json::json!(
            "Up to 3 paths from the SIZE-WEIGHTED critical list (blast_radius * ln(loc+2)). The \
             population is EVERY analyzed file: test files are NOT excluded, so a test module can \
             and does appear here even though nothing depends on it at runtime. Read a slot as \
             \"this file has the widest structural reach in the tree as analyzed\", then check \
             whether it is test code before treating it as a refactor target. The same caveat \
             applies to `topRecommendation`, which ranks over the same population."
        ),
    );
    Some(serde_json::Value::Object(architecture))
}
