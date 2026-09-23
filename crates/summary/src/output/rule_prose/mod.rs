//! THE PER-REPLY PROSE FOLD — one copy of each repeated message text WORTH storing once, with every
//! finding that shares it pointing at that copy from inside the same document.
//!
//! Four files: this one holds the contract (the fields, the legends) and the EXACT fold itself,
//! [`template`] holds the second half — the fold for the rules whose prose carries a per-finding
//! value, which is what keying on the whole message structurally cannot reach — [`cost`] holds the
//! byte arithmetic that decides what folds in either half, and `tests` holds the pins.
//!
//! The two halves run in that order and only that order ([`fold`]), and a finding takes an address
//! from exactly one of them.
//!
//! # The waste this removes
//! A rule's `message` is its prescription, its disqualifying clause and its landing. That prose is
//! the product, so cutting it is the one repair forbidden here. But most rules emit the SAME text
//! for every finding they produce, and the reply used to carry a full copy per finding. Measured on
//! cal.com (2026-08-31, `zzop analyze --config zzop.B-cal.com.jsonc --limit 1000` against this
//! repo's dogfood corpus): 2,659,610 of the reply's 3,198,035 bytes were `shown[].message`, and 30
//! of the 54 firing rules emitted a byte-identical message for every one of their findings —
//! 1,000 stored texts standing for 551 distinct ones.
//!
//! # Why a fold and not a diet
//! Nothing here shortens, drops or rewrites a sentence, and that is structural rather than polite:
//! the requirement this serves retires itself the moment shrinking the wire removes an honesty the
//! reader needed, so the only shrink that cannot trip that condition is one that deletes no bytes.
//! The reply after the fold contains every byte it contained before — once instead of N times,
//! resolvable with no second request. The pins in `super::tests` are deliberately opposed for the
//! same reason: "the reply never grows" alone is satisfiable by DELETING prose, "every text
//! reconstructible byte-identically" alone is satisfiable by shipping three copies, and only the
//! pair forbids both.
//!
//! # Why the key rides in a FIELD and not in the sentence
//! A folded finding carries [`MESSAGE_REF`] — the key, as data. Consumers resolve by reading that
//! field and indexing `ruleMessages`; nothing anywhere parses the human sentence. That line is what
//! `VERSIONING.md` already draws for us: field names and types are frozen, and "exact ... message
//! wording" is explicitly outside the compatibility surface. A resolver that scraped the sentence
//! would have inverted that — it would have made the wording load-bearing in two languages at once
//! (the repo's JS measurement harness reads this reply too) and left the frozen half doing nothing.
//!
//! # Why the pointer text is relative
//! The sentence names `ruleMessages` as a SIBLING of the findings list it sits in, not an absolute
//! path. The lanes do not share a block name — `findings.shown` (analyze / analyze-envelope),
//! `crossLayerFindings.shown` (cross), `findings.list` (the file query) — so an absolute path would
//! be right on one lane and a lie on the rest, and a shaper that must be TOLD which lane it feeds
//! grows a parameter whose only job is to be spelled correctly once per lane.
//!
//! 🔵 That relativity is why the file query could adopt this fold (2026-09-13, ledger V223) by
//! calling [`fold`] on its own list and publishing beside it — no new parameter, no second pointer
//! wording. The lane list above is prose and will go stale; `fold`'s callers are the truth.
//!
//! # Why the gate is NET BYTES and not "did this repeat?"
//! Repetition is the opportunity, not the payoff. Folding a text pays a pointer sentence (~175
//! bytes, once per finding), a `messageRef` field (once per finding), a table row, and a legend
//! (once per reply) — so a text short enough is cheaper carried inline N times, and a gate that
//! asked only "n > 1" ran the fold BACKWARDS on exactly the small and rule-filtered replies a
//! reader reaches for first. Measured against the shipped catalog: 45 of 118 rule messages sit
//! below the n=2 break-even and 44 of those interpolate nothing, so they emit byte-identically
//! every time and would have folded ALWAYS. The reasoning was already in this file — it is the
//! paragraph below about singletons — and only the threshold was wrong: a singleton is the n=1 case
//! of an arithmetic that does not stop being arithmetic at n=2.
//!
//! [`net_gain`] answers the question per candidate, and the fold is PARTIAL: every text that pays
//! folds and every text that does not stays inline, in the same reply. The legend is the one cost
//! that cannot be charged per rule — it rides once however many texts fold — so it is charged once,
//! against the SUM of the accepted gains, after the per-candidate selection. Selecting every
//! positive-gain candidate and then testing the sum against the one-time cost is exactly optimal
//! here, because the gains are independent and the legend is paid if and only if the set is
//! non-empty: no other subset can beat it.
//!
//! # Why singletons are left alone
//! A text carried by exactly one finding is not duplicated, so folding it would ADD bytes: a table
//! entry and a pointer where one inline string stood. It is the same [`net_gain`] arithmetic, which
//! is negative for every possible n=1, and it is also why a tree whose every message is unique
//! grows no table rather than an empty one.
//!
//! # What the `#N` key convention assumes
//! Keys are `<ruleId>` and `<ruleId>#2`, `#3`, ... — which assumes no rule id CONTAINS `#`. Nothing
//! in the schema enforces that, so a third-party pack shipping a rule literally named `foo#2` could
//! collide with the second text of a rule named `foo`. Left as is deliberately: zero of the 118
//! shipped rule ids contain `#`, and the repair (escaping, or a key namespace) costs a wire
//! convention readers would then have to decode. Disclosed rather than silently assumed.

