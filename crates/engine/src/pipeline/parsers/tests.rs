use super::*;

// --- parse_typescript's real parse-failure signal (zzop_parser_typescript::parse_ok) ---

#[test]
fn parse_typescript_does_not_degrade_balanced_well_formed_file() {
    let (symbols, _imports, _loc, degraded, _used_names) = parse_typescript(
        "x.ts",
        "export function foo(a: { x: number }) {\n  return [a.x, (a.x + 1)];\n}\n",
    );
    assert!(!degraded);
    assert_eq!(symbols.len(), 1);
}

#[test]
fn parse_typescript_degrades_unbalanced_brace() {
    let (symbols, imports, loc, degraded, used_names) =
        parse_typescript("b.ts", "function foo( {\n  return 1;\n");
    assert!(degraded);
    assert!(symbols.is_empty());
    assert_eq!(imports, Some(ImportMap::new()));
    assert!(loc > 0); // lexical fallback still counts loc
    assert!(used_names.is_empty());
}

#[test]
fn parse_typescript_degrades_stray_closing_brace() {
    let (_symbols, _imports, _loc, degraded, _used_names) =
        parse_typescript("s.ts", "}\nfunction foo() {}\n");
    assert!(degraded);
}

#[test]
fn parse_typescript_does_not_degrade_braces_inside_strings_and_comments() {
    let (_symbols, _imports, _loc, degraded, _used_names) = parse_typescript(
        "c.ts",
        "const s = \"{ unmatched\"; // } also unmatched\nfunction f() {}\n",
    );
    assert!(!degraded);
}

#[test]
fn parse_typescript_degrades_a_balanced_but_syntactically_invalid_file() {
    // Braces/parens are balanced (there are none), but `const x: = 1;` is not valid TypeScript —
    // a brace-balance-only check would misclassify this as a legitimately empty file.
    let (symbols, imports, _loc, degraded, _used_names) =
        parse_typescript("t.ts", "const x: = 1;\n");
    assert!(degraded);
    assert!(symbols.is_empty());
    assert_eq!(imports, Some(ImportMap::new()));
}

#[test]
fn parse_typescript_does_not_degrade_a_legitimately_empty_file() {
    let (symbols, imports, _loc, degraded, used_names) = parse_typescript("e.ts", "");
    assert!(!degraded);
    assert!(symbols.is_empty());
    assert!(imports.unwrap().is_empty());
    assert!(used_names.is_empty());
}

#[test]
fn parse_typescript_collects_used_names_alongside_symbols() {
    let (_symbols, _imports, _loc, degraded, used_names) = parse_typescript(
        "x.ts",
        "const X = 1;\nfunction foo() { return X + Y; }\nexport { foo };\n",
    );
    assert!(!degraded);
    assert!(used_names.contains(&"X".to_string()));
    assert!(used_names.contains(&"Y".to_string()));
    // `foo`'s own declaration name is excluded — matches `parse_local_identifier_refs`'s contract.
    assert!(!used_names.contains(&"foo".to_string()));
}

// --- what the parser actually does on garbage input ---

#[test]
fn garbage_ts_input_does_not_panic_parse_symbols_or_parse_imports() {
    // Calls the parser functions directly (the real pipeline would already have degraded this file
    // via `parse_ok` before reaching these calls).
    let garbage = "@#$%^&*( ) => => => 123abc <<< >>> \u{0}\u{1}";
    let symbols = zzop_parser_typescript::parse_symbols("g.ts", garbage);
    let imports = zzop_parser_typescript::parse_imports("g.ts", garbage);
    assert!(symbols.is_empty());
    assert!(imports.is_empty());
}

#[test]
fn parse_typescript_degrades_garbage_input() {
    let garbage = "@#$%^&*( ) => => => 123abc <<< >>> \u{0}\u{1}";
    let (symbols, imports, loc, degraded, used_names) = parse_typescript("g.ts", garbage);
    assert!(degraded);
    assert!(symbols.is_empty());
    assert_eq!(imports, Some(ImportMap::new()));
    assert!(loc > 0);
    assert!(used_names.is_empty());
}

#[test]
fn parse_typescript_succeeds_on_well_formed_file() {
    let (symbols, imports, _loc, degraded, _used_names) =
        parse_typescript("ok.ts", "export function foo() { return 1; }\n");
    assert!(!degraded);
    assert_eq!(symbols.len(), 1);
    assert!(imports.unwrap().is_empty());
}
