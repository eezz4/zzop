//! Exercises `rules/dsl/perf/perf.json`'s `jpa-eager-fetch` line-scan rule: an explicit
//! `fetch = FetchType.EAGER` on a JPA/Hibernate relation annotation. Uses its own `jpa_scan` helper
//! (mirrors `perf.rs::scan`'s harness shape) since the shared `scan` helper filters to
//! `perf/api-in-loop` only.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, Finding, RulePackDef};
use zzop_engine::{analyze_tree, EngineConfig};

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

fn perf_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "perf")
        .expect("perf.json pack present")
}

fn jpa_scan(rel: &str, content: &str) -> Vec<Finding> {
    let dir = TempDir::new("zzop-perf-jpa-eager");
    dir.write(rel, content);
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![perf_pack()],
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    out.findings
        .into_iter()
        .filter(|f| f.rule_id == "perf/jpa-eager-fetch")
        .collect()
}

#[test]
fn onetomany_fetch_type_eager_is_flagged() {
    let f = jpa_scan(
        "User.java",
        "package com.example;\n\nimport javax.persistence.*;\nimport java.util.List;\n\n@Entity\npublic class User {\n  @OneToMany(mappedBy = \"user\", fetch = FetchType.EAGER)\n  private List<Photo> photos;\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn manytomany_fetch_type_eager_is_flagged() {
    let f = jpa_scan(
        "User.java",
        "package com.example;\n\nimport javax.persistence.*;\nimport java.util.Set;\n\n@Entity\npublic class User {\n  @ManyToMany(fetch = FetchType.EAGER)\n  private Set<Role> roles;\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn fetch_type_lazy_is_not_flagged() {
    let f = jpa_scan(
        "User.java",
        "package com.example;\n\nimport javax.persistence.*;\nimport java.util.List;\n\n@Entity\npublic class User {\n  @OneToMany(mappedBy = \"user\", fetch = FetchType.LAZY)\n  private List<Photo> photos;\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn no_explicit_fetch_attribute_is_not_flagged() {
    // @ManyToOne defaults to EAGER per the JPA spec with no `fetch` attribute at all — this rule can
    // only see the EXPLICIT declaration, so it correctly stays silent here (see the message's
    // disclosure sentence).
    let f = jpa_scan(
        "Photo.java",
        "package com.example;\n\nimport javax.persistence.*;\n\n@Entity\npublic class Photo {\n  @ManyToOne\n  private User user;\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn commented_out_fetch_type_eager_line_is_not_flagged() {
    let f = jpa_scan(
        "User.java",
        "package com.example;\n\nimport javax.persistence.*;\nimport java.util.List;\n\n@Entity\npublic class User {\n  // @OneToMany(mappedBy = \"user\", fetch = FetchType.EAGER)\n  @OneToMany(mappedBy = \"user\", fetch = FetchType.LAZY)\n  private List<Photo> photos;\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn jpa_eager_ok_marker_above_the_declaration_suppresses_it() {
    let f = jpa_scan(
        "User.java",
        "package com.example;\n\nimport javax.persistence.*;\nimport java.util.List;\n\n@Entity\npublic class User {\n  // jpa-eager-ok: small fixed reference list, always needed with the parent\n  @OneToMany(mappedBy = \"user\", fetch = FetchType.EAGER)\n  private List<Photo> photos;\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn fetch_type_eager_in_a_src_test_directory_is_not_flagged() {
    let f = jpa_scan(
        "src/test/java/com/example/UserTest.java",
        "package com.example;\n\nimport javax.persistence.*;\nimport java.util.List;\n\n@Entity\npublic class UserTest {\n  @OneToMany(mappedBy = \"user\", fetch = FetchType.EAGER)\n  private List<Photo> photos;\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn element_collection_fetch_type_eager_is_flagged() {
    // Live-fire TP from the corpus sweep (be-spring-jwt AppUser.java): the matcher fires on the
    // `fetch = FetchType.EAGER` attribute itself, whichever mapping annotation carries it —
    // @ElementCollection included, not just the four relation annotations. Pins the message's
    // "whichever annotation carries it" scope claim.
    let f = jpa_scan(
        "AppUser.java",
        "package com.example;\n\nimport javax.persistence.*;\nimport java.util.List;\n\n@Entity\npublic class AppUser {\n  @ElementCollection(fetch = FetchType.EAGER)\n  private List<String> roles;\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}