/// The per-finding field carrying the `ruleMessages` key of a folded message. Absent on a finding
/// whose message is inline — its presence IS the "this is folded" signal, so a consumer never has to
/// pattern-match prose to know which it is holding.
pub(super) const MESSAGE_REF: &str = "messageRef";

/// [`MESSAGE_REF`] for the cross-language seam pin in `wire_contract_tests`, so the JS side's
/// spelling is checked against this constant instead of against a second hand-typed literal.
#[cfg(test)]
pub(crate) fn message_ref_key() -> &'static str {
    MESSAGE_REF
}

/// The legend for `ruleMessages`, written to the same contract as `byRuleMeaning` and
/// `packsLoadedMeaning`: a machine field ships beside a sentence saying what it is. It rides ONCE
/// per reply, which is what lets the per-finding pointer stay a pointer instead of restating this
/// explanation on every folded finding.
pub(super) const RULE_MESSAGES_MEANING: &str =
    "A message text that more than one finding in this reply carries is stored here exactly once, \
     keyed by the rule id that produced it (a rule storing more than one distinct text gets \
     `<ruleId>#2`, `#3`, ... for its later ones). A finding whose text lives here carries \
     `messageRef` holding its key: resolve it against the `ruleMessages` object sitting BESIDE the \
     findings list that finding came from — a sibling, not a fixed path, because the same fold is \
     applied to every findings list this tool ships, whatever its block is called. Read \
     `finding.message` directly when \
     `messageRef` is absent. A repeated text can also be absent from here: folding is applied only \
     where it removes more bytes than the pointer, the `messageRef` field and this table cost to \
     add, so a repeated message left inline is a size decision and never a sign that anything is \
     missing. The stored text is byte-identical to what that finding used to carry inline — no \
     sentence was shortened, dropped or reworded, and every byte is reachable without a second \
     request. This is deduplication within one reply, never truncation: `truncated` remains the \
     only key that ever means something was left out.";

/// The per-finding field carrying the residues of a TEMPLATE-folded message: the bytes that differ
/// between this rule's findings, in order, to be spliced into the gaps of
/// `ruleMessageTemplates[ruleId]`. Absent on any other finding — its presence IS the signal, the same
/// contract [`MESSAGE_REF`] keeps, and no finding ever carries both.
pub(super) const TEMPLATE_PARTS: &str = "templateParts";

/// [`TEMPLATE_PARTS`] for the cross-language seam pin in `wire_contract_tests`, so the JS side's
/// spelling is checked against this constant instead of against a second hand-typed literal.
#[cfg(test)]
pub(crate) fn template_parts_key() -> &'static str {
    TEMPLATE_PARTS
}

