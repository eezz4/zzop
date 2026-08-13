//! `zzop explain <rule-id> --config <path>` — the same lookup as [`super::explain`], over the packs a
//! CONFIG's trees actually load instead of only the ones compiled into this binary.
//!
//! ## Why this had to exist
//!
//! A rule that leaves the bundled set does not stop existing: it is exported to `examples/packs/`,
//! served as an `example-pack-*` contract resource, and recovered by saving it under a tree's
//! `zzop/rules/`. After that recovery the rule RUNS — `packsLoaded` reports it (`"source": "dir"`) and
//! its findings carry the full `<pack>/<rule>` id. Measured 2026-08-12 against the `typescript` pack:
//! analysis loaded 12 rules from `zzop/rules/` and `zzop explain typescript/no-explicit-any` answered
//! `unknown rule id` in the same tree. The tool disagreed with itself about whether the rule exists,
//! and the surface a reader would use to resolve that disagreement was the one that was wrong.
//!
//! ## Why `--config`, and why nothing else
//!
//! A pack directory is only ever resolved against a MAPPING BASE — `zzop/rules/` is
//! `zzop_config::DEFAULT_AUTHORED_PACKS_DIR` relative to it, and `packs.extraDirs` replaces it
//! outright. Only a config produces that base. A bare-tree form (`explain <id> <tree>`) was considered
//! and rejected on a measurement rather than on taste: there is no zero-config run at all
//! (`zzop_config::load_for_root` REFUSES a root with no `zzop.config.jsonc` — see its own doc for why),
//! so a tree whose packs this form could read is a tree whose packs can never fire. Explaining a rule
//! that cannot run is not a smaller answer than refusing; it is a wrong one.
//!
//! ## Why bundled-only stays the default
//!
//! `explain <id>` answers about THIS BINARY, needs no filesystem, and is the form every doc and error
//! message already spells. Making `--config` mandatory would put a config file between a reader and the
//! question "what does this shipped rule do". The two forms are one lookup over two corpora
//! ([`super::Corpus`]), and the bundled one's failure message names this one — so the reader who needs
//! the wider corpus is told, exactly where they find out they need it.

use zzop_core::RulePackDef;

use crate::config::build_engine_config;
use crate::request::{AnalyzeRequest, AnalyzeTreesRequest};

use super::{explain_over, native_analysis_ids, Corpus};

/// `zzop explain <rule-id> --config <path>`. `Ok`/`Err` carry the same contract as [`super::explain`];
/// a config that cannot be read or mapped is an `Err` too, carrying `zzop_config`'s own message
/// verbatim rather than a paraphrase (the reader is fixing that config, not this lookup).
pub fn explain_with_config(config_path: &str, query: &str) -> Result<String, String> {
    let loaded = zzop_config::load_config_file(std::path::Path::new(config_path))
        .map_err(|e| e.to_string())?;
    // Both arms report the SAME sentence on failure, and deliberately name no request shape: which of
    // the two a config maps to is decided by whether it declares `trees`, a distinction that belongs to
    // the embedder surface and that no CLI or MCP caller can type or act on (pinned by
    // `host_vocabulary::shared_crate_user_facing_messages_carry_no_facade_entry_point_names`, which
    // caught exactly this sentence naming one of them on 2026-08-12).
    let unreadable = |e: serde_json::Error| {
        format!("this config does not describe a tree this lookup can read: {e}")
    };
    let trees: Vec<AnalyzeRequest> = match loaded.method {
        zzop_config::Method::Analyze => {
            vec![serde_json::from_value(loaded.request).map_err(unreadable)?]
        }
        zzop_config::Method::AnalyzeTrees => {
            serde_json::from_value::<AnalyzeTreesRequest>(loaded.request)
                .map_err(unreadable)?
                .trees
        }
    };
    explain_over(
        &packs_of(&trees),
        &native_analysis_ids(),
        query,
        Corpus::Config(config_path),
    )
}

/// Every DSL pack the config's trees load, deduplicated by pack id.
///
/// Per tree this is `build_engine_config(...).packs` — the EXACT list the analysis lane hands the
/// engine, not a second derivation of it, so `explain --config` cannot come to a different conclusion
/// about which packs exist than the run it is explaining. That is the whole point: the defect this
/// module fixes was two surfaces deriving the pack set differently.
///
/// Across trees, a repeated pack id keeps the LAST tree's copy. Config-only shadowing like this is
/// possible (two trees with different `packs.extraDirs`, each holding a pack of the same id) and there
/// is no better answer available to a lookup that prints ONE rule: later-wins is at least the rule the
/// engine already applies within a tree, so the two levels do not contradict each other.
///
/// Per-tree load WARNINGS are discarded. `explain` is a best-effort read with no warnings channel —
/// the same stance `super::bundled_packs` documents for an unparseable bundled pack — and a pack that
/// failed to load is reported where it matters, on the analyze reply that tried to run it. The
/// unknown-id message for this corpus says exactly that, so the silence here is disclosed rather than
/// merely accepted.
fn packs_of(trees: &[AnalyzeRequest]) -> Vec<RulePackDef> {
    let mut packs: Vec<RulePackDef> = Vec::new();
    let mut discarded_warnings = Vec::new();
    for tree in trees {
        for pack in build_engine_config(tree, &mut discarded_warnings).packs {
            match packs.iter_mut().find(|existing| existing.id == pack.id) {
                Some(slot) => *slot = pack,
                None => packs.push(pack),
            }
        }
    }
    packs
}

