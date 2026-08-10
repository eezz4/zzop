// ---------------------------------------------------------------------------------------------------
// Unknown-key warnings — the port of `collectConfigWarnings`'s scoped walk. Never rejects (the engine
// deliberately ignores unknown fields); this only makes a typo or cross-version drift visible.
// Vocabulary sourced from `crate::CONFIG_SURFACE_JSON`'s `configKeys` — the same vocabulary file the
// JS CLI and the engine's own reference-validation meta-test share, so this port can never disagree
// with either about what a valid config key is.
// ---------------------------------------------------------------------------------------------------

/// Keys the recognized surface used to carry and deliberately no longer does, each paired with the
/// sentence a reader needs INSTEAD of the generic "typo, or a different zzop version" guess below.
///
/// All three were the same defect: accepted by this front end, forwarded into no request, consumed by
/// no binary — so setting one produced no warning, no error, and no effect. (The JS CLI that once read
/// them was removed 2026-07-20; nothing replaced it.) Keeping them "recognized" bought silence for the
/// author who believed they had configured something, which is strictly more expensive than the
/// unknown-key warning they get now. Retiring them from `config-surface.json` is what makes that
/// warning fire at all; this table is what makes it say WHY.
///
/// Keyed by the FULL dotted spelling [`warn_unknown_keys`] composes (`{scope}{key}`) — today every
/// entry is top-level, because the `report` sub-scope walk went away with the key itself.
///
/// Message style note: no backticks anywhere in these strings. `crates/config/src` is inside the
/// reference-validation contract's CHECK B scan set, which reads every backtick-quoted token near the
/// word "config" and requires it to name a REAL knob — a retirement notice necessarily names knobs that
/// no longer exist, so it quotes them the way the surrounding warning already does, with `"`.
const RETIRED_KEYS: &[(&str, &str)] = &[
    (
        "failOn",
        "it was accepted as a severity threshold for a CI gate, but no zzop binary has ever exited \
         non-zero on findings, so setting it gated nothing. Delete it — the run is unchanged — and \
         gate a build by reading the severities out of the JSON output yourself.",
    ),
    (
        "format",
        "it was accepted as an output-format selector, but every zzop binary emits JSON and only \
         JSON, so setting it selected nothing. Delete it — the output is unchanged.",
    ),
    (
        "report",
        "it was accepted as a report-file destination (its dir/formats/enabled sub-keys included), \
         but no zzop binary writes report files at all — the CLI that did was removed 2026-07-20 and \
         nothing replaced it, so setting it wrote nothing anywhere. Delete it and read the JSON \
         output instead.",
    ),
];

