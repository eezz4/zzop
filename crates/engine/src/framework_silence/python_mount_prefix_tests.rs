//! S14's tests. The corpus shape (`be-fastapi-fs`) is distilled into a fixture here rather than read
//! from `corpus/`, which is gitignored — same convention the Java/Python call-graph fixtures follow.

use super::*;
use crate::framework_silence::tests::TempDir;

/// One `.py` file's warning, with a tree that produced routes.
fn warn(name: &str, body: &str) -> Option<String> {
    let dir = TempDir::new("zzop-py-mount");
    dir.write(name, body);
    python_mount_prefix_warning(dir.path(), &[name.to_string()], 5)
}

/// A DOTTED ref is no longer this line's business: it rides as a `RouterMountEntry::MountRef` and the
/// composer either resolves it (nothing to report) or names it precisely (its own warning). Firing here
/// too would tell a tree whose prefix resolved perfectly that its routes may be mis-keyed.
#[test]
fn a_dotted_ref_is_left_to_the_composer() {
    assert!(warn(
        "app/main.py",
        "app.include_router(api_router, prefix=settings.API_V1_STR)
"
    )
    .is_none());
}

/// What remains: shapes that can never become a ref, so nothing downstream will ever speak for them.
#[test]
fn a_shape_that_can_never_become_a_ref_is_reported() {
    let w = warn(
        "app/main.py",
        "app.include_router(api_router, prefix=get_settings().api_prefix)\n",
    )
    .expect("warning");
    assert!(w.contains("get_settings()"), "{w}");
    assert!(w.contains("app/main.py"), "{w}");
    assert!(w.contains("wrong key"), "names the failure mode: {w}");
    assert!(
        w.contains("crossLayer.unconsumedProvides"),
        "names where the other half is sitting: {w}"
    );
}

/// A literal prefix is exactly what the parser DOES read — reporting it would be crying wolf.
#[test]
fn a_literal_prefix_is_silent() {
    assert!(warn(
        "app/main.py",
        "app.include_router(api_router, prefix=\"/api/v1\")\n"
    )
    .is_none());
    assert!(warn(
        "app/main.py",
        "app.include_router(api_router, prefix='/api/v1')\n"
    )
    .is_none());
}

/// No prefix at all mounts at the root — nothing unread.
#[test]
fn no_prefix_is_silent() {
    assert!(warn(
        "app/api/main.py",
        "api_router.include_router(users.router)\n"
    )
    .is_none());
}

/// An f-string is a different AST node, so the parser skips it too — this must not be read as literal.
#[test]
fn an_f_string_prefix_is_reported() {
    let w = warn(
        "app/main.py",
        "app.include_router(r, prefix=f\"{settings.ROOT}/v1\")\n",
    )
    .expect("an f-string prefix is not a StringLiteral to the parser either");
    assert!(w.contains("prefix=f\""), "{w}");
}

/// The corpus spells this across lines with a trailing comma.
#[test]
fn a_multiline_call_is_read() {
    let w = warn(
        "app/main.py",
        "app.include_router(\n    api_router,\n    prefix=build_prefix(),\n    tags=[\"api\"],\n)\n",
    )
    .expect("warning");
    assert!(w.contains("build_prefix()"), "{w}");
    assert!(
        !w.contains("tags"),
        "the argument must stop at its comma: {w}"
    );
}

/// With no routes extracted, S1 is already saying something larger and this line would bury it.
#[test]
fn a_tree_with_no_http_provides_is_silent() {
    let dir = TempDir::new("zzop-py-mount-zero");
    dir.write(
        "app/main.py",
        "app.include_router(api_router, prefix=settings.API_V1_STR)\n",
    );
    assert!(python_mount_prefix_warning(dir.path(), &["app/main.py".to_string()], 0).is_none());
}

/// A file that never mentions the call contributes nothing, and a different function with a similar
/// name is not this one.
#[test]
fn unrelated_python_is_silent() {
    assert!(warn("app/util.py", "def f():\n    return 1\n").is_none());
    assert!(warn("app/util.py", "my_include_router(r, prefix=settings.X)\n").is_none());
}

/// `prefix` appearing as a substring of another keyword must not bind.
#[test]
fn a_lookalike_keyword_does_not_bind() {
    assert!(warn(
        "app/main.py",
        "app.include_router(r, url_prefix=\"/api\")\n"
    )
    .is_none());
}

/// Several mounts in one file are counted, and the count is exact even though examples are capped.
#[test]
fn every_mount_is_counted_and_examples_are_capped() {
    let mut body = String::new();
    for i in 0..5 {
        body.push_str(&format!("app.include_router(r{i}, prefix=make_p{i}())\n"));
    }
    let w = warn("app/main.py", &body).expect("warning");
    assert!(w.contains("5 `include_router(...)` call(s)"), "{w}");
    assert!(w.contains("make_p0()") && w.contains("make_p2()"), "{w}");
    assert!(
        !w.contains("make_p3()"),
        "examples must stop at MAX_EXAMPLES: {w}"
    );
}
