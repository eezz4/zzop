use super::*;

fn frag<'a>(out: &'a [RouterMountFragment], name: &str) -> &'a RouterMountFragment {
    out.iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no fragment named {name:?} in {out:?}"))
}

#[test]
fn no_fastapi_import_yields_nothing() {
    let src = "app = FastAPI()\n\n@app.get(\"/x\")\ndef h():\n    return 1\n";
    assert!(extract_fastapi_router_fragments("a.py", src).is_empty());
}

#[test]
fn app_and_router_verbs() {
    let src = concat!(
        "from fastapi import FastAPI, APIRouter\n",
        "app = FastAPI()\n",
        "router = APIRouter()\n",
        "\n",
        "@app.get(\"/health\")\n",
        "def health():\n",
        "    return 1\n",
        "\n",
        "@router.post(\"/items\")\n",
        "def create_item():\n",
        "    return 1\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/health".into(),
            handler: Some("health".into()),
            line: 5,
            attr_keys: vec![],
        }]
    );
    assert_eq!(
        frag(&out, "router").entries,
        vec![RouterMountEntry::Verb {
            method: "POST".into(),
            path: "/items".into(),
            handler: Some("create_item".into()),
            line: 9,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn dotted_qualified_receiver_call_is_recognized() {
    let src = concat!(
        "import fastapi\n",
        "app = fastapi.FastAPI()\n",
        "\n",
        "@app.get(\"/x\")\n",
        "def h():\n",
        "    return 1\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert_eq!(frag(&out, "app").entries.len(), 1);
}

#[test]
fn api_route_emits_one_entry_per_declared_method() {
    // `@app.api_route(path, methods=[...])` is the generic form the verb shortcuts wrap — each listed
    // method is a real route and must be emitted (not dropped for the unknown `api_route` attr name).
    let src = concat!(
        "from fastapi import FastAPI\n",
        "app = FastAPI()\n",
        "\n",
        "@app.api_route(\"/items\", methods=[\"GET\", \"POST\"])\n",
        "def items():\n",
        "    return 1\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    let mut methods: Vec<&str> = frag(&out, "app")
        .entries
        .iter()
        .map(|e| match e {
            RouterMountEntry::Verb { method, path, .. } => {
                assert_eq!(path, "/items");
                method.as_str()
            }
            _ => panic!("expected Verb"),
        })
        .collect();
    methods.sort_unstable();
    assert_eq!(methods, vec!["GET", "POST"]);
}

#[test]
fn api_route_with_a_repeated_method_emits_that_verb_once() {
    // `methods=["GET", "GET"]` (or a case-variant repeat) must mint ONE GET provide, not two — a single
    // decorator can't be a real duplicate route, so `duplicate-route` must not see a phantom collision.
    let src = concat!(
        "from fastapi import FastAPI\n",
        "app = FastAPI()\n",
        "\n",
        "@app.api_route(\"/items\", methods=[\"GET\", \"get\", \"POST\"])\n",
        "def items():\n",
        "    return 1\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    let mut methods: Vec<&str> = frag(&out, "app")
        .entries
        .iter()
        .map(|e| match e {
            RouterMountEntry::Verb { method, path, .. } => {
                assert_eq!(path, "/items");
                method.as_str()
            }
            _ => panic!("expected Verb"),
        })
        .collect();
    methods.sort_unstable();
    assert_eq!(methods, vec!["GET", "POST"]);
}

#[test]
fn keyword_path_argument_is_recognized() {
    // `@app.get(path="/x")` — the keyword form of the route path (valid FastAPI) must not be dropped.
    let src = concat!(
        "from fastapi import FastAPI\n",
        "app = FastAPI()\n",
        "\n",
        "@app.get(path=\"/kw\")\n",
        "def h():\n",
        "    return 1\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert_eq!(frag(&out, "app").entries.len(), 1, "{out:?}");
}

#[test]
fn api_router_literal_prefix_is_precomposed_onto_verb_paths() {
    let src = concat!(
        "from fastapi import APIRouter\n",
        "router = APIRouter(prefix=\"/items\")\n",
        "\n",
        "@router.get(\"/{id}\")\n",
        "def get_item():\n",
        "    return 1\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert_eq!(
        frag(&out, "router").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/items/{id}".into(),
            handler: Some("get_item".into()),
            line: 4,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn api_router_non_literal_prefix_skips_every_verb() {
    let src = concat!(
        "from fastapi import APIRouter\n",
        "PREFIX = compute_prefix()\n",
        "router = APIRouter(prefix=PREFIX)\n",
        "\n",
        "@router.get(\"/x\")\n",
        "def h():\n",
        "    return 1\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert!(
        out.iter().all(|f| f.name != "router"),
        "non-literal prefix must veto every verb, yielding no fragment: {out:?}"
    );
}

#[test]
fn include_router_with_literal_prefix_and_imported_ident() {
    let src = concat!(
        "from fastapi import FastAPI\n",
        "from .routers import items\n",
        "app = FastAPI()\n",
        "app.include_router(items, prefix=\"/api\")\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/api".into(),
            ident: "items".into(),
            specifier: Some("./routers".into()),
            attr_keys: vec![],
        }]
    );
}

#[test]
fn include_router_without_prefix_mounts_at_root() {
    let src = concat!(
        "from fastapi import FastAPI, APIRouter\n",
        "app = FastAPI()\n",
        "local_router = APIRouter()\n",
        "app.include_router(local_router)\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/".into(),
            ident: "local_router".into(),
            specifier: None,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn include_router_with_module_attribute_child_resolves_to_the_target_module() {
    // The canonical FastAPI form `import <mod>; router.include_router(<mod>.router, prefix=...)`. The
    // `specifier` is the target module's full dotted path (base binding's specifier + "." + original) and
    // `ident` is the BASE module name (`authentication`), not the `.router` attribute — the base name is
    // distinct per module so it doesn't poison the composition's root-exclusion-by-name (every FastAPI
    // router is named `router`). The engine resolves the module to its file and picks its sole fragment.
    let src = concat!(
        "from fastapi import APIRouter\n",
        "from app.api.routes import authentication\n",
        "router = APIRouter()\n",
        "router.include_router(authentication.router, prefix=\"/users\")\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert_eq!(
        frag(&out, "router").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/users".into(),
            ident: "authentication".into(),
            specifier: Some("app.api.routes.authentication".into()),
            attr_keys: vec![],
        }]
    );
}

#[test]
fn include_router_with_attribute_child_whose_base_is_not_imported_is_skipped() {
    // `<base>.router` where `base` is not a known import can't be traced to a module — never guessed.
    let src = concat!(
        "from fastapi import APIRouter\n",
        "router = APIRouter()\n",
        "router.include_router(mystery.router, prefix=\"/x\")\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert!(out.iter().all(|f| f.name != "router"), "{out:?}");
}

#[test]
fn include_router_non_literal_prefix_skips_the_mount() {
    let src = concat!(
        "from fastapi import FastAPI\n",
        "from .routers import items\n",
        "app = FastAPI()\n",
        "p = compute_prefix()\n",
        "app.include_router(items, prefix=p)\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert!(out.iter().all(|f| f.name != "app"), "{out:?}");
}

#[test]
fn include_router_non_identifier_first_arg_is_skipped() {
    let src = concat!(
        "from fastapi import FastAPI\n",
        "app = FastAPI()\n",
        "app.include_router(make_router(), prefix=\"/api\")\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn non_literal_verb_path_skips_only_that_decorator() {
    let src = concat!(
        "from fastapi import FastAPI\n",
        "app = FastAPI()\n",
        "PATH = compute_path()\n",
        "\n",
        "@app.get(PATH)\n",
        "def dynamic():\n",
        "    return 1\n",
        "\n",
        "@app.get(\"/ok\")\n",
        "def ok():\n",
        "    return 1\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/ok".into(),
            handler: Some("ok".into()),
            line: 9,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn unrelated_receiver_decorator_is_not_a_route() {
    let src = concat!(
        "from fastapi import FastAPI\n",
        "app = FastAPI()\n",
        "\n",
        "@cache.get(\"/x\")\n",
        "def h():\n",
        "    return 1\n",
    );
    let out = extract_fastapi_router_fragments("a.py", src);
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn deterministic_across_repeated_extractions() {
    let src = concat!(
        "from fastapi import FastAPI\n",
        "app = FastAPI()\n",
        "\n",
        "@app.get(\"/x\")\n",
        "def h():\n",
        "    return 1\n",
    );
    let a = extract_fastapi_router_fragments("a.py", src);
    let b = extract_fastapi_router_fragments("a.py", src);
    assert_eq!(a, b);
}

#[test]
fn parse_failure_yields_empty_vec() {
    assert!(extract_fastapi_router_fragments("bad.py", "def f(:\n").is_empty());
}

#[test]
fn empty_file_yields_empty_vec() {
    assert!(extract_fastapi_router_fragments("e.py", "").is_empty());
}
