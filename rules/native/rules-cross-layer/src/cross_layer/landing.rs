//! The LANDING clauses shared by the route/path rules — the sentence each of them puts AHEAD of its own
//! imperative because the prescription is the risk (rule-quality.md §27), and BEHIND its own disqualifier
//! because whether the finding is TRUE is a different question from what acting on it costs (§38).
//!
//! ## Why this module exists at all, and why it holds THREE sentences rather than one
//!
//! Six rules here prescribe an edit on the SERVING side of a route — `route-near-miss`, `prefix-drift`,
//! `method-mismatch`, `duplicate-route`, `ambiguous-consume` and `route-shadowing` — and at the altitude
//! of "callers break" they look like one cost. They are not. Applying `eec4eea`'s test (is that constant's
//! NOUN the same as this rule's failure) splits them by what a reader has to go and VERIFY:
//!
//! | noun | what the reader must do | who carries it |
//! |---|---|---|
//! | [`CALLER_BREAKAGE_LANDING`] — the address MOVES | enumerate who holds the old address | `route-near-miss`, `prefix-drift`, `ambiguous-consume`, `duplicate-route` (separate branch), `route-shadowing` (separate branch) |
//! | [`HANDLER_SUBSTITUTION_LANDING`] — the address STAYS and a different handler answers | diff the two handlers | `duplicate-route` (merge branch), `route-shadowing` (gateway-order branch) |
//! | [`METHOD_CHANGE_LANDING`] — the VERB changes | price what the current verb buys, and move the parameters | `method-mismatch` |
//!
//! The split is an axis rather than a degree, and the cell that proves it is the middle one: nothing 404s
//! there, so a reader handed the first sentence goes looking for a failure that never arrives and reads its
//! absence as "the merge was safe". That is exactly the "nearest sibling is the most dangerous reuse" trap
//! rule-quality.md §37 records, and this crate has already paid it once —
//! `retrying_write_no_idempotency::IDEMPOTENCY_ROLLOUT_LANDING` and
//! `sensitive_response_field::message::REMOVAL_LANDING` are both "the edit changes a contract other parties
//! depend on" and are deliberately NOT one constant either.
//!
//! ⚠ [`VENDOR_CONTRACT_LANDING`] is in this file for filing, not because it is a fourth member of that
//! family: its subject is an EXTERNAL host, so the contract being edited is not the reader's to change.
//! Do not splice it into an internal-route rule, and do not splice the three above into an egress rule.
//!
//! Every constant here is field- and route-independent on purpose — names belong in the imperative, so the
//! position pins have ONE spelling to compare an index against.

/// The cost of moving a route's ADDRESS. Spliced byte-identically into the five rules whose serving-side
/// repair changes what the URL is, AHEAD of each rule's imperative (rule-quality.md §27).
///
/// ONE constant for five rules because the dangerous property belongs to HTTP rather than to what any of
/// them detects: an address that stops being served stops answering, and an HTTP call is not type-checked
/// against the route it names, so nothing anywhere fails to build. The five diverge only in the imperative,
/// which is per-rule anyway — the same split `rules-schema`'s `DATA_LOSS_LANDING` already uses across three
/// rules whose DDL granularity differs (`DROP COLUMN` against `DROP TABLE`). Magnitude is not a second noun:
/// `prefix-drift` moves N routes at once and `route-near-miss` moves one, and that is what each rule's own
/// observation already says.
///
/// The last sentence is the part no reader can get from the finding: this join is scoped to the trees named
/// in the run's config, so the callers it CANNOT enumerate are the ones that decide the edit.
pub(super) const CALLER_BREAKAGE_LANDING: &str = "COUNT WHO ELSE ASKS FOR THIS ADDRESS BEFORE YOU MOVE \
     IT: a repair on the SERVING side changes what the route is called, and every caller still asking for \
     the old name gets 404 on the deploy that lands it — with nothing failing to build anywhere, because \
     an HTTP call is not type-checked against the route it names. This join reads only the trees named in \
     this run's config, so the callers it cannot list for you are the ones that decide this: a link or \
     bookmark, a cache or CDN entry keyed on the URL, a mobile or desktop build already shipped, a webhook \
     URL you handed to a third party, and any repository outside this run. If that set is not empty, the \
     edit that costs nothing is on the CALLING side instead — or serve BOTH names for a release and retire \
     the old one once the callers you can see have moved.";