#[cfg(test)]
mod tests {
    use super::explain_with_config;
    use crate::test_support::TempDir;

    /// The exported `typescript` pack, trimmed to one rule — enough to be a loadable pack with an id
    /// this lookup must find, without pinning this test to the real export's contents.
    const RECOVERED_PACK: &str = r#"{
      "id": "typescript",
      "schema_version": 1,
      "rules": [{
        "id": "no-explicit-any",
        "severity": "warning",
        "message": "explicit any defeats the type checker",
        "matcher": {
          "type": "line-scan",
          "file_pattern": "(?i)\\.tsx?$",
          "line_pattern": ":\\s*any\\b"
        }
      }]
    }"#;

    fn tree_with_recovered_pack(prefix: &str) -> TempDir {
        let dir = TempDir::new(prefix);
        dir.write("zzop.config.jsonc", "{}");
        dir.write("zzop/rules/typescript-lint.json", RECOVERED_PACK);
        dir
    }

    /// The defect, as a test: a rule recovered into `zzop/rules/` is explainable through the config
    /// that loads it, and the SAME id is unknown to the bundled-only lookup in the same tree. Both
    /// halves are asserted together — the second is what makes the first worth having.
    #[test]
    fn a_rule_recovered_into_the_authored_packs_dir_is_explainable_through_its_config() {
        let dir = tree_with_recovered_pack("explain-config");
        let config = dir.path().join("zzop.config.jsonc");
        let out = explain_with_config(
            config.to_str().expect("temp path is UTF-8"),
            "typescript/no-explicit-any",
        )
        .expect("a pack loaded from zzop/rules/ must be explainable through its config");
        assert!(
            out.contains("no-explicit-any"),
            "the rendered rule must name the rule asked for: {out}"
        );

        let bundled_only = super::super::explain("typescript/no-explicit-any");
        assert!(
            bundled_only.is_err(),
            "if this id became bundled again, this module's reason for existing changed — re-read \
             the module doc before deleting the assertion"
        );
    }

    /// Bundled packs stay visible through `--config`: the config lane WIDENS the corpus, never
    /// replaces it. (The mapper seeds `BUNDLED_PACK_SOURCES` as inline `packDefs` on every tree, so
    /// this holds by construction — pinned because "by construction" is exactly what stops holding.)
    #[test]
    fn the_config_corpus_still_contains_the_bundled_packs() {
        let dir = tree_with_recovered_pack("explain-config-bundled");
        let config = dir.path().join("zzop.config.jsonc");
        let query = "security/hardcoded-secret";
        assert!(
            super::super::explain(query).is_ok(),
            "fixture assumption: {query} is bundled"
        );
        assert!(
            explain_with_config(config.to_str().unwrap(), query).is_ok(),
            "the config corpus must be a superset of the bundled one, not a replacement"
        );
    }

    /// An unknown id through `--config` names the config that was searched, and does NOT tell the
    /// reader to pass `--config` — they just did. The bundled-only tail is pinned in the same test so
    /// the two halves cannot drift into saying the same thing.
    #[test]
    fn each_corpus_unknown_id_message_fits_the_corpus_it_searched() {
        let dir = tree_with_recovered_pack("explain-config-unknown");
        let config = dir.path().join("zzop.config.jsonc");
        let config_path = config.to_str().unwrap();
        let err = explain_with_config(config_path, "no-such-rule-anywhere")
            .expect_err("an unknown id is a lookup failure on either corpus");
        assert!(
            err.contains(config_path),
            "the config lane must name the config whose packs it searched: {err}"
        );
        assert!(
            !err.contains("--config"),
            "telling a caller who passed --config to pass --config is the message drifting: {err}"
        );

        let bundled = super::super::explain("no-such-rule-anywhere")
            .expect_err("an unknown id is a lookup failure on either corpus");
        assert!(
            bundled.contains("--config") && bundled.contains("zzop/rules/"),
            "the bundled lane's tail is the only place a reader learns the wider corpus exists: \
             {bundled}"
        );
    }

    /// A config that does not exist fails as a CONFIG problem, in `zzop_config`'s own words — this
    /// lookup must not paraphrase a message whose remedy lives in another crate.
    #[test]
    fn a_missing_config_reports_the_config_loaders_own_message() {
        let dir = TempDir::new("explain-config-missing");
        let missing = dir.path().join("nope.jsonc");
        let err = explain_with_config(missing.to_str().unwrap(), "security/hardcoded-secret")
            .expect_err("a config that is not there cannot name any packs");
        assert!(
            err.contains("nope.jsonc"),
            "the failure must name the file the caller pointed at: {err}"
        );
    }
}