/// The legend for `ruleMessageTemplates`, written to the same contract as [`RULE_MESSAGES_MEANING`].
/// It states the splice rule in full because that rule IS the wire format: a reader holding only
/// this reply must be able to rebuild every message from it without knowing what a zzop rule is.
pub(super) const RULE_MESSAGE_TEMPLATES_MEANING: &str =
    "Rules that write a finding's own subject into their prose — a table name, a symbol, a cycle \
     path — emit a different message every time, so the `ruleMessages` table above cannot hold \
     them. What those messages SHARE is stored here instead, keyed by rule id: an array of the \
     literal text segments common to every one of that rule's findings in this reply. A finding \
     whose text is stored this way carries `templateParts`, an array of the bytes that differ. \
     Rebuild the message by interleaving the two, SEGMENT FIRST: \
     `segments[0] + parts[0] + segments[1] + parts[1] + ...`, ending on the last segment \
     (`parts` is always exactly one shorter than `segments`). A segment may be an empty string at \
     either end, which is how a message that begins or ends with per-finding text is carried. Look \
     the template up by the finding's own `ruleId`; there is no separate key field and no `#N` \
     suffix, because one template serves a rule and a rule whose messages do not all fit one is \
     left inline instead. Read `finding.message` directly when neither `templateParts` nor \
     `messageRef` is present. The rebuilt text is byte-identical to what that finding used to carry \
     inline — no sentence was shortened, dropped or reworded, and every byte is reachable without a \
     second request. This is deduplication within one reply, never truncation: `truncated` remains \
     the only key that ever means something was left out.";

mod by_id;
// Private on purpose: nothing outside this module needs the by-id lane's names. Only these two are
// reached from shipped code here — the pass, and the legend `publish` pairs with its marker. The
// lane's own wire spellings are used inside `by_id` and by the tests, which name them by path.
use by_id::{point_at_rule_id, MESSAGE_BY_ID_MEANING};
mod cost;
mod template;
#[cfg(test)]
mod tests;

use cost::{net_gain, one_time_bytes, pointer_sentence};

/// What one shaped findings block gained from the fold: the two sibling tables it must publish, each
/// absent when nothing of its kind was worth folding. A struct rather than a tuple because the caller
/// emits them under different names and a swapped pair would be silently wrong.
pub(crate) struct Folded {
    messages: Option<serde_json::Value>,
    templates: Option<serde_json::Value>,
    /// Whether any shown finding arrived carrying the by-id pointer. Not produced by this module —
    /// `zzop-facade` wrote it before the shaper ever saw the finding — but published here because this is
    /// where a `message` and the legend explaining it are paired, and a third marker landing without
    /// its legend is the failure [`Folded::publish`] exists to make impossible.
    by_id: bool,
}

impl Folded {
    /// Writes whichever tables fired onto a shaped block, each beside its own legend.
    ///
    /// Here rather than at the call site so the four wire names are spelled ONCE, in the module that
    /// owns them, and so adding a third table cannot land with its legend left behind: the pairing is
    /// this function's whole body.
    pub(crate) fn publish(self, out: &mut serde_json::Value) {
        if let Some(table) = self.messages {
            out["ruleMessages"] = table;
            out["ruleMessagesMeaning"] = RULE_MESSAGES_MEANING.into();
        }
        if let Some(table) = self.templates {
            out["ruleMessageTemplates"] = table;
            out["ruleMessageTemplatesMeaning"] = RULE_MESSAGE_TEMPLATES_MEANING.into();
        }
        if self.by_id {
            out["messageByIdMeaning"] = MESSAGE_BY_ID_MEANING.into();
        }
    }
}

/// Folds every message text that more than one finding in `shown` carries AND that folding makes
/// the reply smaller — see the module header for why those are two conditions and not one.
///
/// Mutates `shown` in place — a folded text is replaced by a short pointer sentence and the finding
/// gains [`MESSAGE_REF`] — and returns the `ruleMessages` table, or `None` when no text repeated or
/// when the repeats that did are not worth the bytes.
///
/// Deterministic in the way byte-identical output needs: keys are assigned in the order the texts
/// first appear in `shown`, which the caller has already sorted by its three published ordering keys
/// before calling here.
/// Both halves of the fold, in the only order they compose: whole texts first, then TEMPLATES over
/// what is still inline. Reversed, a template would swallow a rule's identical repeats under the
/// costlier of the two encodings, and a finding could end up holding two addresses for one text.
pub(crate) fn fold(shown: &mut [serde_json::Value]) -> Folded {
    // FIRST, on the prose the facade handed over: the two folds below price `message`, and pricing a
    // pointer they were about to replace would be the wrong arithmetic on the wrong text.
    let by_id = point_at_rule_id(shown);
    let messages = fold_exact(shown);
    let templates = template::fold(shown);
    Folded {
        messages,
        templates,
        by_id,
    }
}

