//! Qualifier-segment guard evidence — the class-name half of `is_guard_id`'s match granularity
//! (see the parent module doc). TWO independent gates must both pass before a receiver-class name
//! counts as auth-guard evidence.
//!
//! ## Gate 1 — the qualifier must name a symbol THIS TREE DECLARES (existence)
//! `zzop_core::callgraph::resolve_method` mints `<file>#<Receiver>.<method>` from the receiver's
//! IMPORT BINDING alone: once the name is imported and its module resolves, the id is built WITHOUT
//! ever checking that a `SourceSymbol` answers to it. So a visited node's qualifier is not
//! necessarily a class — it is whatever text stood in the receiver position, and treating it as a
//! class name is exactly the guess this engine refuses to make elsewhere ("unresolved beats guessed").
//!
//! What exposed it: `corpus/oss/be-fastapi-fs`, where `SessionDep = Annotated[Session, Depends(get_db)]`
//! is a type ALIAS (and not a symbol `parser-python-3` extracts at all — its `const_symbol` takes
//! uppercase LITERAL assignments only), so `session: SessionDep` plus `session.add(...)` minted the
//! phantom node `deps.py#SessionDep.add`, which tokenizes to [Session, Dep] and cleared the route
//! through the then-present `session` token. Gate 2 has since dropped `session` for its own reasons,
//! but the MECHANISM is vocabulary-independent and still live for every kept token: a
//! `PermissionDep`/`AuthDep`-style alias, and — the shape `corpus/oss/be-fastapi` really has — a
//! MODULE receiver, `from app.services import jwt` + `jwt.get_username_from_token(...)`, whose
//! qualifier `jwt` is a file, not a declared symbol. So a qualifier counts only when it is the simple
//! name of some declared symbol (`http_scan::build_name_index`'s keys).
//!
//! Price, disclosed, and it is LANGUAGE-GENERAL — not the Python idiom alone, which is what an earlier
//! draft of this paragraph priced. Two populations lose their class-name vote:
//! 1. **Anything the resolver cannot tell apart from an alias.** It cannot distinguish a MODULE from a
//!    type alias from a class, so admitting module receivers would re-admit aliases — the Python
//!    module-qualifier idiom (`from app.core import security` + `security.check(...)`) goes with them.
//!    Measured cost on the corpus: zero, because every such node there also matches the TAIL arm.
//! 2. **A guard class that lives in a THIRD-PARTY package**, which no in-tree symbol declares by
//!    definition. `import org.apache.shiro.SecurityUtils;` + `SecurityUtils.getSubject()` used to clear
//!    a route on the `security` token and no longer does; same for `SecurityContextHolder`. NOT measured
//!    on this corpus (no tree here uses Shiro, and a Spring app that does this normally also ships a
//!    `SecurityFilterChain`, whose posture producer exempts the route earlier — so the residual is
//!    multi-posture trees, a bailed-out lambda DSL, and non-Spring Java security libraries).
//!
//! Both are RECALL costs — the finding fires where it previously stayed quiet — which is the side this
//! rule errs on by construction (Info severity, middleware-blind by its own admission). The alternative
//! is the direction that costs PRECISION, and precision loss here means an unauthenticated write route
//! disappears, which is the defect this gate exists to fix.
//!
//! The TAIL (method-name) arm needs no such gate and deliberately does not get one: `RawCall::
//! callee_name` is the method name SPELLED at a real call site in real source, so it is observed text
//! whether or not resolution placed the receiver correctly. Only the RECEIVER's identity is the part
//! resolution invents, so only the qualifier arm rests on an unchecked claim.
//!
//! The gate is on the qualifier NAME, never on the whole visited id, because a whole-id check would
//! delete Java's evidence wholesale: `run_callgraph_rules`' Java `resolve_file` is an opaque-specifier
//! stand-in, so a Java id's FILE segment is a package name (`io.spring.core.service.
//! AuthorizationService#AuthorizationService.canWriteComment`) that no real `SourceSymbol` id can ever
//! equal, while the class `AuthorizationService` IS declared and IS the evidence. Bonus: a BARE call
//! id's qualifier is its file-extension token (`ts`, `py`), which nothing declares — the gate retires
//! that residual too, instead of leaving it "harmless either way".
//!
//! ## Gate 2 — exact camel-token match, never substring
//! Class names need STRICTER matching than method names: the loose substring
//! `DEFAULT_AUTH_GUARD_PATTERN` that works for verb-shaped method names (`verifyToken`,
//! `checkPermission`) false-clears on noun-shaped class names that merely CONTAIN a token —
//! `AuthorRepository` ⊃ `auth`, `OracleClient` ⊃ `acl`, `TokenizerService` ⊃ `token` (opus review,
//! feature batch 2026-07-18): a `POST /articles` handler calling `authorRepository.save(...)` would
//! have silently cleared, a recall regression in a security rule. So the class name is split into
//! camelCase/underscore tokens and an EXACT token match against the noun vocabulary is required.

