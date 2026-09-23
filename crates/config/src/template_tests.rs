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
///
/// # BACKTICKS MEAN CONFIG KEY, and two other vocabularies look identical in prose
/// [`is_key_shaped`] accepts any bare identifier or dotted path, and this crate has no dependency that
/// would let it hold a second dictionary — so a backticked RULE ID (`unreachable`) or REPLY FIELD
/// (`nativeAnalyses.disabled`) reads as a config key that does not exist and fails here. That is the
/// guard working, not a false positive: the three vocabularies genuinely look the same, and only the
/// author knows which one a token is from. The template's convention resolves it without weakening the
/// gate:
///
/// * a **rule id** is written the way a user writes it — `"unreachable": "info"` — which carries a
///   quote and a colon and is out of scope by shape, and which is more useful anyway, since it is the
///   line they paste;
/// * a **reply field** is named in plain prose with no backticks, as `ruleOverridesApplied.disabled`
///   already was before either of these arrived.
///
/// Hyphenated ids (`dead-candidates`) fall out on the hyphen, which is why this only ever bites the
/// single-word ones — and why it went unnoticed until both a bare id and a reply field arrived in the
/// same edit on 2026-09-03, each failing this test in turn.
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

/// The pack-id list in the template's prose, held against the packs actually compiled in.
///
/// ## The incident, and why a key-shaped scan could not have caught it
/// On 2026-08-11 the `typescript` pack moved out of the bundle to `examples/packs/typescript-lint.json`
/// (12 rules, 12 packs -> 11). The template's prose list did not follow it, so the shipped starter
/// config went on advertising a pack that no longer loads — and that list is what `zzop init` writes
/// into every new tree, what `zzop contract config-template` prints, and what MCP serves as the
/// `config-template` resource. Both hosts, one stale word.
///
/// The sibling guard above (`the_templates_comments_only_name_knobs_the_config_surface_declares`)
/// could not see it: it reads BACKTICKED, key-SHAPED tokens, and this is a bare word in a
/// space-separated value list. A guard whose needle is narrower than the claim passes on a document
/// that says something the needle cannot read.
///
/// The consequence was reachable by copy-paste, not by contrivance: the very next sentence invites a
/// single-subject repo to write `packs.only` with one id, and `packs.only: ["typescript"]` matches no
/// loaded pack — which switches every DSL rule off while the reply still reports 11 packs loaded.
///
/// Checked in BOTH directions on purpose. A removed pack left in the prose is the bug that happened;
/// a NEW pack never added to the prose is the same defect with the sign flipped, and would otherwise
/// sit undetected until someone wondered why `packs.only` did not offer it.
#[test]
fn the_templates_bundled_pack_list_names_exactly_the_packs_compiled_into_this_binary() {
    const ANCHOR: &str = "The bundled pack ids are:";
    let after = CONFIG_TEMPLATE_JSONC
        .split_once(ANCHOR)
        .unwrap_or_else(|| {
            panic!(
                "the template no longer contains {ANCHOR:?} — this guard's subject moved, and an \
                 absent subject would make it vacuously green"
            )
        })
        .1;
    let line = after
        .lines()
        .nth(1)
        .expect("a line must follow the anchor")
        .trim_start_matches([' ', '/'])
        .trim();
    let advertised: std::collections::BTreeSet<&str> = line.split_whitespace().collect();
    assert!(
        advertised.len() > 5,
        "only {} id(s) parsed out of the template's pack line ({line:?}) — the list's SHAPE changed \
         and this guard is now reading the wrong text",
        advertised.len()
    );

    let compiled: std::collections::BTreeSet<String> = crate::BUNDLED_PACK_SOURCES
        .iter()
        .map(|(rel, source)| {
            let value: serde_json::Value = serde_json::from_str(source)
                .unwrap_or_else(|e| panic!("bundled pack {rel} is not valid JSON: {e}"));
            value["id"]
                .as_str()
                .unwrap_or_else(|| panic!("bundled pack {rel} has no string `id`"))
                .to_string()
        })
        .collect();

    let stale: Vec<&&str> = advertised
        .iter()
        .filter(|id| !compiled.contains(**id))
        .collect();
    assert!(
        stale.is_empty(),
        "the starter template advertises pack id(s) {stale:?} that are NOT compiled into this binary \
         (loaded: {compiled:?}). `zzop init` writes this text into every new tree; a user who copies \
         a stale id into `packs.only` switches off every DSL rule and gets no warning saying so"
    );

    let unlisted: Vec<&String> = compiled
        .iter()
        .filter(|id| !advertised.contains(id.as_str()))
        .collect();
    assert!(
        unlisted.is_empty(),
        "pack(s) {unlisted:?} are compiled in but missing from the starter template's list \
         ({advertised:?}) — the opposite sign of the same defect: a bundled subject area the \
         template never offers as a `packs.only`/`packs.disabled` value"
    );
}

