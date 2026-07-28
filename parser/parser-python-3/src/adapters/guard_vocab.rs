//! The ONE Python auth-guard NAME vocabulary, shared by this crate's two guard producers
//! (`adapters::fastapi::guard`'s `Depends(...)` judgment and `adapters::django::guard`'s
//! `permission_classes` judgment). One list on purpose: a second, drifting guard-word list is how a
//! security recognizer silently rots (the same reason `zzop_parser_typescript::adapters::
//! router_mounts::guard` keeps its two positions on one vocabulary).
//!
//! ## Precision-first, deliberately narrower than the BFS vocabulary
//! A name judged here SUPPRESSES a `mutating-route-no-auth` finding (it feeds the framework-neutral
//! `decorator_guarded` / `auth-guarded` channel), so a false accept is a silent, undiscoverable miss on
//! the consumer side while a false reject only costs recall — the finding still fires and a human still
//! looks. That is why this is NOT `zzop_rules_http::DEFAULT_AUTH_GUARD_PATTERN` (the BFS's own recall-
//! first vocabulary): bare `session`, bare `token`, bare `role`, bare `owner` and the env-axis words are
//! all absent here on purpose.
//!
//! ## Why it is not shared with the TypeScript producer either (deliberate T3 divergence)
//! `parser-typescript`'s `is_guard_name` judges CAMEL-case Express middleware names; this one judges
//! SNAKE-case Python dependency callables, and the two vocabularies do not overlap where it matters —
//! `get_current_active_superuser` and `login_required` are Python-idiomatic guard names that the TS list
//! rejects outright, while `clerkMiddleware`/`expressjwt` name libraries that do not exist in Python.
//! Sharing a symbol would force one list to carry both ecosystems' false-accept surface. The shapes
//! recognized here come from the real corpus checkouts named in each producer's own tests.
//!
//! Matching is done on a NORMALIZED name: lowercased with `_` removed, so `get_current_user` and
//! `getCurrentUser` judge identically and every needle below can be written without separators.
//!
//! ## The VETO layer runs first — the mechanism is ecosystem-neutral
//! The T3 divergence above is about the VOCABULARY (snake-case Python callables vs camel-case Express
//! middleware), not about the veto MECHANISM: `zzop_parser_typescript::adapters::router_mounts::guard`
//! already answers "a name containing a guard word is still not a gate" with unconditional, first-running
//! veto lists (`ROUTER_NAME_VETO_SUFFIXES`, `ENV_AXIS_VETO_SUBSTRINGS`), and that mechanism transplants
//! verbatim. Three axes are vetoed here, each measured against real Python/DRF naming:
//! - [`ANONYMOUS_VETO_SUBSTRINGS`] — the callable RETURNS `None` for an anonymous caller instead of
//!   rejecting it. `get_current_user_optional` / `maybe_current_user` are the exact opposite of a gate.
//! - [`REPORT_VETO_SUFFIXES`] / [`REPORT_VETO_PREFIXES`] — a NOUN-form producer, not a check:
//!   `get_authorization_header` (an extractor), `PermissionSerializer` (a representation),
//!   `permission_denied_handler` (the error view), `get_superuser_stats` / `SuperuserMetrics`,
//!   `list_permissions`.
//!
//! ## There is deliberately NO `oauth` mask (tried 2026-07-27, reverted the same day)
//! `oauth` contains `auth`, so `reusable_oauth2` / `oauth2_scheme` clear the bare-`auth` arm — and that
//! is CORRECT. A masking arm (`n.replace("oauth", "")`) was added to suppress `get_oauth_provider` /
//! `oauth_callback`, but those are route HANDLERS and PROVIDERS: the FastAPI producer judges only the
//! name inside a `Depends(...)` argument, a position neither ever occupies. The mask's only reachable
//! effect was therefore to un-recognize the genuine gate — `oauth2_scheme = OAuth2PasswordBearer(...)`
//! injected as `Depends(oauth2_scheme)` is the canonical FastAPI bearer scheme, and it raises 401 for a
//! caller with no credentials because `auto_error` defaults to `True`. A scheme configured NOT to
//! reject (`OAuth2PasswordBearer(..., auto_error=False)`) is caught STRUCTURALLY at its binding site by
//! `adapters::fastapi::guard::depends`, never by a name rule. Do not re-add the mask.

/// Guard-certain substrings of the normalized name. Ordered longest-intent-first for readability only —
/// the check is a plain "any of these occurs".
const GUARD_SUBSTRINGS: &[&str] = &[
    // Authorization / authentication stems. Checked BEFORE the bare-`auth` rule below so `authorize`
    // (which contains `author`) is never vetoed by it.
    "authoriz",
    "authentic",
    // The FastAPI "who is calling" dependency family. `get_current_user` / `CurrentUser` /
    // `get_current_active_superuser` all raise 401/403 before the handler runs, so they ARE the gate.
    "currentuser",
    "activeuser",
    "superuser",
    "staffuser",
    // Explicit permission/role gates.
    "permission",
    "loginrequired",
    "isauthenticated",
    "isadminuser",
    "requirelogin",
    "requireauth",
    // Credential-shaped gates, anchored (bare `token` is deliberately absent — `tokenizer`,
    // `token_bucket`, `create_access_token` are not gates).
    "verifytoken",
    "checktoken",
    "jwtrequired",
    "apikeyrequired",
];

