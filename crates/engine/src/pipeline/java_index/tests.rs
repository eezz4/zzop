use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

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

#[test]
fn indexes_package_and_type_names() {
    let dir = TempDir::new("zzop-java-index");
    dir.write("a/C.java", "package com.example.a;\npublic class C {}\n");
    let index = scan_java_index(&dir.0, std::iter::once("a/C.java"));
    assert_eq!(
        index
            .by_type
            .get(&("com.example.a".to_string(), "C".to_string())),
        Some(&"a/C.java".to_string())
    );
    assert_eq!(
        index.by_package.get("com.example.a"),
        Some(&vec!["a/C.java".to_string()])
    );
}

#[test]
fn default_package_file_is_never_indexed() {
    let dir = TempDir::new("zzop-java-index");
    dir.write("Root.java", "public class Root {}\n");
    let index = scan_java_index(&dir.0, std::iter::once("Root.java"));
    assert!(index.by_type.is_empty());
    assert!(index.by_package.is_empty());
}

#[test]
fn by_package_fans_out_to_every_file_in_that_package_sorted() {
    let dir = TempDir::new("zzop-java-index");
    dir.write("b/B.java", "package com.example.a;\npublic class B {}\n");
    dir.write("a/A.java", "package com.example.a;\npublic class A {}\n");
    let index = scan_java_index(&dir.0, vec!["b/B.java", "a/A.java"].into_iter());
    assert_eq!(
        index.by_package.get("com.example.a"),
        Some(&vec!["a/A.java".to_string(), "b/B.java".to_string()])
    );
}

#[test]
fn unreadable_file_contributes_nothing() {
    let dir = TempDir::new("zzop-java-index");
    let index = scan_java_index(&dir.0, std::iter::once("missing/Nope.java"));
    assert!(index.by_type.is_empty());
    assert!(index.by_package.is_empty());
}