fn fold_exact(shown: &mut [serde_json::Value]) -> Option<serde_json::Value> {
    let text_of = |f: &serde_json::Value| -> Option<(String, String)> {
        Some((
            f.get("ruleId").and_then(|v| v.as_str())?.to_string(),
            f.get("message").and_then(|v| v.as_str())?.to_string(),
        ))
    };

    // Pass 1 — multiplicity of each (rule, text) pair. The rule id is part of the identity because
    // the key is derived from it: two rules that happened to emit the same sentence would otherwise
    // share one key named after only one of them, and the table would assert something false about
    // which rule said it.
    let mut counts: std::collections::HashMap<(String, String), usize> = Default::default();
    for f in shown.iter() {
        if let Some(id) = text_of(f) {
            *counts.entry(id).or_default() += 1;
        }
    }
    // Pass 2 — THE BYTE GATE, per candidate, in first-appearance order. `counts` was taken over
    // `shown`, which the caller has already cut to the cap, so the population being COUNTED and the
    // population being MEASURED are the same one; counting the full set and pricing the window
    // would have folded prose this reply does not contain.
    //
    // The provisional key here is the one the candidate would take if every candidate before it
    // were accepted, so a rejection can only make the final key SHORTER (`r#3` -> `r#2` -> `r`).
    // That makes the estimate a lower bound on the realized saving, which is the safe direction:
    // the gate never folds something it priced as a win and then loses.
    let mut ordinal: std::collections::HashMap<String, usize> = Default::default();
    let mut seen: std::collections::HashSet<(String, String)> = Default::default();
    let mut kept: Vec<(String, String)> = Vec::new();
    let mut saved: i64 = 0;
    for f in shown.iter() {
        let Some(ident) = text_of(f) else { continue };
        let n = counts.get(&ident).copied().unwrap_or(0);
        if n < 2 || !seen.insert(ident.clone()) {
            continue;
        }
        let slot = ordinal.entry(ident.0.clone()).or_insert(0);
        *slot += 1;
        let provisional = if *slot == 1 {
            ident.0.clone()
        } else {
            format!("{rule}#{slot}", rule = ident.0, slot = *slot)
        };
        let gain = net_gain(n, &ident.1, &provisional);
        if gain > 0 {
            saved += gain;
            kept.push(ident);
        }
    }
    // Strictly greater, not `>=`: a byte-neutral fold buys a table and a legend for nothing, and the
    // reply without them is the simpler of two documents that cost the same.
    if saved <= one_time_bytes() {
        return None;
    }

    // Pass 3 — final keys, dense over the ACCEPTED set only, so the table never shows a `#2` with no
    // sibling above it. Same first-appearance order, so this stays byte-deterministic.
    let mut ordinal: std::collections::HashMap<String, usize> = Default::default();
    let mut key_of: std::collections::HashMap<(String, String), String> = Default::default();
    let mut table = serde_json::Map::new();
    for ident in kept {
        let slot = ordinal.entry(ident.0.clone()).or_insert(0);
        *slot += 1;
        let key = if *slot == 1 {
            ident.0.clone()
        } else {
            format!("{rule}#{slot}", rule = ident.0, slot = *slot)
        };
        table.insert(key.clone(), serde_json::Value::String(ident.1.clone()));
        key_of.insert(ident, key);
    }

    // Pass 4 — rewrite. The sentence names the key too, because the reader who hits this in a
    // terminal is a person, and `messageRef` beside it is what every program reads.
    for f in shown.iter_mut() {
        let Some(ident) = text_of(f) else { continue };
        if let Some(key) = key_of.get(&ident) {
            f["message"] = serde_json::Value::String(pointer_sentence(key));
            f[MESSAGE_REF] = serde_json::Value::String(key.clone());
        }
    }

    Some(serde_json::Value::Object(table))
}
