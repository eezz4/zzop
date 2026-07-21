// ---------------------------------------------------------------------------------------------------
// Unknown-key warnings — the port of `collectConfigWarnings`'s scoped walk. Never rejects (the engine
// deliberately ignores unknown fields); this only makes a typo or cross-version drift visible.
// Vocabulary sourced from `crate::CONFIG_SURFACE_JSON`'s `configKeys` — the same vocabulary file the
// JS CLI and the engine's own reference-validation meta-test share, so this port can never disagree
// with either about what a valid config key is.
// ---------------------------------------------------------------------------------------------------

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
    warn_unknown_keys(
        config.get("report"),
        &known("report"),
        "report.",
        &mut warnings,
    );

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
            if let Some(mounts) = tree.get("mounts").and_then(serde_json::Value::as_array) {
                for (j, entry) in mounts.iter().enumerate() {
                    if entry.is_object() {
                        warn_unknown_keys(
                            Some(entry),
                            &known_mount,
                            &format!("trees[{i}].mounts[{j}]."),
                            &mut warnings,
                        );
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

    warnings
}

/// One scope of `collectConfigWarnings`'s walk: for every key in `obj` (a no-op if `obj` is absent or
/// not itself a JSON object) not present in `known`, push an "unknown config key" warning naming the
/// full dotted key, the scope, and the known-keys list for that scope — verbatim text match with the
/// JS source, including its `${scope}${key}` composition and the `scope.replace(/\.$/, '')` trim
/// (`scope` here always carries at most one trailing `.`, so `trim_end_matches('.')` is equivalent).
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
        if !known.contains(&key.as_str()) {
            let where_ = if scope.is_empty() {
                "at the top level".to_string()
            } else {
                format!("under \"{}\"", scope.trim_end_matches('.'))
            };
            warnings.push(format!(
                "unknown config key \"{scope}{key}\" (ignored) — a typo, or a key from a different zzop \
                 version. Known keys {where_}: {}.",
                known.join(", ")
            ));
        }
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
