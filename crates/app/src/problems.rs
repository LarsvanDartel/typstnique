//! Problem set, bundled into the binary from `assets/problems.toml`.

use serde::{Deserialize, Serialize};
use typst_engine::Kind;

/// A single typesetting challenge.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Problem {
    pub title: String,
    #[serde(default)]
    pub kind: Kind,
    pub source: String,
}

impl Problem {
    /// Points awarded for solving this problem. Source length is the primary
    /// signal (~90% of the value at median length); the structural difficulty
    /// rating adds a small modifier so two equally long problems with different
    /// complexity still differ slightly.
    pub fn points(&self) -> u32 {
        let chars = self.source.chars().count() as u32;
        let raw = chars * 8 + self.difficulty() * 20;
        let rounded = ((raw + 5) / 10) * 10;
        rounded.clamp(
            typst_engine::MIN_DIFFICULTY_SCORE,
            typst_engine::MAX_DIFFICULTY_SCORE,
        )
    }

    /// A 1–5 difficulty tier based on structural complexity (fractions, scripts,
    /// nesting depth), used as stars in the UI and as a minor points modifier.
    pub fn difficulty(&self) -> u32 {
        match typst_engine::difficulty_score(&self.source) {
            0..=150 => 1,
            151..=300 => 2,
            301..=500 => 3,
            501..=800 => 4,
            _ => 5,
        }
    }
}

#[derive(Deserialize)]
struct ProblemFile {
    problem: Vec<Problem>,
}

const RAW: &str = include_str!("../../../assets/problems.toml");

/// Parse the bundled problem set.
///
/// # Panics
///
/// Panics if the embedded `problems.toml` is malformed — a compile-time-known
/// asset, so this is covered by tests rather than handled at runtime.
pub fn load_problems() -> Vec<Problem> {
    toml::from_str::<ProblemFile>(RAW)
        .expect("assets/problems.toml is valid")
        .problem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_set_parses() {
        let problems = load_problems();
        assert!(!problems.is_empty());
    }

    #[test]
    fn all_problems_compile() {
        let failures: Vec<String> = load_problems()
            .into_iter()
            .filter_map(|p| {
                typst_engine::render_svg(&p.source, p.kind)
                    .err()
                    .map(|e| format!("  {:?}: {} -- {}", p.title, p.source, e))
            })
            .collect();
        assert!(
            failures.is_empty(),
            "{} problem(s) do not compile:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    /// Snapshot gate: every problem's normalized SVG must hash to the same
    /// value as when the fixture was last blessed. Run `BLESS=1 cargo test -p
    /// app --features ssr snapshot_all_problem_renders` to regenerate after an
    /// intentional font or rendering change.
    #[test]
    fn snapshot_all_problem_renders() {
        let problems = load_problems();
        let fixture_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/problem_renders.txt");

        let hashes: Vec<String> = problems
            .iter()
            .map(|p| {
                let svg = typst_engine::render_svg(&p.source, p.kind).unwrap_or_default();
                let norm = typst_engine::normalize_svg(&svg);
                format!("{:016x}", fnv1a64(norm.as_bytes()))
            })
            .collect();

        if std::env::var("BLESS").is_ok() {
            std::fs::write(fixture_path, hashes.join("\n") + "\n")
                .expect("write problem_renders.txt");
            eprintln!("blessed {} hashes → {fixture_path}", hashes.len());
            return;
        }

        let fixture =
            std::fs::read_to_string(fixture_path).unwrap_or_else(|_| panic!(
                "fixture {fixture_path} missing — run: BLESS=1 cargo test -p app --features ssr snapshot_all_problem_renders"
            ));
        let expected: Vec<&str> = fixture.lines().collect();
        assert_eq!(
            hashes.len(),
            expected.len(),
            "problem count changed ({} vs {}); re-bless the fixture",
            hashes.len(),
            expected.len()
        );
        let mut failures = vec![];
        for (i, (got, exp)) in hashes.iter().zip(expected.iter()).enumerate() {
            if got != exp {
                failures.push(format!(
                    "  problem {:3}: {} (expected {exp}, got {got})",
                    i, problems[i].title
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} problem render(s) changed — re-bless if intentional:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    /// FNV-1a 64-bit hash — stable across Rust versions, no extra deps.
    fn fnv1a64(data: &[u8]) -> u64 {
        let mut h: u64 = 14_695_981_039_346_656_037;
        for &b in data {
            h ^= u64::from(b);
            h = h.wrapping_mul(1_099_511_628_211);
        }
        h
    }
}
