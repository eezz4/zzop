//! End-to-end pin for the ANT wildcard-route partition (`zzop_core::io::wildcard`), on the shape that
//! produced it: a Spring `@GetMapping("/files/**")` under `@RequestMapping("/api")` plus two FE calls
//! beneath it.
//!
//! Measured on this exact fixture BEFORE the partition existed (2026-08-13) — one cause, three wrong
//! answers:
//!
//! ```text
//! unconsumedProvides : GET /api/files/**            <- a live catch-all reported as a dead route
//! unprovidedConsumes : GET /api/files/a/b/c
//!                      GET /api/files/img/logo.png  <- two served calls reported as missing routes
//! ```
//!
//! What this file pins, and why each part has to be here:
//! - the three false rows are gone, and the EDGE COUNT DID NOT MOVE. That pairing is the whole claim: a
//!   partition removes a provide that could never join and the consumes that provide really serves, so
//!   reading success as "more edges" would be reading the wrong number (the prescription this fix
//!   replaced expected exactly that, and it was wrong).
//! - three CONTROLS, so a green run cannot come from the join going blind: an ordinary exact route still
//!   joins into its edge, a call to a genuinely absent route still lands in `unprovidedConsumes`, and a
//!   `POST` beneath a GET-only catch-all still lands there too (the pattern suppresses its OWN verb, not
//!   the path space).
//! - the DISCLOSURE reaches the declaring tree's own warnings channel, naming the route and how many call
//!   sites it swallowed. The partition buys correctness with silence; an undisclosed silence is the
//!   defect, not the fix. (The outermost carrier — `zzop cross`'s `sources[].warnings` — is pinned one
//!   layer further out, in `zzop-summary`'s `wildcard_route_disclosure` test.)

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_trees, EngineConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn config(source_id: &str) -> EngineConfig {
    EngineConfig {
        source_id: source_id.to_string(),
        ..EngineConfig::default()
    }
}

fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-wildcard-fe");
    dir.write(
        "src/api.ts",
        concat!(
            // Two calls served by the catch-all — the pair that used to read as missing routes.
            "export const deep = () => axios.get(\"/api/files/a/b/c\");\n",
            "export const asset = () => axios.get(\"/api/files/img/logo.png\");\n",
            // Control 1: an ordinary exact route that must still join into an edge.
            "export const health = () => axios.get(\"/api/health\");\n",
            // Control 2: a genuinely absent route — must STAY unprovided.
            "export const ghost = () => axios.get(\"/api/ghost\");\n",
            // Control 3: the catch-all is GET-only, so a POST beneath it is genuinely unprovided.
            "export const upload = () => axios.post(\"/api/files/new\");\n",
        ),
    );
    dir
}

