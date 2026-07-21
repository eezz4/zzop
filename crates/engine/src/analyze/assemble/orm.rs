//! ORM entity-reference -> table-key CONSUME resolution — the engine half shared by the TypeORM
//! (`zzop_parser_typescript::extract_typeorm_repository_consumes`) and GORM
//! (`zzop_parser_go::extract_gorm_db_table_consumes`) parser adapters.
//!
//! Both frameworks reference a model by its TYPE, not the physical table string: TypeORM
//! `@InjectRepository(ArticleEntity)` / `getRepository(ArticleEntity)`, GORM `db.Model(&ArticleModel{})`.
//! So the parser can't key the `db-table` consume — it emits `key: None` with `raw: Some("<type>")`. The
//! table name lives with the model's own definition (a TypeORM `@Entity('article')` decorator, a GORM
//! struct's default naming / `TableName()`), projected as a `db-table` PROVIDE whose `symbol` carries the
//! type name. This pass builds the tree-wide type -> table-key index from those provides and fills in each
//! matching consume's key, so it joins the `db-table` channel exactly like a Prisma-accessor consume.

use std::collections::HashMap;

use zzop_core::{IoConsume, IoProvide};

/// Resolve every unresolved ORM entity-reference consume (`kind == "db-table"`, `key == None`) against the
/// type-name -> table-key index built from the `db-table` provides that carry a type name in `symbol`.
/// A consume whose type is not in the index (model defined outside this tree, or an aliased import the
/// parser recorded under a different name) is left unresolved — honest, never guessed.
pub(super) fn resolve_orm_entity_consumes(provides: &[IoProvide], consumes: &mut [IoConsume]) {
    // class name -> table key (e.g. "ArticleEntity" -> "table:article"). Only `db-table` provides that
    // carry a `symbol` (the `@Entity`-decorated class) participate; a same-named class in two files is a
    // last-writer-wins collision (rare, accepted — same honesty as the other fanout resolvers).
    let index: HashMap<&str, &str> = provides
        .iter()
        .filter(|p| p.kind == "db-table")
        .filter_map(|p| p.symbol.as_deref().map(|sym| (sym, p.key.as_str())))
        .collect();
    if index.is_empty() {
        return;
    }
    for c in consumes.iter_mut() {
        if c.kind != "db-table" || c.key.is_some() {
            continue;
        }
        // `raw` holds the entity class name (see the parser adapter). Keep `raw` as provenance after
        // keying — the late-resolve contract is that a set `raw` does not imply unresolved; `key: None`
        // alone does (mirrors `IoConsume::raw`'s own doc).
        if let Some(table_key) = c.raw.as_deref().and_then(|r| index.get(r)) {
            c.key = Some((*table_key).to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provide(key: &str, class: Option<&str>) -> IoProvide {
        IoProvide {
            kind: "db-table".into(),
            key: key.into(),
            file: "x.entity.ts".into(),
            line: 1,
            symbol: class.map(String::from),
            body: None,
        }
    }

    fn entity_consume(raw: &str) -> IoConsume {
        IoConsume {
            client: None,
            body: None,
            kind: "db-table".into(),
            key: None,
            file: "x.service.ts".into(),
            line: 1,
            raw: Some(raw.into()),
            method: None,
        }
    }

    #[test]
    fn a_repository_consume_resolves_to_its_entity_table_key() {
        let provides = vec![provide("table:article", Some("ArticleEntity"))];
        let mut consumes = vec![entity_consume("ArticleEntity")];
        resolve_orm_entity_consumes(&provides, &mut consumes);
        assert_eq!(consumes[0].key.as_deref(), Some("table:article"));
        assert_eq!(consumes[0].raw.as_deref(), Some("ArticleEntity")); // provenance kept
    }

    #[test]
    fn an_entity_not_in_the_index_stays_unresolved() {
        let provides = vec![provide("table:article", Some("ArticleEntity"))];
        let mut consumes = vec![entity_consume("CommentEntity")];
        resolve_orm_entity_consumes(&provides, &mut consumes);
        assert_eq!(consumes[0].key, None);
    }

    #[test]
    fn a_provide_without_a_class_symbol_does_not_participate() {
        // A Prisma/SQL db-table provide (symbol None) can't resolve an entity-class consume.
        let provides = vec![provide("table:article", None)];
        let mut consumes = vec![entity_consume("ArticleEntity")];
        resolve_orm_entity_consumes(&provides, &mut consumes);
        assert_eq!(consumes[0].key, None);
    }

    #[test]
    fn an_already_keyed_db_table_consume_is_untouched() {
        // A Prisma-accessor consume (already keyed) must not be rewritten.
        let provides = vec![provide("table:article", Some("ArticleEntity"))];
        let mut consumes = vec![IoConsume {
            key: Some("table:user".into()),
            raw: None,
            ..entity_consume("ArticleEntity")
        }];
        resolve_orm_entity_consumes(&provides, &mut consumes);
        assert_eq!(consumes[0].key.as_deref(), Some("table:user"));
    }
}
