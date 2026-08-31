//! TWO-AXIS COVERAGE GUARD — every DSL rule must carry a verdict on EACH of two independent
//! questions about its own message, and a rule missing either turns this red
//! (`1.architecture/rules/rule-quality.md` §27 for axis A, §33/§37 for axis B).
//!
//! THE TWO QUESTIONS, and why they are two. §27 states its own subject twice and not the same way
//! both times. Its judgment sentence is about the remedy (*"following this prescription breaks
//! something and the message does not warn"*), while the population it went on to measure — 46 of 118
//! rules "stating a disqualifying condition" — is about the FINDING (*"this report may be wrong"*).
//! §37 then named a third thing and separated it explicitly: `CREATE INDEX` breaks nothing, so HARMFUL
//! bridge ② does not stand, yet the correct edit still costs a write lock. Bridges ② and ③ are one
//! question — what your CORRECT edit costs you — and it is not the question of when the finding is
//! wrong. All four quadrants are occupied today, which is what makes them axes rather than degrees:
//!
//! |                    | names a disqualifier | names none |
//! |---|---|---|
//! | **carries a landing** | `security/shell-exec-interpolation` | `security/trust-all-tls` |
//! | **carries none**      | `sql/update-no-where`               | `sql/count-in-loop` |
//!
//! WHAT WENT WRONG WITH ONE AXIS. The first version of this file held ONE verdict per rule and derived
//! its strongest bucket from "does a `#[test]` pin an order on a delivered finding" — without asking
//! WHICH order. Landing pins counted as §27-a verdicts. Two consequences, both measured 2026-08-30:
//! eleven rules carried nothing but a landing pin and were therefore green on axis A while asserting
//! nothing about a disqualifier at all; and because a table row may not coexist with a pin, three of
//! them (`security/{trust-all-tls,jwt-none-algorithm,xxe-no-guard}`) had to LEAVE `NO_DISQUALIFIER` to
//! gain a landing pin, deleting a declaration that the newer pin does not replace. Splitting the pin
//! set by helper family is the whole repair: each axis then counts only the pins that speak to it.
//!
//! AXIS A — DISQUALIFIER (§27): *under what named condition is this finding wrong, and does the reader
//! meet that condition before the remedy?*
//!
//! | verdict | meaning | where it lives |
//! |---|---|---|
//! | disqualifier pin | a `#[test]` scans a fixture and pins clause-before-imperative | `rules/dsl/**/*.rs`, DERIVED, never listed |
//! | [`ORDER_CLAIMS`] | the order holds, pinned against the message TEMPLATE in the pack JSON | this file |
//! | [`NO_DISQUALIFIER`] | the message names no condition under which its own finding is wrong | this file |
//! | [`OPEN_ORDER_VIOLATIONS`] | the order is INVERTED and known to be, asserted to STILL be inverted | this file |
//!
//! AXIS B — LANDING (§33/§37): *what does the reader's own CORRECT edit cost, and is that cost reached
//! before the instruction that incurs it?* Both buckets are DERIVED — this axis adds no hand rows:
//!
//! | verdict | meaning | where it lives |
//! |---|---|---|
//! | landing pin | a `#[test]` pins landing-before-imperative on a delivered finding | DERIVED from the landing helper |
//! | no landing | the message carries none of the registered landing constants | DERIVED from the registry |
//!
//! WHY AXIS B IS DERIVABLE AND AXIS A IS NOT. A landing is a SHARED CONSTANT by construction (§37: one
//! spelling, byte-identical across the family, because a position pin needs one spelling to index), so
//! "does this message carry a landing" is a substring question against the constants declared in this
//! tree — no judgment, no list. A disqualifier has no such carrier: it is bespoke prose per rule, and
//! whether a sentence disqualifies or merely hedges is the judgment §27 spends its exclusion list on.
//! That asymmetry is why the burden of the second axis is zero rows rather than 118.
//!
//! WHAT AXIS B DOES NOT ASK. It asks whether a landing that EXISTS is positioned, not whether one is
//! OWED. The 2026-08-30 census put the owed-and-missing population at 69 of 118 and this guard cannot
//! see it: a rule that should carry a landing and carries no constant is indistinguishable here from a
//! rule whose remedy costs nothing. That gap is the open backlog item, not a claim made by this file —
//! the same distinction `NO_DISQUALIFIER` already carries on axis A, where a row records that somebody
//! looked, not that the absence is correct. What the axis DOES close is the failure mode that census
//! named: landings track constant FAMILIES, so a family that grows a member the pins do not follow is
//! exactly the silent case, and it is red here from the first run.
//!
//! ONE KNOWN NON-CARRIER THAT SHOULD BE ONE, named so the registry is not read as complete.
//! `security/jwt-no-expiry` opens with "ADDING AN EXPIRY DOES NOT REACH THE TOKENS THIS FINDING IS
//! ABOUT" — a landing by any reading — in its own spelling rather than through a constant, so this
//! file classifies it as carrying none and nothing positions it. The circulation family is split
//! three ways (`TOKEN_CIRCULATION_LANDING` on `jwt-none-algorithm`, `jwt-sign-literal-secret`'s
//! verify-side exit, and this) and only one of the three is registered. Registering the bespoke
//! spelling here would settle a question that has not been asked — whether the three collapse into
//! one constant — and collapsing them is a MESSAGE change. The backlog owns it.
//!
//! WHY A TEMPLATE PIN AND NOT 35 MORE FIXTURES. §27's claim is about what a reader meets in what
//! order, and the order is a property of the template: delivery splices values in, it never reorders
//! sentences. A template pin therefore makes the same claim for a rule that has no fixture, at no
//! fixture cost — which is the difference between covering 35 rules today and covering them never. It
//! is the WEAKER of the two only in that it does not prove the rule fires at all; the delivered pins
//! keep that stronger claim where a fixture already exists, and this file never re-states them. Axis B
//! has no template table because it has no rows to put in one: every landing-carrying rule today has a
//! fixture. Add the table when a landing arrives on a rule that does not — an empty table asserts
//! nothing and goes stale unread.
//!
//! WHY THE SUBJECT SET IS DERIVED. A hand-written list of rule ids answers "did the ones I remembered
//! stay green", not "is every rule covered" — and it is exactly the shape of list that let 71 rules sit
//! outside the last census. The subjects come from the pack JSONs on disk, with a FLOOR on the counts
//! (`MIN_PACKS`, `MIN_RULES`, `MIN_DISQUALIFIER_PINS`, `MIN_LANDING_PINS`): a scan that reads nothing
//! would otherwise satisfy every set relation below vacuously and report a clean tree. The landing
//! CONSTANT registry carries no floor of its own on purpose — a collapsed registry makes every rule a
//! non-carrier, which fails [`landing_pins_name_a_registered_constant`]'s direction instead, and a
//! floor there would also fight a legitimate merge of two families into one spelling.
//!
//! WHAT COUNTS AS A DISQUALIFIER, and what §27 excludes by name. A disqualifier names a construct the
//! reader can look for in their own code and conclude that this finding is wrong. It is NOT: a
//! false-negative disclosure (what the rule would MISS says nothing about the finding in hand), a
//! suppression-marker spelling note, a sibling-scope partition, a description of a veto the rule
//! already applies, a bare hedge with no named construct — nor, since this file learned to tell them
//! apart, a LANDING. "This is co-occurrence, not dataflow" on its own does not qualify, which is why
//! [`NO_DISQUALIFIER`] records WHICH excluded category a rule's limitation prose falls into rather
//! than a bare id. Two of its rows now read "landing only", and those are not short messages —
//! `security/trust-all-tls` runs 1,433 characters, every one of them about what the correct edit
//! costs. Under one axis that length read as compliance; it answers the other question.
//!
//! HOW TO ADD A RULE. Axis A: give it a disqualifier pin, or one row here. Axis B: give it a landing
//! pin if its message carries a landing constant, and nothing at all if it does not. Adding neither on
//! axis A is red, which is the whole point: the declaration cannot be forgotten because forgetting it
//! is the failure.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Floors, not counts. Each is monotone — packs and rules only ever grow, and a pin is only ever
/// removed by moving that rule into a table here — so none of them goes stale on ordinary growth,
/// while all of them go red the moment a scan silently reads nothing.
const MIN_PACKS: usize = 11;
const MIN_RULES: usize = 118;
const MIN_DISQUALIFIER_PINS: usize = 41;
const MIN_LANDING_PINS: usize = 27;