fn java_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-wildcard-be-java");
    dir.write(
        "src/main/java/apps/controllers/FileController.java",
        concat!(
            "@RequestMapping(\"/api\")\n",
            "@RestController\n",
            "public class FileController {\n",
            "    @GetMapping(\"/files/**\")\n",
            "    public byte[] serveFile() {\n        return null;\n    }\n",
            "    @GetMapping(\"/health\")\n",
            "    public String health() {\n        return null;\n    }\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn a_wildcard_route_is_partitioned_out_of_the_join_and_the_edge_count_does_not_move() {
    let fe = fe_tree();
    let be = java_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be-java")),
    ];
    let out = analyze_trees(&trees);
    let cl = &out.cross_layer;

    // CONTROL: the exact route still joins. Exactly one http edge — the same count the pre-partition
    // run produced, which is the point: the partition creates no edge and destroys none.
    let http_edges: Vec<_> = cl.edges.iter().filter(|e| e.kind == "http").collect();
    assert_eq!(
        http_edges.len(),
        1,
        "the exact route must still join and the pattern must not add an edge, got: {:?}",
        cl.edges
    );
    assert_eq!(http_edges[0].key, "GET /api/health");

    // The wildcard route is NOT a dead route (falsehood #1, was 1 row).
    let unconsumed: Vec<&str> = cl
        .unconsumed_provides
        .iter()
        .map(|p| p.provide.key.as_str())
        .collect();
    assert!(
        !unconsumed.contains(&"GET /api/files/**"),
        "a live catch-all must never be reported as an unconsumed dead route, got: {unconsumed:?}"
    );

    // The two calls beneath it are NOT unprovided (falsehoods #2 and #3, were 2 rows).
    let unprovided: Vec<&str> = cl
        .unprovided_consumes
        .iter()
        .filter_map(|c| c.consume.key.as_deref())
        .collect();
    assert!(
        !unprovided.contains(&"GET /api/files/a/b/c")
            && !unprovided.contains(&"GET /api/files/img/logo.png"),
        "calls served by the catch-all must not be reported as missing routes, got: {unprovided:?}"
    );

    // CONTROLS: the suppression is targeted, not a blanket silence. A route nobody serves and a POST
    // beneath a GET-only catch-all both still fire — if these ever go quiet, the fixture stopped
    // proving anything.
    assert!(
        unprovided.contains(&"GET /api/ghost"),
        "a genuinely absent route must still be reported, got: {unprovided:?}"
    );
    assert!(
        unprovided.contains(&"POST /api/files/new"),
        "a GET-only catch-all must not swallow a POST beneath it — the verb is part of the match, \
         got: {unprovided:?}"
    );
}

#[test]
fn the_partition_is_disclosed_on_the_declaring_trees_own_warnings_with_its_covered_count() {
    let fe = fe_tree();
    let be = java_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be-java")),
    ];
    let out = analyze_trees(&trees);

    // Substrate: the linker names the route, its site, and how many call sites it swallowed.
    let partitions = &out.cross_layer.wildcard_route_partitions;
    assert_eq!(
        partitions.len(),
        1,
        "exactly one wildcard route in this fixture, got: {partitions:?}"
    );
    assert_eq!(partitions[0].source, "be-java");
    assert_eq!(partitions[0].key, "GET /api/files/**");
    assert_eq!(
        partitions[0].file,
        "src/main/java/apps/controllers/FileController.java"
    );
    assert_eq!(
        partitions[0].covered_consumes, 2,
        "both served calls must be charged to the route that swallowed them"
    );

    // Delivery: the DECLARING tree self-reports it, and the FE tree stays silent about someone else's
    // route. A count that reaches no channel is the failure mode this assertion exists for.
    let be_out = &out
        .trees
        .iter()
        .find(|(_, source, _)| source == "be-java")
        .expect("be-java tree present")
        .2;
    let notes: Vec<&String> = be_out
        .warnings
        .iter()
        .filter(|w| w.contains("wildcard route(s) partitioned OUT"))
        .collect();
    assert_eq!(
        notes.len(),
        1,
        "one wildcard-partition warning on the declaring tree, got: {:?}",
        be_out.warnings
    );
    assert!(notes[0].contains("1 wildcard route(s)"), "{}", notes[0]);
    assert!(
        notes[0].contains("`GET /api/files/**` (covered 2 consume call site(s))"),
        "the note must name the route AND how much silence it bought: {}",
        notes[0]
    );
    assert!(
        notes[0].contains("unprovidedConsumes") && notes[0].contains("unconsumedProvides"),
        "the note must name the buckets it moved, or a reader cannot check it: {}",
        notes[0]
    );

    let fe_out = &out
        .trees
        .iter()
        .find(|(_, source, _)| source == "fe")
        .expect("fe tree present")
        .2;
    assert!(
        !fe_out
            .warnings
            .iter()
            .any(|w| w.contains("wildcard route(s) partitioned OUT")),
        "the warning belongs to the tree that DECLARES the route, got: {:?}",
        fe_out.warnings
    );
}
