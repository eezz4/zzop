use super::*;

fn opts() -> IoOptions {
    IoOptions::default()
}

/// `extract_file_io` under the BUILT-IN convention vocabulary — every case below is about extraction
/// shapes, not about which names a project declared.
fn extract_file_io_default(rel: &str, text: &str, opts: &IoOptions) -> Option<zzop_core::IoFacts> {
    let declared = crate::VocabularyConfig::default();
    extract_file_io(rel, text, opts, &declared.resolve())
}

#[test]
fn no_io_in_a_plain_file_is_none() {
    assert!(extract_file_io_default("a.ts", "export const a = 1;\n", &opts()).is_none());
}

#[test]
fn csharp_route_provides_project_when_degraded_but_consumes_are_gated_off() {
    // A `.cs` file carrying both a controller route AND an `HttpClient` egress call. When the CST
    // parse degrades (one broken sibling method elsewhere), the route PROVIDE must still project
    // (Java-parity) while the egress CONSUME is gated off (egress-parity) — the split this test pins.
    let src = concat!(
        "using System.Net.Http;\n",
        "using Microsoft.AspNetCore.Mvc;\n",
        "[ApiController]\n",
        "[Route(\"api/users\")]\n",
        "public class UsersController {\n",
        "    [HttpGet]\n",
        "    public string Get() {\n",
        "        var client = new HttpClient();\n",
        "        var r = client.GetAsync(\"http://svc/data\");\n",
        "        return \"\";\n",
        "    }\n",
        "}\n",
    );
    let degraded =
        extract_csharp_file_io("Users.cs", src, true).expect("routes even when degraded");
    assert!(
        !degraded.provides.is_empty(),
        "routes must project when degraded"
    );
    assert!(
        degraded.consumes.is_empty(),
        "consumes must be gated off when degraded"
    );

    let fresh = extract_csharp_file_io("Users.cs", src, false).expect("both when not degraded");
    assert!(!fresh.provides.is_empty());
    assert!(
        !fresh.consumes.is_empty(),
        "consumes project when not degraded"
    );
}

