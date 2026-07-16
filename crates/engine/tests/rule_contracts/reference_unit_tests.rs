//! Contract 11's unit tests — the three historical defects pinned against a tiny synthetic vocabulary,
//! plus the extraction-scope pins (comment lines, distance window). See `reference_validation.rs` for the
//! contract's rationale header and the two real-tree checks these unit tests back.

use crate::config_surface::{
    extract_config_context_tokens, extract_flag_references, unknown_config_context_tokens,
    unknown_flag_references, ConfigKeysSurface, ConfigSurface,
};

/// Synthetic, deliberately tiny vocabulary for the CHECK A/B unit tests below — independent of the real
/// `config-surface.json` (which can grow over time) so these tests stay a fixed, minimal pin of exactly the
/// three historical defects, never accidentally passing because the real file happens to allowlist
/// something today.
fn tiny_synthetic_vocab() -> ConfigSurface {
    ConfigSurface {
        config_keys: ConfigKeysSurface {
            top: vec!["rules".to_string(), "git".to_string()],
            packs: vec![],
            git: vec!["since".to_string()],
            report: vec![],
            tree: vec![],
            rule_object: vec![],
        },
        config_paths: vec!["git.since".to_string()],
        cli_flags: vec!["--config".to_string()],
        embedder_fields: vec!["disabled_rules".to_string()],
        embedder_field_shapes: std::collections::BTreeMap::new(),
        external_tool_flags: vec!["--unshallow".to_string(), "--depth".to_string()],
        allowlisted_tokens: vec![],
    }
}

#[cfg(test)]
mod flag_reference_unit_tests {
    use super::*;

    /// Pins the shipped `--since=all` defect: `--since` is not a real CLI flag (the real flag is
    /// `--severity`) and not a real external tool flag either — CHECK A must reject it.
    #[test]
    fn rejects_the_shipped_since_all_defect() {
        let vocab = tiny_synthetic_vocab();
        let flags = extract_flag_references("re-run with `--since=all`");
        assert_eq!(
            unknown_flag_references(&flags, &vocab),
            vec!["--since".to_string()]
        );
    }

    /// A real external tool flag (git's own `--unshallow`, the fix for a shallow clone) must be accepted —
    /// proves CHECK A does not reject every unfamiliar-looking flag, only ones absent from the vocabulary.
    #[test]
    fn accepts_a_real_external_git_flag() {
        let vocab = tiny_synthetic_vocab();
        let flags = extract_flag_references("git fetch --unshallow");
        assert!(unknown_flag_references(&flags, &vocab).is_empty());
    }

    /// Pins the shipped `--repo=<path>` defect: `zzop` has no `--repo` flag (roots/trees are config-only,
    /// see `packages/cli/bin/zzop.js`'s real flag set) — CHECK A must reject it.
    #[test]
    fn rejects_the_shipped_repo_path_defect() {
        let vocab = tiny_synthetic_vocab();
        let flags = extract_flag_references("--repo=<path>");
        assert_eq!(
            unknown_flag_references(&flags, &vocab),
            vec!["--repo".to_string()]
        );
    }

    /// A flag reference inside a comment line must be invisible to extraction entirely — comments are not
    /// messages a reader of `zzop`'s OUTPUT ever sees.
    #[test]
    fn ignores_a_flag_reference_inside_a_comment_line() {
        assert!(extract_flag_references("// re-run with --since=all").is_empty());
        assert!(extract_flag_references("* re-run with --since=all").is_empty());
    }
}

#[cfg(test)]
mod config_context_unit_tests {
    use super::*;

    /// Pins the shipped `scanners.vocabulary.commitTypePatterns` defect: `scanners` is not a real top-level
    /// config key (the real top-level keys are `roots`/`trees`/`packs`/`rules`/`exclude`/`git`/`cacheDir`/
    /// `sizeCap`/`format`/`failOn`/`report`) — CHECK B must reject the whole dotted token on its first
    /// segment.
    #[test]
    fn rejects_the_shipped_commit_type_patterns_defect() {
        let vocab = tiny_synthetic_vocab();
        let tokens = extract_config_context_tokens(
            "add patterns in config under `scanners.vocabulary.commitTypePatterns`",
        );
        assert_eq!(
            unknown_config_context_tokens(&tokens, &vocab),
            vec!["scanners.vocabulary.commitTypePatterns".to_string()]
        );
    }

    /// A JSON-snippet-shaped backtick token (spaces/colons/braces/quotes) is not config-key-shaped at all —
    /// `rules` itself IS a real top key, but the snippet as a whole must not even reach the shape gate, let
    /// alone be reported as an offender.
    #[test]
    fn a_json_snippet_shaped_token_is_out_of_scope_not_an_offense() {
        let vocab = tiny_synthetic_vocab();
        let tokens = extract_config_context_tokens(
            "in zzop.config.jsonc via `rules: { \"circular\": \"off\" }`",
        );
        assert!(unknown_config_context_tokens(&tokens, &vocab).is_empty());
    }

    /// An embedder-field reference (the "embedders: `disabled_rules`" leg every native rule's disable-hint
    /// carries) must be accepted.
    #[test]
    fn accepts_an_embedder_field_reference() {
        let vocab = tiny_synthetic_vocab();
        let tokens =
            extract_config_context_tokens("disable via config (embedders: `disabled_rules`)");
        assert!(unknown_config_context_tokens(&tokens, &vocab).is_empty());
    }

    /// A real dotted config path (`git.since`) must be accepted.
    #[test]
    fn accepts_a_real_dotted_config_path() {
        let vocab = tiny_synthetic_vocab();
        let tokens =
            extract_config_context_tokens("configured via `git.since` in your config file");
        assert!(unknown_config_context_tokens(&tokens, &vocab).is_empty());
    }

    /// A config-key-shaped backtick token farther than 120 bytes from any "config" occurrence must not even
    /// be extracted — proves the distance window is enforced, not just the vocabulary lookup.
    #[test]
    fn ignores_a_token_outside_the_120_byte_window() {
        let filler = "x".repeat(200);
        let text = format!("config {filler} `scanners.vocabulary.commitTypePatterns`");
        assert!(extract_config_context_tokens(&text).is_empty());
    }

    /// A config-key-shaped backtick token inside a comment line must be invisible to extraction entirely,
    /// even when a "config" occurrence sits right next to it on the same comment line — a doc comment is
    /// not a message a reader of `zzop`'s OUTPUT ever sees.
    #[test]
    fn ignores_a_token_inside_a_comment_line() {
        assert!(extract_config_context_tokens(
            "// add patterns in config under `scanners.vocabulary.commitTypePatterns`"
        )
        .is_empty());
    }
}