use std::collections::HashMap;

/// Class-name tokens counting as auth-guard evidence when present as an EXACT (case-insensitive)
/// camel/underscore token of the receiver-class qualifier — never substring. `AuthorizationService`
/// tokenizes to [authorization, service] (hit); `AuthorRepository` to [author, repository] (no hit).
/// Noun-shaped by design — the method-name (tail) arm keeps the looser substring pattern; this list
/// only decides what a CLASS name proves.
///
/// ## Every entry names the ACT of access control, never the thing access control is ABOUT
/// The list originally also carried `session`/`sessions`, `token`/`tokens`, `role`/`roles`, `admin`
/// and `owner`. Those are DOMAIN-ENTITY nouns: a system HAS sessions, tokens, roles, admins and
/// owners, and classes named after them are overwhelmingly data/domain classes, not guards. Measured
/// on `corpus/oss/spring-petclinic` (an app with NO authentication anywhere): `owner` cleared
/// `OwnerController.processCreationForm` — the pet-OWNER controller clearing its own unauthenticated
/// `POST /owners/new` — because `OwnerController` tokenizes to [Owner, Controller]. The same class of
/// collision is structural elsewhere, not incidental: Hibernate's `Session`/`SessionFactory` and
/// SQLAlchemy/SQLModel's `Session` make `session.save(x)` a DATABASE WRITE in the two ecosystems this
/// arm exists for, and Python DI spells the same words (`SessionDep`, `TokenDep`, `SessionLocal`).
/// So the rule is now: a token stays only if it names access control ITSELF (`authorization`,
/// `permission`, `guard`, `acl`, `rbac`, `security`, `jwt` — a token FORMAT no business domain owns).
///
/// What that costs, stated plainly: a security class whose ONLY signal is one of the dropped nouns
/// and whose called METHOD is not verb-shaped auth vocabulary (`TokenStore.lookup()`,
/// `SessionManager.load()`) no longer clears. Two things absorb most of it — `RoleGuard`/`AdminGuard`/
/// `OwnershipGuard` still hit via `guard`, and the TAIL arm's `DEFAULT_AUTH_GUARD_PATTERN` still
/// carries `session|token|owner|admin|role` as substrings, so any call whose METHOD name says one of
/// those words (`verifySession`, `requireGuildOwner`, `checkAdminRole`) clears exactly as before.
/// Only the class-name-alone route was withdrawn, and it was withdrawn where it was guessing.
///
/// Residual, disclosed rather than fixed: `security` is itself a domain entity in finance (a tradable
/// security), so `SecurityRepository.save()` in a trading system would still false-clear. Kept anyway
/// — it is the canonical Spring Security naming and dropping it would blind the arm on its main
/// Java target.
pub const QUALIFIER_GUARD_TOKENS: &[&str] = &[
    "auth",
    "authz",
    "authn",
    "authorization",
    "authentication",
    "authenticator",
    "security",
    "permission",
    "permissions",
    "guard",
    "guards",
    "acl",
    "rbac",
    "jwt",
];

/// Whether a receiver-class qualifier segment proves auth-guard evidence — BOTH gates in the module
/// doc. `declared_names` is `http_scan::build_name_index`'s output (simple name -> symbol ids); only
/// KEY PRESENCE is read, i.e. "does this tree declare anything under that name at all".
/// `tokens` is the caller's guard-class vocabulary — [`QUALIFIER_GUARD_TOKENS`] unless the run declared
/// its own (`vocabulary.authGuardQualifierTokens`), since what a project calls its guard classes is a
/// name the project picks.
pub(super) fn is_guard(
    qualifier: &str,
    declared_names: &HashMap<String, Vec<String>>,
    tokens: &[&str],
) -> bool {
    declared_names.contains_key(qualifier) && tokens_are_guard(qualifier, tokens)
}

