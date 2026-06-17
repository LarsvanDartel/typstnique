//! Discover new TeXnique problems and convert them to Typst.
//!
//! `assets/problems.toml` is the **curated, hand-checked** problem set and is
//! the source of truth — this generator NEVER writes it. It parses the vendored
//! `texnique_problems.js`, skips every problem already present in
//! `problems.toml` (matched by title), and converts only the *new* ones with
//! [`tylax`](https://github.com/scipenai/tylax):
//!
//! * those that compile  -> `assets/problems_generated.toml` (review & merge by hand)
//! * those that don't    -> `assets/problems_untranslated.toml` (translate by hand)
//!
//! If there is nothing new, the corresponding file is removed. So re-running is
//! purely additive and can never clobber curated/manual edits.
//!
//! Run from the repo root:  `cargo run -p typst-engine --example gen_problems`

// Dev-only generator; a couple of pedantic style lints aren't worth the noise.
#![allow(clippy::doc_markdown, clippy::format_push_string)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use typst_engine::{render_svg, Kind};

const JS: &str = include_str!("texnique_problems.js");

fn main() {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let curated = existing_titles(&assets.join("problems.toml"));

    let re = Regex::new(
        r#"(?s)"title":\s*(?:String\.raw`([^`]*)`|"((?:[^"\\]|\\.)*)")[\s\S]*?"latex":\s*String\.raw`([^`]*)`"#,
    )
    .unwrap();

    let mut new_ok = Vec::new();
    let mut new_failed = Vec::new();
    let mut skipped = 0usize;

    for cap in re.captures_iter(JS) {
        let title = clean_title(cap.get(1).or_else(|| cap.get(2)).map_or("", |m| m.as_str()));
        if curated.contains(&title) {
            skipped += 1;
            continue; // already curated — leave it alone
        }
        let latex = cap.get(3).map_or("", |m| m.as_str());

        let typst = std::panic::catch_unwind(|| tylax::latex_to_typst(latex))
            .map(|s| s.trim().to_string())
            .ok();

        match typst {
            Some(src) if !src.is_empty() && render_svg(&src, Kind::Math).is_ok() => {
                new_ok.push((title, src));
            }
            other => new_failed.push((title, latex.to_string(), other.unwrap_or_default())),
        }
    }

    write_candidates(&assets.join("problems_generated.toml"), &new_ok);
    write_untranslated(&assets.join("problems_untranslated.toml"), &new_failed);

    println!(
        "{skipped} already curated. {} new & compiling -> problems_generated.toml. \
         {} new & failing -> problems_untranslated.toml.",
        new_ok.len(),
        new_failed.len()
    );
    if new_ok.is_empty() && new_failed.is_empty() {
        println!("Nothing new — problems.toml is up to date and untouched.");
    }
}

/// Read the `title = "…"` values already present in the curated file.
fn existing_titles(path: &Path) -> HashSet<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().strip_prefix("title = "))
        .map(|q| q.trim().trim_matches('"').replace("\\\"", "\"").replace("\\\\", "\\"))
        .collect()
}

fn write_candidates(path: &Path, problems: &[(String, String)]) {
    if problems.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    let mut out = String::from(
        "# New TeXnique problems converted by tylax that compile.\n\
         # Review each and move the ones you want into problems.toml.\n\n",
    );
    for (title, source) in problems {
        out.push_str("[[problem]]\n");
        out.push_str(&format!("title = {}\n", toml_str(title)));
        out.push_str(&format!("source = {}\n\n", toml_str(source)));
    }
    std::fs::write(path, out).expect("write problems_generated.toml");
}

fn write_untranslated(path: &Path, problems: &[(String, String, String)]) {
    if problems.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    let mut out = String::from(
        "# New TeXnique problems whose tylax conversion failed to compile.\n\
         # Fill in `source` with hand-written Typst, then move into problems.toml.\n\
         # `latex` is the original; `attempted` is tylax's output.\n\n",
    );
    for (title, latex, attempted) in problems {
        out.push_str("[[problem]]\n");
        out.push_str(&format!("title = {}\n", toml_str(title)));
        out.push_str(&format!("latex = {}\n", toml_str(latex)));
        out.push_str(&format!("# attempted = {}\n", toml_str(attempted)));
        out.push_str("source = \"\"\n\n");
    }
    std::fs::write(path, out).expect("write problems_untranslated.toml");
}

fn toml_str(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}

/// Turn a TeXnique title (which may contain LaTeX) into readable plain text:
/// `\mathbb{N}`→`ℕ`, accents like `\ddot{o}`/`\"o`→`ö`, style wrappers
/// `\textbf{x}`→`x`, then strip leftover commands/braces.
fn clean_title(t: &str) -> String {
    let bb = Regex::new(r"\\mathbb\{([A-Za-z])\}").unwrap();
    let acc = Regex::new(r"\\([a-zA-Z]+)\{([A-Za-z])\}").unwrap();
    let short = Regex::new(r#"\\(["'`^~])\s*([A-Za-z])"#).unwrap();
    let wrap = Regex::new(r"\\[a-zA-Z]+\{([^{}\\]*)\}").unwrap();

    let mut s = t.to_string();
    // Resolve nested accents/wrappers from the inside out.
    for _ in 0..4 {
        let before = s.clone();
        s = bb.replace_all(&s, |c: &regex::Captures| blackboard(&c[1])).into_owned();
        s = acc.replace_all(&s, |c: &regex::Captures| accent(&c[1], &c[2])).into_owned();
        s = short.replace_all(&s, |c: &regex::Captures| short_accent(&c[1], &c[2])).into_owned();
        s = wrap.replace_all(&s, "$1").into_owned();
        if s == before {
            break;
        }
    }

    // Strip leftover math markers, commands, and braces.
    s = s.replace("\\(", "").replace("\\)", "");
    s = Regex::new(r"\\[a-zA-Z]+").unwrap().replace_all(&s, "").into_owned();
    s = s.replace(['{', '}', '\\'], "");
    Regex::new(r"\s+").unwrap().replace_all(s.trim(), " ").into_owned()
}

fn blackboard(letter: &str) -> String {
    match letter {
        "N" => "ℕ",
        "R" => "ℝ",
        "Z" => "ℤ",
        "Q" => "ℚ",
        "C" => "ℂ",
        "P" => "ℙ",
        "H" => "ℍ",
        other => other,
    }
    .to_string()
}

/// A `\command{x}` accent (or style wrapper) applied to a single letter.
fn accent(cmd: &str, letter: &str) -> String {
    let combining = match cmd {
        "acute" => '\u{0301}',
        "grave" => '\u{0300}',
        "hat" | "circ" => '\u{0302}',
        "tilde" => '\u{0303}',
        "bar" | "macron" => '\u{0304}',
        "dot" => '\u{0307}',
        "ddot" | "umlaut" => '\u{0308}',
        "check" => '\u{030C}',
        "vec" => '\u{20D7}',
        _ => return letter.to_string(), // style wrapper (textbf, mathrm, …): keep letter
    };
    format!("{letter}{combining}")
}

/// A short accent like `\"o` or `\'e`.
fn short_accent(sym: &str, letter: &str) -> String {
    let combining = match sym {
        "\"" => '\u{0308}',
        "'" => '\u{0301}',
        "`" => '\u{0300}',
        "^" => '\u{0302}',
        "~" => '\u{0303}',
        _ => return letter.to_string(),
    };
    format!("{letter}{combining}")
}
