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
fn indexes_a_block_namespace() {
    let dir = TempDir::new("zzop-csharp-index");
    dir.write("a/C.cs", "namespace App.Services { public class C {} }\n");
    let index = scan_csharp_index(&dir.0, std::iter::once("a/C.cs"));
    assert_eq!(
        index.by_namespace.get("App.Services"),
        Some(&vec!["a/C.cs".to_string()])
    );
}

#[test]
fn indexes_a_file_scoped_namespace() {
    let dir = TempDir::new("zzop-csharp-index");
    dir.write("a/C.cs", "namespace App.Services;\npublic class C {}\n");
    let index = scan_csharp_index(&dir.0, std::iter::once("a/C.cs"));
    assert_eq!(
        index.by_namespace.get("App.Services"),
        Some(&vec!["a/C.cs".to_string()])
    );
}

#[test]
fn file_with_no_namespace_contributes_nothing() {
    let dir = TempDir::new("zzop-csharp-index");
    dir.write("Root.cs", "public class Root {}\n");
    let index = scan_csharp_index(&dir.0, std::iter::once("Root.cs"));
    assert!(index.by_namespace.is_empty());
}

#[test]
fn by_namespace_fans_out_to_every_file_declaring_it_sorted() {
    let dir = TempDir::new("zzop-csharp-index");
    dir.write("b/B.cs", "namespace App.Services { public class B {} }\n");
    dir.write("a/A.cs", "namespace App.Services { public class A {} }\n");
    let index = scan_csharp_index(&dir.0, vec!["b/B.cs", "a/A.cs"].into_iter());
    assert_eq!(
        index.by_namespace.get("App.Services"),
        Some(&vec!["a/A.cs".to_string(), "b/B.cs".to_string()])
    );
}

#[test]
fn unreadable_file_contributes_nothing() {
    let dir = TempDir::new("zzop-csharp-index");
    let index = scan_csharp_index(&dir.0, std::iter::once("missing/Nope.cs"));
    assert!(index.by_namespace.is_empty());
}