/// The three shared position-pin helpers that assert a DISQUALIFIER order. A `#[test]` naming any of
/// them is asserting when this rule's own finding is wrong, and where the reader meets that.
const DISQUALIFIER_PIN_HELPERS: &[&str] = &[
    "assert_disqualifier_summary_precedes_imperative",
    "assert_disqualifier_clause_precedes_imperative",
    "assert_clauses_precede_imperative",
];

/// The one shared helper that asserts a LANDING order — a different question, which is why it is
/// counted on its own axis rather than pooled with the three above.
const LANDING_PIN_HELPERS: &[&str] = &["assert_landing_precedes_imperative"];

/// `(rule id, disqualifying clause, imperative)` — the order already holds, and this pins it against
/// the rule's message TEMPLATE. Each needle must occur EXACTLY ONCE, for the reason the helper module
/// gives at length: an index comparison against a doubled needle compares whichever copy `find`
/// reaches first, which is not the claim being made.
const ORDER_CLAIMS: &[(&str, &str, &str)] = &[
    ("sql/update-no-where", "confirm the surrounding statement before acting", "Add a WHERE clause"),
    ("sql/delete-no-where", "`#`-commented-out DELETE is read as live code", "Add a WHERE clause"),
    ("sql/truncate-in-app-code", "`#`-commented-out TRUNCATE is read as live code", "Move destructive schema/data operations into a migration script"),
    ("security/sql-interpolated-statement", "If that value is request-derived this is SQL injection", "Bind the value as a query PARAMETER"),
    ("security/template-unescaped-output", "if any interpolated value is user-influenced", "Use the escaped output form instead"),
    ("browser/location-assign-dynamic", "if that value is influenced by the URL", "Validate the target against an allowlist"),
    ("security/command-interpolated-string", "the worst an argument-position value enables is argument/option injection against the called program", "Pass every argument as its own list element"),
    ("security/command-and-interpolation", "A formatted string passed as an ordinary `.arg(...)` is NOT", "Pass every argument as its own `.arg(value)`"),
    ("http/protected-path-no-auth-evidence", "If a guard exists but isn't recognized", "inject the `auth-guarded` attribute by either route above"),
    ("security/cmd-injection", "this is STILL a co-occurrence heuristic, not dataflow", "Pass arguments as an array element per token"),
    ("egress/http-url-literal", "it may be an id, a docs link, or dead config", "Use https, or route through an env-based"),
    ("redis/counter-get-set", "a `.set(` in an `else` branch still counts", "Use Redis's atomic `INCR`"),
    ("db/tx-and-db-call-in-loop", "not that this specific loop sits inside that transaction", "Move per-row reads out of the transaction"),
    ("reliability/fs-check-then-use", "`fs.existsSync(a)` followed by `fs.writeFile(b)` still fires", "Prefer opening with an exclusive-create flag"),
    ("reliability/listener-subscribe-in-loop", "either misses a real fix or vetoes a real leak", "Register the listener once outside the loop"),
    ("security/html-response-from-request", "this is a co-occurrence heuristic, not a proof", "Escape/sanitize request-derived values"),
    ("db/non-atomic-counter-update", "a `take: cursor + 1` pagination bound", "Use the ORM's atomic increment/decrement"),
    ("security/cors-credentials-wildcard", "they may belong to two unrelated config objects", "Return a specific allow-listed origin"),
    ("security/unsafe-deserialization", "a `readObject(` on an unrelated object satisfies the second pattern just as well", "Use JSON/Protobuf instead"),
    ("reliability/stream-open-no-close-in-loop", "the finding still fires (false positive)", "Close the handle in a `finally` block"),
    ("security/taint-flow", "coarse v1 check (source+sink co-occurrence, not real dataflow)", "For a SQL sink, parameterize the query"),
    ("security/eval-dynamic-code", "if any part of that string can be influenced by user input", "Avoid dynamic code construction"),
    ("browser/vue-v-html", "if the bound value is influenced by user input", "Prefer `{{ }}` text interpolation"),
    ("redis/client-no-error-listener", "the client may be created here and the `'error'` listener attached in another module", "Attach `.on('error', ...)` where the client is created"),
    ("security/java-path-traversal", "on a constant path in a method that reads an unrelated parameter fires just the same", "Reject `..` segments"),
    ("db/manual-tx-no-rollback", "not proof COMMIT actually follows BEGIN", "Wrap the work in a try/catch that issues `ROLLBACK`"),
    ("security/stacktrace-to-response", "that goes to stderr in a method that merely mentions", "Log server-side and return a generic error body"),
    ("security/sql-string-concat", "if the concatenated part is request-derived", "Use a parameterized/bound query instead"),
    ("db/tx-and-empty-catch", "it doesn't prove the empty catch wraps the transaction's body", "Log the error and rethrow it"),
    ("security/path-traversal", "not proof the request-derived value actually flows into the joined path", "Reject `..` segments"),
    ("react/setstate-after-async-unguarded", "an accepted false positive here", "Thread an `AbortController`"),
    ("sql/destructive-migration", "Committed destructive migrations are usually deliberate", "Verify the object name against the PR/ticket"),
    ("security/annotation-sql-concat", "INJECTION IS NOT THE RISK HERE", "Prefer a single literal, with named parameters"),
    // §27's third form, and the section names this rule as its exemplar: the imperative itself is
    // conditioned, so the premise a reader must evaluate sits directly in front of the verb.
    ("security/hardcoded-secret", "IF this value is a real credential", "move it to an environment variable or a secrets manager"),
];

