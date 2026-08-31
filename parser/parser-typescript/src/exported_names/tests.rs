use super::parse_exported_names;

fn names(source: &str) -> Vec<String> {
    parse_exported_names("x.ts", source)
}

#[test]
fn declaration_forms() {
    assert_eq!(
        names(
            "export const a = 1\nexport function b() {}\nexport class C {}\n\
             export interface I {}\nexport type T = number\nexport enum E { X }\n"
        ),
        vec!["a", "b", "C", "I", "T", "E"]
    );
}

#[test]
fn destructured_export_binds_every_name() {
    assert_eq!(
        names("export const { a, b: c, ...rest } = obj\nexport const [d, e] = arr\n"),
        vec!["a", "c", "rest", "d", "e"]
    );
}

/// The shape that motivated this module: a from-less `export { … }` republishing names this file only
/// IMPORTED, so it declares no symbol of its own. `re_exports` skips it (no source module) and
/// `export_aliases` skips it (no rename) — nobody else answers.
#[test]
fn fromless_clause_over_imported_names() {
    assert_eq!(
        names("import { x, y } from 'pkg'\nexport { x, y }\n"),
        vec!["x", "y"]
    );
}

#[test]
fn rename_publishes_the_public_half_only() {
    assert_eq!(names("const a = 1\nexport { a as b }\n"), vec!["b"]);
}

#[test]
fn re_export_clause_with_source() {
    assert_eq!(
        names("export { isLTAR, isLookup } from 'nocodb-sdk'\n"),
        vec!["isLTAR", "isLookup"]
    );
    assert_eq!(names("export * as ns from './y'\n"), vec!["ns"]);
}

/// `export * from "./y"` publishes names this walk cannot know without a resolver — it must contribute
/// NOTHING rather than guess, which is what keeps a caller's "no name matched" honest.
#[test]
fn star_re_export_contributes_nothing() {
    assert!(names("export * from './y'\n").is_empty());
}

#[test]
fn default_forms() {
    assert_eq!(
        names("const useFoo = () => {}\nexport default useFoo\n"),
        vec!["useFoo", "default"]
    );
    assert_eq!(
        names("export default function bar() {}\n"),
        vec!["bar", "default"]
    );
    assert_eq!(names("export default { a: 1 }\n"), vec!["default"]);
}

#[test]
fn non_exported_declarations_are_absent() {
    assert!(names("const a = 1\nfunction b() {}\n").is_empty());
    assert!(names("import { x } from './y'\n").is_empty());
}

#[test]
fn unparseable_source_degrades_to_empty() {
    assert!(names("function f( {").is_empty());
}

#[test]
fn duplicate_names_appear_once() {
    assert_eq!(
        names("export const a = 1\nexport { a }\n"),
        vec!["a"],
        "one public name, however many clauses spell it"
    );
}
