//! Coverage for `compose_trpc_provides`: inline nested + leaf composition, cross-file `Ref` via
//! specifier, same-file `Ref` by name, `mergeRouters` empty-key splice, an unresolvable `Ref` skipped
//! (sibling entries survive), a self-referencing cycle guarded against infinite recursion, and
//! determinism under input-order reshuffling.
use super::*;
use zzop_core::{ProcedureRouterEntry, ProcedureRouterFragment};

/// A resolver that only ever answers the exact `(specifier, from_file)` pairs listed — anything else is
/// `None`, mirroring how a real unresolvable/external specifier behaves.
fn resolver(
    table: &'static [(&'static str, &'static str, &'static str)],
) -> impl Fn(&str, &str) -> Option<String> {
    move |specifier, from_file| {
        table
            .iter()
            .find(|(s, f, _)| *s == specifier && *f == from_file)
            .map(|(_, _, target)| target.to_string())
    }
}

fn no_resolver() -> impl Fn(&str, &str) -> Option<String> {
    |_, _| None
}

fn frag(name: &str, entries: Vec<ProcedureRouterEntry>) -> ProcedureRouterFragment {
    ProcedureRouterFragment {
        name: name.to_string(),
        entries,
    }
}

fn keys(out: &[IoProvide]) -> Vec<(String, String, u32)> {
    out.iter()
        .map(|p| (p.key.clone(), p.file.clone(), p.line))
        .collect()
}

#[test]
fn root_with_inline_nested_and_leaf() {
    let fragments = vec![(
        "a.ts".to_string(),
        vec![frag(
            "appRouter",
            vec![
                ProcedureRouterEntry::Nested {
                    key: "greeting".into(),
                    entries: vec![ProcedureRouterEntry::Leaf {
                        key: "hello".into(),
                        verb: "QUERY".into(),
                        line: 2,
                    }],
                },
                ProcedureRouterEntry::Leaf {
                    key: "ping".into(),
                    verb: "QUERY".into(),
                    line: 5,
                },
            ],
        )],
    )];
    let out = compose_trpc_provides(fragments, no_resolver());
    assert_eq!(
        keys(&out),
        vec![
            ("QUERY greeting.hello".to_string(), "a.ts".to_string(), 2),
            ("QUERY ping".to_string(), "a.ts".to_string(), 5),
        ]
    );
}

#[test]
fn ref_via_specifier_resolves_to_another_files_fragment() {
    let fragments = vec![
        (
            "trpc.ts".to_string(),
            vec![frag(
                "appRouter",
                vec![ProcedureRouterEntry::Ref {
                    key: "viewer".into(),
                    ident: "viewerRouter".into(),
                    specifier: Some("./viewer".into()),
                }],
            )],
        ),
        (
            "viewer.ts".to_string(),
            vec![frag(
                "viewerRouter",
                vec![ProcedureRouterEntry::Leaf {
                    key: "me".into(),
                    verb: "QUERY".into(),
                    line: 1,
                }],
            )],
        ),
    ];
    let out = compose_trpc_provides(fragments, resolver(&[("./viewer", "trpc.ts", "viewer.ts")]));
    assert_eq!(
        keys(&out),
        vec![("QUERY viewer.me".to_string(), "viewer.ts".to_string(), 1)]
    );
}

#[test]
fn same_file_ref_by_name_has_no_specifier() {
    let fragments = vec![(
        "r.ts".to_string(),
        vec![
            frag(
                "outer",
                vec![ProcedureRouterEntry::Ref {
                    key: "nested".into(),
                    ident: "inner".into(),
                    specifier: None,
                }],
            ),
            frag(
                "inner",
                vec![ProcedureRouterEntry::Leaf {
                    key: "x".into(),
                    verb: "QUERY".into(),
                    line: 3,
                }],
            ),
        ],
    )];
    let out = compose_trpc_provides(fragments, no_resolver());
    assert_eq!(
        keys(&out),
        vec![("QUERY nested.x".to_string(), "r.ts".to_string(), 3)]
    );
}

