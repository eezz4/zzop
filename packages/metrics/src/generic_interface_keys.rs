//! Default "low confidence" HTTP interface-key pattern table — analysis vocabulary (which paths are
//! generic enough that many unrelated services legitimately share them) used by
//! `zzop_engine::analyze_trees` as the default `zzop_core::LinkOptions::low_confidence_key_patterns`
//! override. Lives here rather than in `zzop-core`, mirroring the mechanism/vocabulary split of
//! `zzop_git::CollectOptions::commit_type_patterns` (see that module's doc): `zzop-core`'s
//! `link_cross_layer_io` owns the injectable pattern-matching mechanism only, not which paths count
//! as "generic".

/// Default low-confidence key patterns, in match order: `(compiled pattern, reason)`. Anchored against
/// the full `"METHOD /path"` key shape `http_interface_key` produces (`^[A-Z]+ /path$`), so e.g.
/// `GET /health-report` does not falsely match `/health`.
pub fn default_generic_interface_key_patterns() -> Vec<(regex::Regex, String)> {
    let reason = "generic path shared by many services (health/ping/metrics/status/login)";
    [
        r"^[A-Z]+ /health$",
        r"^[A-Z]+ /healthz$",
        r"^[A-Z]+ /ping$",
        r"^[A-Z]+ /metrics$",
        r"^[A-Z]+ /status$",
        r"^[A-Z]+ /login$",
        r"^[A-Z]+ /logout$",
        r"^[A-Z]+ /version$",
        r"^[A-Z]+ /favicon\.ico$",
    ]
    .into_iter()
    .map(|p| (regex::Regex::new(p).unwrap(), reason.to_string()))
    .collect()
}

#[cfg(test)]
mod tests {
    //! Pins the shipped table's shape (count, every pattern compiles) plus a handful of match/no-match
    //! cases exercising the anchoring — end-to-end join behavior (an edge actually getting
    //! `low_confidence_reason` set) is `zzop_core::io`'s job, covered there with an injected table.
    use super::*;

    #[test]
    fn has_nine_patterns_all_sharing_the_same_reason() {
        let patterns = default_generic_interface_key_patterns();
        assert_eq!(patterns.len(), 9);
        let reason = &patterns[0].1;
        assert!(patterns.iter().all(|(_, r)| r == reason));
        assert!(reason.contains("health/ping/metrics/status/login"));
    }

    #[test]
    fn matches_the_exact_generic_path_shapes() {
        let patterns = default_generic_interface_key_patterns();
        let matches = |key: &str| patterns.iter().any(|(re, _)| re.is_match(key));
        assert!(matches("GET /health"));
        assert!(matches("GET /healthz"));
        assert!(matches("POST /login"));
        assert!(matches("GET /favicon.ico"));
    }

    #[test]
    fn anchored_so_a_longer_path_sharing_the_prefix_does_not_match() {
        let patterns = default_generic_interface_key_patterns();
        let matches = |key: &str| patterns.iter().any(|(re, _)| re.is_match(key));
        assert!(!matches("GET /health-report"));
        assert!(!matches("GET /api/health"));
        assert!(!matches("GET /healthy"));
    }
}
