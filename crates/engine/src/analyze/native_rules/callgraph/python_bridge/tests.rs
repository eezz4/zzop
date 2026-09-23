use std::collections::{HashMap, HashSet};

use zzop_core::callgraph::SymbolEdge;

use super::bridge_edges;

fn files(xs: &[&str]) -> HashSet<String> {
    xs.iter().map(|s| (*s).to_string()).collect()
}

fn locals(pairs: &[(&str, &[&str])]) -> HashMap<String, HashSet<String>> {
    pairs
        .iter()
        .map(|(f, names)| ((*f).to_string(), files(names)))
        .collect()
}

fn edge(from: &str, to: &str) -> SymbolEdge {
    SymbolEdge {
        from: from.to_string(),
        to: to.to_string(),
    }
}

/// The V100 shape: `from app import helpers` then `helpers.ensure()`.
#[test]
fn a_submodule_receiver_is_bridged_to_the_module_that_declares_the_name() {
    let graph = vec![edge(
        "app/main.py#create_item",
        "app/__init__.py#helpers.ensure",
    )];
    let out = bridge_edges(
        &graph,
        &files(&["app/__init__.py", "app/main.py", "app/helpers.py"]),
        &locals(&[("app/helpers.py", &["ensure"])]),
    );
    assert_eq!(
        out,
        vec![edge(
            "app/__init__.py#helpers.ensure",
            "app/helpers.py#ensure"
        )]
    );
}

/// A package submodule spelled as a directory works the same way.
#[test]
fn a_package_submodule_is_bridged_through_its_initializer() {
    let graph = vec![edge("app/main.py#h", "app/__init__.py#svc.run")];
    let out = bridge_edges(
        &graph,
        &files(&["app/__init__.py", "app/svc/__init__.py"]),
        &locals(&[("app/svc/__init__.py", &["run"])]),
    );
    assert_eq!(
        out,
        vec![edge("app/__init__.py#svc.run", "app/svc/__init__.py#run")]
    );
}

/// The anti-invention check. A resolver-minted id is a CANDIDATE; bridging to a file that does not
/// declare the name would assert something false about that file.
#[test]
fn a_name_the_module_does_not_declare_is_never_bridged() {
    let graph = vec![edge("app/main.py#h", "app/__init__.py#helpers.ensure")];
    let out = bridge_edges(
        &graph,
        &files(&["app/__init__.py", "app/helpers.py"]),
        &locals(&[("app/helpers.py", &["something_else"])]),
    );
    assert!(out.is_empty(), "{out:?}");
}

/// A receiver that names no file beside the initializer is a real class, not a module.
#[test]
fn a_class_receiver_is_left_alone() {
    let graph = vec![edge("app/main.py#h", "app/__init__.py#Session.add")];
    let out = bridge_edges(
        &graph,
        &files(&["app/__init__.py", "app/main.py"]),
        &locals(&[("app/__init__.py", &["Session"])]),
    );
    assert!(out.is_empty(), "{out:?}");
}

/// Only a package initializer can have submodules beside it. A target inside an ordinary module means
/// the receiver is an attribute of THAT module, and `app/helpers/x.py` is not where it lives.
#[test]
fn a_target_in_an_ordinary_module_is_not_bridged() {
    let graph = vec![edge("app/main.py#h", "app/helpers.py#x.run")];
    let out = bridge_edges(
        &graph,
        &files(&["app/helpers.py", "app/x.py"]),
        &locals(&[("app/x.py", &["run"])]),
    );
    assert!(out.is_empty(), "{out:?}");
}

/// A bare member is already the namespace shape (`import * as X`), which resolves correctly without help.
#[test]
fn a_bare_member_target_needs_no_bridge() {
    let graph = vec![edge("app/main.py#h", "app/__init__.py#ensure")];
    let out = bridge_edges(
        &graph,
        &files(&["app/__init__.py", "app/ensure.py"]),
        &locals(&[("app/ensure.py", &["ensure"])]),
    );
    assert!(out.is_empty(), "{out:?}");
}

/// Both hops of the two-hop fixture are module-attribute calls, and ONE pass mints both bridges —
/// the property that makes this reach any depth.
#[test]
fn one_pass_bridges_every_hop_of_a_chain() {
    let graph = vec![
        edge("app/main.py#create_item", "app/__init__.py#helpers.ensure"),
        edge(
            "app/helpers.py#ensure",
            "app/__init__.py#auth.require_admin",
        ),
    ];
    let out = bridge_edges(
        &graph,
        &files(&[
            "app/__init__.py",
            "app/main.py",
            "app/helpers.py",
            "app/auth.py",
        ]),
        &locals(&[
            ("app/helpers.py", &["ensure"]),
            ("app/auth.py", &["require_admin"]),
        ]),
    );
    assert_eq!(
        out,
        vec![
            edge(
                "app/__init__.py#auth.require_admin",
                "app/auth.py#require_admin"
            ),
            edge("app/__init__.py#helpers.ensure", "app/helpers.py#ensure"),
        ]
    );
}

/// Deterministic output: sorted and deduplicated, so appending to the graph cannot reorder it.
#[test]
fn output_is_sorted_and_deduplicated() {
    let graph = vec![
        edge("a.py#x", "app/__init__.py#helpers.ensure"),
        edge("b.py#y", "app/__init__.py#helpers.ensure"),
    ];
    let out = bridge_edges(
        &graph,
        &files(&["app/__init__.py", "app/helpers.py"]),
        &locals(&[("app/helpers.py", &["ensure"])]),
    );
    assert_eq!(out.len(), 1, "two callers, one bridge: {out:?}");
}
