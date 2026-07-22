//! Fully-populated sample builders — every `Option` `Some`, every `Vec`/`Map` non-empty,
//! recursively, so every `skip_serializing_if`-gated field actually appears in the serialized
//! JSON. See the target doc in `main.rs` for the method.

use zzop_core::{
    ClassShapeFragment, ConsumeBodyShape, FileProjection, ImportBinding, IoConsume, IoFacts,
    IoProvide, NonIdempotentKind, NormalizedEnvelope, ProcedureRouterEntry,
    ProcedureRouterFragment, ProvideBodyField, ProvideBodyShape, ReExport, RouterMountEntry,
    RouterMountFragment, SourceSymbol, SourceSymbolKind, WriteSite, NORMALIZED_AST_FORMAT,
};

/// One fully-populated `ProvideBodyField` — every `Option` `Some`, nothing to default.
pub(crate) fn sample_provide_body_field() -> ProvideBodyField {
    ProvideBodyField {
        name: "email".to_string(),
        optional: true,
    }
}

pub(crate) fn sample_provide_body_shape() -> ProvideBodyShape {
    ProvideBodyShape {
        sub_key: Some("user".to_string()),
        dto_ref: Some("CreateUserDto".to_string()),
        fields: vec![sample_provide_body_field()],
        complete: true,
    }
}

pub(crate) fn sample_consume_body_shape() -> ConsumeBodyShape {
    ConsumeBodyShape {
        keys: vec!["user".to_string(), "user.email".to_string()],
        complete_at: vec!["".to_string(), "user".to_string()],
    }
}

pub(crate) fn sample_write_site() -> WriteSite {
    WriteSite {
        file: "src/user.service.ts".to_string(),
        line: 42,
        sink: "prisma.user.update".to_string(),
        kind: Some(NonIdempotentKind::Create),
    }
}

pub(crate) fn sample_symbol() -> SourceSymbol {
    SourceSymbol {
        id: "src/user.service.ts#createUser".to_string(),
        file: "src/user.service.ts".to_string(),
        name: "createUser".to_string(),
        kind: SourceSymbolKind::Function,
        line: 10,
        exported: true,
        is_default: true,
        body_start: Some(11),
        body_end: Some(20),
        write_sites: vec![sample_write_site()],
    }
}

pub(crate) fn sample_import_binding() -> ImportBinding {
    ImportBinding {
        specifier: "../shared/prisma".to_string(),
        original: "default".to_string(),
        deferred: true,
        type_only: true,
    }
}

pub(crate) fn sample_re_export() -> ReExport {
    ReExport {
        specifier: "./bar".to_string(),
        original: "Bar".to_string(),
        local_alias: "BarAlias".to_string(),
        type_only: true,
    }
}

pub(crate) fn sample_io_provide() -> IoProvide {
    IoProvide {
        kind: "http".to_string(),
        key: "GET /users/{}".to_string(),
        file: "src/user.controller.ts".to_string(),
        line: 15,
        symbol: Some("createUser".to_string()),
        body: Some(sample_provide_body_shape()),
    }
}

pub(crate) fn sample_io_consume() -> IoConsume {
    IoConsume {
        kind: "http".to_string(),
        key: Some("GET /users/{}".to_string()),
        file: "src/user.client.ts".to_string(),
        line: 25,
        raw: Some("axios.get(url)".to_string()),
        method: Some("GET".to_string()),
        retry_configured: None,
        body: Some(sample_consume_body_shape()),
        client: Some("axios".to_string()),
    }
}

pub(crate) fn sample_class_shape_fragment() -> ClassShapeFragment {
    ClassShapeFragment {
        name: "CreateUserDto".to_string(),
        fields: vec![sample_provide_body_field()],
        complete: true,
    }
}

pub(crate) fn sample_trpc_leaf() -> ProcedureRouterEntry {
    ProcedureRouterEntry::Leaf {
        key: "get".to_string(),
        verb: "QUERY".to_string(),
        line: 3,
    }
}

pub(crate) fn sample_trpc_ref() -> ProcedureRouterEntry {
    ProcedureRouterEntry::Ref {
        key: "sub".to_string(),
        ident: "subRouter".to_string(),
        specifier: Some("./sub".to_string()),
    }
}

pub(crate) fn sample_trpc_nested() -> ProcedureRouterEntry {
    ProcedureRouterEntry::Nested {
        key: "nested".to_string(),
        entries: vec![sample_trpc_leaf()],
    }
}

pub(crate) fn sample_trpc_fragment() -> ProcedureRouterFragment {
    ProcedureRouterFragment {
        name: "appRouter".to_string(),
        entries: vec![sample_trpc_leaf(), sample_trpc_ref(), sample_trpc_nested()],
    }
}

pub(crate) fn sample_mount_verb() -> RouterMountEntry {
    RouterMountEntry::Verb {
        method: "POST".to_string(),
        path: "/setup".to_string(),
        handler: Some("handler".to_string()),
        line: 7,
        // Non-empty so the `#[serde(default, skip_serializing_if)]` field actually serializes —
        // the parity probes only see fields the fully-populated sample emits (this file's rule).
        attr_keys: vec!["auth-guarded".to_string()],
    }
}

pub(crate) fn sample_mount_mount() -> RouterMountEntry {
    RouterMountEntry::Mount {
        prefix: "/two-factor".to_string(),
        ident: "twoFactorRoute".to_string(),
        specifier: Some("./two-factor".to_string()),
        // Non-empty for the same serialize-visibility reason as `sample_mount_verb`.
        attr_keys: vec!["auth-guarded".to_string()],
    }
}

pub(crate) fn sample_mount_scoped_attr() -> RouterMountEntry {
    RouterMountEntry::ScopedAttr {
        prefix: "/admin".to_string(),
        key: "auth-guarded".to_string(),
        line: 3,
    }
}

pub(crate) fn sample_router_mount_fragment() -> RouterMountFragment {
    RouterMountFragment {
        name: "auth".to_string(),
        entries: vec![
            sample_mount_verb(),
            sample_mount_mount(),
            sample_mount_scoped_attr(),
        ],
    }
}

/// One fully-populated `FileProjection` — every optional/`#[serde(default)]` field non-default so
/// every one of them actually appears in the serialized JSON.
pub(crate) fn sample_file_projection() -> FileProjection {
    let mut imports = std::collections::BTreeMap::new();
    imports.insert("prisma".to_string(), sample_import_binding());

    let mut const_map_fragment = std::collections::HashMap::new();
    const_map_fragment.insert("USERS_TABLE".to_string(), "users".to_string());

    FileProjection {
        path: "src/user.controller.ts".to_string(),
        loc: 100,
        symbols: vec![sample_symbol()],
        imports,
        re_exports: vec![sample_re_export()],
        dynamic_imports: vec!["./lazy".to_string()],
        used_names: vec!["createUser".to_string()],
        const_map_fragment,
        procedure_router_fragments: vec![sample_trpc_fragment()],
        router_mount_fragments: vec![sample_router_mount_fragment()],
        class_shape_fragments: vec![sample_class_shape_fragment()],
        io: IoFacts {
            provides: vec![sample_io_provide()],
            consumes: vec![sample_io_consume()],
        },
        loop_spans: vec![(10, 20)],
        degraded: true,
        is_entry: true,
        attributes: Vec::new(),
    }
}

pub(crate) fn sample_envelope() -> NormalizedEnvelope {
    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "test-adapter/1".to_string(),
        source: "test-source".to_string(),
        files: vec![sample_file_projection()],
    }
}
