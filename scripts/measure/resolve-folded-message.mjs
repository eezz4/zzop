// Resolving a folded finding message — the ONE reader every JS consumer of a zzop reply shares.
//
// A shaped findings block (`reply.findings`, `reply.crossLayerFindings`) carries each distinct
// message text exactly once, in a sibling `ruleMessages` object, and a finding whose text lives
// there carries `messageRef` holding its key. The producer is
// `crates/summary/src/output/rule_prose.rs`; `ruleMessagesMeaning` states the same contract on the
// wire, for a reader who has only the reply.
//
// Resolve through the FIELD, never by pattern-matching `message`. `VERSIONING.md` freezes field
// names and types and puts "exact ... message wording" explicitly OUTSIDE the compatibility
// surface, so a consumer that scraped the pointer sentence would be built on the half that is free
// to move while ignoring the half that is promised.
//
// Backward compatible in the direction that matters here: a reply or a stored snapshot taken before
// the fold existed has no `messageRef` on any finding and no `ruleMessages` table, so every call
// returns the inline message unchanged. Old snapshots therefore need no re-taking, which is the same
// promise `diff.mjs` already makes about its anchor key.

/**
 * @param {object} block  the shaped findings block (the object holding `shown` and `ruleMessages`)
 * @param {object} finding one entry of that block's `shown`
 * @returns {string} the finding's full message text
 */
export function resolveMessage(block, finding) {
  if (!finding || typeof finding !== "object") return "";
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
