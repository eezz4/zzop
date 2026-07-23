//! Exercises `rules/dsl/perf/perf.json`'s `eager-relation-declared` line-scan rule: TypeORM's
//! `eager: true` relation option and Sequelize's same-line `include: [{ ... all: true` association-
//! select-everything shape. Uses its own `eager_scan` helper (mirrors `perf.rs::scan`'s harness shape)
//! since the shared `scan` helper filters to `perf/api-in-loop` only.

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

fn eager_scan(rel: &str, content: &str) -> Vec<Finding> {
    let dir = TempDir::new("zzop-perf-eager");
    dir.write(rel, content);
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![perf_pack()],
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    out.findings
        .into_iter()
        .filter(|f| f.rule_id == "perf/eager-relation-declared")
        .collect()
}

#[test]
fn typeorm_onetomany_eager_true_is_flagged() {
    let f = eager_scan(
        "photo.entity.ts",
        "import { Entity, OneToMany } from \"typeorm\";\nimport { Photo } from \"./photo.entity\";\n@Entity()\nexport class User {\n  @OneToMany(() => Photo, (photo) => photo.user, { eager: true })\n  photos: Photo[];\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn typeorm_manytoone_object_form_eager_true_is_flagged() {
    let f = eager_scan(
        "photo.entity.ts",
        "import { Entity, ManyToOne, JoinColumn } from \"typeorm\";\nimport { User } from \"./user.entity\";\n@Entity()\nexport class Photo {\n  @ManyToOne(() => User, { eager: true })\n  @JoinColumn()\n  user: User;\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn sequelize_include_all_true_same_line_is_flagged() {
    let f = eager_scan(
        "user.service.ts",
        "import { User } from \"./models/user\";\nexport async function loadUsers() {\n  return User.findAll({ include: [{ all: true }] });\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn sequelize_include_all_true_nested_true_same_line_is_flagged() {
    let f = eager_scan(
        "user.service.ts",
        "import { User } from \"./models/user\";\nexport async function loadUsers() {\n  return User.findAll({ include: [{ all: true, nested: true }] });\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn typeorm_query_time_relations_array_is_not_flagged() {
    let f = eager_scan(
        "user.service.ts",
        "import { User } from \"./user.entity\";\nimport { getRepository } from \"typeorm\";\nexport async function loadUser(id: string) {\n  return getRepository(User).findOne({ where: { id }, relations: [\"photos\"] });\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn typeorm_eager_false_is_not_flagged() {
    let f = eager_scan(
        "photo.entity.ts",
        "import { Entity, OneToMany } from \"typeorm\";\nimport { Photo } from \"./photo.entity\";\n@Entity()\nexport class User {\n  @OneToMany(() => Photo, (photo) => photo.user, { eager: false })\n  photos: Photo[];\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn commented_out_eager_true_line_is_not_flagged() {
    let f = eager_scan(
        "photo.entity.ts",
        "import { Entity, OneToMany } from \"typeorm\";\nimport { Photo } from \"./photo.entity\";\n@Entity()\nexport class User {\n  // @OneToMany(() => Photo, (photo) => photo.user, { eager: true })\n  @OneToMany(() => Photo, (photo) => photo.user)\n  photos: Photo[];\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn eager_relation_ok_marker_above_the_declaration_suppresses_it() {
    let f = eager_scan(
        "photo.entity.ts",
        "import { Entity, OneToMany } from \"typeorm\";\nimport { Photo } from \"./photo.entity\";\n@Entity()\nexport class User {\n  // eager-relation-ok: tiny fixed lookup table, always read with its parent\n  @OneToMany(() => Photo, (photo) => photo.user, { eager: true })\n  photos: Photo[];\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn eager_true_in_a_tests_directory_is_not_flagged() {
    let f = eager_scan(
        "__tests__/photo.entity.ts",
        "import { Entity, OneToMany } from \"typeorm\";\nimport { Photo } from \"./photo.entity\";\n@Entity()\nexport class User {\n  @OneToMany(() => Photo, (photo) => photo.user, { eager: true })\n  photos: Photo[];\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn eager_true_on_a_non_decorator_line_in_an_orm_signal_file_still_fires_documented_heuristic() {
    // FP-adversarial edge the corpus cannot provide, pinned as a DOCUMENTED limitation rather than
    // fixed: the `require_file` gate is file-level (here satisfied by a `typeorm` import elsewhere in
    // the file), so a bare `eager: true` config line fires even though it is not a relation option.
    // The message discloses exactly this ("fires on any `eager: true` line in a file with
    // TypeORM/Sequelize signals, a deliberate heuristic") — if this test starts failing because the
    // matcher got smarter, update the message's disclosure in the same change.
    let f = eager_scan(
        "src/config.ts",
        "import { DataSource } from \"typeorm\";\n\nexport const loaderOptions = {\n  eager: true,\n};\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}