/// `(rule id, which excluded category its limitation prose falls into)` — this rule names no condition
/// under which its own finding is wrong, so there is no order to assert. The second field is required
/// and is not decoration: it is what separates "the message says nothing at all" from "the message
/// hedges, but only in a way §27 excludes by name", and writing it forces the next author to decide
/// which of those they are claiming.
///
/// "landing only" is the category the two-axis split added, and it is the one that does NOT mean "this
/// message is short". `security/trust-all-tls` carries it at 1,433 characters, all of them about what
/// the reader's correct edit costs. Under one axis that prose registered as compliance; it answers the
/// other question. Ten of the rows below were written or rewritten on 2026-08-30 for that reason.
const NO_DISQUALIFIER: &[(&str, &str)] = &[
    // --- Restored 2026-08-30. These three declared an absence, then had to drop the row to take a
    // landing pin, because the guard could not tell the two claims apart. Re-read at restore time:
    // all three still name no disqualifier, and two of the three CATEGORIES had gone stale, because
    // the landing added prose where the row said there was none.
    (
        "security/trust-all-tls",
        "landing only (was 'no limitation prose at all' — the §37 landing added 1,100 characters of \
         cost prose and none of it disqualifies)",
    ),
    ("security/jwt-none-algorithm", "suppression boilerplate + landing"),
    (
        "security/xxe-no-guard",
        "veto description (any one recognized guard silences it) + landing (was 'no limitation prose \
         at all')",
    ),
    // --- Added 2026-08-30. Eight rules whose only pin was a LANDING pin, so nothing had ever asked
    // them the axis-A question. Seven name no disqualifier; the eighth (`hardcoded-secret`) does and
    // moved to ORDER_CLAIMS.
    (
        "security/config-file-secret",
        "veto description (five not-flagged arms) + FN disclosure (16-character floor)",
    ),
    (
        "security/conn-string-credentials",
        "veto description (interpolated/printf/placeholder slots) + FN disclosure (short bare-word \
         password)",
    ),
    ("security/hardcoded-password", "landing only"),
    (
        "security/private-key-committed",
        "veto description (prose mention, `placeholder=` attribute) + suppression boilerplate; the \
         message asserts the OPPOSITE — no plausible non-key reading exists",
    ),
    (
        "security/vendor-token-committed",
        "veto description (`sk_test_` is deliberately not matched) + suppression boilerplate",
    ),
    (
        "security/weak-cipher",
        "FN disclosure (Java/JSP-only scope) + suppression-marker spelling note",
    ),
    (
        "security/jwt-sign-literal-secret",
        "sibling-scope partition (vs `hardcoded-secret`) + veto description",
    ),
    // --- The 2026-08-30 `audit74` population.
    (
        "security/bcrypt-cost-too-low",
        "FN disclosure (constant cost invisible)",
    ),
    (
        "redis/keys-command-in-code",
        "FN disclosure + suppression boilerplate",
    ),
    ("go/goroutine-in-loop", "under-report disclosure"),
    (
        "browser/markdown-and-html-sink-unsanitized",
        "FN disclosure (cross-span, .vue)",
    ),
    (
        "db/client-new-in-loop",
        "veto description + sibling boundary",
    ),
    ("sql/nplus1", "FN disclosure (path scope)"),
    ("browser/javascript-url", "FN disclosure (scope limit)"),
    ("perf/api-in-loop", "FN disclosure (degraded parse)"),
    (
        "db/idempotency-key-regenerated-in-loop",
        "no limitation prose at all",
    ),
    (
        "security/cors-reflected-origin-credentials",
        "FN disclosure (multi-line object)",
    ),
    (
        "reliability/await-inside-promise-all-array",
        "FN disclosure (multi-line array)",
    ),
    ("reliability/emitter-async-listener", "veto description"),
    (
        "reliability/debug-true-committed",
        "suppression boilerplate",
    ),
    (
        "security/jwt-verify-bypass",
        "gate description (retired FP class)",
    ),
    (
        "reliability/sync-fs-in-handler",
        "accepted-miss (FN) parenthetical",
    ),
    ("browser/postmessage-wildcard", "suppression boilerplate"),
    (
        "db/client-new-in-handler",
        "accepted-miss (FN) parenthetical",
    ),
    ("sql/count-in-loop", "no limitation prose at all"),
    (
        "security/api-key-in-url",
        "suppression-marker spelling note",
    ),
    ("db/unawaited-transaction", "no limitation prose at all"),
    (
        "security/error-leak-to-client",
        "no limitation prose at all",
    ),
    (
        // Both this row and `weak-token-random` below read "no limitation prose at all" until the
        // §27 leg-3 batch gave each rule a landing. The ABSENCE is still true — neither message names
        // a condition under which its own finding is wrong — but the CATEGORY went stale the moment
        // the landing added prose, which is exactly what the three restored rows at the top of this
        // table record.
        "reliability/map-async-no-promise-all",
        "landing only (was 'no limitation prose at all' — it carries ALLSETTLED_WRAPPER_LANDING, \
         shared byte-identically with the sibling that already owned the sentence)",
    ),
    (
        "security/weak-token-random",
        "landing only (was 'no limitation prose at all' — the CSPRNG landing added ~1,000 characters \
         about what the swap costs, and none of it disqualifies)",
    ),
    ("db/unbounded-user-limit", "no limitation prose at all"),
    (
        "reliability/json-parse-no-try",
        "no limitation prose at all",
    ),
    (
        "reliability/interval-no-clear",
        "no limitation prose at all",
    ),
    ("security/weak-random", "no limitation prose at all"),
    ("browser/no-document-write", "suppression boilerplate"),
    ("db/float-money-compare", "no limitation prose at all"),
    ("egress/ws-no-auth", "no limitation prose at all"),
];

