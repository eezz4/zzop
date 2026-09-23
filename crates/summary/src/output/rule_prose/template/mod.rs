//! THE TEMPLATE FOLD — the second half of the prose fold, for the rules whose prose the first half
//! cannot touch.
//!
//! # The waste the exact fold leaves behind
//! [`super::fold`] keys on the WHOLE message, so one interpolated identifier defeats it: a rule that
//! writes the table name, the symbol or the cycle path into its prescription emits a different string
//! for every finding and folds nothing, even though the sentences around that identifier are the same
//! sentences every time. Measured on cal.com (`zzop analyze --config zzop.B-cal.com.jsonc
//! --limit 1000`, this repo's dogfood corpus, run against `d887d6b` and against this module): 520 of
//! that reply's 1,000 findings were still carrying prose inline; 509 of them fold under 13 rule
//! templates, and the reply goes from 1,705,102 to 993,947 bytes — -41.7%, with all 1,000 messages
//! still rebuilding BYTE-IDENTICALLY, and `total`/`bySeverity`/`byRule`/the anchor sequence
//! unmoved.
//!
//! # Where this lever does NOT help, said out loud
//! The same tree at the DEFAULT `--limit 50` is byte-identical before and after. Fifty rows spread
//! over thirty-three rules leave exactly one template candidate, worth -5 bytes, and the gate below
//! correctly declines it. This is a lever for the LARGE window a caller asks for explicitly, and a
//! reader who measured only the default view would reasonably conclude this module does nothing.
//!
//! # Why this does NOT erase per-finding facts
//! Splitting the message into an INVARIANT part and a per-finding RESIDUE keeps both. The template
//! holds only bytes every message of that rule shares; everything that differs stays on the finding,
//! in order, in [`super::TEMPLATE_PARTS`]. Interleaving the two reproduces the original message
//! byte-for-byte — which is asserted on every finding at fold time (see [`fold`]), not only in tests,
//! because a fold that silently dropped a residue would be the exact failure this whole requirement
//! retires itself over.
//!
//! # Why the key is the RULE ID with no `#N` and no ref field
//! A finding already carries `ruleId`, so a per-finding pointer field would spell a second time what
//! the reply already says. The cost of that reach is that ONE template serves a rule: a rule whose
//! messages do not all fit one template folds nothing here, rather than growing a key namespace and a
//! ref field to carry the exception. It also sidesteps the `#N` collision the exact fold discloses —
//! there is no `#` in these keys at all.
//!
//! # Why the segmentation is derived per reply and not declared per rule
//! Nothing here knows what a rule's format string looked like. The template is MINED from the texts
//! this reply actually carries, by aligning each message against the first one, so a rule whose prose
//! changes needs no registration and cannot go stale. The alignment is a heuristic — it proposes
//! where the seams are — and the exact split plus the byte-identity check are what make the result
//! correct regardless of what the heuristic proposed.
//!
//! # Two properties of the mining, disclosed rather than assumed
//! It ANCHORS on the group's first finding, so an unusual first message yields a thinner template
//! than another anchor would have. Deterministic (the caller has already sorted `shown`) and never
//! wrong — a thin template just saves fewer bytes, or none, and the gate declines it. And it is
//! ALL-OR-NOTHING per rule: one message that does not fit costs the whole group. Both leave bytes on
//! the table and neither can cost a fact, which is why neither is repaired here — the repair is
//! clustering, plus a second key namespace to address the clusters with, and the measured saving
//! does not ask for it yet.

mod mine;

use super::{cost, TEMPLATE_PARTS};
use mine::{derive_parts, prune};

/// Splits `m` into the residues between `parts`, leftmost-first. `None` when `m` does not fit.
///
/// Leftmost is complete here, not merely a guess: taking each part at its earliest possible position
/// leaves the most room for the rest, so a set of positions exists if and only if this finds one.
pub(super) fn split(m: &str, parts: &[String]) -> Option<Vec<String>> {
    let k = parts.len() - 1;
    if !m.starts_with(parts[0].as_str()) {
        return None;
    }
    let mut pos = parts[0].len();
    let mut res = Vec::with_capacity(k);
    for part in &parts[1..k] {
        let idx = m.get(pos..)?.find(part.as_str())? + pos;
        res.push(m.get(pos..idx)?.to_string());
        pos = idx + part.len();
    }
    let last = &parts[k];
    if !m.ends_with(last.as_str()) {
        return None;
    }
    let idx = m.len().checked_sub(last.len())?;
    if idx < pos {
        return None;
    }
    res.push(m.get(pos..idx)?.to_string());
    Some(res)
}