/// Anonymous-permitting words — module doc's veto axis 1. A dependency spelled with one of these hands
/// the handler `None` when no credentials were presented; the request still runs. Vetoing costs recall
/// only on a name that mixes both readings, which is the safe direction here.
const ANONYMOUS_VETO_SUBSTRINGS: &[&str] = &["optional", "ornone", "maybe", "anonymous"];

/// Noun-form producer TAILS — module doc's veto axis 2. Each names a thing the callable RETURNS or
/// RENDERS (a header, a serialized representation, an error view, a report), never a decision to reject.
///
/// `header` is the one tail that a real gate also wears: `api_key_header = APIKeyHeader(name=...)` is a
/// fastapi security scheme, not an extractor like `get_authorization_header`. The NAME cannot tell those
/// apart, so it is not asked to — `adapters::fastapi::guard::depends` recognizes the scheme
/// CONSTRUCTION at its binding site and answers for that name before this vocabulary is consulted.
const REPORT_VETO_SUFFIXES: &[&str] = &[
    "header",
    "headers",
    "serializer",
    "serializers",
    "handler",
    "stats",
    "metrics",
    "report",
    "summary",
];

/// Read/report VERB heads — module doc's veto axis 2. `list_permissions` enumerates permissions; it does
/// not require one. No corpus guard name starts with any of these.
const REPORT_VETO_PREFIXES: &[&str] = &["list", "count", "serialize", "render", "format"];

/// The four lists above, in the shape both producers take — all four are declarable
/// (`vocabulary.pythonGuard*`) because all four name things a PROJECT chooses: what its dependency
/// callables are called, and which naming shapes mean "this reads/renders" rather than "this rejects".
/// Grouped rather than passed as four positional `&[&str]`s, which are transposable by accident.
#[derive(Clone, Copy)]
pub struct PythonGuardVocab<'a> {
    pub substrings: &'a [&'a str],
    pub anonymous_veto_substrings: &'a [&'a str],
    pub report_veto_prefixes: &'a [&'a str],
    pub report_veto_suffixes: &'a [&'a str],
}

impl PythonGuardVocab<'static> {
    /// The built-in defaults — the single accessor `zzop_engine::VocabularyConfig::built_in` reads, so
    /// the four constants stay private to this module and there is no second copy of any of them.
    pub fn built_in() -> Self {
        PythonGuardVocab {
            substrings: GUARD_SUBSTRINGS,
            anonymous_veto_substrings: ANONYMOUS_VETO_SUBSTRINGS,
            report_veto_prefixes: REPORT_VETO_PREFIXES,
            report_veto_suffixes: REPORT_VETO_SUFFIXES,
        }
    }
}

/// True when `name` reads as a Python auth guard — see module doc. `name` may be written in any case or
/// separator style; it is normalized here. The veto layer runs FIRST and unconditionally, exactly as the
/// TypeScript sibling's does: a name this producer cannot read as a DECISION must never suppress a
/// finding, whatever guard word it happens to contain.
pub(crate) fn is_guard_name(name: &str, vocab: &PythonGuardVocab<'_>) -> bool {
    let n: String = name
        .chars()
        .filter(|c| *c != '_')
        .flat_map(char::to_lowercase)
        .collect();
    if is_vetoed(&n, vocab) {
        return false;
    }
    if vocab.substrings.iter().any(|needle| n.contains(needle)) {
        return true;
    }
    // Bare `auth` — but never the `auth` that is only there because the name says `author*`
    // (`get_author`, `author_id`). `author` is MASKED OUT rather than tested with `!contains`, so a name
    // carrying both a real `auth` and a decoy (`author_auth_check`) still matches. `authorize`/
    // `authentic*` already returned true above on the unmasked name. Rust's `regex` crate has no
    // lookaround, so this is an explicit predicate rather than one pattern. `oauth` is NOT masked — see
    // the module doc for why that mask was removed.
    n.replace("author", "").contains("auth")
}

/// The veto layer — module doc. Order among the three axes does not matter (any hit rejects); the layer
/// as a whole runs before every guard rule.
fn is_vetoed(n: &str, vocab: &PythonGuardVocab<'_>) -> bool {
    vocab
        .anonymous_veto_substrings
        .iter()
        .any(|v| n.contains(v))
        || vocab.report_veto_suffixes.iter().any(|v| n.ends_with(v))
        || vocab.report_veto_prefixes.iter().any(|v| n.starts_with(v))
}

#[cfg(test)]
mod tests;