#[test]
fn merge_routers_empty_key_splices_at_the_current_level() {
    let fragments = vec![
        (
            "r.ts".to_string(),
            vec![frag(
                "combined",
                vec![
                    ProcedureRouterEntry::Ref {
                        key: String::new(),
                        ident: "aRouter".into(),
                        specifier: Some("./a".into()),
                    },
                    ProcedureRouterEntry::Ref {
                        key: String::new(),
                        ident: "bRouter".into(),
                        specifier: Some("./b".into()),
                    },
                ],
            )],
        ),
        (
            "a.ts".to_string(),
            vec![frag(
                "aRouter",
                vec![ProcedureRouterEntry::Leaf {
                    key: "x".into(),
                    verb: "QUERY".into(),
                    line: 1,
                }],
            )],
        ),
        (
            "b.ts".to_string(),
            vec![frag(
                "bRouter",
                vec![ProcedureRouterEntry::Leaf {
                    key: "y".into(),
                    verb: "MUTATION".into(),
                    line: 2,
                }],
            )],
        ),
    ];
    let out = compose_trpc_provides(
        fragments,
        resolver(&[("./a", "r.ts", "a.ts"), ("./b", "r.ts", "b.ts")]),
    );
    assert_eq!(
        keys(&out),
        vec![
            ("MUTATION y".to_string(), "b.ts".to_string(), 2),
            ("QUERY x".to_string(), "a.ts".to_string(), 1),
        ]
    );
}

#[test]
fn unresolvable_ref_is_skipped_sibling_leaf_survives() {
    let fragments = vec![(
        "a.ts".to_string(),
        vec![frag(
            "appRouter",
            vec![
                ProcedureRouterEntry::Ref {
                    key: "missing".into(),
                    ident: "ghost".into(),
                    specifier: Some("./ghost".into()),
                },
                ProcedureRouterEntry::Leaf {
                    key: "ok".into(),
                    verb: "QUERY".into(),
                    line: 1,
                },
            ],
        )],
    )];
    // resolver answers nothing -> `./ghost` never resolves; `ghost` also names no known fragment even
    // if it did.
    let out = compose_trpc_provides(fragments, no_resolver());
    assert_eq!(
        keys(&out),
        vec![("QUERY ok".to_string(), "a.ts".to_string(), 1)]
    );
}

#[test]
fn self_referencing_cycle_is_guarded_without_infinite_recursion() {
    let fragments = vec![(
        "app.ts".to_string(),
        vec![
            frag(
                "app",
                vec![ProcedureRouterEntry::Ref {
                    key: "a".into(),
                    ident: "a".into(),
                    specifier: None,
                }],
            ),
            frag(
                "a",
                vec![
                    ProcedureRouterEntry::Leaf {
                        key: "x".into(),
                        verb: "QUERY".into(),
                        line: 5,
                    },
                    // Cycles back to itself — must be skipped, not re-composed.
                    ProcedureRouterEntry::Ref {
                        key: "loop".into(),
                        ident: "a".into(),
                        specifier: None,
                    },
                ],
            ),
        ],
    )];
    let out = compose_trpc_provides(fragments, no_resolver());
    assert_eq!(
        keys(&out),
        vec![("QUERY a.x".to_string(), "app.ts".to_string(), 5)]
    );
}

#[test]
fn composition_is_deterministic_under_input_order_reshuffling() {
    let build = |reversed: bool| {
        let mut fragments = vec![
            (
                "trpc.ts".to_string(),
                vec![frag(
                    "appRouter",
                    vec![ProcedureRouterEntry::Ref {
                        key: "viewer".into(),
                        ident: "viewerRouter".into(),
                        specifier: Some("./viewer".into()),
                    }],
                )],
            ),
            (
                "viewer.ts".to_string(),
                vec![frag(
                    "viewerRouter",
                    vec![ProcedureRouterEntry::Leaf {
                        key: "me".into(),
                        verb: "QUERY".into(),
                        line: 1,
                    }],
                )],
            ),
        ];
        if reversed {
            fragments.reverse();
        }
        fragments
    };
    let resolve = || resolver(&[("./viewer", "trpc.ts", "viewer.ts")]);
    let out1 = compose_trpc_provides(build(false), resolve());
    let out2 = compose_trpc_provides(build(true), resolve());
    assert_eq!(keys(&out1), keys(&out2));
}