#[test]
fn captures_fe_http_egress_consume() {
    let io = extract_file_io_default("Ctx.tsx", r#"axios.get("/authen/getUserInfo");"#, &opts())
        .expect("expected io facts");
    assert!(io.provides.is_empty());
    assert_eq!(io.consumes.len(), 1);
    assert_eq!(
        io.consumes[0].key.as_deref(),
        Some("GET /authen/getUserInfo")
    );
    assert_eq!(io.consumes[0].file, "Ctx.tsx");
}

#[test]
fn file_local_constant_indirection_still_resolves() {
    let src = r#"const ControlKey = { AUTHEN: { getUserInfo: "/authen/getUserInfo" } };
axios.get(ControlKey.AUTHEN.getUserInfo);"#;
    let io = extract_file_io_default("Ctx.tsx", src, &opts()).expect("expected io facts");
    assert_eq!(
        io.consumes[0].key.as_deref(),
        Some("GET /authen/getUserInfo")
    );
}

#[test]
fn cross_file_constant_indirection_is_unresolved_at_this_one_file_call_site() {
    // Same indirection shape as egress.rs's own test, but the constant lives in a different file
    // this one-file-slice call never sees — see module doc. `analyze::late_resolve_cross_file_consumes`
    // resolves this shape end to end (see lib.rs's e2e test).
    let io = extract_file_io_default(
        "Ctx.tsx",
        "axios.get(ControlKey.AUTHEN.getUserInfo);",
        &opts(),
    )
    .expect("expected io facts (unresolved consume is still reported)");
    assert_eq!(io.consumes.len(), 1);
    assert!(io.consumes[0].key.is_none());
    assert_eq!(io.consumes[0].method.as_deref(), Some("GET"));
    assert_eq!(
        io.consumes[0].raw.as_deref(),
        Some("ControlKey.AUTHEN.getUserInfo")
    );
}

#[test]
fn hono_route_provides_no_longer_come_from_the_per_file_pass() {
    // Router provides now come from the fragment-then-compose pipeline (module doc), not this
    // per-file pass — a Hono file with no egress/Nest facts yields nothing here.
    let src = "const apiRoutes = new Hono();\napiRoutes.get(\"/users\", api.listUsers);\n";
    assert!(extract_file_io_default("routes/apiRoutes.ts", src, &opts()).is_none());
}

#[test]
fn captures_nestjs_controller_route_provide_through_the_fused_seam() {
    let src = "@Controller('users')\nclass UsersController {\n  @Get(':id')\n  findOne() {}\n}\n";
    let io =
        extract_file_io_default("users.controller.ts", src, &opts()).expect("expected io facts");
    assert!(io.consumes.is_empty());
    assert_eq!(io.provides.len(), 1);
    assert_eq!(io.provides[0].key, "GET /users/{}");
    assert_eq!(io.provides[0].line, 3);
    assert_eq!(io.provides[0].symbol.as_deref(), Some("findOne"));
}

#[test]
fn captures_nest_global_prefix_marker_through_the_fused_seam() {
    let src = "app.setGlobalPrefix('api');\n";
    let io = extract_file_io_default("main.ts", src, &opts()).expect("expected io facts");
    assert!(io.consumes.is_empty());
    assert_eq!(io.provides.len(), 1);
    assert_eq!(io.provides[0].kind, "nest-global-prefix");
    assert_eq!(io.provides[0].key, "api");
}

#[test]
fn captures_hono_client_consume_through_the_fused_seam() {
    let src = "import { hc } from 'hono/client';\nconst client = hc<T>('/api/auth');\nclient.signout.$post();\n";
    let io = extract_file_io_default("client.ts", src, &opts()).expect("expected io facts");
    assert!(io.provides.is_empty());
    assert_eq!(io.consumes.len(), 1);
    assert_eq!(io.consumes[0].kind, "http");
    assert_eq!(
        io.consumes[0].key.as_deref(),
        Some("POST /api/auth/signout")
    );
    assert_eq!(io.consumes[0].file, "client.ts");
}

// --- extract_java_file_io ---

#[test]
fn no_java_io_in_a_plain_class_is_none() {
    assert!(extract_java_file_io("C.java", "class C {}\n", false).is_none());
}

#[test]
fn captures_spring_get_mapping_provide_with_no_consumes() {
    let src = "@RestController\nclass CtrlAuthen {\n  @GetMapping(\"/getUserInfo\")\n  UserInfo getUserInfo() { return null; }\n}\n";
    let io = extract_java_file_io("CtrlAuthen.java", src, false).expect("expected io facts");
    assert!(io.consumes.is_empty());
    assert_eq!(io.provides.len(), 1);
    assert_eq!(io.provides[0].key, "GET /getUserInfo");
    assert_eq!(io.provides[0].symbol.as_deref(), Some("getUserInfo"));
}

#[test]
fn java_provides_survive_degraded_but_consumes_are_gated() {
    // Same split extract_csharp_file_io pins above: PROVIDES (Spring routes + JPA entities) project
    // regardless of `degraded`; egress CONSUMES only on a trusted parse.
    let src = concat!(
        "import org.springframework.web.client.RestTemplate;\n",
        "import jakarta.persistence.Entity;\n",
        "@Entity\nclass OrderItem { long id; }\n",
        "class Gw { String m(RestTemplate rt) { return rt.getForObject(\"/api/users\", String.class); } }\n",
    );
    let degraded = extract_java_file_io("Gw.java", src, true).expect("provides even when degraded");
    assert!(degraded
        .provides
        .iter()
        .any(|p| p.key == "table:order_item"));
    assert!(degraded.consumes.is_empty(), "degraded gates the consumes");
    let fresh = extract_java_file_io("Gw.java", src, false).expect("both when not degraded");
    assert!(fresh
        .consumes
        .iter()
        .any(|c| c.key.as_deref() == Some("GET /api/users")));
}
