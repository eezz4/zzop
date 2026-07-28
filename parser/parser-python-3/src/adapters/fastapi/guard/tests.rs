use super::*;

// NOTE ON FIXTURE SHAPE: every fixture below is a raw string whose FIRST source line is the module's
// line 1, so an asserted line number reads straight off the literal. Python is indentation-sensitive, so
// these must not use `\`-continuation (which strips leading whitespace).

fn no_aliases() -> BTreeSet<String> {
    BTreeSet::new()
}

fn lines(text: &str) -> Vec<u32> {
    extract_fastapi_guarded_lines("app/api/routes/x.py", text, &no_aliases())
}

fn lines_with(text: &str, aliases: &[&str]) -> Vec<u32> {
    let set: BTreeSet<String> = aliases.iter().map(|s| s.to_string()).collect();
    extract_fastapi_guarded_lines("app/api/routes/x.py", text, &set)
}

/// SHAPE FROM CORPUS: `corpus/oss/be-fastapi/app/api/routes/articles/articles_resource.py` lines 89-99
/// (`@router.put(..., dependencies=[Depends(check_article_modification_permissions)])`).
/// Seals shape 1 — the decorator dependency list — AND that the emitted line is the DECORATOR's own
/// start line, which is exactly where `extract_fastapi_router_fragments` anchors the provide.
#[test]
fn decorator_dependencies_list_guards_that_decorators_line() {
    let text = r#"from fastapi import APIRouter, Body, Depends
router = APIRouter()

@router.put(
    "/{slug}",
    dependencies=[Depends(check_article_modification_permissions)],
)
async def update_article_by_slug(article_update = None):
    return None
"#;
    assert_eq!(lines(text), vec![4]);
}

/// SHAPE FROM CORPUS: `corpus/oss/be-fastapi/app/api/routes/articles/articles_resource.py` lines 53-62
/// (`user: User = Depends(get_current_user_authorizer())`). Seals shape 2 — a parameter default whose
/// `Depends` argument is a FACTORY CALL, the dominant idiom in that checkout.
#[test]
fn parameter_default_depends_factory_call_guards_the_route() {
    let text = r#"from fastapi import APIRouter, Depends
router = APIRouter()

@router.post("")
async def create_new_article(
    user: User = Depends(get_current_user_authorizer()),
):
    return None
"#;
    assert_eq!(lines(text), vec![4]);
}

/// SHAPE FROM CORPUS: `corpus/oss/be-fastapi/app/api/routes/articles/articles_resource.py` lines 82-86
/// (`article: Article = Depends(get_article_by_slug_from_path)`) — a real dependency that is NOT a
/// guard. Seals the precision direction: a route whose only `Depends` are plumbing must keep firing.
#[test]
fn non_guard_dependencies_do_not_clear_the_route() {
    let text = r#"from fastapi import APIRouter, Depends
router = APIRouter()

@router.delete("/{slug}")
async def delete_article_by_slug(
    article: Article = Depends(get_article_by_slug_from_path),
    articles_repo: ArticlesRepository = Depends(get_repository(ArticlesRepository)),
):
    return None
"#;
    assert!(lines(text).is_empty());
}

