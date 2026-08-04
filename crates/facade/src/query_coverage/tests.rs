use serde_json::{json, Value};

fn analysis(trees: Value) -> String {
    json!({ "trees": trees }).to_string()
}

/// A tree whose files cover all three dispatch classes: two structural (one via symbols, one via dep
/// membership alone), one lexical-only, one degraded.
fn mixed_tree() -> Value {
    json!({
        "sourceId": "web",
        "output": {
            "ir": {
                "loc": { "src/a.ts": 10, "src/b.ts": 5, "README.md": 40, "src/broken.ts": 7 },
                "symbols": [ { "file": "src/a.ts", "name": "f" } ],
                "dep": { "src/b.ts": [] }
            },
            "degraded": ["src/broken.ts"],
            "coverage": {
                "files": 4, "parserDispatched": 3, "symbols": 1, "resolvedImportEdges": 0,
                "ioProvides": 0, "ioConsumesKeyed": 0, "ioConsumesUnresolved": 0,
                "degraded": 1, "joinContributionZero": true
            }
        }
    })
}

/// A tree whose only structural files are Python — the motivating blind-spot case: every
/// TypeScript-witnessed trigger (retry recognizer, write sites, query call sites) is dark here.
fn python_tree() -> Value {
    json!({
        "sourceId": "py-svc",
        "output": {
            "ir": {
                "loc": { "app/main.py": 30 },
                "symbols": [ { "file": "app/main.py", "name": "handler" } ],
                "dep": {}
            },
            "degraded": [],
            "coverage": {}
        }
    })
}

/// A minimal tree whose every listed file is structural (each carries a symbol) — the shape the
/// class-aware cross probes need, where only the extension MIX varies.
fn structural_tree(id: &str, files: &[&str]) -> Value {
    let loc: serde_json::Map<String, Value> =
        files.iter().map(|f| (f.to_string(), json!(10))).collect();
    let symbols: Vec<Value> = files
        .iter()
        .map(|f| json!({ "file": f, "name": "s" }))
        .collect();
    json!({
        "sourceId": id,
        "output": { "ir": { "loc": loc, "symbols": symbols, "dep": {} },
                     "degraded": [], "coverage": {} }
    })
}

fn run(trees: Value) -> Value {
    let out = crate::query_coverage_json(&analysis(trees)).expect("should answer");
    serde_json::from_str(&out).expect("valid JSON")
}

