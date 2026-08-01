use super::*;

/// The `be-fastapi-fs` shape, which is the reason this producer exists: pydantic-settings declares the
/// class and instantiates it once in the same module, and every consumer writes `settings.X`.
#[test]
fn a_settings_class_and_its_module_level_instance_both_resolve() {
    let m = const_map_fragment(
        "class Settings(BaseSettings):\n    API_V1_STR: str = \"/api/v1\"\n    PROJECT_NAME: str = \"app\"\n\nsettings = Settings()\n",
    );
    assert_eq!(
        m.get("Settings.API_V1_STR").map(String::as_str),
        Some("/api/v1")
    );
    assert_eq!(
        m.get("settings.API_V1_STR").map(String::as_str),
        Some("/api/v1")
    );
    assert_eq!(
        m.get("settings.PROJECT_NAME").map(String::as_str),
        Some("app")
    );
}

/// The bare spelling binds the same way the annotated one does.
#[test]
fn an_unannotated_class_attribute_resolves() {
    let m = const_map_fragment("class C:\n    P = \"/p\"\n\nc = C()\n");
    assert_eq!(m.get("c.P").map(String::as_str), Some("/p"));
}

/// A bare module-level constant is NOT captured — the whole reason the map is dotted-only. Capturing
/// it would let `API_URL` in one file mis-key an unrelated `api_url` parameter in another.
#[test]
fn a_bare_module_constant_is_not_captured() {
    let m = const_map_fragment("API_URL = \"https://x\"\nprefix = \"/api\"\n");
    assert!(m.is_empty(), "{m:?}");
}

/// Values that are not plain string literals have no compile-time value here — skipped, never guessed.
#[test]
fn non_string_values_are_skipped() {
    let m = const_map_fragment(
        "import os\n\nclass C:\n    N: int = 8\n    E: str = os.environ[\"X\"]\n    F: str = f\"{N}\"\n    OK: str = \"/ok\"\n\nc = C()\n",
    );
    assert_eq!(m.get("c.OK").map(String::as_str), Some("/ok"));
    for absent in ["c.N", "c.E", "c.F"] {
        assert!(!m.contains_key(absent), "{absent} must not resolve: {m:?}");
    }
}

/// An instance of a class this file does not declare cannot be linked from this file alone. That is
/// the `be-fastapi` shape (a factory in another module), and it must stay unresolved rather than be
/// invented — S14 keeps reporting it.
#[test]
fn an_instance_of_an_unknown_class_resolves_nothing() {
    let m = const_map_fragment(
        "from app.core.config import get_settings\n\nsettings = get_settings()\n",
    );
    assert!(m.is_empty(), "{m:?}");
}

/// Top-level only, the same v1 scope `adapters::fastapi` and `lang::symbols` already state.
#[test]
fn a_nested_class_is_out_of_scope() {
    let m = const_map_fragment("def f():\n    class C:\n        P: str = \"/p\"\n    return C()\n");
    assert!(m.is_empty(), "{m:?}");
}

/// First writer wins inside the file, so the result never depends on iteration order — the same rule
/// the engine-side merge applies across files.
#[test]
fn the_first_binding_wins() {
    let m = const_map_fragment(
        "class C:\n    P: str = \"/first\"\n    P: str = \"/second\"\n\nc = C()\n",
    );
    assert_eq!(m.get("c.P").map(String::as_str), Some("/first"));
}

/// A file with no classes at all is the common case and must cost nothing.
#[test]
fn ordinary_python_yields_nothing() {
    let m = const_map_fragment("def add(a, b):\n    return a + b\n");
    assert!(m.is_empty(), "{m:?}");
}

/// Unparseable input yields an empty fragment rather than a panic — the contract `parse_module` upholds.
#[test]
fn unparseable_input_is_empty() {
    assert!(const_map_fragment("class ??? :\n  &&&\n").is_empty());
}
