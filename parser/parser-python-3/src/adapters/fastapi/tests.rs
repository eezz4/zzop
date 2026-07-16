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
