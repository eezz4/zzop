//! Coverage for `extract_typeorm_repository_consumes`: the `@InjectRepository` and `getRepository`
//! shapes, per-class dedup, the never-guess boundary, the framework-presence (typeorm-import) gate, and
//! the test-file skip. Fixtures carry a `typeorm` import so the gate admits them (see the module doc).

use super::extract_typeorm_repository_consumes;

#[test]
fn inject_repository_decorator_yields_an_unresolved_entity_consume() {
    let src = "import { Repository } from 'typeorm';\nclass ArticleService {\n  constructor(\n    @InjectRepository(ArticleEntity)\n    private readonly repo: Repository<ArticleEntity>,\n  ) {}\n}\n";
    let out = extract_typeorm_repository_consumes("article.service.ts", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].kind, "db-table");
    assert_eq!(
        out[0].key, None,
        "key stays None — resolved engine-side against the entity index"
    );
    assert_eq!(out[0].raw.as_deref(), Some("ArticleEntity"));
    assert_eq!(out[0].file, "article.service.ts");
}

#[test]
fn get_repository_call_yields_an_unresolved_entity_consume() {
    let src = "import { getRepository } from 'typeorm';\nfunction load() {\n  return getRepository(UserEntity).find();\n}\n";
    let out = extract_typeorm_repository_consumes("user.service.ts", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].raw.as_deref(), Some("UserEntity"));
    assert_eq!(out[0].key, None);
}

#[test]
fn member_get_repository_call_is_recognized() {
    // `this.connection.getRepository(X)` / `manager.getRepository(X)` — the member-call form.
    let src = "import { getRepository } from 'typeorm';\nclass R {\n  find() {\n    return this.manager.getRepository(TagEntity).find();\n  }\n}\n";
    let out = extract_typeorm_repository_consumes("tag.repo.ts", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].raw.as_deref(), Some("TagEntity"));
}

#[test]
fn the_same_entity_via_decorator_and_type_is_emitted_once() {
    // `@InjectRepository(X)` on a param whose type is `Repository<X>` is ONE table touch — deduped by
    // entity class, and `getRepository(X)` for the same class elsewhere in the file doesn't re-add it.
    let src = "import { Repository, getRepository } from 'typeorm';\nclass S {\n  constructor(@InjectRepository(ArticleEntity) private repo: Repository<ArticleEntity>) {}\n  other() { return getRepository(ArticleEntity).count(); }\n}\n";
    let out = extract_typeorm_repository_consumes("s.service.ts", src);
    assert_eq!(
        out.len(),
        1,
        "one consume per distinct entity class: {out:?}"
    );
    assert_eq!(out[0].raw.as_deref(), Some("ArticleEntity"));
}

#[test]
fn two_distinct_entities_yield_two_consumes() {
    let src = "import { Repository } from 'typeorm';\nclass S {\n  constructor(\n    @InjectRepository(ArticleEntity) private a: Repository<ArticleEntity>,\n    @InjectRepository(UserEntity) private u: Repository<UserEntity>,\n  ) {}\n}\n";
    let out = extract_typeorm_repository_consumes("s.service.ts", src);
    let mut raws: Vec<&str> = out.iter().filter_map(|c| c.raw.as_deref()).collect();
    raws.sort_unstable();
    assert_eq!(raws, vec!["ArticleEntity", "UserEntity"]);
}

#[test]
fn a_dynamic_entity_argument_is_never_guessed() {
    // `getRepository(entityFromConfig())` / a computed entity — not a bare identifier, so skipped.
    let src = "import { getRepository } from 'typeorm';\nfunction f() { return getRepository(pickEntity()).find(); }\n";
    assert!(extract_typeorm_repository_consumes("f.ts", src).is_empty());
}

#[test]
fn an_unrelated_decorator_or_call_is_ignored() {
    let src = "import { Something } from 'typeorm';\nclass C {\n  constructor(@Inject('TOKEN') private x: any) {}\n  f() { return someOtherFn(Thing).run(); }\n}\n";
    assert!(extract_typeorm_repository_consumes("c.ts", src).is_empty());
}

#[test]
fn a_get_repository_call_in_a_non_typeorm_file_is_gated_out() {
    // The framework-presence gate: a custom `getRepository` helper in a file that never mentions typeorm
    // must NOT mint a spurious consume.
    let src = "class Custom {\n  find() { return this.orm.getRepository(Thing).all(); }\n}\n";
    assert!(extract_typeorm_repository_consumes("custom.ts", src).is_empty());
}

#[test]
fn test_files_are_skipped_before_parsing() {
    let src = "import { Repository } from 'typeorm';\nclass S {\n  constructor(@InjectRepository(ArticleEntity) private a: Repository<ArticleEntity>) {}\n}\n";
    assert!(extract_typeorm_repository_consumes("article.service.spec.ts", src).is_empty());
}
