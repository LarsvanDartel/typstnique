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
    /// Points awarded for solving this problem, from the source heuristic.
    pub fn points(&self) -> u32 {
        typst_engine::difficulty_score(&self.source)
    }

    /// A 1–5 difficulty tier derived from the points, for display.
    pub fn difficulty(&self) -> u32 {
        match self.points() {
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
}