/// SHAPE FROM CORPUS: `corpus/oss/be-fastapi-fs/backend/app/api/deps.py` — the alias block VERBATIM,
/// including the `reusable_oauth2` binding and the `TokenDep` alias that depends on it (both are in the
/// real file; a fixture that drops them would let this test's precision assertion pass on a shape the
/// producer never actually meets) — plus `.../api/routes/items.py`
/// (`def create_item(*, session: SessionDep, current_user: CurrentUser, item_in: ItemCreate)`).
/// Seals shape 4 and its resolution gate together — the alias is declared in a DIFFERENT file, so the
/// route only clears when the tree-wide alias set actually carries it AND this file imports the name.
#[test]
fn tree_resolved_annotated_alias_guards_the_route_only_when_resolved() {
    let deps = r#"from fastapi import Depends
from fastapi.security import OAuth2PasswordBearer
from typing import Annotated
reusable_oauth2 = OAuth2PasswordBearer(tokenUrl="/login/access-token")
SessionDep = Annotated[Session, Depends(get_db)]
TokenDep = Annotated[str, Depends(reusable_oauth2)]
CurrentUser = Annotated[User, Depends(get_current_user)]
"#;
    let aliases = extract_python_guard_aliases(deps);
    // Every top-level subscript alias is reported, each with its own verdict — the engine needs the
    // `false`s to see a same-name disagreement. `SessionDep` injects `get_db` and is not a gate.
    // `TokenDep` injects `reusable_oauth2`, which THIS FILE binds to `OAuth2PasswordBearer(...)` with no
    // `auto_error=` — the scheme's default is `auto_error=True`, so it raises 401 on a caller with no
    // bearer token. It IS a gate, and the verdict comes from that construction, not from the spelling.
    assert_eq!(
        aliases,
        vec![
            ("SessionDep".to_string(), false),
            ("TokenDep".to_string(), true),
            ("CurrentUser".to_string(), true),
        ]
    );

    let route = r#"from fastapi import APIRouter
from app.api.deps import CurrentUser, SessionDep
router = APIRouter(prefix="/items")

@router.post("/")
def create_item(*, session: SessionDep, current_user: CurrentUser, item_in: ItemCreate):
    return None
"#;
    assert_eq!(lines_with(route, &["CurrentUser"]), vec![5]);
    // Unresolved (this route's own annotations were never seen as guard aliases) => never judged, so
    // the finding still fires.
    assert!(lines_with(route, &["SomeOtherAlias"]).is_empty());
    assert!(lines(route).is_empty());
}

/// Seals the shape-4 BINDING check (module doc, "Shape 4 is BOUND-name resolution"): a tree-wide guard
/// alias name must also be bound HERE. An unrelated module whose own `class CurrentUser(BaseModel)` is a
/// plain pydantic body model must not have its anonymous POST cleared by a same-named alias declared in
/// some `deps.py` it never imports.
#[test]
fn tree_alias_name_does_not_clear_a_route_that_never_bound_it() {
    let route = r#"from fastapi import APIRouter
from pydantic import BaseModel
router = APIRouter()

class CurrentUser(BaseModel):
    name: str

@router.post("/x")
def create(payload: CurrentUser):
    return None
"#;
    assert!(lines_with(route, &["CurrentUser"]).is_empty());
}

/// Seals the LOCAL-declaration half of the same rule: a file that declares the alias itself needs no
/// import, and its own verdict wins over the tree-wide set (a local `CurrentUser` that injects a
/// non-guard must not be cleared by another file's same-named guard alias).
#[test]
fn a_locally_declared_alias_answers_for_its_own_file() {
    let guarded = r#"from fastapi import APIRouter, Depends
from typing import Annotated
CurrentUser = Annotated[User, Depends(get_current_user)]
router = APIRouter()

@router.post("/x")
def create(user: CurrentUser):
    return None
"#;
    assert_eq!(lines(guarded), vec![6]);

    let shadowed = r#"from fastapi import APIRouter, Depends
from typing import Annotated
CurrentUser = Annotated[User, Depends(get_db)]
router = APIRouter()

@router.post("/x")
def create(user: CurrentUser):
    return None
"#;
    assert!(lines_with(shadowed, &["CurrentUser"]).is_empty());
}

/// Seals the anonymous-permitting factory switch: `get_current_user_authorizer(required=False)` is
/// `corpus/oss/be-fastapi/app/api/dependencies/authentication.py`'s own opt-out — it returns
/// `_get_current_user_optional`, which hands the handler `None` rather than rejecting. The callee NAME is
/// identical to the guarded call one line away, so only the argument can tell them apart.
#[test]
fn a_required_false_factory_call_is_not_a_guard() {
    let optional = r#"from fastapi import APIRouter, Depends
router = APIRouter()

@router.post("")
async def create_new_article(
    user: Optional[User] = Depends(get_current_user_authorizer(required=False)),
):
    return None
"#;
    assert!(lines(optional).is_empty());

    // An unreadable switch value is undecidable, so it is treated the same way — recall, never precision.
    let dynamic = optional.replace("required=False", "required=settings.strict");
    assert!(lines(&dynamic).is_empty());
}

