//! Whole-tree `CommonIr` projection (`build_common_ir`) — the parser -> engine bridge — plus raw
//! LOC counting.

use zzop_core::{CommonIr, IoFacts, MinimalIr};

use crate::{
    adapters, lang, parse_dynamic_imports, parse_imports, parse_re_exports, parse_symbols,
};

/// Project a whole source tree into a `CommonIr` — the parser -> engine bridge. `files` = (rel-path, text) for every file in the tree; produces symbols + the resolved dep graph + loc.
pub fn build_common_ir(source_id: &str, files: &[(String, String)]) -> CommonIr {
    let all_paths: std::collections::HashSet<String> =
        files.iter().map(|(rel, _)| rel.clone()).collect();
    let mut symbols = Vec::new();
    let mut import_pairs = Vec::new();
    let mut re_export_pairs = Vec::new();
    let mut dynamic_import_pairs = Vec::new();
    let mut loc = std::collections::HashMap::new();
    for (rel, text) in files {
        symbols.extend(parse_symbols(rel, text));
        import_pairs.push((rel.clone(), parse_imports(rel, text)));
        re_export_pairs.push((rel.clone(), parse_re_exports(rel, text)));
        dynamic_import_pairs.push((rel.clone(), parse_dynamic_imports(rel, text)));
        loc.insert(rel.clone(), count_loc(text));
    }
    // `build_dep`'s second return value (the ephemeral noncycle-edge exclusion set) feeds circular
    // detection only; this whole-tree, non-incremental projection doesn't run `circular_from_dep` itself
    // (that's an engine-side whole-graph pass), so it's discarded here.
    let (dep, _noncycle_edges) = lang::resolve::build_dep(
        &import_pairs,
        &re_export_pairs,
        &dynamic_import_pairs,
        &all_paths,
    );
    // Project the IO this tree consumes (HTTP egress) so the cross-layer linker can join it to BE providers.
    let consumes = adapters::egress::extract_http_egress(files);
    let io = if consumes.is_empty() {
        None
    } else {
        Some(IoFacts {
            provides: Vec::new(),
            consumes,
        })
    };
    CommonIr {
        source: source_id.to_string(),
        parser: "typescript".to_string(),
        ir: MinimalIr {
            dep,
            symbols,
            loc,
            io,
        },
    }
}

/// Raw physical line count — the Rust equivalent of JS `content.split("\n").length`. Blank/comment-only
/// lines and lines inside block comments or multi-line strings all count; the file is never parsed, just
/// counted. A trailing newline adds 1 (`"a\nb\n".split("\n")` -> length 3) — use `str::split('\n').count()`, not `str::lines()`, which drops that trailing piece.
pub fn count_loc(text: &str) -> u32 {
    text.split('\n').count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- count_loc / build_common_ir --- (see count_loc's doc for the exact semantics these encode)

    #[test]
    fn count_loc_counts_blank_and_comment_lines() {
        assert_eq!(count_loc("export const x = 1;\n\n// comment\nfoo();\n"), 5);
    }

    #[test]
    fn count_loc_counts_block_comment_interior_lines() {
        assert_eq!(count_loc("/* block\n still block */\ncode();\n"), 4);
    }

    #[test]
    fn count_loc_trailing_newline_adds_one() {
        assert_eq!(count_loc("a\nb\n"), 3);
        assert_eq!(count_loc("a\nb"), 2);
    }

    #[test]
    fn count_loc_empty_text_is_one() {
        assert_eq!(count_loc(""), 1);
    }

    #[test]
    fn detects_circular_imports_end_to_end() {
        // Vertical slice: parse TS -> build_common_ir -> circular_from_dep -> cycle found.
        let files = vec![
            (
                "a.ts".to_string(),
                "import { b } from './b';\nexport const a = 1;\n".to_string(),
            ),
            (
                "b.ts".to_string(),
                "import { a } from './a';\nexport const b = 1;\n".to_string(),
            ),
        ];
        let ir = build_common_ir("app", &files);
        let cycles = zzop_core::circular_from_dep(&ir.ir.dep);
        assert_eq!(cycles.len(), 1);
        let mut got = cycles[0].clone();
        got.sort();
        assert_eq!(got, vec!["a.ts".to_string(), "b.ts".to_string()]);
    }

    #[test]
    fn cross_layer_fe_to_be_end_to_end() {
        // Crown-jewel slice: FE TS egress -> IoFacts -> cross-layer join to a BE provider.
        use zzop_core::{link_cross_layer_io, IoFacts, IoProvide, SourceIo};
        let fe_files = vec![(
            "Ctx.tsx".to_string(),
            r#"axios.get("/authen/getUserInfo")"#.to_string(),
        )];
        let fe_ir = build_common_ir("fe", &fe_files);
        let fe = SourceIo {
            source: "fe".to_string(),
            io: fe_ir.ir.io.clone().expect("FE consumes the route"),
        };
        let be = SourceIo {
            source: "be".to_string(),
            io: IoFacts {
                provides: vec![IoProvide {
                    body: None,
                    kind: "http".to_string(),
                    key: "GET /authen/getUserInfo".to_string(),
                    file: "CtrlAuthen.java".to_string(),
                    line: 40,
                    symbol: Some("getUserInfo".to_string()),
                }],
                consumes: Vec::new(),
            },
        };
        let r = link_cross_layer_io(&[fe, be], &zzop_core::LinkOptions::default());
        assert_eq!(r.edges.len(), 1);
        assert!(r.edges[0].cross_source);
        assert_eq!(r.edges[0].key, "GET /authen/getUserInfo");
        assert_eq!(r.edges[0].to.source, "be");
    }

    #[test]
    fn build_common_ir_projects_symbols_dep_loc() {
        let files = vec![
            (
                "a.ts".to_string(),
                "import { x } from './b';\nexport function foo() {}\n".to_string(),
            ),
            ("b.ts".to_string(), "export const x = 1;\n".to_string()),
        ];
        let ir = build_common_ir("app", &files);
        assert_eq!(ir.source, "app");
        assert_eq!(ir.parser, "typescript");
        assert_eq!(ir.ir.dep["a.ts"], vec!["b.ts".to_string()]);
        assert!(ir.ir.symbols.iter().any(|s| s.name == "foo"));
        assert!(ir.ir.symbols.iter().any(|s| s.name == "x"));
        assert_eq!(ir.ir.loc["a.ts"], 3); // trailing-newline artifact, see count_loc's doc
    }
}
