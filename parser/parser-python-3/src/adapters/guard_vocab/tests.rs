//! Unit tests for the Python guard-name vocabulary (`super`) — split out of it for the file-size cap.
use super::{is_guard_name, PythonGuardVocab};

/// Seals every guard name this crate's two producers actually rely on in the shipped corpus — if
/// one stops matching, the corresponding real route silently loses its exemption.
#[test]
fn accepts_the_corpus_guard_names() {
    for name in [
        // corpus/oss/be-fastapi
        "get_current_user_authorizer",
        "check_article_modification_permissions",
        "check_comment_modification_permissions",
        // corpus/oss/be-fastapi-fs
        "get_current_active_superuser",
        "get_current_user",
        "CurrentUser",
        // corpus/oss/be-django (DRF permission classes)
        "IsAuthenticated",
        "IsAuthenticatedOrReadOnly",
    ] {
        assert!(
            is_guard_name(name, &PythonGuardVocab::built_in()),
            "{name} should read as a guard"
        );
    }
}

/// Seals the precision side: every one of these is a real non-guard `Depends(...)` argument or
/// permission class from the same corpus checkouts. Accepting any of them would silently suppress a
/// genuine missing-auth finding.
#[test]
fn rejects_the_corpus_non_guard_dependencies() {
    for name in [
        "get_repository",
        "get_app_settings",
        "get_db",
        "get_articles_filters",
        "get_article_by_slug_from_path",
        "get_profile_by_username_from_path",
        "get_comment_by_id_from_path",
        "SessionDep",
        "AllowAny",
    ] {
        assert!(
            !is_guard_name(name, &PythonGuardVocab::built_in()),
            "{name} must not read as a guard"
        );
    }
}

/// Seals the `author*` veto — the one lookaround-shaped rule in the vocabulary. A Conduit-style app
/// is full of `author` names, and a bare-substring `auth` rule would clear every route touching one.
#[test]
fn author_names_are_not_guards_but_authorize_still_is() {
    assert!(!is_guard_name("get_author", &PythonGuardVocab::built_in()));
    assert!(!is_guard_name("author_id", &PythonGuardVocab::built_in()));
    assert!(!is_guard_name(
        "ArticleAuthorRepository",
        &PythonGuardVocab::built_in()
    ));
    assert!(is_guard_name(
        "authorize_request",
        &PythonGuardVocab::built_in()
    ));
}

/// Seals veto axis 1: a dependency that hands the handler `None` for an anonymous caller is the
/// OPPOSITE of a gate, so it must never suppress a finding. `_get_current_user_optional` is a real
/// callable in `corpus/oss/be-fastapi/app/api/dependencies/authentication.py`.
#[test]
fn anonymous_permitting_names_are_not_guards() {
    for name in [
        "_get_current_user_optional",
        "get_current_user_or_none",
        "maybe_current_user",
        "optional_current_user",
    ] {
        assert!(
            !is_guard_name(name, &PythonGuardVocab::built_in()),
            "{name} must not read as a guard"
        );
    }
    // The veto must not eat `IsAuthenticatedOrReadOnly`, a real DRF permission class in the corpus.
    assert!(is_guard_name(
        "IsAuthenticatedOrReadOnly",
        &PythonGuardVocab::built_in()
    ));
}

/// Seals veto axis 2: a NOUN-form producer names what it returns, not a decision. Accepting any of
/// these clears a route on evidence that never rejects anything.
#[test]
fn noun_form_producers_are_not_guards() {
    for name in [
        "get_authorization_header",
        "list_permissions",
        "PermissionSerializer",
        "permission_denied_handler",
        "get_superuser_stats",
        "SuperuserMetrics",
    ] {
        assert!(
            !is_guard_name(name, &PythonGuardVocab::built_in()),
            "{name} must not read as a guard"
        );
    }
}

/// Seals the ABSENCE of an `oauth` mask (module doc). `reusable_oauth2` is the real
/// `corpus/oss/be-fastapi-fs/backend/app/api/deps.py` bearer scheme and `oauth2_scheme` is the name
/// FastAPI's own tutorial uses; both reach this vocabulary only as `Depends(...)` arguments, where
/// they name a scheme that raises 401. Masking `oauth` killed exactly those and nothing else.
#[test]
fn oauth_scheme_names_read_as_guards() {
    for name in ["reusable_oauth2", "oauth2_scheme", "oauth_authorize"] {
        assert!(
            is_guard_name(name, &PythonGuardVocab::built_in()),
            "{name} should read as a guard"
        );
    }
    // The `author` mask is unaffected: it still leaves a second, genuine `auth` behind.
    assert!(is_guard_name(
        "author_auth_check",
        &PythonGuardVocab::built_in()
    ));
}

/// Seals the deliberately-absent bare words: accepting them is the recall/precision trade this
/// vocabulary refuses (see module doc).
#[test]
fn bare_session_token_and_role_words_are_not_guards() {
    for name in [
        "get_session",
        "create_access_token",
        "tokenizer",
        "user_role",
        "owner_of",
    ] {
        assert!(
            !is_guard_name(name, &PythonGuardVocab::built_in()),
            "{name} must not read as a guard"
        );
    }
}