/// Seals the route-decorator gate's parity with the PROVIDE side: `@router.api_route` mints a provide
/// only when it carries a literal `methods=` list, so a bare `api_route` must mint no guard line either —
/// otherwise the module doc's "an emitted line always coincides with a provide's anchor" is false.
#[test]
fn api_route_without_a_literal_methods_list_anchors_nothing() {
    let bare = r#"from fastapi import APIRouter, Depends
router = APIRouter()

@router.api_route("/x")
async def h(user: User = Depends(get_current_user)):
    return None
"#;
    assert!(lines(bare).is_empty());

    let with_methods = r#"from fastapi import APIRouter, Depends
router = APIRouter()

@router.api_route("/x", methods=["POST"])
async def h(user: User = Depends(get_current_user)):
    return None
"#;
    assert_eq!(lines(with_methods), vec![4]);
}

/// SHAPE FROM CORPUS: `corpus/oss/be-fastapi-fs/backend/app/api/routes/users.py` line 214
/// (`@router.delete("/{user_id}", dependencies=[Depends(get_current_active_superuser)])`) — the second
/// framework checkout's own decorator idiom, with a BARE (non-factory) `Depends` argument.
#[test]
fn bare_depends_argument_in_decorator_dependencies_is_judged() {
    let text = r#"from fastapi import APIRouter, Depends
router = APIRouter(prefix="/users")

@router.delete("/{user_id}", dependencies=[Depends(get_current_active_superuser)])
def delete_user(session: SessionDep, user_id: str):
    return None
"#;
    assert_eq!(lines(text), vec![4]);
}

/// Seals shape 3 — an inline `Annotated[..., Depends(...)]` parameter annotation, resolved with no
/// tree-wide alias input at all.
#[test]
fn inline_annotated_depends_parameter_guards_the_route() {
    let text = r#"from fastapi import Depends, FastAPI
from typing import Annotated
app = FastAPI()

@app.patch("/me")
def update_me(user: Annotated[User, Depends(get_current_user)]):
    return None
"#;
    assert_eq!(lines(text), vec![5]);
}

/// Seals the receiver gate: a decorator on a name this crate does not project routes from can never
/// mint a guard line (an emitted line would then belong to no provide — or worse, to another one).
#[test]
fn decorator_on_an_unrecognized_receiver_is_ignored() {
    let text = r#"from fastapi import APIRouter, Depends

@celery.post("/x", dependencies=[Depends(get_current_user)])
def task():
    return None
"#;
    assert!(lines(text).is_empty());
}

/// Seals the import gate — a file that never imports fastapi yields nothing, never a bare-name guess.
#[test]
fn import_gate_and_parse_failure_yield_nothing() {
    let text = r#"router = APIRouter()

@router.post("/x", dependencies=[Depends(get_current_user)])
def f():
    return None
"#;
    assert!(lines(text).is_empty());
    assert!(lines("from fastapi import APIRouter\ndef f(:\n").is_empty());
    assert!(extract_python_guard_aliases("def f(:\n").is_empty());
}

/// SHAPE FROM UPSTREAM: FastAPI's own security tutorial (`oauth2_scheme = OAuth2PasswordBearer(
/// tokenUrl="token")` + `token: Annotated[str, Depends(oauth2_scheme)]`) — the canonical bearer scheme,
/// and the single most common way a FastAPI route is authenticated. Seals that it reads as a gate: the
/// scheme raises 401 for a caller with no credentials, so a route injecting it must NOT fire.
#[test]
fn the_canonical_bearer_scheme_guards_the_route() {
    let annotated = r#"from fastapi import APIRouter, Depends
from fastapi.security import OAuth2PasswordBearer
from typing import Annotated
router = APIRouter()
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

@router.post("/items/")
def create_item(token: Annotated[str, Depends(oauth2_scheme)]):
    return None
"#;
    assert_eq!(lines(annotated), vec![7]);

    // Same scheme as a parameter default — shape 2 reaches the same binding verdict.
    let default = r#"from fastapi import APIRouter, Depends
from fastapi.security import OAuth2PasswordBearer
router = APIRouter()
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

@router.post("/items/")
def create_item(token: str = Depends(oauth2_scheme)):
    return None
"#;
    assert_eq!(lines(default), vec![6]);
}

