//! `cross-layer/unresolved-consume-ratio` (info) — a source tree whose `http` consumes are majority
//! unresolved (key extraction failed — typically a generated SDK client, a `fetch` wrapper, or a constant
//! assembled across files). This is a deliberate self-report: every other `cross-layer/*` rule reasons only
//! over consumes it could resolve, so a mostly-unresolved tree would otherwise look quiet while actually
//! being invisible to the join. This rule surfaces that blind spot instead of staying silent about it.
//!
//! `http_consume_totals` is threaded in from the engine rather than recomputed here, since `packages/core`
//! stays rule-vocabulary-free.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::TaggedConsume;
use zzop_core::{Finding, Severity};

// Shared with `sdk_import_no_visible_consume`, which fires only BELOW this floor — the two rules
// partition the blind-spot space and never co-fire on one tree (see mod.rs).
use super::MIN_TOTAL_CONSUMES;

/// Majority threshold, integer math only (no floats — output must be byte-stable across platforms):
/// `unresolved * 2 >= total` is equivalent to `unresolved / total >= 0.5` without any floating-point division.
fn is_majority_unresolved(unresolved: usize, total: usize) -> bool {
    unresolved * 2 >= total
}

pub fn unresolved_consume_ratio_findings(
    unresolved_consumes: &[TaggedConsume],
    http_consume_totals: &[(String, usize)],
) -> Vec<Finding> {
    let mut by_source: BTreeMap<&str, Vec<&TaggedConsume>> = BTreeMap::new();
    for c in unresolved_consumes {
        if c.consume.kind == "http" {
            by_source.entry(c.source.as_str()).or_default().push(c);
        }
    }

    let mut totals: Vec<&(String, usize)> = http_consume_totals.iter().collect();
    totals.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = Vec::new();
    for (source, total) in totals {
        let total = *total;
        if total < MIN_TOTAL_CONSUMES {
            continue;
        }
        let Some(source_unresolved) = by_source.get(source.as_str()) else {
            continue;
        };
        let unresolved_count = source_unresolved.len();
        if !is_majority_unresolved(unresolved_count, total) {
            continue;
        }

        let mut sorted: Vec<&TaggedConsume> = source_unresolved.clone();
        sorted.sort_by(|a, b| {
            a.consume
                .file
                .cmp(&b.consume.file)
                .then(a.consume.line.cmp(&b.consume.line))
        });
        let anchor = sorted[0];

        let mut raw_samples: BTreeSet<&str> = BTreeSet::new();
        for c in &sorted {
            if let Some(raw) = c.consume.raw.as_deref() {
                raw_samples.insert(raw);
            }
        }
        let sample_raw: Vec<&str> = raw_samples.into_iter().take(3).collect();

        let ratio_percent = unresolved_count * 100 / total;

        let message = format!(
            "source `{source}` has {unresolved_count} of {total} `http` consumes ({ratio_percent}%) that could \
             not be statically resolved to a concrete path — typical causes are a generated SDK client, \
             wrapper functions around `fetch`, a constant assembled across files, or a URL built from \
             runtime configuration (e.g. health-check pings to an env-configured service). The cross-layer join is \
             mostly BLIND for this source: join-based findings (e.g. `cross-layer/unconsumed-endpoint`) are \
             correspondingly weaker here, since they can only reason about the consumes this analysis actually \
             resolved. Prefer literal paths at call sites where practical, or feed this source through a \
             Normalized AST adapter that can resolve the indirection. Disable via rule config `disabled_rules: \
             [\"cross-layer/unresolved-consume-ratio\"]` if this source is intentionally SDK-driven and the \
             resulting blindness is accepted.",
        );

        out.push(Finding {
            rule_id: "cross-layer/unresolved-consume-ratio".to_string(),
            severity: Severity::Info,
            file: anchor.consume.file.clone(),
            line: anchor.consume.line,
            message,
            data: Some(serde_json::json!({
                "source": source,
                "unresolvedCount": unresolved_count,
                "totalHttpConsumes": total,
                "ratioPercent": ratio_percent,
                "sampleRaw": sample_raw,
            })),
        });
    }

    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::io::IoConsume;

    fn unresolved(source: &str, file: &str, line: u32, raw: Option<&str>) -> TaggedConsume {
        TaggedConsume {
            source: source.to_string(),
            consume: IoConsume {
                kind: "http".to_string(),
                key: None,
                file: file.to_string(),
                line,
                raw: raw.map(str::to_string),
                method: None,
            },
        }
    }

    #[test]
    fn majority_unresolved_above_minimum_fires() {
        let unresolved_list = vec![
            unresolved("fe", "a.ts", 5, Some("client.get(url)")),
            unresolved("fe", "b.ts", 1, Some("sdk.fetch(path)")),
            unresolved("fe", "c.ts", 3, None),
        ];
        let totals = vec![("fe".to_string(), 5usize)]; // 3 of 5 unresolved = 60%
        let out = unresolved_consume_ratio_findings(&unresolved_list, &totals);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/unresolved-consume-ratio");
        assert_eq!(out[0].severity, Severity::Info);
        // anchored at first unresolved consume sorted by (file, line)
        assert_eq!(out[0].file, "a.ts");
        assert_eq!(out[0].line, 5);
        assert!(out[0].message.contains("disabled_rules"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["source"], "fe");
        assert_eq!(data["unresolvedCount"], 3);
        assert_eq!(data["totalHttpConsumes"], 5);
        assert_eq!(data["ratioPercent"], 60);
    }

    #[test]
    fn below_min_total_consumes_does_not_fire_even_if_100_percent_unresolved() {
        let unresolved_list = vec![unresolved("fe", "a.ts", 1, None)];
        let totals = vec![("fe".to_string(), 1usize)];
        assert!(unresolved_consume_ratio_findings(&unresolved_list, &totals).is_empty());
    }

    #[test]
    fn below_majority_does_not_fire() {
        let unresolved_list = vec![
            unresolved("fe", "a.ts", 1, None),
            unresolved("fe", "b.ts", 2, None),
        ];
        let totals = vec![("fe".to_string(), 5usize)]; // 2 of 5 = 40%, below majority
        assert!(unresolved_consume_ratio_findings(&unresolved_list, &totals).is_empty());
    }

    #[test]
    fn sample_raw_is_deduped_and_capped_at_three() {
        let unresolved_list = vec![
            unresolved("fe", "a.ts", 1, Some("dup")),
            unresolved("fe", "b.ts", 2, Some("dup")),
            unresolved("fe", "c.ts", 3, Some("one")),
            unresolved("fe", "d.ts", 4, Some("two")),
            unresolved("fe", "e.ts", 5, Some("three")),
        ];
        let totals = vec![("fe".to_string(), 5usize)];
        let out = unresolved_consume_ratio_findings(&unresolved_list, &totals);
        assert_eq!(out.len(), 1);
        let sample = out[0].data.as_ref().unwrap()["sampleRaw"]
            .as_array()
            .unwrap();
        assert_eq!(sample.len(), 3);
        // sorted, deduped: "dup", "one", "three" (alphabetical, "two" excluded by cap)
        assert_eq!(sample[0], "dup");
        assert_eq!(sample[1], "one");
        assert_eq!(sample[2], "three");
    }

    #[test]
    fn multiple_sources_are_each_evaluated_and_output_is_deterministic() {
        let unresolved_list = vec![
            unresolved("be", "z.ts", 1, None),
            unresolved("be", "y.ts", 2, None),
            unresolved("be", "x.ts", 3, None),
            unresolved("fe", "a.ts", 1, None),
            unresolved("fe", "b.ts", 2, None),
            unresolved("fe", "c.ts", 3, None),
        ];
        let totals = vec![("fe".to_string(), 5usize), ("be".to_string(), 5usize)];
        let out = unresolved_consume_ratio_findings(&unresolved_list, &totals);
        assert_eq!(out.len(), 2);
        // final sort is by (file, line) regardless of source iteration order
        assert_eq!(out[0].file, "a.ts");
        assert_eq!(out[1].file, "x.ts");

        let totals_reversed = vec![("be".to_string(), 5usize), ("fe".to_string(), 5usize)];
        let out2 = unresolved_consume_ratio_findings(&unresolved_list, &totals_reversed);
        assert_eq!(out.len(), out2.len());
        for (a, b) in out.iter().zip(out2.iter()) {
            assert_eq!(a.file, b.file);
            assert_eq!(a.line, b.line);
            assert_eq!(a.data, b.data);
        }
    }
}
