//! Tests for the starter `zzop.config.jsonc` document (`template.rs`).

use crate::template::CONFIG_TEMPLATE_JSONC;
use crate::test_support::TempDir;
use crate::{load_for_root, DEFAULT_CONFIG_FILENAME};

/// Parses the template the way every real consumer does — JSONC strip, then JSON.
fn parsed_template() -> serde_json::Value {
    let stripped = crate::jsonc::strip_json_comments(CONFIG_TEMPLATE_JSONC);
    serde_json::from_str(&stripped).expect("the shipped template must be valid JSONC")
}

/// Seals that the starter file is a config this crate can actually load: valid JSONC, a JSON object,
/// and non-empty. A template that does not parse would fail only in the user's editor, after
/// `zzop init` had already written it.
#[test]
fn the_template_is_valid_jsonc_carrying_a_non_empty_config_object() {
    let value = parsed_template();
    let obj = value.as_object().expect("template must be a JSON object");
    assert!(!obj.is_empty(), "an empty starter file teaches nothing");
}

/// Seals requirement one of the template's contract: every key it ACTIVELY sets is a key zzop really
/// knows. Checked through this crate's own unknown-key walk — the exact `config-surface.json`
/// vocabulary a user's own run would judge their file against — so a template key that no surface
/// consumes fails here instead of shipping as a starter file that warns on its own first run.
#[test]
fn every_key_the_template_sets_is_in_the_knob_dictionary() {
    let dir = TempDir::new("zzop-config-template-keys");
    let base = crate::mapper::config_to_request(&parsed_template(), dir.path())
        .expect("the template must map without a config error");
    let offenders: Vec<&String> = base
        .warnings
        .iter()
        .filter(|w| w.contains("unknown config key"))
        .collect();
    assert!(
        offenders.is_empty(),
        "the starter template names keys zzop does not know: {offenders:?}"
    );
}

/// Seals the same requirement for the half a parser cannot see: the keys the COMMENTS name. Annotated
/// prose is the whole value of this template, and prose is exactly where a knob that never existed (or
/// stopped existing) survives unnoticed — the defect class `warnings::RETIRED_KEYS` was written to
/// clean up. The template's own convention is what makes this checkable: a key is named in backticks,
/// everything else (commands, JSON snippets) is not key-shaped and is out of scope, the same narrow
/// gate the engine's reference-validation contract uses on shipped messages.
#[test]
fn every_key_the_template_comments_name_is_in_the_knob_dictionary() {
    let surface: serde_json::Value =
        serde_json::from_str(crate::CONFIG_SURFACE_JSON).expect("embedded config surface parses");
    let mut known: Vec<String> = surface["configPaths"]
        .as_array()
        .expect("configPaths array")
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    for (_scope, keys) in surface["configKeys"]
        .as_object()
        .expect("configKeys object")
        .iter()
    {
        known.extend(
            keys.as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_string)),
        );
    }

    let tokens: Vec<&str> = CONFIG_TEMPLATE_JSONC
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|t| is_key_shaped(t))
        .collect();
    assert!(
        tokens.len() > 5,
        "the annotated template must name its keys in backticks — found {tokens:?}"
    );
    for token in tokens {
        assert!(
            known.iter().any(|k| k == token),
            "the template's comments name `{token}`, which is not in the knob dictionary \
             (crates/config/config-surface.json) — a starter file must never advertise a key no \
             surface consumes"
        );
    }
}

/// The narrow "could a reader go try this?" gate: a bare identifier or a dotted/bracketed path
/// (`cacheDir`, `git.since`, `trees[].root`). Anything with a space, quote, colon or angle bracket —
/// a shell command, a JSON snippet, a placeholder — is not a knob name and is deliberately not judged.
fn is_key_shaped(token: &str) -> bool {
    let mut chars = token.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '[' | ']'))
}

