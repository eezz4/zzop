use super::*;

#[test]
fn attribute_controller_composes_class_and_method_route() {
    let src = r#"
        [ApiController]
        [Route("api/[controller]")]
        public class UsersController {
            [HttpGet("{id}")]
            public string Get(int id) { return ""; }
        }
    "#;
    let provides = extract_csharp_http_provides("f.cs", src);
    let p = provides
        .iter()
        .find(|p| p.symbol.as_deref() == Some("Get"))
        .unwrap();
    assert_eq!(p.key, "GET /api/users/{}");
    assert_eq!(p.kind, "http");
}

#[test]
fn controller_name_suffix_gates_without_attribute() {
    let src = r#"
        public class OrdersController {
            [HttpPost]
            public string Create() { return ""; }
        }
    "#;
    let provides = extract_csharp_http_provides("f.cs", src);
    assert_eq!(provides.len(), 1);
    assert_eq!(provides[0].key, "POST /");
}

#[test]
fn non_controller_class_emits_nothing() {
    let src = r#"
        public class PlainClass {
            [HttpGet]
            public string Get() { return ""; }
        }
    "#;
    assert!(extract_csharp_http_provides("f.cs", src).is_empty());
}

#[test]
fn route_only_method_with_no_verb_is_skipped() {
    let src = r#"
        [ApiController]
        public class UsersController {
            [Route("/x")]
            public string Get() { return ""; }
        }
    "#;
    assert!(extract_csharp_http_provides("f.cs", src).is_empty());
}

#[test]
fn route_and_http_verb_together_use_the_route_path() {
    let src = r#"
        [ApiController]
        public class UsersController {
            [HttpGet]
            [Route("special")]
            public string Get() { return ""; }
        }
    "#;
    let provides = extract_csharp_http_provides("f.cs", src);
    assert_eq!(provides[0].key, "GET /special");
}

#[test]
fn non_literal_method_path_drops_the_route() {
    // `[HttpGet(Routes.List)]` — the path is a `const`-string reference (valid C#) this pass cannot
    // resolve. The old `first_quoted_string(..).unwrap_or_default()` keyed a phantom `GET /` (empty base);
    // the tri-state now drops it. The sibling literal route survives.
    let src = r#"
        [ApiController]
        public class UsersController {
            [HttpGet(Routes.List)]
            public string List() { return ""; }
            [HttpPost("create")]
            public string Create() { return ""; }
        }
    "#;
    let provides = extract_csharp_http_provides("f.cs", src);
    let keys: Vec<&str> = provides.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["POST /create"], "{keys:?}");
}

#[test]
fn non_literal_class_route_prefix_blocks_the_classes_routes() {
    // `[Route(ApiRoutes.Base)]` — a non-literal class prefix. Keying the method route at the empty base
    // would fabricate a phantom under the wrong (missing) prefix, so the class's own routes are blocked.
    let src = r#"
        [ApiController]
        [Route(ApiRoutes.Base)]
        public class UsersController {
            [HttpGet("{id}")]
            public string Get(int id) { return ""; }
        }
    "#;
    assert!(extract_csharp_http_provides("f.cs", src).is_empty());
}

#[test]
fn non_literal_class_prefix_does_not_block_a_nested_independently_gated_controller() {
    // The outer class's routes are blocked by its non-literal prefix, but a nested controller gates
    // independently on its own (literal) annotations — the block must not leak into it.
    let src = r#"
        [ApiController]
        [Route(ApiRoutes.Base)]
        public class OuterController {
            [HttpGet("{id}")]
            public string Get(int id) { return ""; }

            [ApiController]
            [Route("inner")]
            public class InnerController {
                [HttpGet("ping")]
                public string Ping() { return ""; }
            }
        }
    "#;
    let provides = extract_csharp_http_provides("f.cs", src);
    let keys: Vec<&str> = provides.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["GET /inner/ping"], "{keys:?}");
}

#[test]
fn a_non_literal_http_verb_path_falls_back_to_a_literal_route_attr() {
    // `[HttpGet(Routes.List)]` (non-literal) + `[Route("x")]` (literal) — both attributes register routes;
    // the `HttpX` template is unknown but the `[Route]` template is a known endpoint, so we surface it
    // rather than dropping the whole method (a literal on EITHER attribute is a route we can key).
    let src = r#"
        [ApiController]
        public class UsersController {
            [HttpGet(Routes.List)]
            [Route("x")]
            public string List() { return ""; }
        }
    "#;
    let provides = extract_csharp_http_provides("f.cs", src);
    let keys: Vec<&str> = provides.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["GET /x"], "{keys:?}");
}

