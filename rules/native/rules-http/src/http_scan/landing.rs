//! The §27 landing clauses for the two `http_scan` rules that prescribe a change to a live endpoint —
//! the sentence each puts AHEAD of its own imperative because the prescription is the risk, and BEHIND
//! its own disqualifier because whether the finding is TRUE is a different question (rule-quality.md §38).
//!
//! Two constants rather than one, and neither is a copy of a sibling elsewhere in this workspace. Both of
//! these rules stand at a WRITE SITE inside one handler: they resolve an endpoint to a symbol and walk the
//! call graph to a write, and they read nothing at all about who calls that endpoint. That is the property
//! both sentences are built on, and it is also why the nearest-looking constants in the other rule crates
//! do not fit — `rules-cross-layer`'s route landings are about an address a JOIN can partly enumerate,
//! and `rules-schema`'s are about DDL a migration emits. Applying `eec4eea`'s test (is that constant's
//! NOUN the same as this rule's failure) separates all four.
//!
//! ⚠ The nearest miss for [`UNIQUE_ENFORCEMENT_LANDING`] is
//! `zzop_rules_cross_layer`'s `IDEMPOTENCY_ROLLOUT_LANDING`, which names the same two mechanisms. It is
//! `pub(crate)` in another crate and could not be spliced here even if it fit, and it does not fit: that
//! sentence is written for a rule whose trigger IS a retrying caller, so it can say "this finding names
//! ONE caller". This rule names none — it never looked outside the handler — so the sentence has to make
//! a weaker and different claim about who the key rejects.

/// How `unsafe-read-endpoint`'s prescription LANDS. Spliced AHEAD of its imperative.
///
/// The rule's two ways out cost opposite things and only one of them announces itself, which is the whole
/// reason this sentence exists. Changing the route's method is LOUD (405 on a URL that still resolves).
/// Moving the write to a new mutating route while the GET stays is SILENT: the GET keeps answering 200 and
/// simply stops performing the write, so a caller that depended on the side effect fails somewhere else,
/// later, with nothing pointing back here. A message that prescribes both without separating them hands a
/// reader the quiet one as if it were the safe one.
///
/// Deliberately says nothing about how COMMON a GET with a deliberate write is. That is unmeasured, and a
/// message that teaches a reader to doubt true findings costs more than the one it saves (rule-quality.md
/// §30/§32). What it states is mechanical: what a GET is reachable by, and what each way out does to that.
pub(super) const SAFE_METHOD_MOVE_LANDING: &str = "COUNT WHAT REACHES THIS URL WITH A GET BEFORE YOU MOVE \
     THE WRITE, AND KNOW WHICH WAY OUT IS THE QUIET ONE: this analysis resolved the endpoint to a handler \
     and walked the call graph to the write, and it never looked at a single caller — a link, a browser \
     address bar, a prefetch or link-preview fetch, a health check, a shipped client build, a cache or CDN \
     in front of it. Changing this route's method is the LOUD repair: those callers get 405 on the deploy, \
     and you find out immediately. Adding a mutating route and leaving the GET is the QUIET one: the GET \
     keeps returning 200 and silently stops writing, so whatever depended on that side effect fails \
     somewhere else with nothing pointing back here. If the write is what those callers actually want, \
     ship the new route first and give the GET a deadline, rather than flipping the method underneath it.";

/// How `non-idempotent-write`'s prescription LANDS. Spliced AHEAD of its imperative.
///
/// The rule offers two remedies and the message priced neither. Both cost something, and — the part a
/// reader cannot get from the finding — the cheap-LOOKING one is the one that does not work: a read-then-
/// write check is not a dedup, because two retries that arrive together both read "absent". What makes it
/// hold is a database constraint, and that is a migration validated against every row already stored.
///
/// The second half is the wire cost of the other remedy, and it is stated more weakly than its sibling in
/// `rules-cross-layer` states it, on purpose: that rule's trigger is a retrying caller, so it can say the
/// finding names one. This rule walked a call graph inside one handler and looked at no caller at all, so
/// the honest claim is that it cannot name any of them.
pub(super) const UNIQUE_ENFORCEMENT_LANDING: &str = "BOTH OF THOSE COST SOMETHING, AND THE CHEAP-LOOKING \
     ONE IS THE ONE THAT DOES NOT WORK: a check that READS before it writes is not a dedup — two retries \
     that arrive together both read `absent` and both insert. What makes it hold is the database, and \
     `ADD CONSTRAINT ... UNIQUE` is a migration, not a handler edit: Postgres builds it over every row \
     already stored and ABORTS MID-DEPLOY on the first duplicate pair it meets, so select for those before \
     you write the migration rather than after. An idempotency key is that same constraint on a different \
     column plus a change to the wire contract, and this rule cannot tell you what that costs: it walked \
     the call graph inside this handler and looked at no caller at all, so every client that would start \
     getting rejected for omitting the header is outside what it saw. Accept the key as OPTIONAL first, \
     and require it once the callers you can find are sending it.";
