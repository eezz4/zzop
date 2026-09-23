//! WHAT FOLDING ONE TEXT COSTS AND SAVES, in the bytes that actually reach the wire.
//!
//! Split out of the fold itself so the arithmetic can be read without the three passes around it,
//! and so [`pointer_sentence`] has ONE home: it is both what a folded finding ends up carrying and
//! what [`net_gain`] prices, and a gate that priced a different sentence than the one that ships
//! would be wrong in a way no test of either half would notice.

use super::{MESSAGE_REF, RULE_MESSAGES_MEANING, RULE_MESSAGE_TEMPLATES_MEANING, TEMPLATE_PARTS};

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

// ---------------------------------------------------------------------------
// THE TEMPLATE FOLD'S ARITHMETIC. Same shape, different terms: a template pays a LIST per finding
// instead of one key, so the pretty lane's per-element newline and indent become a real term rather
// than a rounding error. Kept here beside [`net_gain`] on purpose — two byte models for one wire,
// living in two files, is how one of them stops describing the reply.
// ---------------------------------------------------------------------------

/// The sentence a template-folded finding carries in place of its prose. Shared by the rewrite in
/// [`super::template`] and by [`template_net_gain`], for the reason [`pointer_sentence`] is.
pub(super) fn template_pointer_sentence(key: &str) -> String {
    format!(
        "[templated] this rule's shared text is carried once in this reply at \
         ruleMessageTemplates[\"{key}\"] beside this list — splice `templateParts` into its gaps; \
         see ruleMessageTemplatesMeaning."
    )
}

/// One pretty-printed array of strings: the brackets, and per element a newline, its indent, its
/// serialized value and the comma joining it to the next. `elem_indent` and `close_indent` are
/// zzop's own envelope depths, which is why they are arguments rather than two more literals —
/// the same list is priced at two different depths (a finding's field, and a table row).
///
/// Not defined for an empty list, which cannot occur: a template has at least two parts and
/// therefore at least one residue.
fn array_bytes(values: &[String], elem_indent: i64, close_indent: i64) -> i64 {
    let elems: i64 = values
        .iter()
        .map(|v| 1 + elem_indent + json_bytes(v) + 1)
        .sum();
    // '[' ... last element takes no comma ... newline, indent, ']'
    1 + elems - 1 + 1 + close_indent + 1
}

/// What ONE added `"templateParts": [...]` field costs beyond the list itself — read from
/// [`TEMPLATE_PARTS`] for the reason [`REF_FIELD_BYTES`] is read from [`MESSAGE_REF`].
const TEMPLATE_FIELD_BYTES: i64 = (TEMPLATE_PARTS.len() as i64 + 2) + 2 + 1 + 1 + 8;

/// Whether a middle part of `len` bytes, on a group of `n` findings, costs more than it saves.
///
/// Dropping it merges the residues on either side: each finding loses one array element (its
/// newline, its ten spaces of indent and its comma) and one pair of quotes, and gains the part's own
/// bytes; the table loses that element too. A SHAPE heuristic only — escapes are not modelled here
/// because [`template_net_gain`] re-prices the pruned template exactly.
pub(super) fn template_part_is_dead_weight(len: i64, n: i64) -> bool {
    n * (len - 2 - 12) - (12 + len) < 0
}

/// Bytes this reply SAVES by folding one rule group onto `parts` — negative when the template costs
/// more than the prose it replaces. Excludes [`template_one_time_bytes`], charged once against the
/// sum, exactly as [`one_time_bytes`] is.
///
/// The `"message"` field name, its indent and its comma are on both sides and cancel; only the
/// VALUES differ, plus the whole `templateParts` field, which is new on each folded finding.
pub(super) fn template_net_gain(
    key: &str,
    parts: &[String],
    msgs: &[&str],
    splits: &[Vec<String>],
) -> i64 {
    let pointer = json_bytes(&template_pointer_sentence(key));
    let before: i64 = msgs.iter().map(|m| json_bytes(m)).sum();
    let per_finding: i64 = splits
        .iter()
        .map(|res| pointer + TEMPLATE_FIELD_BYTES + array_bytes(res, 10, 8))
        .sum();
    // The table row: its key, `: `, the `,`, the newline and six spaces of indent, then the list.
    let row = json_bytes(key) + 2 + 1 + 1 + 6 + array_bytes(parts, 8, 6);
    before - per_finding - row
}

/// The cost a reply pays ONCE the moment anything templates, however many rules do: the
/// `"ruleMessageTemplates"` field with its braces, and the whole legend beside it. Charged against
/// the SUM for the reason [`one_time_bytes`] is — per rule, it would conclude that nothing ever pays.
pub(super) fn template_one_time_bytes() -> i64 {
    // `"ruleMessageTemplates": {` — name(22) + `: `(2) + `{`(1) + `,`(1) + newline(1) + indent(4) ...
    22 + 2 + 1 + 1 + 1 + 4
        // ... and its `}` on its own line at the same depth.
        + 1 + 1 + 4
        // `"ruleMessageTemplatesMeaning": "..."` — name(29) + `: `(2) + `,`(1) + newline(1) + indent(4).
        + 29 + 2 + 1 + 1 + 4
        + json_bytes(RULE_MESSAGE_TEMPLATES_MEANING)
}