/// The inverse of [`split`]: parts and residues interleaved, part first. This is the whole wire
/// contract in one function, and the one every consumer implements.
pub(super) fn reconstruct(parts: &[String], res: &[String]) -> String {
    let mut out = parts[0].clone();
    for (i, r) in res.iter().enumerate() {
        out.push_str(r);
        out.push_str(&parts[i + 1]);
    }
    out
}

/// Folds every rule group whose messages share a template AND whose folding makes the reply smaller.
///
/// Runs over what [`super::fold`] left INLINE: a finding already pointing at `ruleMessages` is done,
/// and re-folding it would give one finding two contradictory addresses for its own text.
///
/// Mutates `shown` in place and returns the `ruleMessageTemplates` table, or `None` when no rule
/// group templated or the ones that did are not worth the bytes.
pub(super) fn fold(shown: &mut [serde_json::Value]) -> Option<serde_json::Value> {
    // Pass 1 — group by rule in FIRST-APPEARANCE order, so the table is byte-deterministic over a
    // `shown` the caller has already sorted by its three published ordering keys.
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<usize>> = Default::default();
    for (i, f) in shown.iter().enumerate() {
        if f.get(super::MESSAGE_REF).is_some() {
            continue;
        }
        let (Some(rule), Some(_)) = (
            f.get("ruleId").and_then(|v| v.as_str()),
            f.get("message").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        groups
            .entry(rule.to_string())
            .or_insert_with(|| {
                order.push(rule.to_string());
                Vec::new()
            })
            .push(i);
    }

    // Pass 2 — mine, verify, price. The byte-identity assertion is INSIDE the accept path: a group
    // whose reconstruction differs by one byte is not folded at all, so the lossless guarantee holds
    // for every reply this ships and not only for the replies a test thought to build.
    let mut accepted: Vec<Mined> = Vec::new();
    let mut saved: i64 = 0;
    for rule in &order {
        let idxs = &groups[rule];
        if idxs.len() < 2 {
            continue;
        }
        let msgs: Vec<&str> = idxs
            .iter()
            .map(|&i| shown[i]["message"].as_str().unwrap_or_default())
            .collect();
        let Some(parts) = derive_parts(&msgs).and_then(|p| prune(p, msgs.len())) else {
            continue;
        };
        let splits: Option<Vec<Vec<String>>> = msgs
            .iter()
            .map(|m| split(m, &parts).filter(|r| reconstruct(&parts, r) == **m))
            .collect();
        let Some(splits) = splits else { continue };
        let gain = cost::template_net_gain(rule, &parts, &msgs, &splits);
        if gain > 0 {
            saved += gain;
            accepted.push(Mined {
                rule: rule.clone(),
                parts,
                splits,
                at: idxs.clone(),
            });
        }
    }
    // Strictly greater, for the same reason the exact fold uses `>`: a byte-neutral fold buys a table
    // and a legend for nothing.
    if saved <= cost::template_one_time_bytes() {
        return None;
    }

    // Pass 3 — rewrite. The sentence names the key too, because the reader who hits this in a
    // terminal is a person; `templateParts` beside it is what every program reads.
    let strings = |v: &[String]| {
        serde_json::Value::Array(v.iter().cloned().map(serde_json::Value::String).collect())
    };
    let mut table = serde_json::Map::new();
    for m in accepted {
        table.insert(m.rule.clone(), strings(&m.parts));
        for (slot, &i) in m.at.iter().enumerate() {
            shown[i]["message"] =
                serde_json::Value::String(cost::template_pointer_sentence(&m.rule));
            shown[i][TEMPLATE_PARTS] = strings(&m.splits[slot]);
        }
    }
    Some(serde_json::Value::Object(table))
}

/// One rule group that mined a template and cleared the gate: what to store, what each of its
/// findings keeps, and where those findings sit in `shown`. A named struct rather than a 4-tuple
/// because two of its fields are `Vec<String>` and swapping them would compile.
struct Mined {
    rule: String,
    /// The shared literal segments — one more of these than there are residues.
    parts: Vec<String>,
    /// Per finding, in the order of `at`: the bytes that differ.
    splits: Vec<Vec<String>>,
    /// Indices into `shown`.
    at: Vec<usize>,
}
