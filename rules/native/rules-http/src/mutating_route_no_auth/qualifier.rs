//! Qualifier-segment guard evidence — the class-name half of `is_guard_id`'s match granularity
//! (see the parent module doc). Split from the tail check because class names need STRICTER
//! matching than method names: the loose substring `DEFAULT_AUTH_GUARD_PATTERN` that works for
//! verb-shaped method names (`verifyToken`, `checkPermission`) false-clears on noun-shaped class
//! names that merely CONTAIN a token — `AuthorRepository` ⊃ `auth`, `OracleClient` ⊃ `acl`,
//! `TokenizerService` ⊃ `token` (opus review, feature batch 2026-07-18): a `POST /articles`
//! handler calling `authorRepository.save(...)` would have silently cleared, a recall regression
//! in a security rule. So the qualifier arm splits the class name into camelCase/underscore
//! tokens and requires an EXACT token match against the noun vocabulary below.

/// Class-name tokens counting as auth-guard evidence when present as an EXACT (case-insensitive)
/// camel/underscore token of the receiver-class qualifier — never substring. `AuthorizationService`
/// tokenizes to [authorization, service] (hit); `AuthorRepository` to [author, repository] (no
/// hit). Noun-shaped by design — the method-name (tail) arm keeps the looser substring pattern;
/// this list only decides what a CLASS name proves. `role`/`token`/`session` stay in (their exact
/// tokens are authz-domain nouns); their collision superstrings (`Tokenizer`, `RoleModel`-style
/// derived words) don't tokenize to them, which is the whole point of exact-token matching.
pub(super) const QUALIFIER_GUARD_TOKENS: &[&str] = &[
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
    "token",
    "tokens",
    "role",
    "roles",
    "admin",
    "owner",
    "session",
    "sessions",
];

/// Whether a receiver-class qualifier segment proves auth-guard evidence — exact-token matching
/// per [`QUALIFIER_GUARD_TOKENS`]'s doc.
pub(super) fn qualifier_is_guard(qualifier: &str) -> bool {
    camel_tokens(qualifier)
        .iter()
        .any(|t| QUALIFIER_GUARD_TOKENS.contains(&t.to_ascii_lowercase().as_str()))
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
    use super::qualifier_is_guard;

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
            "RoleMapper",
        ] {
            assert!(qualifier_is_guard(q), "{q} should count as guard evidence");
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
                !qualifier_is_guard(q),
                "{q} must NOT count as guard evidence"
            );
        }
    }
}
