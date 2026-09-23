//! "WHICH of my conventions did zzop actually judge by?" — the coverage reply's answer.
//!
//! # The question this exists for
//! `zzop coverage` already answers *how much of this tree did zzop read* (structural dispatch, blind
//! spots, io channels). Nothing answered the other half a first-time reader asks in the same breath:
//! *my config declared some conventions and left others out — which ones did anything?*
//!
//! It matters because SILENCE IS NOT NEUTRAL, and it is not neutral in BOTH directions:
//!
//! * Delete a key and the judgement that used it stops happening. The reply gets quieter and nothing
//!   says a check was dropped.
//! * Delete an AUTH key and the opposite happens — findings go UP. 📏 Measured on
//!   `corpus/frameworks/fastapi`: no vocabulary at all gives `mutating-route-no-auth` 132 findings,
//!   declaring `authGuardPattern` ALONE gives 123, and the full template gives 117. A guard rule
//!   reports a route it cannot prove is guarded, so an undeclared vocabulary reads as "no evidence of
//!   innocence" rather than as "nothing to check".
//!
//! Nobody is told either way today, and this block is the telling.
//!
//! # Why this needs no guessing at all
//! Both sides of the comparison are already ours and already machine-checked. The keys an author
//! wrote come from `LoadedRequest::declared_vocabulary`, and the complete set of declarable keys is
//! `config-surface.json`, which `crates/config`'s own tests pin against the Rust request types. So
//! this is a set difference over two committed lists — no heuristic, no inference, and no engine round
//! trip: the config front end has already run by the time `coverage_summary` calls the engine at all,
//! which is what makes this a host-layer block rather than a new wire field on the per-tree output.
//!
//! 🔴 IT READ THE WRONG LIST FOR ONE DAY (2026-09-15, external review round 23, ledger V249). This doc
//! said "the keys an author actually wrote arrive in the MAPPED REQUEST (`trees[].vocabulary`)", and
//! that sentence is false: `mapper::options::build_vocabulary` strips the keys the front end consumes
//! itself — `FRONT_END_ONLY_VOCABULARY_KEYS`, `workspaceSkipDirs` today — because forwarding them
//! would put a key in the request no engine surface reads, which that function refuses by name. So the
//! request is structurally the wrong place to ask this question, and the answer was wrong in the worst
//! possible direction: 📏 a tree whose config `zzop init` had JUST written, with `workspaceSkipDirs`
//! in it, reported `silent: ["workspaceSkipDirs"]`. A block built to report silence reported a
//! declaration as silent — on every tree, on both surfaces.
//!
//! That key is not inert: declaring it changes which trees `trees: "auto"` even discovers, so the
//! reply said "this judgement did not happen" about a judgement that had removed a whole tree.

use serde_json::{json, Value};

/// The legend. Run-invariant by construction — it names no count from THIS run and interpolates
/// nothing, so the numbers stay in the rows and the reading stays here.
pub(crate) const MEANING: &str = "Per tree: which convention-vocabulary keys your config DECLARED \
     (zzop judged by your values) and which it left SILENT. A silent key is not a default — this \
     product applies no built-in convention vocabulary to a config-driven run, so a silent key means \
     the judgement that reads it did not happen. \
     Silence is not symmetric, and the asymmetry is the reason this block exists. For most keys \
     silence makes the reply QUIETER: the check that would have used your value simply does not run, \
     and nothing else marks its absence. For the AUTH keys it makes the reply LOUDER: a guard rule \
     that cannot recognize your project's guard reports every mutating route as unguarded, so an \
     undeclared auth vocabulary reads as \"no evidence of innocence\" rather than as \"nothing to \
     check\". Measured on one corpus tree: declaring nothing gives that rule 132 findings, declaring \
     the auth guard pattern alone gives 123, and the full starter vocabulary gives 117. \
     So read `silent` as a list of questions nobody asked, never as a list of clean answers — and \
     read a silent auth key as findings you are being shown BECAUSE of the silence. The silentAuth \
     field beside this one is that subset, called out because it is the one a reader should act on \
     first. The starter config \
     writes the full vocabulary; keys deleted from it appear here.";

