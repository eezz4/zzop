//! Contract 17: no DSL pack may hand-write the disable hint the engine already appends.

use std::fs;
use std::path::PathBuf;

use crate::dsl_dir;

/// The tokens a DSL author must never write. `disabledRules` is the embedder-facing half of the ONE
/// disable-hint fragment `crates/engine/src/pipeline/findings.rs`'s `append_hints` appends to
/// EVERY finding `eval_packs` (and `envelope::file_pass`'s Mode B `eval_pack`) produces —
/// unconditionally, before the finding reaches the cache, so the appended sentence is present in
/// warm-cache reads too. `disabled_rules` is the snake_case spelling that fragment emitted until
/// 2026-08-02 (it is the Rust `RuleConfig` field name, but NOT a spelling the JSON wire accepts) — kept
/// forbidden so the original hand-written-hint incident cannot recur under the old habit either.
const ENGINE_OWNED_TOKENS: [&str; 2] = ["disabledRules", "disabled_rules"];

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

/// Contract 17 — no shipped DSL pack's `message` field names `disabledRules` (or the retired
/// `disabled_rules` spelling).
///
/// The engine appends the disable hint to every DSL finding itself
/// (`pipeline::findings::append_hints`), so a hand-written copy does not ADD the sentence — it
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
/// which accepts `disabled_rules`/`disabledRules` as one of two ways to satisfy the "how to exclude"
/// leg: the two compose rather than conflict. Contract 2 says a DSL message must name its marker OR that
/// token; this one removes the second option, leaving exactly one thing a DSL author writes — their own
/// derived `zzop-<id>-ok` marker — while the engine owns the disable sentence. Contract 2's token leg
/// is not deleted there because it is also the leg a future non-DSL caller of that helper could use; here
/// it is simply unavailable.
///
/// **Scope**: both spellings of the embedder field. `disabledRules` is what the engine's fragment emits
/// verbatim since the 2026-08-02 wire-spelling fix (the request surfaces are camelCase and silently drop
/// unknown keys, so the old snake_case hint was uncopyable); `disabled_rules` stays forbidden so a
/// hand-written copy of the RETIRED hint — the exact shape of the original incident — cannot return
/// either.
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
            if ENGINE_OWNED_TOKENS.iter().any(|tok| message.contains(tok)) {
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
        "DSL rule messages that hand-write the engine-owned disable-hint token ({ENGINE_OWNED_TOKENS:?}). The engine appends the disable hint to \
         every DSL finding already (`pipeline::findings::append_hints`), so this text renders \
         TWICE in the finding a user reads — delete the hand-written sentence, keep only your rule's own \
         `zzop-<id>-ok` marker (docs/rules/authoring-guide.md): {offenders:#?}"
    );
}

/// A rule that can never reach its own goal must SAY where its ceiling is, in the finding itself —
/// and when the ceiling OPENS, the message must say exactly what opened and what did not.
///
/// `hardcoded-secret` matches on VALUE SHAPE, so a passphrase built from dictionary words and a
/// no-digit base64url token are both silent in ITS arms — that part is unchanged and still pinned.
/// What changed (A17, 2026-08-03): the string-literal-with-binding-name IR node the message used to
/// name as the missing fix now EXISTS, and `security/high-entropy-secret` reads it. So this pin's job
/// grew a second half: the line-scan message must point at the sibling that covers its measured
/// blindness (or a reader still concludes the classes are uncovered), and the SIBLING's message must
/// state its own residuals — the measured threshold, the sub-floor passphrases it misses, and the
/// value-veto it structurally cannot have (the value is hashed at extraction) — so the pair can never
/// be read as closure.
///
/// Both messages are long, and the sentences most likely to look trimmable are exactly these.
#[test]
fn hardcoded_secret_and_its_entropy_sibling_state_their_ceilings() {
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
        // this rule's OWN ceiling, unchanged: shape, named as what it is
        "SHAPE test, not entropy",
        // the silenced classes and their measured rates
        "ALWAYS silenced",
        "200k samples",
        // the opened half: the node exists and the sibling reads it — with the both-rules residual
        "string-literal-with-binding-name",
        "security/high-entropy-secret",
        "Still silent in BOTH rules",
    ] {
        assert!(
            message.contains(token),
            "hardcoded-secret's message must keep stating its ceiling AND where the opened half now \
             lives, missing {token:?}.\n\
             Trimming these sentences leaves a rule that looks complete and is not — the exact \
             silence this repo's disclosure discipline exists to abolish."
        );
    }

    let (_, sibling) = messages
        .iter()
        .find(|(id, _)| id == "high-entropy-secret")
        .expect("high-entropy-secret must still exist and carry a message");

    for token in [
        // the judgment and its measured floor
        "threshold 80",
        // the no-plaintext contract, stated where a user reads it
        "NEVER the value itself",
        // the residuals that keep the pair honest: sub-floor passphrases, the impossible value-side
        // veto, and the indistinguishable long-identifier class
        "88.5% below 80",
        "invisible by design",
        "indistinguishable from a passphrase",
    ] {
        assert!(
            sibling.contains(token),
            "high-entropy-secret's message must keep stating its measured floor and residuals, \
             missing {token:?} — without them the hardcoded-secret + high-entropy-secret pair reads \
             as closure, which the measurements say it is not."
        );
    }
}
