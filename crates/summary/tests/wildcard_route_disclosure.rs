//! THE OUTERMOST PIN for the ANT wildcard-route partition's disclosure.
//!
//! The partition itself (`zzop_core::io::wildcard`) buys three removed false findings with silence: the
//! route stops being an edge candidate, stops being reportable as a dead route, and swallows the calls it
//! serves. Every one of those is invisible in the bucket counts, so the run has to SAY it — and this repo
//! has already measured, twice, that a disclosure written into a rule is not the same thing as a
//! disclosure that arrives (2026-08-13: one rule's caveat had zero runtime carriers across 18
//! checkouts). An assertion inside the engine proves the string was built; only an assertion on the
//! REPLY proves it was delivered.
//!
//! So this pin sits on `zzop_summary::cross_summary` — the exact bytes `zzop cross` / the MCP
//! `cross_repo` tool hand a reader — and reads the field the reader reads (`sources[].warnings`), not an
//! engine struct. Break the carrier anywhere between `link_cross_layer_io` and the reply (drop the
//! substrate field, drop the engine's `wildcard_disclosure::disclose` call, or stop copying
//! `output.warnings` into `sources[]`) and this fails while every inner test stays green.

use std::fs;

fn default_filters() -> zzop_summary::FindingFilters {
    zzop_summary::FindingFilters::new(None, None, None).expect("no-filter view always constructs")
}

fn tmp_tree(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("zzop-wildcard-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("zzop.config.jsonc"),
        zzop_config::template::CONFIG_TEMPLATE_JSONC,
    )
    .unwrap();
    for (rel, content) in files {
        let full = dir.join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }
    dir
}

#[test]
fn the_cross_reply_tells_the_reader_which_routes_were_partitioned_and_how_much_they_swallowed() {
    let be = tmp_tree(
        "be",
        &[(
            "src/main/java/apps/controllers/FileController.java",
            concat!(
                "@RequestMapping(\"/api\")\n",
                "@RestController\n",
                "public class FileController {\n",
                "    @GetMapping(\"/files/**\")\n",
                "    public byte[] serveFile() {\n        return null;\n    }\n",
                "}\n",
            ),
        )],
    );
    let fe = tmp_tree(
        "fe",
        &[(
            "src/api.ts",
            "export const deep = () => axios.get(\"/api/files/a/b/c\");\n",
        )],
    );

    let paths = vec![fe.display().to_string(), be.display().to_string()];
    let out = zzop_summary::cross_summary(&paths, None, &default_filters())
        .expect("cross must succeed on two configured trees");
    let v: serde_json::Value = serde_json::from_str(&out).expect("a reply is JSON");

    let sources = v["sources"].as_array().expect("sources array");
    let notes: Vec<&str> = sources
        .iter()
        .filter_map(|s| s["warnings"].as_array())
        .flatten()
        .filter_map(|w| w.as_str())
        .filter(|w| w.contains("wildcard route(s) partitioned OUT"))
        .collect();
    assert_eq!(
        notes.len(),
        1,
        "the partition must reach the reply's own sources[].warnings exactly once, got reply: {out}"
    );
    // The two facts a reader cannot recompute from any bucket count: WHICH route, and HOW MUCH silence.
    assert!(
        notes[0].contains("`GET /api/files/**`"),
        "the note must name the partitioned route: {}",
        notes[0]
    );
    assert!(
        notes[0].contains("covered 1 consume call site(s)"),
        "the note must say how many calls it swallowed — a partition with an unstated cost is the \
         silence this disclosure exists to abolish: {}",
        notes[0]
    );

    // And the reply's own buckets agree that the silence happened: no dead-route row for the pattern,
    // no missing-route row for the call it serves. Read off the SUMMARY, not the engine.
    assert_eq!(
        v["buckets"]["unconsumedProvides"], 0,
        "a live catch-all must not be counted as a dead route: {out}"
    );
    assert_eq!(
        v["buckets"]["unprovidedConsumes"], 0,
        "a call the catch-all serves must not be counted as a missing route: {out}"
    );

    let _ = fs::remove_dir_all(&be);
    let _ = fs::remove_dir_all(&fe);
}