/// The vocabulary keys the config surface declares, read from the committed surface document rather
/// than from a list here — the same document the unknown-key warning and `crates/config`'s own guards
/// read, so this block cannot disagree with them about what is declarable.
fn surface_keys() -> Vec<String> {
    let surface: Value = match serde_json::from_str(zzop_config::CONFIG_SURFACE_JSON) {
        Ok(v) => v,
        // Unreachable in a shipped build (the constant is validated by `crates/config`'s own tests),
        // and a reply path must not abort: an empty surface makes this block report NOTHING rather
        // than report a wrong thing.
        Err(_) => return Vec::new(),
    };
    surface["configKeys"]["vocabulary"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The AUTH family — the keys whose silence makes the reply LOUDER rather than quieter.
///
/// Derived from the surface list by prefix rather than enumerated here, so a new auth key joins the
/// asymmetry the day it is declared. The prefix is the naming convention this surface already uses
/// (`authGuardPattern`, `authFamilyPathPattern`, `authAcquisitionStandalonePattern`, ...), and a key
/// that adopts it is making the same claim about itself.
pub(crate) fn is_auth_key(key: &str) -> bool {
    key.starts_with("auth")
}

/// One row per tree, carrying the reply fields root, declared, silent and silentAuth.
///
/// ⚠ Those four are REPLY field names and are deliberately written without backticks here: in shipped
/// source a backtick-quoted bare word near the word "config" means a CONFIG key, and
/// `rule_contracts::reference_validation` fails the build over one that names no real key. It caught
/// this file.
///
/// `declared` and `silent` PARTITION the surface list, so their lengths always sum to it — a reader
/// can check this block against the config-surface contract document without trusting this code, and
/// [`tests`] asserts the partition rather than the counts.
pub(crate) fn rows(request: &Value, declared: &[Vec<String>]) -> Value {
    let surface = surface_keys();
    let trees = request.get("trees").and_then(Value::as_array);
    let mut out: Vec<Value> = Vec::new();
    for (i, tree) in trees.into_iter().flatten().enumerate() {
        // `declared` is index-aligned with `trees` and comes from the config front end, which is the
        // only layer that saw the author's file. Falling back to the tree's own `vocabulary` would
        // reintroduce exactly the bug in this module's doc, so a missing entry reports an EMPTY author
        // set — every surface key then reads as silent, which is loud and wrong-in-the-safe-direction
        // rather than quiet and wrong.
        let written: Vec<String> = declared.get(i).cloned().unwrap_or_default();
        let declared: Vec<&String> = surface.iter().filter(|k| written.contains(k)).collect();
        let silent: Vec<&String> = surface.iter().filter(|k| !written.contains(k)).collect();
        let silent_auth: Vec<&&String> = silent.iter().filter(|k| is_auth_key(k)).collect();
        out.push(json!({
            "root": tree.get("root").cloned().unwrap_or(Value::Null),
            "declared": declared,
            "silent": silent,
            "silentAuth": silent_auth,
        }));
    }
    Value::Array(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two lists PARTITION the surface — no key in both, none in neither. Asserted as a partition
    /// rather than as counts, so it stays true the day the surface gains a key.
    #[test]
    fn declared_and_silent_partition_the_config_surface() {
        let surface = surface_keys();
        // FLOOR: an empty surface would make every assertion below vacuously true, and the `Err`
        // branch in `surface_keys` returns exactly that.
        assert!(
            surface.len() >= 30,
            "the config surface yielded {} vocabulary keys — this block is reading nothing",
            surface.len()
        );

        let request = json!({"trees": [{"root": "/t"}]});
        let author = vec![vec!["authGuardPattern".to_string(), "skipDirs".to_string()]];
        let rows = rows(&request, &author);
        let row = &rows[0];
        let declared: Vec<&str> = row["declared"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let silent: Vec<&str> = row["silent"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();

        assert_eq!(
            declared.len() + silent.len(),
            surface.len(),
            "declared + silent must be the whole surface: {declared:?} / {silent:?}"
        );
        assert!(declared.contains(&"authGuardPattern") && declared.contains(&"skipDirs"));
        assert!(!silent.contains(&"authGuardPattern"));
        assert!(
            silent.contains(&"moneyTokens"),
            "a key the fixture never wrote must land in `silent`: {silent:?}"
        );
    }

    /// A key the author never wrote but which the SURFACE does not declare either must not appear at
    /// all — this block reports on the declarable surface, never on whatever a config happens to hold.
    #[test]
    fn a_key_outside_the_surface_reaches_neither_list() {
        let request = json!({"trees": [{"root": "/t"}]});
        let rows = rows(&request, &[vec!["notAllowlisted".to_string()]]);
        for list in ["declared", "silent"] {
            assert!(
                !rows[0][list]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == "notAllowlisted"),
                "`notAllowlisted` is not on the config surface and must not be reported as {list}"
            );
        }
    }

    /// The auth subset is DERIVED, and its members are exactly the ones whose silence makes the reply
    /// louder. A tree that declares every auth key must report an EMPTY silentAuth — the negative
    /// half, without which "it listed some auth keys" would pass on a function that listed all of them.
    #[test]
    fn silent_auth_is_the_auth_subset_of_silent_and_empties_when_they_are_declared() {
        let surface = surface_keys();
        let auth: Vec<&String> = surface.iter().filter(|k| is_auth_key(k)).collect();
        assert!(
            auth.len() >= 3,
            "the surface declares {} auth keys — the subset this row is about is missing",
            auth.len()
        );

        let none = rows(&json!({"trees": [{"root": "/t"}]}), &[Vec::new()]);
        assert_eq!(
            none[0]["silentAuth"].as_array().unwrap().len(),
            auth.len(),
            "a config declaring nothing leaves every auth key silent"
        );

        let all_auth: Vec<String> = auth.iter().map(|k| (*k).clone()).collect();
        let full = rows(&json!({"trees": [{"root": "/t"}]}), &[all_auth]);
        assert!(
            full[0]["silentAuth"].as_array().unwrap().is_empty(),
            "declaring every auth key must empty `silentAuth`: {}",
            full[0]["silentAuth"]
        );
    }
}
