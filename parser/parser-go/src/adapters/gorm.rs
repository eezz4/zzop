//! GORM model -> `db-table` PROVIDE and repository-access -> `db-table` CONSUME extraction (the Go member of the ORM db-table
//! family, alongside `zzop_parser_typescript`'s TypeORM adapters and the Prisma/SQL provide sides). Import-gated on
//! `gorm.io/gorm`; a file that never imports it yields nothing.
//! ## Provide side
//! A struct that embeds `gorm.Model` is a model; its table name is the literal returned by an explicit `func (X) TableName() string
//! { return "…" }` method if present, else GORM's default naming (`ArticleModel` -> `article_models`, see `naming`). Emitted as
//! `IoProvide { kind: "db-table", key: "table:<casing(name)>", symbol: Some(<struct name>) }` — `symbol` carries the struct name so the engine can resolve the consumes below (identical mechanism to the TypeORM `@Entity` provide).
//! ## Consume side
//! A composite literal of a model type passed to a GORM query method (`db.Model(&ArticleModel{})`, `db.Where(FavoriteModel{…})`,
//! `db.Delete(&FavoriteModel{})`, …) is a table touch. Emitted as `IoConsume { kind: "db-table", key: None, raw: Some(<struct name>) }`
//! — unkeyable at parse time (the table name derivation lives with the model's own struct/`TableName()`, possibly in another file), so
//! the engine's `resolve_orm_entity_consumes` pass keys it from the provide `symbol` index. The model name is taken from the composite literal's TYPE (a `type_identifier`, or the `.name` of a cross-package `qualified_type`); a non-literal argument is never guessed.

use std::collections::{BTreeSet, HashMap, HashSet};

use tree_sitter::Node;
use zzop_core::{ImportMap, IoConsume, IoProvide};

use crate::util::{line_of, node_text, string_literal_text, valid_named_children};

mod naming;

/// GORM query methods whose composite-literal argument names a touched model. A broad-but-bounded set: every one takes a model value/pointer as an argument in normal use.
///
/// Parity note vs the TypeORM mirror (`zzop_parser_typescript::adapters::typeorm_repository`, which gates on the two SPECIFIC
/// shapes `@InjectRepository`/`getRepository`): several of these names (`Find`, `First`, `Save`, `Create`, `Delete`, `Scan`) are
/// generic, so — beyond the file-level `gorm.io/gorm` import gate — an unrelated `foo.Find(SomeStruct{})` in a gorm-importing file
/// DOES mint a consume, with no receiver check. That wider surface is safe because the real guard is downstream: the consume is
/// `key: None` and only becomes a finding-bearing fact if the engine resolves `SomeStruct` against a real db-table model provide
/// (`resolve_orm_entity_consumes`). A coincidental non-model struct leaves the consume unresolved and inert — same net effect as TypeORM's own resolution-drop for a stray class.
const QUERY_METHODS: &[&str] = &[
    "Model",
    "Where",
    "Find",
    "First",
    "Take",
    "Last",
    "Create",
    "Save",
    "Delete",
    "Updates",
    "FirstOrCreate",
    "FirstOrInit",
    "Preload",
    "Related",
    "Association",
    "Scan",
    "Assign",
    "Attrs",
];

fn gorm_names(imports: &ImportMap) -> HashSet<String> {
    imports
        .iter()
        .filter(|(_, b)| b.specifier == "gorm.io/gorm")
        .map(|(local, _)| local.clone())
        .collect()
}

/// Extract GORM model `db-table` provides from one Go file. Empty when the file does not import gorm.
pub fn extract_gorm_db_table_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let gorm = gorm_names(&crate::lang::imports::parse_imports(text));
    if gorm.is_empty() {
        return Vec::new();
    }
    let root = tree.root_node();
    let overrides = table_name_overrides(root, text);
    let mut out = Vec::new();
    collect_model_provides(root, rel, text, &gorm, &overrides, &mut out);
    out
}

