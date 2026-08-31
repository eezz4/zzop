//! Index-coverage lookup for `fk_no_index` — split out of `rules.rs` when the sentinel veto pushed that
//! file past the 300-line guard. Self-contained: it reads only the two `@@index`/`@@unique` group lists
//! off a model and answers one question about one column name.

/// A field's `@@index`/`@@unique` coverage relative to a single-column lookup: `Leading` if the field
/// leads some group (fully covered — composite indexes serve lookups via their leading prefix);
/// `NonLeading` if it appears later in a group but never leads one (covered only for queries that also
/// constrain the leading column(s)); `None` if it never appears in any group.
pub(super) enum Coverage {
    Leading,
    NonLeading {
        cols: Vec<String>,
        kind: &'static str,
    },
    None,
}

/// Tie-break for a field in multiple groups: `uniques` is checked before `indexes`, first hit wins.
pub(super) fn index_coverage(
    field_name: &str,
    uniques: &[Vec<String>],
    indexes: &[Vec<String>],
) -> Coverage {
    let leads = |groups: &[Vec<String>]| {
        groups
            .iter()
            .any(|g| g.first().map(String::as_str) == Some(field_name))
    };
    if leads(uniques) || leads(indexes) {
        return Coverage::Leading;
    }
    for g in uniques {
        if g.iter().any(|c| c == field_name) {
            return Coverage::NonLeading {
                cols: g.clone(),
                kind: "unique",
            };
        }
    }
    for g in indexes {
        if g.iter().any(|c| c == field_name) {
            return Coverage::NonLeading {
                cols: g.clone(),
                kind: "index",
            };
        }
    }
    Coverage::None
}
