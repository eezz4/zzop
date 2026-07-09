//! Coverage self-report: detects BE frameworks whose route-registration idiom isn't recognized by any
//! provides extractor (Nest-, `@n8n/decorators`-, and Spring-style controller decorators are the shapes
//! currently taught to `zzop_parser_typescript::adapters::controller_decorators`), so cross-layer joins
//! would otherwise silently go dark. This is a lexical, extractor-independent tripwire for the *next*
//! unknown framework.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

fn controller_decorator_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"@\w*(?:Controller|Mapping|Get|Post|Put|Delete|Patch)\b").unwrap()
    })
}

const MIN_FILES: usize = 3;
const MAX_SAMPLES: usize = 3;

/// Returns a ready-to-push `warnings` entry if `candidate_rels` show a controller-decorator-looking line
/// in at least `MIN_FILES` distinct files while `http_provides_count` is exactly zero. Cheap on the
/// success path: skips the disk re-read entirely when `http_provides_count > 0`.
///
/// Determinism: relies on `candidate_rels` already being sorted/deduped by the caller
/// (`analyze::assemble`) — this function performs no re-sort, so an unsorted input would yield a
/// non-deterministic sample.
pub fn controller_silence_warning(
    root: &Path,
    candidate_rels: &[String],
    http_provides_count: usize,
) -> Option<String> {
    if http_provides_count > 0 {
        return None;
    }
    let re = controller_decorator_re();
    let mut matched: Vec<&str> = Vec::new();
    for rel in candidate_rels {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        if text.lines().any(|line| re.is_match(line)) {
            matched.push(rel.as_str());
        }
    }
    if matched.len() < MIN_FILES {
        return None;
    }
    let sample: Vec<&str> = matched.iter().take(MAX_SAMPLES).copied().collect();
    let mut sample_str = sample.join(", ");
    if matched.len() > MAX_SAMPLES {
        sample_str.push_str(&format!(", +{} more", matched.len() - MAX_SAMPLES));
    }
    Some(format!(
        "{} file(s) carry controller-style route decorators/annotations but no http routes were extracted \
— the framework's registration idiom may be unsupported; cross-layer joins will be silent for this tree \
(e.g. {sample_str}) — project this tree's routes with a Mode B overlay adapter (see the adapter examples) \
to restore cross-layer visibility.",
        matched.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
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

    #[test]
    fn three_or_more_matching_files_with_zero_http_provides_warns() {
        let dir = TempDir::new("zzop-coverage-warn");
        // `@FastController`/`@FastGet` — an invented decorator idiom matching the regex
        // (`@\w*(?:Controller|...)\b`): the suffix sits directly after `@` with only word chars between.
        dir.write(
            "a.ts",
            "@FastController('/a')\nclass A {\n  @FastGet('/x')\n  x() {}\n}\n",
        );
        dir.write(
            "b.ts",
            "@FastController('/b')\nclass B {\n  @FastGet('/y')\n  y() {}\n}\n",
        );
        dir.write(
            "c.ts",
            "@FastController('/c')\nclass C {\n  @FastGet('/z')\n  z() {}\n}\n",
        );
        let rels = vec!["a.ts".to_string(), "b.ts".to_string(), "c.ts".to_string()];
        let warning = controller_silence_warning(dir.path(), &rels, 0);
        assert!(
            warning
                .as_deref()
                .is_some_and(|w| w
                    .contains("route decorators/annotations but no http routes were extracted")),
            "got: {warning:?}"
        );
    }

    #[test]
    fn nonzero_http_provides_short_circuits_without_even_reading_files() {
        // Paths don't exist on disk; if this ever performed a real read it would silently skip
        // unreadable files rather than panic, so this just verifies the cheap short-circuit path
        // returns `None`.
        let rels = vec![
            "does/not/exist/a.ts".to_string(),
            "does/not/exist/b.ts".to_string(),
            "does/not/exist/c.ts".to_string(),
        ];
        let warning = controller_silence_warning(Path::new("."), &rels, 1);
        assert!(warning.is_none());
    }

    #[test]
    fn below_threshold_file_count_does_not_warn() {
        let dir = TempDir::new("zzop-coverage-below-threshold");
        dir.write(
            "a.ts",
            "@FastController('/a')\nclass A {\n  @FastGet('/x')\n  x() {}\n}\n",
        );
        dir.write(
            "b.ts",
            "@FastController('/b')\nclass B {\n  @FastGet('/y')\n  y() {}\n}\n",
        );
        let rels = vec!["a.ts".to_string(), "b.ts".to_string()];
        let warning = controller_silence_warning(dir.path(), &rels, 0);
        assert!(warning.is_none(), "got: {warning:?}");
    }

    #[test]
    fn angular_style_decorators_never_match_the_controller_regex() {
        // None of Angular's own decorator vocabulary lexically matches
        // Controller/Mapping/Get/Post/Put/Delete/Patch.
        let dir = TempDir::new("zzop-coverage-angular");
        dir.write(
            "a.ts",
            "@Component({ selector: 'app-a' })\nclass A {\n  @Input() x: string;\n  @Output() y = new EventEmitter();\n  @HostListener('click')\n  onClick() {}\n}\n",
        );
        dir.write(
            "b.ts",
            "@Component({ selector: 'app-b' })\nclass B {\n  @Inject(TOKEN) dep: any;\n}\n",
        );
        dir.write(
            "c.ts",
            "@Component({ selector: 'app-c' })\nclass C {\n  @Input() z: number;\n}\n",
        );
        let rels = vec!["a.ts".to_string(), "b.ts".to_string(), "c.ts".to_string()];
        let warning = controller_silence_warning(dir.path(), &rels, 0);
        assert!(warning.is_none(), "got: {warning:?}");
    }
}
