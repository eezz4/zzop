//! WHAT FOLDING ONE TEXT COSTS AND SAVES, in the bytes that actually reach the wire.
//!
//! Split out of the fold itself so the arithmetic can be read without the three passes around it,
//! and so [`pointer_sentence`] has ONE home: it is both what a folded finding ends up carrying and
//! what [`net_gain`] prices, and a gate that priced a different sentence than the one that ships
//! would be wrong in a way no test of either half would notice.

use super::{MESSAGE_REF, RULE_MESSAGES_MEANING};

/// The sentence a folded finding carries in place of its prose. Shared by the rewrite in [`super`]
/// AND by [`net_gain`] on purpose: the gate's arithmetic is only honest while it measures the string
/// that actually ships, and two copies of this format would drift the moment one was reworded.
pub(super) fn pointer_sentence(key: &str) -> String {
    format!(
        "[folded] this rule's full text is carried once in this reply at ruleMessages[\"{key}\"] \
         beside this list — resolve via `messageRef`; see ruleMessagesMeaning."
    )
}

/// Serialized length of `s` as a JSON string, quotes and escapes included — what the wire actually
/// spends on it, not `str::len`.
fn json_bytes(s: &str) -> i64 {
    serde_json::to_string(s).map_or(s.len() as i64 + 2, |v| v.len() as i64)
}

/// What ONE added `"messageRef": "..."` field costs beyond its own value: the quoted field name
/// (`MESSAGE_REF` plus its two quotes), `: ` (2), the `,` joining it to the next field (1), and the
/// newline plus eight spaces of indent the PRETTY lane charges for one more field four levels deep —
/// root > `findings` or `crossLayerFindings` > `shown` > element, the same depth on both lanes.
///
/// The name term is READ from [`MESSAGE_REF`], not spelled `12`. Only `2 + 1 + 1` of this sum is
/// fixed by the serializer; the name is zzop's own wire spelling and the indent is zzop's own
/// envelope depth, and `VERSIONING.md` permits renaming a wire field on a version bump. A literal
/// `12` therefore priced a field this repo is free to rename, with nothing red when it did: the
/// gate would keep folding on arithmetic that no longer describes the reply. Same crate, same
/// module, so the repair is the symbol rather than a pin. The `+ 2` is the JSON quotes, which is
/// exact rather than `json_bytes` because a field name needing an escape would be a wire-contract
/// change long before it were a byte-accounting one.
///
/// NOT the same for `"ruleMessages"` / `"ruleMessagesMeaning"` in [`one_time_bytes`] below: those two
/// names are bare literals at their emit site (`super::super::mod.rs`) with no constant to read, so
/// their lengths are still hand-copied here. Disclosed rather than silently uneven — closing it means
/// giving those two field names an owner first.
const REF_FIELD_BYTES: i64 = (MESSAGE_REF.len() as i64 + 2) + 2 + 1 + 1 + 8;

/// The same accounting for one `ruleMessages` row, which sits one level shallower (six spaces):
/// `: ` (2), the `,` (1), the newline (1) and the indent (6). Its key and text are counted
/// separately because they vary per row. No name term here — a row's key IS its data — which is why
/// this one needs no symbol the way [`REF_FIELD_BYTES`] does; the indent is still zzop's own
/// envelope depth rather than anything the serializer fixed.
const TABLE_ROW_BYTES: i64 = 2 + 1 + 1 + 6;

/// The cost a reply pays ONCE the moment anything folds, however many texts fold: the
/// `"ruleMessages"` field with its braces, and the whole `"ruleMessagesMeaning"` legend. This is
/// the one term that cannot be charged per rule, and charging it per rule is precisely how a
/// per-candidate gate would conclude that nothing is ever worth folding.
pub(super) fn one_time_bytes() -> i64 {
    // `"ruleMessages": {` — name(14) + `: `(2) + `{`(1) + `,`(1) + newline(1) + indent(4) ...
    14 + 2 + 1 + 1 + 1 + 4
        // ... and its `}` on its own line at the same depth.
        + 1 + 1 + 4
        // `"ruleMessagesMeaning": "..."` — name(21) + `: `(2) + `,`(1) + newline(1) + indent(4).
        + 21 + 2 + 1 + 1 + 4
        + json_bytes(RULE_MESSAGES_MEANING)
}

/// Bytes this reply SAVES by folding one text carried by `n` of its findings under `key` — negative
/// when folding it would cost more than it removes. Excludes [`one_time_bytes`], which the caller
/// charges once against the sum.
///
/// The `"message"` field name, its indent and its comma are on both sides of the trade and cancel;
/// only the VALUES differ, which is why they are the only per-finding term here.
pub(super) fn net_gain(n: usize, text: &str, key: &str) -> i64 {
    let n = n as i64;
    let text = json_bytes(text);
    n * text
        - n * json_bytes(&pointer_sentence(key))
        - n * (REF_FIELD_BYTES + json_bytes(key))
        - (json_bytes(key) + text + TABLE_ROW_BYTES)
}
