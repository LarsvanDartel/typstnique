/// Font sweep: renders all 185 problems with each font group excluded in turn
/// and reports which problems (if any) produce different output. This lets us
/// determine the minimal font set needed for correctness before vendoring.
///
/// Run with: cargo run -p typst-engine --example font_sweep
use std::collections::{HashMap, HashSet};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::layout::Abs;
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

struct SweepWorld {
    source: Source,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    library: LazyHash<Library>,
}

impl SweepWorld {
    fn new(text: String, fonts: Vec<Font>) -> Self {
        let book = LazyHash::new(FontBook::from_fonts(&fonts));
        Self {
            source: Source::detached(text),
            book,
            fonts,
            library: LazyHash::new(Library::builder().build()),
        }
    }
}

impl World for SweepWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound("not-found.typ".into()))
        }
    }

    fn file(&self, _id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound("not-found.typ".into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

fn wrap(source: &str, kind: typst_engine::Kind) -> String {
    const PREAMBLE: &str =
        "#set page(width: auto, height: auto, margin: 6pt, fill: none)\n#set text(size: 18pt)\n";
    match kind {
        typst_engine::Kind::Math => format!("{PREAMBLE}$ {source} $"),
        typst_engine::Kind::Markup => format!("{PREAMBLE}{source}"),
    }
}

fn render_with_fonts(fonts: &[Font], source: &str, kind: typst_engine::Kind) -> Option<String> {
    let world = SweepWorld::new(wrap(source, kind), fonts.to_vec());
    let result = typst::compile::<PagedDocument>(&world);
    comemo::evict(5);
    match result.output {
        Ok(doc) => Some(typst_svg::svg_merged(
            &doc,
            &typst_svg::SvgOptions::default(),
            Abs::pt(0.0),
        )),
        Err(_) => None,
    }
}

fn normalize(svg: &str) -> String {
    let svg = regex::Regex::new(r#"\s(?:xlink:href|href|id)="[^"]*""#)
        .unwrap()
        .replace_all(svg, "");
    regex::Regex::new(r"\s+")
        .unwrap()
        .replace_all(&svg, " ")
        .trim()
        .to_string()
}

fn main() {
    // Build the baseline font set (all fonts).
    let all_font_data: Vec<&[u8]> = typst_assets::fonts().collect();
    let all_fonts: Vec<Font> = all_font_data
        .iter()
        .flat_map(|b| Font::iter(Bytes::new(*b)))
        .collect();

    // Group fonts by family.
    let mut families: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, f) in all_fonts.iter().enumerate() {
        families
            .entry(f.info().family.to_string())
            .or_default()
            .push(i);
    }

    println!("Font families:");
    let mut family_names: Vec<String> = families.keys().cloned().collect();
    family_names.sort();
    for name in &family_names {
        println!("  {name}");
    }

    // Load problems from the assets file.
    let toml_src = include_str!("../../../assets/problems.toml");
    #[derive(serde::Deserialize)]
    struct Toml {
        problem: Vec<ProblemEntry>,
    }
    #[derive(serde::Deserialize)]
    struct ProblemEntry {
        title: String,
        source: String,
        #[serde(default)]
        kind: typst_engine::Kind,
    }
    let parsed: Toml = toml::from_str(toml_src).expect("parse problems.toml");
    let problems = parsed.problem;
    println!(
        "\nRendering {} problems with full font set…",
        problems.len()
    );

    // Render baseline (all fonts).
    let baseline: Vec<Option<String>> = problems
        .iter()
        .map(|p| render_with_fonts(&all_fonts, &p.source, p.kind).map(|s| normalize(&s)))
        .collect();

    // For each family, check which problems render differently when excluded.
    println!("\nSweeping font families:\n");
    let mut safe_to_drop: HashSet<String> = family_names.iter().cloned().collect();

    for family in &family_names {
        let exclude_indices: HashSet<usize> = families[family].iter().copied().collect();
        let subset: Vec<Font> = all_fonts
            .iter()
            .enumerate()
            .filter(|(i, _)| !exclude_indices.contains(i))
            .map(|(_, f)| f.clone())
            .collect();

        let mut changed = vec![];
        for (i, p) in problems.iter().enumerate() {
            let out = render_with_fonts(&subset, &p.source, p.kind).map(|s| normalize(&s));
            if out != baseline[i] {
                changed.push((i, p.title.as_str()));
            }
        }

        if changed.is_empty() {
            println!("  ✓ DROP   {family} — no problems change");
        } else {
            println!("  ✗ KEEP   {family} — {} problems change:", changed.len());
            for (_, title) in changed.iter().take(5) {
                println!("           · {title}");
            }
            if changed.len() > 5 {
                println!("           … and {} more", changed.len() - 5);
            }
            safe_to_drop.remove(family);
        }
    }

    println!("\n── Summary ────────────────────────────────────────────────");
    println!("Safe to drop ({}):", safe_to_drop.len());
    let mut drop_bytes = 0usize;
    for name in &safe_to_drop {
        let fam_bytes: usize = families[name]
            .iter()
            .map(|&i| {
                // Approximate: find the font data size for this font.
                let f = &all_fonts[i];
                f.data().len()
            })
            .sum();
        println!("  - {name} (~{} kB)", fam_bytes / 1024);
        drop_bytes += fam_bytes;
    }
    println!(
        "Estimated savings: {:.1} MB of font data",
        drop_bytes as f64 / 1_048_576.0
    );
}