/// Seals requirement two, in the form it survives in: the starter file is what makes a run POSSIBLE, and
/// every value in it still equals zzop's own — the template documents the defaults instead of quietly
/// re-deciding them behind a file the user now owns.
///
/// The old form of this pin compared the request before and after the template landed and demanded they
/// be byte-identical. That comparison is gone because its "before" is gone: as of 2026-07-27 a directory
/// with no config has no request at all ([`load_for_root`]), so the transition a user performs is
/// refusal → analysis, not default → same default. What the pin protected — the template silently
/// re-deciding a value — moved to [`the_template_declares_every_vocabulary_key_with_zzops_own_value`],
/// which compares VALUES against their owning constants directly and is strictly stronger.
#[test]
fn a_directory_gets_a_mapped_request_only_once_the_template_lands() {
    let dir = TempDir::new("zzop-config-template-enables");
    assert!(
        load_for_root(dir.path()).is_err(),
        "a directory with no config must be refused, not analyzed on assumed defaults"
    );
    dir.write(DEFAULT_CONFIG_FILENAME, CONFIG_TEMPLATE_JSONC);
    let after =
        load_for_root(dir.path()).expect("the starter file must produce a mappable request");
    assert_eq!(
        after.method,
        crate::Method::Analyze,
        "the starter file's `roots: [\".\"]` is one tree, so it must take the single-tree entry"
    );
}

/// Seals the half of the convention-vocabulary decision the other pins cannot see: the starter file must
/// actually CARRY every declarable vocabulary key. A template that simply omitted `vocabulary` would pass
/// requirement one (it names no unknown key) while leaving a fresh `zzop init` user with every name
/// judgment switched off — since 2026-07-27 an undeclared key is not a default, it is a judgment not made.
#[test]
fn the_template_declares_every_vocabulary_key_the_knob_dictionary_lists() {
    let surface: serde_json::Value =
        serde_json::from_str(crate::CONFIG_SURFACE_JSON).expect("embedded config surface parses");
    let declarable: Vec<&str> = surface["configKeys"]["vocabulary"]
        .as_array()
        .expect("configKeys.vocabulary array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        !declarable.is_empty(),
        "the vocabulary scope must not be empty"
    );

    let written = parsed_template();
    let block = written["vocabulary"]
        .as_object()
        .expect("the starter template must carry a `vocabulary` block");
    for key in &declarable {
        assert!(
            block.contains_key(*key),
            "`vocabulary.{key}` is declarable but the starter template does not name it — a built-in \
             the user cannot see is the guess this axis exists to remove"
        );
    }
    let extra: Vec<&String> = block
        .keys()
        .filter(|k| !declarable.contains(&k.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "template names non-vocabulary keys: {extra:?}"
    );
}

/// Seals what the retired no-op pin used to seal as a side effect: every value the starter file writes is
/// zzop's OWN value for that vocabulary, compared field by field against `VocabularyConfig::built_in()`.
///
/// This became load-bearing on 2026-07-27. While the front end injected the built-ins, a template value
/// that had drifted was still harmless — the request carried the real default either way, and the old
/// before/after comparison caught the drift. Now the template is the ONLY path by which these values
/// reach a run, so a drifted literal here is not a documentation bug: it silently gives every new user a
/// vocabulary zzop never chose, with no surface anywhere that disagrees.
///
/// `workspaceSkipDirs` is deliberately NOT compared here: it steers `trees: "auto"` discovery in this
/// front end and the engine never sees it (`mapper::options::FRONT_END_ONLY_VOCABULARY_KEYS`), so there
/// is no owning constant left to compare against — the template literal IS zzop's statement of that list,
/// and a pin between a literal and a copy of itself would check nothing. Its presence is still enforced,
/// by [`the_template_declares_every_vocabulary_key_the_knob_dictionary_lists`].
#[test]
fn the_template_declares_every_vocabulary_key_with_zzops_own_value() {
    let block = parsed_template()["vocabulary"]
        .as_object()
        .expect("the starter template must carry a `vocabulary` block")
        .clone();

    let owned = serde_json::to_value(zzop_engine::VocabularyConfig::built_in())
        .expect("the engine's own vocabulary serializes");
    for (key, expected) in owned.as_object().expect("built_in is an object") {
        let written = block
            .get(key)
            .unwrap_or_else(|| panic!("`vocabulary.{key}` is missing from the starter template"));
        assert_eq!(
            written, expected,
            "`vocabulary.{key}` in the starter template is not zzop's own value — a user who runs \
             `init` would silently get this literal instead of the constant its consumer owns"
        );
    }
}