pub(super) fn collect_config_warnings(config: &serde_json::Value) -> Vec<String> {
    let mut warnings = Vec::new();
    if !config.is_object() {
        return warnings;
    }

    let surface: serde_json::Value = serde_json::from_str(crate::CONFIG_SURFACE_JSON)
        .expect("embedded config-surface.json must be valid JSON");
    let config_keys = &surface["configKeys"];
    let known = |scope: &str| -> Vec<&str> {
        config_keys[scope]
            .as_array()
            .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default()
    };

    warn_unknown_keys(Some(config), &known("top"), "", &mut warnings);
    warn_unknown_keys(
        config.get("packs"),
        &known("packs"),
        "packs.",
        &mut warnings,
    );
    warn_unknown_keys(config.get("git"), &known("git"), "git.", &mut warnings);
    // `vocabulary` and its one nested scope. Wired 2026-08-07, when `vocabulary.fsd` was renamed to
    // `vocabulary.featureSlicedDesign` and it turned out the old spelling would have been accepted in
    // TOTAL SILENCE — this scope was the only declared one the walk never descended into, though
    // `config-surface.json` has listed its keys all along. That silence costs in the expensive
    // direction: an undeclared vocabulary key is NOT JUDGED. `FeatureSlicedDesignConfig::default()` does
    // hold the built-in layer names, but the facade never reaches it — `config/declared.rs` builds the
    // matcher by CLONING the request's own values, so a key that stopped arriving arrives EMPTY and the
    // axis goes SILENT rather than falling back. (This comment said "keeps scoring against zzop's
    // built-in layout" until 2026-08-09, which is the more comforting of the two readings and the wrong
    // one: a fallback would still score, and a reader who believed it would rank this warning as
    // cosmetic.) Measured when the corpus config's own `fsd` typo was found: every declared FSD list
    // arrived empty on all four probes, and nothing anywhere said so except this warning. The nested scope is walked separately because it has its own key list; stopping
    // one level up would still swallow `featureSlicedDesign.entrY`.
    let vocabulary = config.get("vocabulary");
    warn_unknown_keys(
        vocabulary,
        &known("vocabulary"),
        "vocabulary.",
        &mut warnings,
    );
    warn_unknown_keys(
        vocabulary.and_then(|v| v.get("featureSlicedDesign")),
        &known("featureSlicedDesign"),
        "vocabulary.featureSlicedDesign.",
        &mut warnings,
    );
    // No `report` scope walk: `report` itself is retired (see `RETIRED_KEYS`), so it is caught by the
    // top-level pass above with the retirement notice. Walking INTO it would bury that one honest
    // sentence under three "unknown config key report.dir" lines whose known-keys list is empty.

    if let Some(trees) = config.get("trees").and_then(serde_json::Value::as_array) {
        let known_tree = known("tree");
        let known_mount = known("mount");
        let known_route = known("route");
        for (i, tree) in trees.iter().enumerate() {
            warn_unknown_keys(
                Some(tree),
                &known_tree,
                &format!("trees[{i}]."),
                &mut warnings,
            );
            // Deployment topology moved under its own object on 2026-07-28, so the walk descends one
            // level further. `topology` itself is walked for unknown keys too — without that, a typo
            // like `topology.mountAt` would be silently dropped by a mapper that only looks up the
            // three names it knows.
            if let Some(topology) = tree.get("topology") {
                let known_topology = known("topology");
                warn_unknown_keys(
                    Some(topology),
                    &known_topology,
                    &format!("trees[{i}].topology."),
                    &mut warnings,
                );
                if let Some(mounts) = topology.get("mounts").and_then(serde_json::Value::as_array) {
                    for (j, entry) in mounts.iter().enumerate() {
                        if entry.is_object() {
                            warn_unknown_keys(
                                Some(entry),
                                &known_mount,
                                &format!("trees[{i}].topology.mounts[{j}]."),
                                &mut warnings,
                            );
                        }
                    }
                }
            }
            if let Some(routes) = tree.get("routes").and_then(serde_json::Value::as_array) {
                for (j, entry) in routes.iter().enumerate() {
                    if entry.is_object() {
                        warn_unknown_keys(
                            Some(entry),
                            &known_route,
                            &format!("trees[{i}].routes[{j}]."),
                            &mut warnings,
                        );
                    }
                }
            }
        }
    }

    if let Some(rules) = config.get("rules").and_then(serde_json::Value::as_object) {
        let known_rule_object = known("ruleObject");
        for (rule_id, entry) in rules {
            if entry.is_object() {
                warn_unknown_keys(
                    Some(entry),
                    &known_rule_object,
                    &format!("rules.{rule_id}."),
                    &mut warnings,
                );
            }
        }
    }

    warn_unmatchable_vocabulary_entries(config.get("vocabulary"), &mut warnings);

    warnings
}