/// Gate 2 alone — exact camel-token match against the guard-class vocabulary.
fn tokens_are_guard(qualifier: &str, tokens: &[&str]) -> bool {
    camel_tokens(qualifier)
        .iter()
        .any(|t| tokens.contains(&zzop_core::vocab_norm::ascii_lowercase(t).as_str()))
}

/// Split an identifier into its camelCase/underscore words: `AuthorizationService` ->
/// [Authorization, Service]; `ACLManager` -> [ACL, Manager] (an all-caps run breaks before its
/// last capital when a lowercase follows); `auth_service` -> [auth, service]. Non-alphanumeric
/// characters separate; digits stay attached to their token.
fn camel_tokens(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut tokens = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let prev = i.checked_sub(1).and_then(|j| chars.get(j));
        let next = chars.get(i + 1);
        let boundary = c.is_uppercase()
            && (prev.is_some_and(|p| p.is_lowercase() || p.is_numeric())
                || (prev.is_some_and(|p| p.is_uppercase())
                    && next.is_some_and(|n| n.is_lowercase())));
        if boundary && !cur.is_empty() {
            tokens.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::{is_guard, tokens_are_guard, QUALIFIER_GUARD_TOKENS};
    use std::collections::HashMap;

    /// Every name in `names` counts as declared, the shape `build_name_index` produces.
    fn declared(names: &[&str]) -> HashMap<String, Vec<String>> {
        names
            .iter()
            .map(|n| (n.to_string(), vec![format!("some/file.ts#{n}")]))
            .collect()
    }

    #[test]
    fn guard_shaped_class_names_are_evidence() {
        for q in [
            "AuthorizationService",
            "AuthGuard",
            "ACLManager",
            "SecurityConfig",
            "auth_service",
            "JwtFilter",
            "PermissionChecker",
            "RoleGuard",
        ] {
            assert!(
                is_guard(q, &declared(&[q]), QUALIFIER_GUARD_TOKENS),
                "{q} should count as evidence"
            );
        }
    }

    #[test]
    fn domain_nouns_containing_guard_substrings_are_not_evidence() {
        // The opus-review failure class: substring matching cleared routes over these.
        for q in [
            "AuthorRepository",
            "AuthorService",
            "OracleClient",
            "ObstacleService",
            "MiracleService",
            "TokenizerService",
            "Authoring",
        ] {
            assert!(
                !is_guard(q, &declared(&[q]), QUALIFIER_GUARD_TOKENS),
                "{q} must NOT be evidence"
            );
        }
    }

    /// SEALS the vocabulary narrowing: a class named after the ENTITY access control is about proves
    /// nothing about access control. `OwnerController` is verbatim `corpus/oss/spring-petclinic`,
    /// where it cleared an unauthenticated `POST /owners/new` in an app with no auth at all; the rest
    /// are the ecosystems' own collisions (Hibernate/SQLModel sessions, RBAC data classes). Each is
    /// DECLARED here, so Gate 1 cannot be the reason — the vocabulary itself has to reject them.
    #[test]
    fn entity_nouns_access_control_is_merely_about_are_not_evidence() {
        for q in [
            "OwnerController",
            "SessionFactory",
            "Session",
            "TokenRepository",
            "RoleMapper",
            "AdminController",
        ] {
            assert!(
                !is_guard(q, &declared(&[q]), QUALIFIER_GUARD_TOKENS),
                "{q} must NOT be evidence"
            );
        }
    }

    /// Seals Gate 1 at its own level: a guard-token-shaped name that NOTHING in the tree declares is
    /// not evidence, even though its tokens hit the vocabulary — the phantom-receiver class.
    #[test]
    fn a_guard_token_name_no_symbol_declares_is_not_evidence() {
        // `jwt` is `corpus/oss/be-fastapi`'s real module receiver (`from app.services import jwt`):
        // a FILE, never a declared symbol. The other two are the alias shape.
        for q in ["jwt", "AuthDep", "PermissionContext"] {
            assert!(
                tokens_are_guard(q, QUALIFIER_GUARD_TOKENS),
                "{q} must hit gate 2 (else vacuous)"
            );
            assert!(
                !is_guard(q, &declared(&["SomethingElse"]), QUALIFIER_GUARD_TOKENS),
                "{q} is declared by nothing — must NOT be evidence"
            );
        }
    }
}