#[test]
fn bare_http_verb_falling_back_to_a_non_literal_route_attr_drops_the_route() {
    // A bare `[HttpGet]` falls back to a co-located `[Route]`'s path — but here that `[Route]` is itself a
    // non-literal constant reference, so the resolved path is unknown and the route is dropped.
    let src = r#"
        [ApiController]
        public class UsersController {
            [HttpGet]
            [Route(Routes.List)]
            public string List() { return ""; }
        }
    "#;
    assert!(extract_csharp_http_provides("f.cs", src).is_empty());
}

#[test]
fn the_anchor_is_the_routing_attribute_not_the_methods_first_attribute() {
    // Every other fixture in this file puts the routing attribute FIRST, so the `method_declaration`'s own
    // start row coincided with it — and no fixture here asserted `line` at all, which is the stronger
    // reason none of them could see the defect. Measured 2026-08-24 on `corpus/frameworks/aspnetcore`:
    // 13 of 448 attribute routes anchored one line ABOVE their route attribute, on a NON-route attribute,
    // and 7 of those on a security attribute. A non-routing attribute above the routing one separates
    // the two nodes.
    let src = "[ApiController]\n[Route(\"api/[controller]\")]\npublic class PetController {\n    [Authorize(\"pet-store-writer\")]\n    [HttpPost(\"add-pet\")]\n    public string Add() { return \"\"; }\n}\n";
    let provides = extract_csharp_http_provides("f.cs", src);
    let p = provides
        .iter()
        .find(|p| p.key == "POST /api/pet/add-pet")
        .expect("the route is still extracted");
    assert_eq!(
        p.line, 5,
        "anchor must be the `[HttpPost]` line, not the `[Authorize]` above it"
    );
}

#[test]
fn the_anchor_is_the_route_attribute_when_that_is_where_the_path_came_from() {
    // `[Route("x")]` + a bare `[HttpGet]`: the emitted key's PATH was read off the `[Route]` attribute, so
    // that is the attribute the route was read from and the line the reader must land on. Anchoring on the
    // verb attribute instead would point at a line carrying no path at all.
    let src = "[ApiController]\npublic class UsersController {\n    [Produces(\"application/json\")]\n    [Route(\"special\")]\n    [HttpGet]\n    public string Get() { return \"\"; }\n}\n";
    let provides = extract_csharp_http_provides("f.cs", src);
    let p = provides
        .iter()
        .find(|p| p.key == "GET /special")
        .expect("the route is still extracted");
    assert_eq!(p.line, 4, "anchor must be the `[Route(\"special\")]` line");
}

#[test]
fn minimal_api_map_get_is_extracted() {
    let src = r#"
        var app = builder.Build();
        app.MapGet("/x", () => "ok");
    "#;
    let provides = extract_csharp_http_provides("f.cs", src);
    let p = provides.iter().find(|p| p.key == "GET /x").unwrap();
    assert!(p.symbol.is_none());
}

#[test]
fn minimal_api_grouped_route_composes_prefix() {
    let src = r#"app.MapGroup("/api").MapGet("/y", Handler);"#;
    let provides = extract_csharp_http_provides("f.cs", src);
    let p = provides.iter().find(|p| p.key == "GET /api/y").unwrap();
    assert_eq!(p.symbol.as_deref(), Some("Handler"));
}

#[test]
fn minimal_api_non_literal_path_is_skipped() {
    let src = r#"app.MapGet(path, Handler);"#;
    assert!(extract_csharp_http_provides("f.cs", src).is_empty());
}

#[test]
fn minimal_api_cross_statement_group_variable_is_not_mis_keyed() {
    // `api` is a cross-statement `MapGroup("/api")` variable whose prefix this v1 cannot see. Emitting a
    // bare `GET /ping` would be a WRONG key (real route is `/api/ping`), so it must be SKIPPED entirely —
    // never a prefix-less guess. Only the `app` root's own bare route is kept.
    let src = r#"
        var app = builder.Build();
        app.MapGet("/health", () => "ok");
        var api = app.MapGroup("/api");
        api.MapGet("/ping", () => "pong");
    "#;
    let provides = extract_csharp_http_provides("f.cs", src);
    assert!(provides.iter().any(|p| p.key == "GET /health"));
    assert!(
        !provides.iter().any(|p| p.key == "GET /ping"),
        "bare non-`app` receiver must not emit a prefix-less key"
    );
    assert!(!provides.iter().any(|p| p.key == "GET /api/ping"));
}

#[test]
fn nested_controller_gates_independently() {
    let src = r#"
        public class Outer {
            [ApiController]
            public class InnerController {
                [HttpGet]
                public string Get() { return ""; }
            }
        }
    "#;
    let provides = extract_csharp_http_provides("f.cs", src);
    assert_eq!(provides.len(), 1);
}

#[test]
fn empty_on_parse_failure() {
    assert!(extract_csharp_http_provides("f.cs", "\u{0}\u{1}not csharp{{{{").is_empty());
}