/// A declared `vocabulary.*` entry that CANNOT MATCH ANYTHING, because the rule that reads that key
/// normalizes its input first and the entry does not survive the same normalization.
///
/// This is the one warning here that is not about a key being unrecognized — the key is real, the value
/// is a well-formed string, and the run accepts it. It is about a value that is inert. Declaring
/// `sensitiveResponseFieldExactNames: ["sessionToken"]` reads as switching a protection ON, and until
/// 2026-08-05 it silently switched nothing: the rule normalized the field name it read to
/// `sessiontoken`, compared it against the verbatim `sessionToken`, and never matched. No lane said so.
/// That is this repo's first-ranked failure mode — believing a check is on while it is off — and it is
/// why "just document the normalization" was rejected as the fix.
///
/// The transform per key comes from [`zzop_engine::NORMALIZED_VOCABULARY_KEYS`], and the transforms
/// themselves are the same `zzop_core::vocab_norm` functions the consuming rules call, so this warning
/// cannot drift away from the comparison it predicts.
///
/// **Not every key is checked, deliberately.** Most vocabulary keys are compared verbatim and any
/// spelling can match; and even among the checked ones a blanket fold would be wrong —
/// `secretParamNames` legitimately carries both `api_key` and `api-key`, so only CASE is corrected there.
/// The message names the spelling that would work rather than only reporting a problem, because the
/// author's next action is always the same edit.
///
/// Message style note: no backticks, same reason [`RETIRED_KEYS`] gives — this file is inside the
/// reference-validation contract's CHECK B scan set.
fn warn_unmatchable_vocabulary_entries(
    vocabulary: Option<&serde_json::Value>,
    warnings: &mut Vec<String>,
) {
    let Some(object) = vocabulary.and_then(serde_json::Value::as_object) else {
        return;
    };
    for (key, value) in object {
        let Some(normalize) = zzop_engine::normalizer_for(key) else {
            continue;
        };
        let Some(entries) = value.as_array() else {
            continue;
        };
        for entry in entries.iter().filter_map(serde_json::Value::as_str) {
            let normalized = normalize(entry);
            if normalized == entry {
                continue;
            }
            warnings.push(format!(
                "vocabulary.{key} entry \"{entry}\" can never match: the rule that reads this key \
                 normalizes the name it finds before comparing, so no input can ever equal this \
                 spelling. Declare it as \"{normalized}\" instead — the entry is accepted today and \
                 does nothing."
            ));
        }
    }
}

/// One scope of `collectConfigWarnings`'s walk: for every key in `obj` (a no-op if `obj` is absent or
/// not itself a JSON object) not present in `known`, push an "unknown config key" warning naming the
/// full dotted key, the scope, and the known-keys list for that scope — verbatim text match with the
/// JS source, including its `${scope}${key}` composition and the `scope.replace(/\.$/, '')` trim
/// (`scope` here always carries at most one trailing `.`, so `trim_end_matches('.')` is equivalent).
///
/// One deviation from that source, added 2026-07-26: a key listed in [`RETIRED_KEYS`] keeps the same
/// `unknown config key "..." (ignored)` opening — so the warning stays greppable and lands in the same
/// channel — but replaces the "typo, or a different version" guess with why it was REMOVED and what to
/// do instead. The known-keys list is dropped for those: an author who wrote a key that used to be
/// valid does not need the surviving vocabulary recited at them, they need to be told it went away.
fn warn_unknown_keys(
    obj: Option<&serde_json::Value>,
    known: &[&str],
    scope: &str,
    warnings: &mut Vec<String>,
) {
    let Some(map) = obj.and_then(serde_json::Value::as_object) else {
        return;
    };
    for key in map.keys() {
        if known.contains(&key.as_str()) {
            continue;
        }
        let dotted = format!("{scope}{key}");
        if let Some((_, why)) = RETIRED_KEYS.iter().find(|(k, _)| *k == dotted) {
            warnings.push(format!(
                "unknown config key \"{dotted}\" (ignored) — REMOVED from zzop's recognized config \
                 keys: {why}"
            ));
            continue;
        }
        let where_ = if scope.is_empty() {
            "at the top level".to_string()
        } else {
            format!("under \"{}\"", scope.trim_end_matches('.'))
        };
        warnings.push(format!(
            "unknown config key \"{dotted}\" (ignored) — a typo, or a key from a different zzop \
             version. Known keys {where_}: {}.",
            known.join(", ")
        ));
    }
}

// ---------------------------------------------------------------------------------------------------
// Bundled packs — the `withDefaults` layer's pack-injection half. `sources` is always
// `crate::BUNDLED_PACK_SOURCES` in production; parameterized so a fabricated bad source can exercise
// the skip-on-parse-failure path in tests without depending on a real pack ever going invalid.
// ---------------------------------------------------------------------------------------------------

pub(super) fn parse_pack_defs(
    sources: &[(&str, &str)],
    warnings: &mut Vec<String>,
) -> Vec<serde_json::Value> {
    let mut out = Vec::with_capacity(sources.len());
    for (rel_path, source) in sources {
        match serde_json::from_str::<serde_json::Value>(source) {
            Ok(v) => out.push(v),
            Err(err) => warnings.push(format!(
                "bundled pack \"{rel_path}\" failed to parse and was skipped: {err}."
            )),
        }
    }
    out
}