/// Binds the vocabulary block's "TypeScript/JavaScript files only" sentences to the fact that makes them
/// true, because this file has already shipped a sentence that went false with nobody noticing —
/// `template.rs`'s module doc owns that incident, and names why the sibling pins could not catch it:
/// they check that every key NAMED is real, never that a sentence ABOUT a key is true.
///
/// The fact those sentences rest on is not prose anywhere else either. The rules that consume
/// `ormWriteMethods`/`ormReceiverPattern` (write sites), `prismaClientGetter` (query call sites) and
/// `retryWrappers` (the egress retry witness) each DECLARE the extensions their evidence can be
/// witnessed in, and `zzop_engine::sightlines`' own test already holds those declarations against the
/// dispatch table's TypeScript arm. So this reads the declarations — never a hand list — and asserts the
/// one property the template's claim needs: while every declared extension is a JS/TS spelling, saying
/// "TypeScript/JavaScript files only" is honest. The day a Java, Go or Python producer lands and one of
/// those declarations widens, this goes red HERE, at the one surface that writes the claim into a user's
/// own repository.
///
/// SCOPE, stated as a list rather than as an audit: this binds the four keys above. `routerNames`,
/// `fetchWrapperExportNames` and the five Express/Hono keys carry the same sentence and have NO
/// sightline declaration to bind to, so those sentences are held by review alone. Naming that gap is
/// the point — a pin whose coverage is assumed wider than it is, is worse than no pin.
#[test]
fn the_typescript_only_vocabulary_disclosure_holds_only_while_its_channels_are_typescript_only() {
    let declared = zzop_engine::rule_sightlines();
    assert!(
        declared.len() >= 4,
        "empty-set floor: only {} sightline declarations were read, so this pin would pass on an \
         extraction failure rather than on the property it claims",
        declared.len()
    );
    for sightline in &declared {
        assert!(
            !sightline.trigger_extensions.is_empty(),
            "`{}` declares an empty evidence channel",
            sightline.rule_id
        );
        for ext in sightline.trigger_extensions {
            assert!(
                matches!(
                    *ext,
                    "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts"
                ),
                "`{}` now declares evidence in `.{ext}` files, which is not a JS/TS spelling — the \
                 starter template still tells every Java/Go/Python/C# user that the ORM, \
                 query-call-site and retry vocabulary keys are read for TypeScript/JavaScript files \
                 ONLY. Re-word `crates/config/src/template.rs` in this same commit",
                sightline.rule_id
            );
        }
    }

    // The other direction: the fact holding is only half of it — the sentence has to still be there.
    assert!(
        CONFIG_TEMPLATE_JSONC.contains("TypeScript/JavaScript"),
        "the starter template no longer discloses that part of its vocabulary is read for one language \
         family only, while the declarations above say it still is"
    );
}

/// The half the two tests above BOTH declared out of scope: a JSON snippet inside a comment.
///
/// 🔴 That exemption cost a real defect (2026-09-07, review ledger V53). The template's `trees` example
/// taught `{ "root": …, "clientBase": …, "mountedAt": … }` FLAT, and the deployment-topology keys had
/// moved under `topology`. A reader who copied it hit two different failures from one snippet: a flat
/// `mountedAt` is a hard load error naming the new path, but a flat `clientBase` is an unknown key —
/// warned, then IGNORED. Exit 0, and a join that silently lost its prefix. That is the single biggest
/// join blocker this repo has measured, taught by the file `zzop init` writes into every tree.
///
/// The snippets are findable because the template already separates them by indentation: prose is
/// `// `, a snippet line is `//` plus three spaces. So this reads what a copying user reads, wraps it
/// into an object, and runs the SAME unknown-key walk a real run would.
///
/// ⚠ Scope, stated so the next reader does not over-trust a green: this validates snippets that parse
/// as an object body. A snippet that is a bare fragment (one array, one scalar) is skipped rather than
/// guessed at, and a snippet naming a key whose VALUE is wrong is out of reach — key vocabulary is what
/// `config-surface.json` can judge.
#[test]
fn every_json_snippet_in_the_templates_comments_loads_without_an_unknown_key() {
    let mut snippets: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in CONFIG_TEMPLATE_JSONC.lines() {
        let trimmed = line.trim_start();
        match trimmed.strip_prefix("//") {
            Some(rest) if rest.starts_with("   ") && !rest.trim().is_empty() => {
                current.push(rest[3..].to_string());
            }
            _ => {
                if !current.is_empty() {
                    snippets.push(current.join("\n"));
                    current.clear();
                }
            }
        }
    }
    if !current.is_empty() {
        snippets.push(current.join("\n"));
    }

    let mut checked = 0usize;
    for snippet in &snippets {
        let body = snippet.trim().trim_end_matches(',');
        let wrapped = format!("{{{body}}}");
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&wrapped) else {
            continue; // not an object body — see the scope note above
        };
        checked += 1;
        let dir = TempDir::new("zzop-config-template-snippet");
        let mapped = crate::mapper::config_to_request(&value, dir.path()).unwrap_or_else(|e| {
            panic!(
                "a JSON snippet in the template's comments is REFUSED by the loader — a reader who \
                 copies it gets this error instead of a run:\n  snippet: {body}\n  error: {e:?}"
            )
        });
        // A snippet whose ROOT keys are unknown is not a top-level config fragment at all — it is a
        // VALUE-position example (the `rules` block's rule ids, a vocabulary list). The sibling test's
        // doc already names that ambiguity: three vocabularies look identical in prose. A NESTED
        // unknown (`trees[0].clientBase`) is different in kind — the root key IS real, so the reader is
        // being taught a wrong SHAPE under a right name, and that is the defect this test exists for.
        let offenders: Vec<&String> = mapped
            .warnings
            .iter()
            .filter(|w| w.contains("unknown config key"))
            .filter(|w| {
                w.split_once('"')
                    .and_then(|(_, rest)| rest.split_once('"'))
                    .is_some_and(|(key, _)| key.contains('.') || key.contains('['))
            })
            .collect();
        assert!(
            offenders.is_empty(),
            "a JSON snippet in the template's comments names a key zzop ignores — the reader gets \
             exit 0 and the key does nothing, which is worse than the refusal above:\n  \
             snippet: {body}\n  warnings: {offenders:?}"
        );
    }

    assert!(
        checked > 0,
        "no object-shaped snippet was found in the template's comments — the extractor stopped \
         matching (the convention is `//` plus three spaces), so this test is now vacuous"
    );
}