/// Extract GORM repository-access `db-table` consumes (`key: None`, `raw: <model struct name>`).
pub fn extract_gorm_db_table_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    if gorm_names(&crate::lang::imports::parse_imports(text)).is_empty() {
        return Vec::new();
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    collect_consumes(tree.root_node(), rel, text, &mut seen, &mut out);
    out
}

// --- provide side ---

/// `func (X) TableName() string { return "literal" }` -> `{ X: "literal" }`. Only a bare string-literal return is captured (a computed name is never guessed).
fn table_name_overrides(root: Node, src: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    walk(root, &mut |n| {
        if n.kind() != "method_declaration" {
            return;
        }
        if n.child_by_field_name("name").map(|nm| node_text(nm, src)) != Some("TableName") {
            return;
        }
        let Some(recv) = receiver_type_name(n, src) else {
            return;
        };
        if let Some(lit) = table_name_return_literal(n, src) {
            out.insert(recv, lit);
        }
    });
    out
}

/// The receiver's TYPE name for a method declaration: `func (a ArticleModel)` / `func (a *ArticleModel)` -> `ArticleModel`.
fn receiver_type_name(method: Node, src: &str) -> Option<String> {
    let recv = method.child_by_field_name("receiver")?;
    let mut found = None;
    walk(recv, &mut |n| {
        if found.is_none() && n.kind() == "type_identifier" {
            found = Some(node_text(n, src).to_string());
        }
    });
    found
}

/// The string literal in the FIRST `return "…"` of a method body, if any.
fn table_name_return_literal(method: Node, src: &str) -> Option<String> {
    let body = method.child_by_field_name("body")?;
    let mut lit = None;
    walk(body, &mut |n| {
        if lit.is_none() && n.kind() == "return_statement" {
            walk(n, &mut |c| {
                if lit.is_none() {
                    if let Some(s) = string_literal_text(c, src) {
                        lit = Some(s);
                    }
                }
            });
        }
    });
    lit
}

fn collect_model_provides(
    root: Node,
    rel: &str,
    src: &str,
    gorm: &HashSet<String>,
    overrides: &HashMap<String, String>,
    out: &mut Vec<IoProvide>,
) {
    walk(root, &mut |n| {
        if n.kind() != "type_spec" {
            return;
        }
        let Some(name_node) = n.child_by_field_name("name") else {
            return;
        };
        let struct_name = node_text(name_node, src);
        let Some(ty) = n.child_by_field_name("type") else {
            return;
        };
        if ty.kind() != "struct_type" || !is_gorm_model(ty, gorm, src) {
            return;
        }
        let table = overrides
            .get(struct_name)
            .cloned()
            .unwrap_or_else(|| naming::default_table_name(struct_name));
        out.push(IoProvide {
            kind: "db-table".to_string(),
            key: format!("table:{}", zzop_core::db_table_channel_casing(&table)),
            file: rel.to_string(),
            line: line_of(name_node),
            symbol: Some(struct_name.to_string()),
            body: None,
        });
    });
}

/// True when a `struct_type` is a GORM model — either it embeds `<gorm>.Model` (an unnamed field_declaration whose type is the
/// `qualified_type` `<gorm>.Model`), OR any field carries a `gorm:` struct tag (the tag-driven model shape, e.g. `Username string \`gorm:"column:username"\``, which has no embed). Either signal is unambiguous evidence the struct is a GORM-mapped table.
fn is_gorm_model(struct_ty: Node, gorm: &HashSet<String>, src: &str) -> bool {
    valid_named_children(struct_ty).iter().any(|list| {
        list.kind() == "field_declaration_list"
            && valid_named_children(*list).iter().any(|f| {
                f.kind() == "field_declaration"
                    && (is_embedded_gorm_model(*f, gorm, src) || has_gorm_tag(*f, src))
            })
    })
}