/// `(rule id, disqualifying clause, imperative)` — OPEN §27-a violations: the clause sits BEHIND the
/// remedy today. This is a ratchet, not an amnesty: the assertion below demands the inversion still
/// HOLDS, so repairing one of these turns this red until its row moves into [`ORDER_CLAIMS`], and a
/// violation not on this list is red from the first run. None of these three is `critical` and none is
/// over 2000 characters, which is the band the batch that wrote this file was scoped to repair.
const OPEN_ORDER_VIOLATIONS: &[(&str, &str, &str)] = &[
    // Running fs writes concurrently is what the remedy asks for, and it breaks a loop whose
    // iterations must stay ordered — the message says so only inside its suppression parenthetical.
    (
        "reliability/fs-in-loop-serial",
        "deliberately sequential writes to preserve on-disk ordering",
        "Gather the paths up front and run the calls concurrently",
    ),
    // The finding may already be sanitized elsewhere in the function; the rule says so after telling
    // the reader to add the check.
    (
        "security/sendfile-from-request",
        "a real sanitizer elsewhere in the function is not detected",
        "Reject `..` segments",
    ),
    // The prescribed cookie reintroduces CSRF and does not stop an active XSS riding the session —
    // stated one sentence after the prescription.
    (
        "security/localstorage-jwt",
        "This isn't a full fix either",
        "Default fix: store it in an `httpOnly` cookie",
    ),
];