/// The motivating case for the CAPABILITY axis: a Python-only tree, where the retry recognizer
/// (TypeScript-only) can never witness a trigger — the reply must SAY that instead of letting the
/// rule's zero read as "no replayed write". The entry is derived from the rule's own declaration, so
/// this test also proves the whole plumbing (rule crate -> engine aggregator -> facade cross).
#[test]
fn a_python_tree_surfaces_the_retrying_write_blind_spot() {
    let v = run(json!([python_tree()]));
    let spots = v["trees"][0]["blindSpots"].as_array().expect("array");
    let retry = spots
        .iter()
        .find(|s| s["ruleId"] == "cross-layer/retrying-write-no-idempotency")
        .expect("the retrying-write sightline must cross a py-structural tree");
    let outside: Vec<&str> = retry["structuralOutside"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
        .collect();
    assert_eq!(outside, ["py"]);
    let witnessed = retry["witnessedIn"].as_array().unwrap();
    assert!(witnessed.iter().any(|e| e == "ts"), "{witnessed:?}");
    assert!(!witnessed.iter().any(|e| e == "py"), "{witnessed:?}");
    // The consequence sentence is the rule's own — the zero-means-not-analyzed reading must survive.
    let meaning = retry["meaning"].as_str().unwrap();
    assert!(meaning.contains("NOT ANALYZED"), "{meaning}");
    // The vocabulary ships with the reply, same discipline as dispatchMeaning.
    assert!(v["blindSpotMeaning"]
        .as_str()
        .unwrap()
        .contains("upper bound"));
}

/// The other direction: a tree whose structural files all sit INSIDE every declared sightline gets an
/// EMPTY `blindSpots` — present (a schema position), but with nothing manufactured. `mixed_tree`'s
/// lexical-only `.md` must not leak in: the lexicalOnly legend already covers those files wholesale.
#[test]
fn an_all_typescript_tree_declares_no_blind_spots() {
    let v = run(json!([mixed_tree()]));
    let spots = v["trees"][0]["blindSpots"].as_array().expect("array");
    assert_eq!(spots.as_slice(), &[] as &[Value], "{spots:?}");
    // The basis rides in its crossed form, and an absent engine `warnings` field still forwards [].
    let basis = v["trees"][0]["blindSpotBasis"].as_str().expect("string");
    assert!(basis.contains("crossed against"), "{basis}");
    assert_eq!(v["trees"][0]["warnings"], json!([]));
}

/// Refuter probe 1 of the class-aware cross: TS+Prisma. Under the naive subtraction every declared
/// rule reported blind on `.prisma` — false for all 8: a declaration-only extension cannot host the
/// silence-class evidence, and the `.ts` files DO feed the inverse-class (usage-rule) channel.
#[test]
fn a_ts_plus_prisma_tree_declares_no_blind_spots() {
    let v = run(json!([structural_tree(
        "app",
        &["src/a.ts", "db/schema.prisma"]
    )]));
    let spots = v["trees"][0]["blindSpots"].as_array().expect("array");
    assert_eq!(spots.as_slice(), &[] as &[Value], "{spots:?}");
}

/// Refuter probe 2: schema-only. The assert-when-blind channel is wholly unfed, so exactly the two
/// usage rules disclose — the disclosure a naive "exclude prisma everywhere" would have killed —
/// and `structuralOutside` honestly names prisma, where the flood's subject lives. The
/// silence-class rules must NOT appear: prisma cannot host their evidence either way.
#[test]
fn a_schema_only_tree_discloses_exactly_the_two_assert_when_blind_rules() {
    let v = run(json!([structural_tree("schema", &["db/schema.prisma"])]));
    let spots = v["trees"][0]["blindSpots"].as_array().expect("array");
    let mut ids: Vec<&str> = spots
        .iter()
        .map(|s| s["ruleId"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        [
            "schema/unreferenced-field-name",
            "schema/unreferenced-model-name"
        ]
    );
    for s in spots {
        assert_eq!(
            s["structuralOutside"].as_array().unwrap().as_slice(),
            &[json!("prisma")]
        );
        let meaning = s["meaning"].as_str().unwrap();
        assert!(meaning.contains("evidence-channel blindness"), "{meaning}");
    }
}

/// Refuter probe 3: docs-only — no structural extension exists, so nothing could be crossed, and
/// `blindSpotBasis` must say the empty list is absence of INPUT, never let it read as a verdict.
#[test]
fn a_docs_only_tree_gets_the_absence_of_input_basis_not_a_verdict() {
    let tree = json!({
        "sourceId": "docs",
        "output": { "ir": { "loc": { "README.md": 4 }, "symbols": [], "dep": {} },
                     "degraded": [], "coverage": {} }
    });
    let v = run(json!([tree]));
    assert_eq!(v["trees"][0]["blindSpots"].as_array().unwrap().len(), 0);
    let basis = v["trees"][0]["blindSpotBasis"].as_str().expect("string");
    assert!(basis.contains("absence of input"), "{basis}");
}

/// Refuter probe 4: Go handlers — the silence-class entries survive the class-aware rework
/// unchanged, and the tree's own engine warnings (where the S8 framework-silence self-reports
/// live, e.g. the mutating-route-no-auth call-graph gap) now ride the coverage reply verbatim.
#[test]
fn a_go_tree_keeps_silence_class_entries_and_forwards_its_warnings() {
    let mut tree = structural_tree("go-svc", &["cmd/main.go"]);
    tree["output"]["warnings"] = json!(["Call-graph coverage gap: go routes, example warning"]);
    let v = run(json!([tree]));
    let spots = v["trees"][0]["blindSpots"].as_array().expect("array");
    let silence = spots
        .iter()
        .find(|s| s["ruleId"] == "unsafe-read-endpoint")
        .expect("the write-site silence-class entry must survive on a go tree");
    assert_eq!(
        silence["structuralOutside"].as_array().unwrap().as_slice(),
        &[json!("go")]
    );
    assert_eq!(
        v["trees"][0]["warnings"],
        json!(["Call-graph coverage gap: go routes, example warning"])
    );
}

#[test]
fn groups_by_extension_and_classifies_all_three_dispatch_classes() {
    let v = run(json!([mixed_tree()]));
    let exts = v["trees"][0]["extensions"].as_array().expect("array");
    // BTreeMap order: md before ts.
    assert_eq!(exts[0]["ext"], "md");
    assert_eq!(exts[0]["lexicalOnly"], 1);
    assert_eq!(exts[0]["structural"], 0);
    assert_eq!(exts[1]["ext"], "ts");
    assert_eq!(exts[1]["files"], 3);
    // a.ts via symbols, b.ts via dep membership alone — both count as structural.
    assert_eq!(exts[1]["structural"], 2);
    assert_eq!(exts[1]["degraded"], 1);
    assert_eq!(exts[1]["lexicalOnly"], 0);
    // b.ts sits in the dep map with an EMPTY target list — key presence is "parsed", not "resolved
    // an edge", so it must NOT count toward inDepGraph (the non-empty rule the field's meaning pins).
    assert_eq!(exts[1]["inDepGraph"], 0);
    assert_eq!(exts[0]["inDepGraph"], 0);
}

/// The F4 baseline the 91-files/3-edges tree was missing: a file with a resolved outgoing edge counts
/// toward its extension's `inDepGraph`, one that merely APPEARS in the dep map (empty entry) does not
/// — so per-extension edge sparsity is visible without any new engine data, and the reply's own
/// legend explains how to read a low count.
#[test]
fn in_dep_graph_counts_only_files_with_a_resolved_outgoing_edge() {
    let tree = json!({
        "sourceId": "py-sparse",
        "output": {
            "ir": {
                "loc": { "a.py": 5, "b.py": 5, "c.py": 5 },
                "symbols": [ { "file": "a.py", "name": "f" }, { "file": "b.py", "name": "g" },
                             { "file": "c.py", "name": "h" } ],
                // Engine shape on a real tree: every parsed file has an entry, most empty.
                "dep": { "a.py": ["b.py"], "b.py": [], "c.py": [] }
            },
            "degraded": [], "coverage": {}
        }
    });
    let v = run(json!([tree]));
    let exts = v["trees"][0]["extensions"].as_array().expect("array");
    assert_eq!(exts[0]["ext"], "py");
    assert_eq!(exts[0]["files"], 3);
    assert_eq!(exts[0]["structural"], 3);
    assert_eq!(
        exts[0]["inDepGraph"], 1,
        "only a.py resolved an edge: {exts:?}"
    );
    // The field self-describes in the legend, including the one thing it is NOT (a declared count).
    let meaning = v["dispatchMeaning"]["inDepGraph"].as_str().expect("string");
    assert!(meaning.contains("RESOLVED"), "{meaning}");
    assert!(
        meaning.contains("NOT a declared-imports count"),
        "{meaning}"
    );
}

/// F4: the declared-side denominator rides the extension table. A census key present for the
/// extension becomes a MEASURED cell (the number itself), and an extension with NO key — a parser
/// that projects no import channel, e.g. prisma, or a docs row — renders `null`, never 0: absence
/// of data must stay distinguishable from a measured zero.
#[test]
fn declared_imports_ride_the_extension_table_and_absent_keys_render_null() {
    let tree = json!({
        "sourceId": "py-blind",
        "output": {
            "ir": {
                "loc": { "a.py": 5, "b.py": 5, "db/schema.prisma": 9 },
                "symbols": [ { "file": "a.py", "name": "f" }, { "file": "b.py", "name": "g" },
                             { "file": "db/schema.prisma", "name": "User" } ],
                "dep": { "a.py": [], "b.py": [] }
            },
            "degraded": [],
            // The F4 engine shape on the motivating tree: 12 declared specifiers, nothing resolved.
            "coverage": { "declaredImportsByExt": { "py": 12 } }
        }
    });
    let v = run(json!([tree]));
    let exts = v["trees"][0]["extensions"].as_array().expect("array");
    let py = exts.iter().find(|e| e["ext"] == "py").expect("py row");
    assert_eq!(
        py["declaredImports"], 12,
        "the measured cell must carry the census number: {py}"
    );
    assert_eq!(py["inDepGraph"], 0, "{py}");
    let prisma = exts
        .iter()
        .find(|e| e["ext"] == "prisma")
        .expect("prisma row");
    assert_eq!(
        prisma["declaredImports"],
        Value::Null,
        "no census key = never measured = null, not 0: {prisma}"
    );
    // The yardstick ships with the reply: the definition (pre-resolution specifiers), the non-1:1
    // disclaimer against resolvedImportEdges, and what null means.
    let meaning = v["dispatchMeaning"]["declaredImports"]
        .as_str()
        .expect("string");
    assert!(meaning.contains("BEFORE resolution"), "{meaning}");
    assert!(meaning.contains("NOT 1:1"), "{meaning}");
    assert!(meaning.contains("null"), "{meaning}");
}

/// F4's backward edge: an analysis produced by a build (or Mode A envelope ingest) that carries no
/// `declaredImportsByExt` at all yields `null` in every row — the query must never manufacture a 0
/// out of a census that measured nothing.
#[test]
fn a_census_without_the_declared_map_yields_null_cells_everywhere() {
    let v = run(json!([mixed_tree()]));
    for ext in v["trees"][0]["extensions"].as_array().expect("array") {
        assert_eq!(
            ext["declaredImports"],
            Value::Null,
            "no map, no measurement: {ext}"
        );
    }
}

/// F3: a config with no `vocabulary` walks with an EMPTY skip list (a deliberate contract this
/// surface must not change) — so when .git internals show up in loc, the reply names the count and
/// the cause+fix instead of leaving them as cryptic extension rows like `sample: 14`.
#[test]
fn walked_git_internals_get_a_walk_note_naming_count_and_cause() {
    let tree = json!({
        "sourceId": "raw",
        "output": {
            "ir": {
                "loc": { ".git/config": 5, ".git/objects/pack/x.sample": 1,
                         "vendor/lib/.git/HEAD": 1, "src/main.py": 10 },
                "symbols": [ { "file": "src/main.py", "name": "f" } ],
                "dep": {}
            },
            "degraded": [], "coverage": {}
        }
    });
    let v = run(json!([tree]));
    let note = v["trees"][0]["walkNote"]
        .as_str()
        .expect("walkNote present");
    // All three .git-segment paths count (top-level and nested); `src/main.py` does not.
    assert!(note.starts_with("3 file(s) under .git/"), "{note}");
    assert!(
        note.contains("vocabulary.skipDirs"),
        "names the cause: {note}"
    );
    assert!(note.contains("starter template"), "names the fix: {note}");
}

/// The conditional-absence half of F3, the `otherTrees` convention: absence is unambiguous here
/// (nothing under .git/ was walked), so a normal tree carries NO `walkNote` key at all.
#[test]
fn a_tree_without_git_internals_has_no_walk_note_key() {
    let v = run(json!([mixed_tree()]));
    assert!(
        v["trees"][0].get("walkNote").is_none(),
        "walkNote must be absent, not null/empty: {}",
        v["trees"][0]
    );
}

#[test]
fn join_invisibility_is_a_sentence_not_a_bare_bool() {
    let v = run(json!([mixed_tree()]));
    let vis = v["trees"][0]["joinVisibility"].as_str().expect("string");
    assert!(vis.starts_with("INVISIBLE"), "{vis}");
    // F5: the fix must name WHAT the overlay contributes — the generic "an adapter overlay restores
    // visibility" was measured misleading on a tree that already carried a non-io (import-alias)
    // overlay and stayed join-blind. There is no structured overlays-applied signal on the output to
    // branch on, so the sentence itself must be unable to read as "add any overlay".
    assert!(
        vis.contains("adapter overlay that contributes io facts"),
        "names the fix precisely: {vis}"
    );
    assert!(vis.contains("only imports or attributes does not"), "{vis}");
    // The census still rides verbatim — the sentence adds meaning, it does not replace the numbers.
    assert_eq!(v["trees"][0]["census"]["joinContributionZero"], true);
}

/// THE ruling, as a test: no single score field, ever, and the unmeasured axis is a schema position.
/// The ban is on KEY NAMES, not on the reply text — the prose legitimately says "no single coverage
/// score" to explain why, and a full-text scan would ban the explanation of the ban.
#[test]
fn no_single_score_field_and_recall_is_declared_unmeasured() {
    fn keys_of(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                for (k, child) in m {
                    out.push(k.clone());
                    keys_of(child, out);
                }
            }
            Value::Array(a) => a.iter().for_each(|c| keys_of(c, out)),
            _ => {}
        }
    }
    // Both fixtures, so the scan also walks a NON-empty `blindSpots` array's entry keys.
    let v = run(json!([mixed_tree(), python_tree()]));
    let mut keys = Vec::new();
    keys_of(&v, &mut keys);
    for key in &keys {
        let k = key.to_ascii_lowercase();
        assert!(
            !k.contains("score") && !k.contains("percent") && !k.contains("ratio"),
            "a rollup-shaped FIELD leaked into the reply: {key}"
        );
    }
    let unmeasured = v["unmeasured"].as_array().expect("unmeasured is a FIELD");
    assert_eq!(unmeasured[0]["axis"], "recall");
    assert!(unmeasured[0]["note"]
        .as_str()
        .unwrap()
        .contains("not yours"));
}

