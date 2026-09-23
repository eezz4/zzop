// Resolving a folded finding message — the ONE reader every JS consumer of a zzop reply shares.
//
// A shaped findings block (`reply.findings`, `reply.crossLayerFindings`) carries each distinct
// message text exactly once, in a sibling `ruleMessages` object, and a finding whose text lives
// there carries `messageRef` holding its key. The producer is
// `crates/summary/src/output/rule_prose/`; `ruleMessagesMeaning` states the same contract on the
// wire, for a reader who has only the reply.
//
// TWO TABLES, not one. A rule that writes its finding's own subject into its prose (a table name, a
// symbol, a cycle path) emits a different string every time, so keying on the whole message folds
// none of it. Those rules fold on their TEMPLATE instead: the segments every one of that rule's
// findings shares live in a sibling `ruleMessageTemplates`, keyed by the RULE ID, and the bytes that
// differ ride the finding as `templateParts`. Rebuild by interleaving, SEGMENT FIRST, ending on the
// last segment — `templateParts` is always exactly one shorter than the template. Splitting a
// message this way keeps every per-finding fact: the claim that such prose cannot be folded without
// erasing those facts is a property of the whole-message key, not of the prose.
//
// A finding never carries both fields, so the two branches below are exclusive rather than ordered.
//
// Resolve through the FIELD, never by pattern-matching `message`. `VERSIONING.md` freezes field
// names and types and puts "exact ... message wording" explicitly OUTSIDE the compatibility
// surface, so a consumer that scraped the pointer sentence would be built on the half that is free
// to move while ignoring the half that is promised.
//
// Backward compatible in the direction that matters here: a reply or a stored snapshot taken before
// the fold existed has no `messageRef` on any finding and no `ruleMessages` table, so every call
// returns the inline message unchanged.
//
// 🔴 THAT PROMISE HAS ONE EXCEPTION, AND IT IS NOT REPAIRABLE HERE (2026-09-13, ledger V203). A third
// lane landed: a finding carrying `messageBy` does not hold its text at all — the text is reached by
// `ruleId` through `zzop explain`, OUTSIDE the reply. This function cannot return it, because it is not
// there to return.
//
// The damage that caused, measured by an external review before this branch existed: `diff.mjs` uses
// the resolved message as its fallback discriminator, so every by-id finding keyed on the SAME constant
// pointer sentence. Comparing a pre-lane snapshot against a post-lane run reported 149 of 227 anchors as
// GONE + NEW while exiting 0 and printing "LIKE-FOR-LIKE" — precisely the reading the paragraph above
// says this resolver exists to prevent.
//
// So the branch below returns a marker NAMING the rule rather than the pointer text. That makes the
// post-lane side stable and self-describing. What it cannot do is make the two sides AGREE: the pre-lane
// snapshot stores the rule's prose and the post-lane one stores no prose at all, and no function reading
// one reply can turn one into the other. ⇒ A SNAPSHOT TAKEN BEFORE THIS LANE MUST BE RE-TAKEN before it
// is diffed against one taken after. That is a real cost of the lane, it is stated here rather than
// discovered, and it is why the sentence this replaced — "old snapshots therefore need no re-taking" —
// is no longer true without qualification.

/**
 * @param {object} block  the shaped findings block (the object holding `shown` and `ruleMessages`)
 * @param {object} finding one entry of that block's `shown`
 * @returns {string} the finding's full message text
 */
export function resolveMessage(block, finding) {
  if (!finding || typeof finding !== "object") return "";
  // The by-id lane, checked FIRST: its findings carry no prose to splice or look up, and the pointer
  // sentence is a constant, so returning `finding.message` would hand every caller the same string for
  // every rule. The rule id is the whole of the information the reply actually holds.
  if (typeof finding.messageBy === "string") {
    const id = typeof finding.ruleId === "string" ? finding.ruleId : "<unknown rule>";
    return `<by-${finding.messageBy} ${id} — text is not in this reply; run \`zzop explain ${id}\`>`;
  }
  if (Array.isArray(finding.templateParts)) return spliceTemplate(block, finding);
  const ref = finding.messageRef;
  if (typeof ref !== "string") return typeof finding.message === "string" ? finding.message : "";
  const table = (block && block.ruleMessages) || {};
  if (Object.prototype.hasOwnProperty.call(table, ref) && typeof table[ref] === "string") {
    return table[ref];
  }
  // A dangling ref is a producer bug, and staying silent about it would turn this into the very
  // failure the fold exists to avoid — prose that vanished with nobody told. Return something that
  // reads as wrong rather than something that reads as empty.
  return `<unresolved messageRef ${JSON.stringify(ref)} — not in this reply's ruleMessages>`;
}

/**
 * The template branch. Looks the segments up by the finding's OWN `ruleId` — there is no separate
 * key field, because a finding already names its rule and one template serves a rule.
 *
 * @param {object} block  the shaped findings block (the object holding `shown` and `ruleMessageTemplates`)
 * @param {object} finding one entry of that block's `shown`, carrying `templateParts`
 * @returns {string} the finding's full message text
 */
function spliceTemplate(block, finding) {
  const rule = finding.ruleId;
  const table = (block && block.ruleMessageTemplates) || {};
  const segments = typeof rule === "string" ? table[rule] : undefined;
  const parts = finding.templateParts;
  // Same policy as a dangling `messageRef`, for the same reason: a producer bug must read as wrong,
  // never as empty and never as a half-sentence a reader would act on. The length relation is
  // checked rather than assumed — a template one segment short would otherwise silently drop the
  // last residue, which is exactly the "a fact vanished with nobody told" failure the fold exists
  // to avoid.
  if (!Array.isArray(segments) || segments.length !== parts.length + 1) {
    return `<unresolved templateParts for rule ${JSON.stringify(rule)} — no matching template in this reply's ruleMessageTemplates>`;
  }
  let out = String(segments[0]);
  for (let i = 0; i < parts.length; i++) out += String(parts[i]) + String(segments[i + 1]);
  return out;
}