/// `rules/dsl`, from this crate's manifest dir (`rules/`).
fn dsl_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl")
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("dsl dir readable") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every `<pack id>/<rule id>` on disk, with its message. Read straight from the JSON rather than
/// through `load_dsl_packs` so this costs no regex compilation — the subject set is the ids, and the
/// claims below index into the message text, neither of which needs a compiled matcher.
fn all_rules() -> (usize, BTreeMap<String, String>) {
    let mut packs = 0usize;
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(dsl_dir()).expect("dsl dir readable") {
        let dir = entry.expect("dir entry").path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir
            .file_name()
            .expect("named dir")
            .to_string_lossy()
            .to_string();
        let json = dir.join(format!("{name}.json"));
        if !json.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(&json).expect("pack json readable");
        let v: serde_json::Value = serde_json::from_str(&raw).expect("pack json parses");
        let pack_id = v["id"].as_str().expect("pack id").to_string();
        let rules = v["rules"].as_array().expect("pack rules array");
        assert!(
            !rules.is_empty(),
            "{}: pack ships zero rules — a pack directory with a JSON that yields nothing is a \
             broken read, not an empty pack",
            json.display()
        );
        packs += 1;
        for r in rules {
            let id = r["id"].as_str().expect("rule id");
            let message = r["message"].as_str().unwrap_or("").to_string();
            out.insert(format!("{pack_id}/{id}"), message);
        }
    }
    (packs, out)
}

/// Every LANDING constant declared in this tree, by name.
///
/// This is the whole of axis B's judgment, and it is a substring test rather than a list because §37
/// made a landing a SHARED CONSTANT: one spelling, byte-identical across its family, because a
/// position pin needs one spelling to index. So the constants a pack's tests declare ARE the registry,
/// and a message either contains one of them or does not.
///
/// Single-line declarations only, asserted rather than assumed: every landing in the tree is one long
/// line, and a silently-skipped multi-line constant would make its carriers look like non-carriers,
/// which is the direction that goes quiet. `trim` rather than a `^` anchor because these files are
/// CRLF/LF mixed.
fn landing_texts() -> BTreeMap<String, String> {
    let mut files = Vec::new();
    rs_files(&dsl_dir(), &mut files);
    files.sort();
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for path in files {
        let src = std::fs::read_to_string(&path).expect("rs file readable");
        for line in src.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("const ") else {
                continue;
            };
            let Some((name, rest)) = rest.split_once(": &str = ") else {
                continue;
            };
            if !name.contains("LANDING") {
                continue;
            }
            let Some(body) = rest.strip_prefix('"') else {
                continue;
            };
            let mut text = String::new();
            let mut chars = body.chars();
            let mut closed = false;
            while let Some(c) = chars.next() {
                match c {
                    '\\' => match chars.next() {
                        Some('n') => text.push('\n'),
                        Some('t') => text.push('\t'),
                        Some('r') => text.push('\r'),
                        Some(other) => text.push(other),
                        None => break,
                    },
                    '"' => {
                        closed = true;
                        break;
                    }
                    other => text.push(other),
                }
            }
            assert!(
                closed,
                "{}: landing constant {name} does not close its string literal on one line. Every \
                 landing in this tree is a single long line, and a multi-line one would be read as \
                 truncated here — its carriers would look like non-carriers and go quiet on axis B.",
                path.display()
            );
            if let Some(previous) = out.insert(name.to_string(), text.clone()) {
                assert_eq!(
                    previous,
                    text,
                    "{}: two different landing constants are both named {name}. The registry is \
                     keyed by name, so one of them would shadow the other and its carriers would \
                     look like non-carriers.",
                    path.display()
                );
            }
        }
    }
    out
}