/// The cost of the ways out that do NOT move the address — `duplicate-route`'s "merge the handlers" and
/// `route-shadowing`'s "reorder the gateway". Spliced byte-identically into both, AHEAD of that branch's
/// imperative.
///
/// A SECOND constant rather than a reuse of [`CALLER_BREAKAGE_LANDING`], because the two describe opposite
/// symptoms and the reuse would be the more dangerous direction: there is no 404 here, no error and no log
/// line, so a reader handed the caller-enumeration sentence measures the one thing that will look fine.
/// What both branches actually do is decide which of two independently written handlers answers a request
/// that keeps arriving — and in `route-shadowing`'s case, decide it for every other literal/pattern pair the
/// same gateway holds, not only the one this finding names.
pub(super) const HANDLER_SUBSTITUTION_LANDING: &str = "THIS WAY OUT COSTS SOMETHING ELSE, AND NOTHING \
     REPORTS IT: it moves no address, so no caller 404s and no log line appears — the same request arrives \
     and a DIFFERENT handler answers it. The two implementations were written apart from each other, so \
     read them side by side before you make one of them win: the response shape, the status codes, the \
     auth or tenancy check each performs, and what each one writes. Reordering a shared first-match router \
     decides this for every literal/pattern pair it holds, not only the one named here. A caller that keeps \
     getting 200 is not evidence the answer is the same.";

/// The cost of changing a route's METHOD — `method-mismatch`'s serving-side repair. AHEAD of its imperative.
///
/// A THIRD constant rather than a reuse of [`CALLER_BREAKAGE_LANDING`], because the address does not move
/// here and the failure a reader should look for is a different status on a route that still resolves.
/// Two facts follow only from the verb: the parameters change SIDE (URL against body), so the other callers
/// need an edit rather than a redeploy; and a route that stops being a GET stops being reachable by every
/// mechanism that only issues GETs.
pub(super) const METHOD_CHANGE_LANDING: &str = "IF YOU CHANGE THE VERB ON THE SERVING SIDE YOU CHANGE IT \
     FOR EVERY CALLER: the path still resolves, so the ones still sending the old method get 405 rather \
     than a missing-route 404 you would spot in a log, and nothing fails to build. Two costs follow from \
     the verb alone — a GET carries its parameters in the URL and a POST/PUT/PATCH carries them in the \
     body, so each of those callers needs an edit and not just a redeploy; and a route that stops being a \
     GET stops being linkable, bookmarkable, prefetchable and cacheable by anything sitting in front of it. \
     This join reads only the trees in this run's config, so browsers, shipped clients and third-party \
     integrations are outside what it can count. When the CALLER in this finding is the side that is wrong, \
     fixing the call is the edit that costs nothing.";

/// The cost of moving a credential off the query string of an EXTERNAL call — `external-secret-in-url`.
/// AHEAD of its imperative.
///
/// Filed beside the three above and shared with none of them, because the thing being edited is not the
/// reader's: the host is external, so WHERE that vendor accepts a credential is a published contract and
/// not a code change. Its failure mode is also the reverse of theirs — there the reader's own service
/// stops answering someone else, here the reader's call stops being answered.
///
/// The closing sentence exists because ending at "the vendor may refuse" would read as "then do nothing",
/// which is false: the query string is still in the logs either way, and that half of the repair is
/// entirely on the reader's side.
pub(super) const VENDOR_CONTRACT_LANDING: &str = "CHECK THE VENDOR'S DOCUMENTATION FIRST — THIS CONTRACT \
     IS NOT YOURS TO EDIT: the host here is external, so where that API accepts a credential is its \
     decision, published by it, and not something a change in this repository can negotiate. Plenty of \
     APIs accept a key ONLY as a query parameter; against one of those, moving it to a header or a body \
     does not degrade gracefully — every call returns 401, and the first place that shows up is production. \
     If the parameter is query-only, the repair is entirely on your side instead and is still worth doing: \
     keep the value out of source (an environment variable or secret store), keep it out of your own logs \
     (a redacting formatter, or a proxy that strips the query string), and rotate the key if it has already \
     been reaching them.";