/// A field_declaration that is an embedded `<gorm>.Model` (no field name, type is the gorm `Model`).
fn is_embedded_gorm_model(field: Node, gorm: &HashSet<String>, src: &str) -> bool {
    field.child_by_field_name("name").is_none()
        && field
            .child_by_field_name("type")
            .is_some_and(|t| is_gorm_model_type(t, gorm, src))
}

/// A field_declaration whose struct tag carries a `gorm:"…"` KEY (`\`gorm:"column:x"\``). Matches the `gorm:"` key-open form rather
/// than a bare `gorm:` substring, so a foreign tag whose VALUE happens to contain `gorm:` (e.g. `\`example:"gorm:foo"\``) does not spuriously mark the struct as a model.
fn has_gorm_tag(field: Node, src: &str) -> bool {
    field
        .child_by_field_name("tag")
        .is_some_and(|t| node_text(t, src).contains("gorm:\""))
}

/// A `qualified_type` `<gorm>.Model`.
fn is_gorm_model_type(ty: Node, gorm: &HashSet<String>, src: &str) -> bool {
    ty.kind() == "qualified_type"
        && ty
            .child_by_field_name("package")
            .is_some_and(|p| gorm.contains(node_text(p, src)))
        && ty.child_by_field_name("name").map(|nm| node_text(nm, src)) == Some("Model")
}

// --- consume side ---

fn collect_consumes(
    root: Node,
    rel: &str,
    src: &str,
    seen: &mut BTreeSet<String>,
    out: &mut Vec<IoConsume>,
) {
    walk(root, &mut |n| {
        if n.kind() != "call_expression" {
            return;
        }
        let Some(func) = n.child_by_field_name("function") else {
            return;
        };
        if func.kind() != "selector_expression" {
            return;
        }
        let is_query = func
            .child_by_field_name("field")
            .map(|f| node_text(f, src))
            .is_some_and(|m| QUERY_METHODS.contains(&m));
        if !is_query {
            return;
        }
        let Some(args) = n.child_by_field_name("arguments") else {
            return;
        };
        for arg in valid_named_children(args) {
            if let Some(model) = composite_literal_type_name(arg, src) {
                if seen.insert(model.clone()) {
                    out.push(IoConsume {
                        client: None,
                        body: None,
                        kind: "db-table".to_string(),
                        key: None,
                        file: rel.to_string(),
                        line: line_of(n),
                        raw: Some(model),
                        method: None,
                    });
                }
            }
        }
    });
}

/// The model type name of a composite literal argument: `X{…}` or `&X{…}` -> `X`; a cross-package `pkg.X{…}` -> `X` (the `.name`). `None` for any non-composite-literal argument.
fn composite_literal_type_name(arg: Node, src: &str) -> Option<String> {
    let lit = match arg.kind() {
        "composite_literal" => arg,
        // `&X{…}` — a unary expression wrapping the composite literal.
        "unary_expression" => valid_named_children(arg)
            .into_iter()
            .find(|c| c.kind() == "composite_literal")?,
        _ => return None,
    };
    let ty = lit.child_by_field_name("type")?;
    match ty.kind() {
        "type_identifier" => Some(node_text(ty, src).to_string()),
        "qualified_type" => ty
            .child_by_field_name("name")
            .map(|nm| node_text(nm, src).to_string()),
        _ => None,
    }
}

/// Pre-order walk applying `f` to every node (named or not) — the shared recursion for this adapter's several passes. Unlike the
/// sibling walkers in `http_clients`/`lang::used_names` (which recurse only `valid_named_children`, skipping ERROR/MISSING
/// subtrees), this one descends into every child: each caller filters by `node.kind()`, so descending an ERROR region is inert
/// (nothing inside it matches a `type_spec`/`call_expression`/… kind), and visiting unnamed children costs only the kind check. The simpler all-children form is deliberate here.
fn walk(node: Node, f: &mut impl FnMut(Node)) {
    f(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, f);
    }
}

#[cfg(test)]
mod tests;