/// True when this `fn` block compares two `find`-derived offsets IN PLACE, without a helper. This is
/// what the 2026-08-30 census found that a helper-name grep could not: seven rules pin their own
/// offsets inline, and a name search reports every one of them as unpinned.
///
/// Read as an AXIS-A claim. Every inline comparison in this tree is clause-before-imperative, and a
/// landing has to travel through the helper anyway — [`landing_pins_name_a_registered_constant`]
/// demands the needle be a registered constant, which an inlined literal is not.
///
/// Deliberately says nothing about the helpers. A block that merely MENTIONS a helper — the `use` line
/// at the top of every pack root does — is not an assertion, and folding that test in here made the
/// module preamble of eight files look like a pin.
fn block_has_inline_order_comparison(block: &str) -> bool {
    if !block.contains(".find(") || !block.contains("hits(&out") {
        return false;
    }
    block.lines().any(|line| {
        let t = line
            .trim()
            .trim_end_matches(&[',', ')'][..])
            .trim_end_matches(" &&");
        let Some((lhs, rhs)) = t.split_once(" < ") else {
            return false;
        };
        let ident = |s: &str| {
            !s.is_empty()
                && s.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        };
        ident(lhs) && ident(rhs)
    })
}

/// The rule ids that a call to one of `helpers` in this block pins, read from each call's FIRST
/// ARGUMENT.
///
/// Not "every rule id the block mentions": measured 2026-08-30, `redis/lock-get-then-set`'s pin block
/// also calls `hits(&out, "counter-get-set")` to assert the sibling stays silent on the same line, and
/// a mention-based reading credited the sibling with a pin it does not have. The first argument is the
/// subject of the assertion; anything else in the block is context.
fn helper_subjects(block: &str, helpers: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for helper in helpers {
        let mut from = 0usize;
        while let Some(rel) = block[from..].find(helper) {
            let after = from + rel + helper.len();
            from = after;
            let tail = block[after..].trim_start();
            // A `use` line or a doc-comment reference has no argument list; only a CALL does.
            let Some(args) = tail.strip_prefix('(') else {
                continue;
            };
            let args = args.trim_start();
            let Some(rest) = args.strip_prefix('"') else {
                continue;
            };
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
            }
        }
    }
    out
}