/// Seals the reverse hole the binding verdict exists for: the SAME name and the SAME `Depends(...)`
/// expression, with the anonymous switch turned off one statement earlier. `auto_error=False` makes the
/// scheme hand the handler `None` instead of raising, so the route is not guarded — and the `Depends`
/// site alone cannot see that.
#[test]
fn a_scheme_bound_with_auto_error_false_is_not_a_guard() {
    let text = r#"from fastapi import APIRouter, Depends
from fastapi.security import OAuth2PasswordBearer
router = APIRouter()
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token", auto_error=False)

@router.post("/items/")
def create_item(token: str = Depends(oauth2_scheme)):
    return None
"#;
    assert!(lines(text).is_empty());

    // An unreadable switch value is undecidable AT A VISIBLE CONSTRUCTION, so it costs recall too.
    let dynamic = text.replace("auto_error=False", "auto_error=settings.STRICT");
    assert!(lines(&dynamic).is_empty());

    // The same shape on a plain factory binding (not a fastapi scheme class) is judged identically.
    let factory = r#"from fastapi import APIRouter, Depends
router = APIRouter()
authorizer = get_current_user_authorizer(required=False)

@router.post("/items/")
def create_item(user = Depends(authorizer)):
    return None
"#;
    assert!(lines(factory).is_empty());
}

/// Seals that a RECOGNIZED scheme construction beats the name vocabulary's `header` noun-form veto:
/// `api_key_header` is spelled exactly like the `get_authorization_header` extractor the veto targets,
/// but `APIKeyHeader(...)` is a gate. Without the binding verdict this route fires — a false positive on
/// a genuinely authenticated route.
#[test]
fn an_api_key_header_scheme_beats_the_header_noun_veto() {
    let text = r#"from fastapi import APIRouter, Depends
from fastapi.security import APIKeyHeader
router = APIRouter()
api_key_header = APIKeyHeader(name="X-API-Key")

@router.post("/items/")
def create_item(key: str = Depends(api_key_header)):
    return None
"#;
    assert_eq!(lines(text), vec![6]);

    // The name is still not a guard on its own — an extractor with no visible construction stays vetoed.
    let extractor = r#"from fastapi import APIRouter, Depends
router = APIRouter()

@router.post("/items/")
def create_item(key: str = Depends(get_authorization_header)):
    return None
"#;
    assert!(lines(extractor).is_empty());
}

/// Seals the BOUNDARY the binding verdict must not cross (module `depends`'s "absence of a binding is
/// not undecidable"): a `Depends` name this file does not construct is judged by the vocabulary, exactly
/// as before. Treating an unresolved name as an unreadable switch would reject the dominant corpus shape
/// — an imported guard callable — and silently disable this producer.
#[test]
fn a_dependency_name_with_no_visible_construction_is_judged_by_name() {
    let text = r#"from fastapi import APIRouter, Depends
from app.api.deps import get_current_user, get_db
router = APIRouter()

@router.post("/a")
def create(user = Depends(get_current_user)):
    return None

@router.post("/b")
def create_b(db = Depends(get_db)):
    return None
"#;
    assert_eq!(lines(text), vec![5]);
}

/// Seals the OTHER half of the provide-parity claim in `is_route_decorator`'s doc: `collect_verb_entries`
/// requires a LITERAL path, so a constant-path decorator mints no provide — and must therefore mint no
/// guard line either. Before this mirroring the guard line was emitted with nothing to anchor to.
#[test]
fn a_non_literal_route_path_anchors_nothing() {
    let constant = r#"from fastapi import APIRouter, Depends
router = APIRouter()
ROOT = "/x"

@router.post(ROOT)
def create(user = Depends(get_current_user)):
    return None
"#;
    assert!(super::super::extract_fastapi_router_fragments("x.py", constant).is_empty());
    assert!(lines(constant).is_empty());

    // The keyword path form is a literal and DOES mint a provide, so it keeps its guard line.
    let keyword = r#"from fastapi import APIRouter, Depends
router = APIRouter()

@router.post(path="/x")
def create(user = Depends(get_current_user)):
    return None
"#;
    assert_eq!(lines(keyword), vec![4]);
}

/// Seals that signature evidence covers EVERY route decorator stacked on one function, while a
/// decorator's own `dependencies=` covers only itself.
#[test]
fn signature_evidence_covers_all_decorators_of_one_function() {
    let text = r#"from fastapi import APIRouter, Depends
router = APIRouter()

@router.post("/a")
@router.put("/a")
def f(user: User = Depends(get_current_user_authorizer())):
    return None
"#;
    assert_eq!(lines(text), vec![4, 5]);
}
