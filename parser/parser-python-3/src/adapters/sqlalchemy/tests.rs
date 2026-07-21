//! Coverage for the SQLModel/SQLAlchemy `db-table` provide/consume adapters: model recognition
//! (`table=True` class arg, `__tablename__` body literal), default-vs-explicit naming, the import gate,
//! and the query-call consume shapes (`select`/`session.get`/`session.query`/`.select_from`).

use super::{extract_sqlalchemy_db_table_consumes, extract_sqlalchemy_db_table_provides};

const IMPORT: &str = "from sqlmodel import SQLModel, Field, select\n\n";

#[test]
fn a_sqlmodel_table_true_class_provides_its_default_named_table() {
    let src = format!(
        "{IMPORT}class User(SQLModel, table=True):\n    id: int = Field(primary_key=True)\n"
    );
    let out = extract_sqlalchemy_db_table_provides("app/models.py", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].kind, "db-table");
    assert_eq!(
        out[0].key, "table:user",
        "SQLModel default __tablename__ is the class name lowercased"
    );
    assert_eq!(out[0].symbol.as_deref(), Some("User"));
    assert_eq!(out[0].file, "app/models.py");
}

#[test]
fn a_tablename_literal_overrides_the_default_naming() {
    let src = format!(
        "{IMPORT}class User(SQLModel, table=True):\n    __tablename__ = \"users\"\n    id: int\n"
    );
    let out = extract_sqlalchemy_db_table_provides("m.py", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(
        out[0].key, "table:users",
        "__tablename__ literal wins over the class-name default"
    );
    assert_eq!(out[0].symbol.as_deref(), Some("User"));
}

#[test]
fn a_sqlalchemy_declarative_class_with_only_tablename_is_a_model() {
    // The classic SQLAlchemy declarative idiom: no `table=True`, a `__tablename__` assignment marks it.
    let src = "from sqlalchemy.orm import declarative_base\n\nBase = declarative_base()\n\nclass Item(Base):\n    __tablename__ = \"items\"\n    id = Column(Integer, primary_key=True)\n";
    let out = extract_sqlalchemy_db_table_provides("m.py", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].key, "table:items");
    assert_eq!(out[0].symbol.as_deref(), Some("Item"));
}

#[test]
fn a_non_table_class_provides_nothing() {
    // A plain SQLModel subclass WITHOUT `table=True` is a schema/DTO, not a table.
    let src = format!("{IMPORT}class UserCreate(SQLModel):\n    name: str\n");
    assert!(extract_sqlalchemy_db_table_provides("m.py", &src).is_empty());
}

#[test]
fn a_table_false_class_provides_nothing() {
    // Explicit `table=False` is not a table.
    let src = format!("{IMPORT}class UserBase(SQLModel, table=False):\n    name: str\n");
    assert!(extract_sqlalchemy_db_table_provides("m.py", &src).is_empty());
}

#[test]
fn a_file_that_imports_no_orm_is_gated_out() {
    let src = "class User:\n    __tablename__ = \"users\"\n\ndef f():\n    return select(User)\n";
    assert!(extract_sqlalchemy_db_table_provides("m.py", src).is_empty());
    assert!(extract_sqlalchemy_db_table_consumes("m.py", src).is_empty());
}

#[test]
fn two_models_yield_two_provides() {
    let src = format!(
        "{IMPORT}class User(SQLModel, table=True):\n    id: int\n\nclass Item(SQLModel, table=True):\n    id: int\n"
    );
    let out = extract_sqlalchemy_db_table_provides("m.py", &src);
    let mut keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["table:item", "table:user"]);
}

#[test]
fn a_bare_select_call_is_a_consume() {
    let src =
        format!("{IMPORT}def read_items(session):\n    return session.exec(select(Item)).all()\n");
    let out = extract_sqlalchemy_db_table_consumes("app/routes.py", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].kind, "db-table");
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].raw.as_deref(), Some("Item"));
    assert_eq!(out[0].file, "app/routes.py");
}

#[test]
fn a_session_get_call_is_a_consume() {
    let src =
        format!("{IMPORT}def current_user(session, sub):\n    return session.get(User, sub)\n");
    let out = extract_sqlalchemy_db_table_consumes("m.py", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].raw.as_deref(), Some("User"));
}

#[test]
fn a_session_query_and_select_from_are_consumes() {
    let src = format!(
        "{IMPORT}def f(session):\n    session.query(Item)\n    session.query(select(User).select_from(Article))\n"
    );
    let out = extract_sqlalchemy_db_table_consumes("m.py", &src);
    let mut raws: Vec<&str> = out.iter().filter_map(|c| c.raw.as_deref()).collect();
    raws.sort_unstable();
    assert_eq!(
        raws,
        vec!["Article", "Item", "User"],
        "nested query calls each counted once"
    );
}

#[test]
fn the_same_model_queried_twice_is_deduped() {
    let src = format!(
        "{IMPORT}def f(session):\n    session.query(Item)\n    session.exec(select(Item))\n"
    );
    let out = extract_sqlalchemy_db_table_consumes("m.py", &src);
    assert_eq!(out.len(), 1, "deduped per model: {out:?}");
    assert_eq!(out[0].raw.as_deref(), Some("Item"));
}

#[test]
fn a_multi_entity_select_touches_every_model_argument() {
    // SQLAlchemy 2.0: `select(User, Item)` is a two-table join — both are touched.
    let src =
        format!("{IMPORT}def f(session):\n    return session.exec(select(User, Item)).all()\n");
    let out = extract_sqlalchemy_db_table_consumes("m.py", &src);
    let mut raws: Vec<&str> = out.iter().filter_map(|c| c.raw.as_deref()).collect();
    raws.sort_unstable();
    assert_eq!(raws, vec!["Item", "User"]);
}

#[test]
fn an_annotated_tablename_literal_is_recognized() {
    // `__tablename__: str = "accounts"` (an annotated assign) marks a SQLAlchemy declarative model.
    let src = "from sqlalchemy.orm import declarative_base\n\nBase = declarative_base()\n\nclass Account(Base):\n    __tablename__: str = \"accounts\"\n    id = Column(Integer, primary_key=True)\n";
    let out = extract_sqlalchemy_db_table_provides("m.py", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].key, "table:accounts");
    assert_eq!(out[0].symbol.as_deref(), Some("Account"));
}

#[test]
fn a_non_query_call_with_a_name_arg_is_ignored() {
    // `print(User)` shares nothing with a query verb — no consume.
    let src = format!("{IMPORT}def f():\n    print(User)\n    return foo(Item)\n");
    assert!(extract_sqlalchemy_db_table_consumes("m.py", &src).is_empty());
}

#[test]
fn a_query_call_whose_first_arg_is_not_a_bare_name_is_ignored() {
    // `select("*")` / `session.get(some.attr, id)` — no bare-Name model to key.
    let src =
        format!("{IMPORT}def f(session):\n    select(\"*\")\n    session.get(models.User, 1)\n");
    assert!(extract_sqlalchemy_db_table_consumes("m.py", &src).is_empty());
}