/// Rule ids pinned on a DELIVERED finding, split by which QUESTION the pin answers. Derived, never
/// listed. Returns `(disqualifier pins, landing pins)`, and a rule may be in both — five are.
///
/// Two shapes on the disqualifier side, and the second is why a helper-NAME search is not enough:
/// several rules compare their own offsets in place rather than through a helper, and a name grep
/// reports every one of them as unpinned. An inline block must name EXACTLY ONE rule, because there is
/// no first argument to read the subject from — a block that names two is not mis-attributed silently,
/// it fails here and asks the author to route the assertion through a shared helper instead.
fn delivered_pins(all: &BTreeMap<String, String>) -> (BTreeSet<String>, BTreeSet<String>) {
    let bare: BTreeMap<&str, &String> = all
        .keys()
        .map(|full| (full.rsplit_once('/').expect("pack-qualified id").1, full))
        .collect();
    let mut files = Vec::new();
    rs_files(&dsl_dir(), &mut files);
    files.sort();
    let mut disqualifier = BTreeSet::new();
    let mut landing = BTreeSet::new();
    for path in files {
        let name = path
            .file_name()
            .expect("named file")
            .to_string_lossy()
            .to_string();
        if name == "message_order_pins.rs" || name == "message_order_verdicts.rs" {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("rs file readable");
        // Top-level `fn` boundaries. Every pin in this tree lives in a `#[test] fn` at column 0.
        for block in src.split("\nfn ") {
            let mut any_helper = false;
            for (helpers, target) in [
                (DISQUALIFIER_PIN_HELPERS, &mut disqualifier),
                (LANDING_PIN_HELPERS, &mut landing),
            ] {
                for id in helper_subjects(block, helpers) {
                    any_helper = true;
                    if let Some(full) = bare.get(id.as_str()) {
                        target.insert((*full).clone());
                    }
                }
            }
            if any_helper {
                continue;
            }
            if !block_has_inline_order_comparison(block) {
                continue;
            }
            let named: Vec<&&String> = bare
                .iter()
                .filter(|(id, _)| block.contains(&format!("\"{id}\"")))
                .map(|(_, full)| full)
                .collect();
            let head = block.lines().next().unwrap_or("").trim();
            assert_eq!(
                named.len(),
                1,
                "{}: the inline order assertion in `fn {head}` names {} rule ids, so which rule it \
                 pins cannot be read off the source. Route it through one of the shared helpers in \
                 `rules/dsl/message_order_pins.rs`, whose first argument states the subject.",
                path.display(),
                named.len()
            );
            disqualifier.insert((**named[0]).clone());
        }
    }
    (disqualifier, landing)
}

fn assert_order(full: &str, message: &str, clause: &str, imperative: &str, table: &str) {
    for needle in [clause, imperative] {
        let n = message.matches(needle).count();
        assert_eq!(
            n, 1,
            "{table} row {full}: {needle:?} occurs {n} time(s) in this rule's message, not once. At \
             zero the sentence this row claims to pin has left the message; above one the offset \
             comparison below would compare against an arbitrary copy. In: {message}"
        );
    }
    let at_clause = message.find(clause).expect("asserted above");
    let at_imperative = message.find(imperative).expect("asserted above");
    if table == "OPEN_ORDER_VIOLATIONS" {
        assert!(
            at_clause > at_imperative,
            "{full} is listed as an OPEN §27-a violation but its clause (byte {at_clause}) now \
             PRECEDES its imperative (byte {at_imperative}) — the defect was repaired. Move this row \
             into ORDER_CLAIMS, or give the rule a disqualifier pin and drop the row."
        );
    } else {
        assert!(
            at_clause < at_imperative,
            "{full}: the imperative sits at byte {at_imperative}, AHEAD of the clause that \
             disqualifies this finding at byte {at_clause} — a reader who edits on the first \
             instruction never reaches it (rule-quality.md §27). Move the clause or condition the \
             verb; do not rewrite either. In: {message}"
        );
    }
}

/// AXIS A. Every DSL rule carries exactly one verdict about when its own finding is WRONG, the three
/// declared tables hold no rule that has gone away, and the two order tables still say what they
/// claim.
#[test]
fn every_dsl_rule_carries_a_disqualifier_verdict() {
    let (packs, all) = all_rules();
    assert!(
        packs >= MIN_PACKS && all.len() >= MIN_RULES,
        "subject scan collapsed: {packs} packs / {} rules, floor is {MIN_PACKS}/{MIN_RULES}. Every \
         set relation below is satisfied vacuously by an empty subject set, so a small number here \
         is a broken enumeration, never a smaller rule set.",
        all.len()
    );

    let (delivered, _) = delivered_pins(&all);
    assert!(
        delivered.len() >= MIN_DISQUALIFIER_PINS,
        "disqualifier-pin scan collapsed: found {} of at least {MIN_DISQUALIFIER_PINS}. Pins are \
         only ever removed by moving that rule into a table in this file, so a drop here means the \
         scan stopped seeing the pins rather than that the pins went away. Found: {delivered:?}",
        delivered.len()
    );

    let claims: BTreeSet<&str> = ORDER_CLAIMS.iter().map(|(id, _, _)| *id).collect();
    let none: BTreeSet<&str> = NO_DISQUALIFIER.iter().map(|(id, _)| *id).collect();
    let open: BTreeSet<&str> = OPEN_ORDER_VIOLATIONS.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        claims.len(),
        ORDER_CLAIMS.len(),
        "ORDER_CLAIMS holds a duplicate rule id"
    );
    assert_eq!(
        none.len(),
        NO_DISQUALIFIER.len(),
        "NO_DISQUALIFIER holds a duplicate rule id"
    );
    assert_eq!(
        open.len(),
        OPEN_ORDER_VIOLATIONS.len(),
        "OPEN_ORDER_VIOLATIONS holds a duplicate id"
    );

    // No rule may hold two verdicts on this axis — a rule in a table AND pinned on delivery means one
    // of the two is describing a message that no longer looks the way it says. Only DISQUALIFIER pins
    // count here: a landing pin makes no claim about a disqualifier, and treating it as one is what
    // deleted three declarations on 2026-08-30.
    for (a, an, b, bn) in [
        (&claims, "ORDER_CLAIMS", &none, "NO_DISQUALIFIER"),
        (&claims, "ORDER_CLAIMS", &open, "OPEN_ORDER_VIOLATIONS"),
        (&none, "NO_DISQUALIFIER", &open, "OPEN_ORDER_VIOLATIONS"),
    ] {
        let both: Vec<_> = a.intersection(b).collect();
        assert!(
            both.is_empty(),
            "rules listed in both {an} and {bn}: {both:?}"
        );
    }
    for (table, ids) in [
        ("ORDER_CLAIMS", &claims),
        ("NO_DISQUALIFIER", &none),
        ("OPEN_ORDER_VIOLATIONS", &open),
    ] {
        let dupes: Vec<_> = ids.iter().filter(|id| delivered.contains(**id)).collect();
        assert!(
            dupes.is_empty(),
            "{table} lists rules that already carry a DISQUALIFIER pin on a delivered finding: \
             {dupes:?}. That pin is the stronger claim about the SAME order; drop the row rather \
             than keeping two records of it. (A LANDING pin is not this — it answers the other axis \
             and never displaces a row here.)"
        );
        let stale: Vec<_> = ids.iter().filter(|id| !all.contains_key(**id)).collect();
        assert!(
            stale.is_empty(),
            "{table} lists rule ids that no longer exist in any pack: {stale:?}"
        );
    }

    // THE COVERAGE ASSERTION. Anything the four verdicts do not reach is a rule about which nothing
    // asserts when its own finding is wrong — which is the state this file exists to make impossible.
    let covered: BTreeSet<String> = delivered
        .iter()
        .cloned()
        .chain(claims.iter().map(|s| (*s).to_string()))
        .chain(none.iter().map(|s| (*s).to_string()))
        .chain(open.iter().map(|s| (*s).to_string()))
        .collect();
    let uncovered: Vec<&String> = all.keys().filter(|id| !covered.contains(*id)).collect();
    assert!(
        uncovered.is_empty(),
        "{} DSL rule(s) carry NO §27 disqualifier verdict: {uncovered:?}\n\
         Give each one a disqualifier position pin on a delivered finding, or a row in this file: \
         ORDER_CLAIMS if its disqualifying clause already precedes its remedy, NO_DISQUALIFIER if \
         the message names no condition under which its own finding is wrong (say which excluded \
         category its limitation prose falls into — 'landing only' is one of them), or \
         OPEN_ORDER_VIOLATIONS if the clause sits behind the remedy and the repair is not in this \
         change. A rule with no verdict is not a rule that complies — it is a rule nobody asked.",
        uncovered.len()
    );

    // The declared orders still hold.
    for (id, clause, imperative) in ORDER_CLAIMS {
        assert_order(id, &all[*id], clause, imperative, "ORDER_CLAIMS");
    }
    for (id, clause, imperative) in OPEN_ORDER_VIOLATIONS {
        assert_order(id, &all[*id], clause, imperative, "OPEN_ORDER_VIOLATIONS");
    }
}

