//! End-to-end proof that the four 2026-08-02 `loop_spans` producers (Python/Java/C#/Rust — see each
//! `parser/*/src/lang/loop_spans.rs`) REACH `MethodScan::trigger_in_loop` — the whole chain
//! `parser -> pipeline::fresh::spans -> SourceFile::loop_spans -> dsl::method_scan`, driven through
//! the real `analyze_tree` entry point. Same rationale as `analyze_rust_test_spans.rs`: a parser unit
//! test proves the fact EXISTS, not that it ARRIVES across the four struct boundaries on the way to a
//! matcher.
//!
//! ## Three directions per language, all in ONE analyze run
//! - **positive**: the probe call structurally inside a real loop fires, on the call's line;
//! - **negative**: the IDENTICAL call outside any loop is silent (proves the gate exists — without
//!   this, "fires" above could just mean `trigger_in_loop` degraded to plain co-occurrence);
//! - **lazy-silence**: the call inside that language's lazy iteration form (Python generator
//!   expression, Java Stream `.map` lambda, C# LINQ `.Select` lambda, Rust `.iter().map` closure) is
//!   silent — the contract boundary `zzop_core::dsl::SourceFile::loop_spans`'s field doc owns:
//!   lazy bodies run zero times unless consumed, so no per-iteration proof exists.
//!
//! Python additionally pins the EAGER side of its own boundary: a list comprehension (eager) fires.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

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

/// A synthetic `trigger_in_loop` probe rule, loaded through the real `load_dsl_packs` path (same JSON
/// schema every shipped pack uses) — no SHIPPED rule admits these four environments today (the
/// 2026-08-02 rule-widening adjudication kept all eleven TS-vocabulary `trigger_in_loop` rules
/// unwidened), so the channel is proven with a vocabulary-neutral probe instead.
const PROBE_PACK_JSON: &str = r#"{
  "id": "loop-spans-probe",
  "schema_version": 1,
  "framework": "any",
  "rules": [
    {
      "id": "call-in-loop",
      "severity": "info",
      "message": "loop-spans reach probe (NOT a real finding): a zzop_probe/zzopProbe call structurally inside a projected loop span.",
      "matcher": {
        "type": "method-scan",
        "file_pattern": "(?i)\\.(py|java|cs|rs)$",
        "patterns": [{ "pattern": "(?i)zzop_?probe\\s*\\(", "label": "probe" }],
        "trigger": "probe",
        "trigger_in_loop": true
      }
    }
  ]
}
"#;

fn probe_pack() -> RulePackDef {
    let dir = TempDir::new("zzop-loop-spans-probe-pack");
    dir.write("loop-spans-probe.json", PROBE_PACK_JSON);
    let result = load_dsl_packs(dir.path());
    assert!(
        result.errors.is_empty(),
        "probe pack failed to load: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "loop-spans-probe")
        .expect("loop-spans-probe pack present")
}

/// `(rel, source, expected probe-hit lines)` — every fixture in one tree, one analyze run.
fn fixtures() -> Vec<(&'static str, &'static str, Vec<u32>)> {
    vec![
        // ---- Python ----
        (
            "py_in_loop.py",
            "def handler(items):\n    for item in items:\n        zzop_probe(item)\n",
            vec![3],
        ),
        (
            "py_outside.py",
            "def handler(item):\n    zzop_probe(item)\n",
            vec![],
        ),
        // Lazy: a generator expression's element runs zero times unless consumed — must stay silent.
        (
            "py_lazy.py",
            "def handler(items):\n    g = (zzop_probe(x) for x in items)\n    return g\n",
            vec![],
        ),
        // Eager twin of the genexp above — same element code, brackets instead of parens: fires.
        // MULTI-line on purpose: a single-line comprehension projects no span at all (line-granular
        // containment cannot tell its body from the receiver line — 2026-08-03 repair), which the
        // next fixture pins as the negative.
        (
            "py_comp.py",
            "def handler(items):\n    return [\n        zzop_probe(x)\n        for x in items\n    ]\n",
            vec![3],
        ),
        // A SINGLE-line comprehension is deliberately span-less: at line granularity the span could
        // not distinguish `probe(x) for x in items` inside the brackets from a one-shot call sharing
        // the line, so the parser emits nothing and the rule stays silent (under-report, never a
        // false "proved in a loop" claim).
        (
            "py_comp_oneline.py",
            "def handler(items):\n    return [zzop_probe(x) for x in items]\n",
            vec![],
        ),
        // ---- Java ----
        (
            "InLoop.java",
            "class InLoop {\n  void handler(int[] xs) {\n    for (int x : xs) {\n      zzopProbe(x);\n    }\n  }\n}\n",
            vec![4],
        ),
        (
            "Outside.java",
            "class Outside {\n  void handler(int x) {\n    zzopProbe(x);\n  }\n}\n",
            vec![],
        ),
        (
            "LazyStream.java",
            "class LazyStream {\n  Object handler(java.util.List<Integer> xs) {\n    return xs.stream().map(x -> zzopProbe(x)).toList();\n  }\n}\n",
            vec![],
        ),
        // ---- C# ----
        (
            "InLoop.cs",
            "class InLoopCs {\n  void Handler(int[] xs) {\n    foreach (var x in xs) {\n      ZzopProbe(x);\n    }\n  }\n}\n",
            vec![4],
        ),
        (
            "Outside.cs",
            "class OutsideCs {\n  void Handler(int x) {\n    ZzopProbe(x);\n  }\n}\n",
            vec![],
        ),
        (
            "LazyLinq.cs",
            "class LazyLinqCs {\n  object Handler(System.Collections.Generic.List<int> xs) {\n    return xs.Select(x => ZzopProbe(x)).ToList();\n  }\n}\n",
            vec![],
        ),
        // ---- Rust ----
        (
            "in_loop.rs",
            "fn handler(xs: &[u32]) {\n    for x in xs {\n        zzop_probe(*x);\n    }\n}\n",
            vec![3],
        ),
        (
            "outside.rs",
            "fn handler(x: u32) {\n    zzop_probe(x);\n}\n",
            vec![],
        ),
        (
            "lazy.rs",
            "fn handler(xs: &[u32]) -> Vec<u32> {\n    xs.iter().map(|x| zzop_probe(*x)).collect()\n}\n",
            vec![],
        ),
    ]
}

fn hit_lines(out: &AnalyzeOutput, rel: &str) -> Vec<u32> {
    let mut lines: Vec<u32> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "loop-spans-probe/call-in-loop" && f.file == rel)
        .map(|f| f.line)
        .collect();
    lines.sort_unstable();
    lines
}

#[test]
fn trigger_in_loop_fires_inside_real_loops_and_stays_silent_outside_and_in_lazy_forms() {
    let dir = TempDir::new("zzop-loop-spans-languages");
    for (rel, source, _) in fixtures() {
        dir.write(rel, source);
    }

    let out = analyze_tree(
        dir.path(),
        &EngineConfig {
            source_id: "loop-spans-languages-fixture".to_string(),
            packs: vec![probe_pack()],
            ..EngineConfig::default()
        },
    );

    let mut mismatches = Vec::new();
    for (rel, _, expected) in fixtures() {
        let actual = hit_lines(&out, rel);
        if actual != expected {
            mismatches.push(format!(
                "{rel}: expected probe hits {expected:?}, got {actual:?}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "loop_spans reach/boundary mismatches (positive = fires in a real loop; negative = same call \
         outside any loop is silent; lazy = genexp/Stream/LINQ/iterator-adapter bodies are silent): \
         {mismatches:#?}"
    );
}
