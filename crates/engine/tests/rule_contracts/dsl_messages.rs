//! Contract 17: no DSL pack may hand-write the disable hint the engine already appends.

use std::fs;
use std::path::PathBuf;

use crate::dsl_dir;

/// The token a DSL author must never write. It is the embedder-facing half of the ONE disable-hint
/// fragment `crates/engine/src/pipeline/findings.rs`'s `append_disable_hints` appends to EVERY finding
/// `eval_packs` (and `envelope::file_pass`'s Mode B `eval_pack`) produces — unconditionally, before the
/// finding reaches the cache, so the appended sentence is present in warm-cache reads too.
const ENGINE_OWNED_TOKEN: &str = "disabled_rules";

/// Every `rules/dsl/<pack>/<pack>.json`, as raw files. Read from the same directory
/// [`crate::load_all_packs`] loads, but deliberately NOT through the typed loader: this contract judges
/// what an AUTHOR wrote in the pack file, and a raw `serde_json::Value` walk sees every `"message"` key
/// in the document — including one on a rule shape the typed loader might one day normalize away, or one
/// in a pack that fails to load at all.
///
/// Scanned via `serde_json`, never a text scan of the file bytes: pack JSON is mostly regex source
/// (`"pattern": "[\"']lazy[\"']\\s*:..."`), so a substring/line scan for `"message"` reads quoting it has
/// no parser for. That mistake has already been made once in this repo.
fn dsl_pack_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dsl_dir()) else {
        return out;
    };
    for entry in entries.filter_map(Result::ok) {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(files) = fs::read_dir(&dir) else {
            continue;
        };
        for file in files.filter_map(Result::ok) {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Depth-first walk collecting every `"message"` string value in `value`, each tagged with the nearest
/// enclosing object's `"id"` when it has one — so an offender report names the rule, not just the file.
fn collect_messages(
    value: &serde_json::Value,
    owner: Option<&str>,
    out: &mut Vec<(String, String)>,
) {
    match value {
        serde_json::Value::Object(map) => {
            let owner = map.get("id").and_then(serde_json::Value::as_str).or(owner);
            for (key, child) in map {
                if key == "message" {
                    if let Some(text) = child.as_str() {
                        out.push((owner.unwrap_or("<no id>").to_string(), text.to_string()));
                        continue;
                    }
                }
                collect_messages(child, owner, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_messages(item, owner, out);
            }
        }
        _ => {}
    }
}

/// Contract 17 — no shipped DSL pack's `message` field names `disabled_rules`.
///
/// The engine appends the disable hint to every DSL finding itself
/// (`pipeline::findings::append_disable_hints`), so a hand-written copy does not ADD the sentence — it
/// makes the finding carry it TWICE. `docs/rules/authoring-guide.md` states the rule for authors ("Do NOT
/// write your own 'Disable via config ...' sentence in `message`"); this is that rule mechanized.
///
/// It shipped: `perf/sqlalchemy-eager-relationship`'s message ended "...or turn the rule off wholesale
/// through `disabled_rules`." and the finding a user actually read ended "...through `disabled_rules`.
/// Disable via config `rules: { "perf/sqlalchemy-eager-relationship": "off" }` (embedders:
/// `disabled_rules`)". The hand-written half was also WORSE than the engine's: it named only the embedder
/// field and never the config-file spelling, so a `zzop.config.jsonc` user was told to set a key their
/// dialect does not have.
///
/// **Relationship to contract 2** (`markers.rs`'s `every_dsl_rule_message_documents_how_to_exclude_it`),
/// which accepts `disabled_rules` as one of two ways to satisfy the "how to exclude" leg: the two compose
/// rather than conflict. Contract 2 says a DSL message must name its marker OR `disabled_rules`; this one
/// removes the second option, leaving exactly one thing a DSL author writes — their own derived
/// `zzop-<id>-ok` marker — while the engine owns the disable sentence. Contract 2's `disabled_rules` leg
/// is not deleted there because it is also the leg a future non-DSL caller of that helper could use; here
/// it is simply unavailable.
///
/// **Scope**: the `disabled_rules` spelling only. `disabledRules` (the config-file dialect) is not
/// forbidden — nothing has ever hand-written it, and the token this contract exists to stop is the one the
/// engine's own fragment emits verbatim.
#[test]
fn no_dsl_pack_message_hand_writes_the_engine_appended_disable_hint() {
    let files = dsl_pack_files();
    assert!(
        !files.is_empty(),
        "no `rules/dsl/*/*.json` pack files were found under {} — this guard would have passed while \
         checking NOTHING. Re-point `dsl_pack_files` at the real pack directory rather than leaving a \
         scan that is green because it is empty.",
        dsl_dir().display()
    );

    let mut messages = Vec::new();
    let mut offenders = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let value: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()));
        let mut found = Vec::new();
        collect_messages(&value, None, &mut found);
        for (rule_id, message) in found {
            if message.contains(ENGINE_OWNED_TOKEN) {
                offenders.push(format!("{}: rule `{rule_id}`", path.display()));
            }
            messages.push(message);
        }
    }

    assert!(
        !messages.is_empty(),
        "{} pack file(s) parsed but not one `\"message\"` field was found — the walk is looking in the \
         wrong place and this guard is judging an empty set.",
        files.len()
    );

    assert!(
        offenders.is_empty(),
        "DSL rule messages that hand-write `{ENGINE_OWNED_TOKEN}`. The engine appends the disable hint to \
         every DSL finding already (`pipeline::findings::append_disable_hints`), so this text renders \
         TWICE in the finding a user reads — delete the hand-written sentence, keep only your rule's own \
         `zzop-<id>-ok` marker (docs/rules/authoring-guide.md): {offenders:#?}"
    );
}

/// A rule that can never reach its own goal must SAY where its ceiling is, in the finding itself.
///
/// `hardcoded-secret` is the case with the most to lose: it matches on VALUE SHAPE, so a passphrase
/// built from dictionary words is always silent and a random base64url token is silent whenever it
/// draws no digits. Neither is fixable by tuning the rule — closing the first needs entropy (not
/// computable in a line scan) and the second needs a string-literal-with-binding-name IR node the
/// matcher does not have. That is why `2.backlog/waiting.md` keeps the row: the door does not open
/// from inside this rule.
///
/// The message already carries all of it, including measured false-negative rates. This pins it,
/// because a message that long is exactly the kind an editor trims for brevity — and the sentences most
/// likely to look trimmable are the ones admitting the ceiling, which are the reason a reader can trust
/// the rest.
#[test]
fn hardcoded_secret_states_the_ceiling_it_cannot_pass() {
    let mut messages = Vec::new();
    for path in dsl_pack_files() {
        let text = fs::read_to_string(&path).expect("pack readable");
        let value: serde_json::Value = serde_json::from_str(&text).expect("pack parses");
        collect_messages(&value, None, &mut messages);
    }
    let (_, message) = messages
        .iter()
        .find(|(id, _)| id == "hardcoded-secret")
        .expect("hardcoded-secret must still exist and carry a message");

    for token in [
        // the two ceilings, each named as what it IS rather than as a vague limitation
        "SHAPE test, not entropy",
        "string-literal-with-binding-name",
        // and the honesty that makes the ceiling actionable rather than decorative: what is silenced,
        // and how often
        "ALWAYS silenced",
        "200k samples",
    ] {
        assert!(
            message.contains(token),
            "hardcoded-secret's message must keep stating its own ceiling, missing {token:?}.\n\
             Trimming these sentences leaves a rule that looks complete and is not — the exact \
             silence this repo's disclosure discipline exists to abolish."
        );
    }
}