/// AXIS B. Every DSL rule carries exactly one verdict about what its own CORRECT edit costs: either a
/// landing position pin, or no landing at all. Both buckets are derived, so this axis adds no rows —
/// what it forbids is a landing that exists and that nothing positions.
#[test]
fn every_dsl_rule_carries_a_landing_verdict() {
    let (packs, all) = all_rules();
    assert!(
        packs >= MIN_PACKS && all.len() >= MIN_RULES,
        "subject scan collapsed: {packs} packs / {} rules, floor is {MIN_PACKS}/{MIN_RULES}",
        all.len()
    );

    let (_, landing_pins) = delivered_pins(&all);
    assert!(
        landing_pins.len() >= MIN_LANDING_PINS,
        "landing-pin scan collapsed: found {} of at least {MIN_LANDING_PINS}. A landing pin is only \
         ever removed by removing the landing itself, so a drop here means the scan stopped seeing \
         the pins. Found: {landing_pins:?}",
        landing_pins.len()
    );

    let landings = landing_texts();
    let carriers: BTreeMap<&String, Vec<&String>> = all
        .iter()
        .filter_map(|(id, message)| {
            let names: Vec<&String> = landings
                .iter()
                .filter(|(_, text)| message.contains(text.as_str()))
                .map(|(name, _)| name)
                .collect();
            (!names.is_empty()).then_some((id, names))
        })
        .collect();

    // THE COVERAGE ASSERTION, in the same shape axis A uses: the two buckets are "carries a landing
    // and pins it" and "carries no landing", and a rule outside both is one whose stated cost sits
    // wherever an edit last left it.
    let uncovered: Vec<(&&String, &Vec<&String>)> = carriers
        .iter()
        .filter(|(id, _)| !landing_pins.contains(**id))
        .collect();
    assert!(
        uncovered.is_empty(),
        "{} DSL rule(s) carry a registered LANDING that no test positions: {uncovered:?}\n\
         A landing says what the reader's own correct edit costs, and a reader who acts on the first \
         instruction pays that cost before reading about it (rule-quality.md §33, §37). Pin it with \
         `assert_landing_precedes_imperative` on a delivered finding. This is the failure mode the \
         2026-08-30 census named: landings travel as shared constants, so a family that gains a \
         member the pins do not follow is exactly the case that goes quiet.",
        uncovered.len()
    );
}

/// Every landing pin names a REGISTERED constant, which is what keeps axis B's registry honest.
///
/// The registry is built from the `const ..._LANDING` declarations in this tree, so a pin that passed
/// a bare string literal instead would position a landing that the registry cannot see — and every
/// OTHER rule splicing that same sentence would then read as a non-carrier. Requiring the constant is
/// also §37's own discipline stated as a check: one spelling per family, byte-identical, because a
/// position pin needs one spelling to index.
#[test]
fn landing_pins_name_a_registered_constant() {
    let (_, all) = all_rules();
    let (_, landing_pins) = delivered_pins(&all);
    let landings = landing_texts();
    assert!(
        !landings.is_empty(),
        "the landing-constant registry is empty. Every landing in this tree is declared as a \
         `const ..._LANDING: &str = \"...\";`, so an empty registry is a broken scan, not a tree \
         without landings."
    );
    let orphans: Vec<&String> = landing_pins
        .iter()
        .filter(|id| {
            let message = &all[*id];
            !landings
                .values()
                .any(|text| message.contains(text.as_str()))
        })
        .collect();
    assert!(
        orphans.is_empty(),
        "{} rule(s) carry a landing pin whose text is not a registered landing constant: \
         {orphans:?}\n\
         Declare the landing as a `const ..._LANDING: &str` and pass that constant to \
         `assert_landing_precedes_imperative`. An inlined needle positions one message and leaves \
         every sibling that splices the same sentence invisible to the coverage check above.",
        orphans.len()
    );
}