/// The build-capability disclosure that had no user surface at all (framework recognizers stood in
/// `zzop_engine` reachable only as a library call): every coverage reply now carries the compiled-in
/// table verbatim, top-level and uncrossed with any tree — a fact of the BUILD, not of this run.
/// The fastapi probe proves the whole plumbing (parser crate declaration -> engine aggregator ->
/// this reply) through one known row without restating the roster, which the engine-side contracts
/// already pin.
#[test]
fn framework_recognizers_ride_the_reply_with_their_channels() {
    let v = run(json!([mixed_tree()]));
    let rows = v["frameworkRecognizers"].as_array().expect("array");
    assert!(!rows.is_empty(), "the compiled-in table cannot be empty");
    for r in rows {
        for key in ["framework", "extensions", "emits"] {
            assert!(!r[key].is_null(), "row missing {key}: {r}");
        }
        assert!(
            !r["emits"].as_array().unwrap().is_empty(),
            "a row with no channel is undeclarable engine-side: {r}"
        );
    }
    let fastapi = rows
        .iter()
        .find(|r| r["framework"] == "fastapi" && r["emits"][0] == "io.provides")
        .expect("the fastapi provide-side row must ride the reply");
    assert_eq!(fastapi["extensions"], json!(["py"]));
    // The legend ships with the reply (the blindSpotMeaning discipline) and must keep both
    // misreading guards: presence is not idiom completeness, absence means no recognizer exists.
    let legend = v["frameworkRecognizerMeaning"].as_str().expect("string");
    assert!(legend.contains("every idiom"), "{legend}");
    assert!(legend.contains("no recognizer in this build"), "{legend}");
}

