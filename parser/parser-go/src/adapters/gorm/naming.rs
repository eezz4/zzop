//! GORM default table-naming: the deterministic `struct name -> table name` transform GORM's default
//! `NamingStrategy` applies (CamelCase -> snake_case, then pluralize) — replicated here so a model's
//! `db-table` provide keys on the SAME physical table GORM itself creates. An explicit `TableName()`
//! override (handled by the caller) takes precedence; this is only the default path.
//!
//! Pluralization is the basic English ruleset (regular plurals + the common `-y`/`-s`/`-x`/`-z`/`-ch`/`-sh`
//! endings), NOT GORM's full `inflection` library. An irregular plural (`Person` -> GORM `people`, here
//! `persons`) mis-derives — but the provide and its resolved consumes share this same transform, so the
//! INTRA-app join is always consistent; only a cross-layer join to a SQL DDL that spells the irregular
//! plural would miss (an under-report, never a false positive). Documented limitation.

/// `ArticleModel` -> `article_models`. Applies CamelCase -> snake_case then pluralizes the result.
pub(super) fn default_table_name(struct_name: &str) -> String {
    pluralize(&to_snake_case(struct_name))
}

/// CamelCase / PascalCase -> snake_case. A `_` is inserted before each uppercase letter that follows a
/// lowercase letter or a digit (so an acronym run like `APIKey` -> `api_key`, matching GORM's own
/// boundary handling closely enough for the model-name shapes that occur in practice).
fn to_snake_case(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower_or_digit =
                i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_numeric());
            // Acronym boundary: `APIKey` -> `api_key` (the `K` follows an uppercase `I` but is itself
            // followed by a lowercase `e`, so a new word starts).
            let next_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();
            let prev_upper = i > 0 && chars[i - 1].is_uppercase();
            if i > 0 && (prev_lower_or_digit || (prev_upper && next_lower)) {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Basic English pluralization of a snake_case name (operates on the trailing word, which is the whole
/// string's ending): `-y` (after a consonant) -> `-ies`; `-s`/`-x`/`-z`/`-ch`/`-sh` -> `+es`; else `+s`.
fn pluralize(s: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    let ends_with_any = |suffixes: &[&str]| suffixes.iter().any(|suf| s.ends_with(suf));
    if let Some(stem) = s.strip_suffix('y') {
        // `-y` after a consonant -> `-ies` (`city` -> `cities`); after a vowel (or nothing) just +s
        // (`key` -> `keys`). `stem`'s last char is exactly the char preceding the trailing `y`.
        let consonant_before = stem
            .chars()
            .last()
            .is_some_and(|c| !matches!(c, 'a' | 'e' | 'i' | 'o' | 'u'));
        if consonant_before {
            return format!("{stem}ies");
        }
        return format!("{s}s");
    }
    if ends_with_any(&["s", "x", "z", "ch", "sh"]) {
        return format!("{s}es");
    }
    format!("{s}s")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_model_name_snakes_and_pluralizes() {
        assert_eq!(default_table_name("ArticleModel"), "article_models");
        assert_eq!(default_table_name("User"), "users");
        assert_eq!(default_table_name("TagModel"), "tag_models");
    }

    #[test]
    fn y_and_sibilant_endings_pluralize_correctly() {
        assert_eq!(default_table_name("Category"), "categories"); // consonant + y -> ies
        assert_eq!(default_table_name("Box"), "boxes"); // x -> es
        assert_eq!(default_table_name("Class"), "classes"); // s -> es
        assert_eq!(default_table_name("Dish"), "dishes"); // sh -> es
        assert_eq!(default_table_name("Key"), "keys"); // vowel + y -> just s
    }

    #[test]
    fn acronym_boundaries_snake_reasonably() {
        assert_eq!(default_table_name("APIKey"), "api_keys");
        assert_eq!(default_table_name("HTTPRequest"), "http_requests");
    }
}