#[test]
fn dispatch_meanings_ship_in_the_reply() {
    let v = run(json!([mixed_tree()]));
    for key in ["structural", "lexicalOnly", "degraded"] {
        assert!(
            v["dispatchMeaning"][key]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            "missing meaning for {key}"
        );
    }
    // The lexical-only sentence must carry the misreading it exists to prevent.
    assert!(v["dispatchMeaning"]["lexicalOnly"]
        .as_str()
        .unwrap()
        .contains("does NOT mean clean"));
}

#[test]
fn extensionless_files_group_by_their_lowercased_name() {
    let tree = json!({
        "sourceId": "s",
        "output": { "ir": { "loc": { "Makefile": 3, "src/Dockerfile": 8 }, "symbols": [], "dep": {} },
                     "degraded": [], "coverage": {} }
    });
    let v = run(json!([tree]));
    let exts: Vec<&str> = v["trees"][0]["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["ext"].as_str().unwrap())
        .collect();
    assert_eq!(exts, ["dockerfile", "makefile"]);
}

#[test]
fn a_non_trees_analysis_is_refused_with_the_reason() {
    let err = crate::query_coverage_json(&json!({"findings": []}).to_string()).unwrap_err();
    // Spelling-free by contract 15b (host_vocabulary's facade-entry-point sweep): the guidance names
    // the artifact shape, never the `analyzeTrees` symbol no host user can type.
    assert!(err.contains("multi-tree analysis output"), "{err}");
}

/// F4 through the REAL pipeline (parse -> assemble census -> this table). The engine census
/// (`crates/engine/src/analyze/assemble/declared.rs`) and this module's `ext_of` are deliberate
/// byte-for-byte twins with no shared code, and a grain drift between them does not fail loudly —
/// the census key misses its table row and the cell degrades to `null` ("never measured"), the exact
/// misreading F4 exists to prevent. This is the one test that crosses the two halves (every other F4
/// pin hand-authors one side's fixture), so it also pins the `declaredImportsByExt` wire key both
/// sides read.
#[test]
fn declared_imports_cell_is_measured_end_to_end_from_a_real_engine_run() {
    let dir = crate::test_support::cycle_fixture();
    dir.write("schema.prisma", "model A {\n  id Int @id\n}\n");
    let config = json!({ "trees": [{ "root": dir.path().to_string_lossy(), "sourceId": "e2e" }] });
    let analysis = crate::analyze_trees_json(&config.to_string()).expect("analyze should succeed");
    let v: Value = serde_json::from_str(
        &crate::query_coverage_json(&analysis).expect("coverage should answer"),
    )
    .expect("valid JSON");
    let rows = v["trees"][0]["extensions"].as_array().expect("rows");
    let cell = |ext: &str| {
        rows.iter()
            .find(|r| r["ext"] == ext)
            .unwrap_or_else(|| panic!("no `{ext}` row"))["declaredImports"]
            .clone()
    };
    // `cycle_fixture`: a.ts <-> b.ts, one relative import each = 2 distinct declared specifiers.
    assert_eq!(cell("ts"), json!(2));
    // Same run's unmeasured cell: prisma projects no import channel, so `null` — never 0.
    assert_eq!(cell("prisma"), Value::Null);
}
