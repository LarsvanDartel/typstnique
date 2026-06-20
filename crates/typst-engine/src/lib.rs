//! Typst compilation engine for Typstnique.
//!
//! This crate is shared between the WASM frontend (live preview + answer
//! checking, all client-side) and the server. It exposes three things:
//!
//! * [`Kind`]   — whether a problem is rendered as inline math or raw markup.
//! * [`render_svg`] — compile a Typst snippet to an SVG string (or an error).
//! * [`matches`] — judge whether a user's snippet renders the same as a target.
//!
//! Fonts are embedded via the `typst-assets` crate so the engine is fully
//! self-contained and needs no filesystem or network — essential for WASM.
//!
//! Fonts and the standard library are each built once and shared, and Typst's
//! `comemo` memoization cache is bounded after every compile — so the live
//! preview's per-keystroke recompiles are incremental and cheap.

use std::sync::OnceLock;

use regex::Regex;
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::layout::Abs;
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

/// How a problem's source is wrapped before compilation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Wrapped in `$ … $` and rendered as inline math.
    #[default]
    Math,
    /// Rendered verbatim as Typst markup.
    Markup,
}

// Font bytes selected at build time by `build.rs`: only the families in
// `KEEP` are written to `$OUT_DIR`; all others are excluded from the binary.
include!(concat!(env!("OUT_DIR"), "/fonts.rs"));

/// Shared, lazily-initialised font data (loaded once, reused across compiles).
struct Fonts {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let fonts: Vec<Font> = BUNDLED_FONTS
            .iter()
            .flat_map(|bytes| Font::iter(Bytes::new(*bytes)))
            .collect();
        let book = FontBook::from_fonts(&fonts);
        Fonts {
            book: LazyHash::new(book),
            fonts,
        }
    })
}

/// The Typst standard library, built once and shared (it never changes).
fn library() -> &'static LazyHash<Library> {
    static LIBRARY: OnceLock<LazyHash<Library>> = OnceLock::new();
    LIBRARY.get_or_init(|| LazyHash::new(Library::builder().build()))
}

/// A minimal [`World`] backed by a single in-memory source file.
struct GameWorld {
    source: Source,
}

impl GameWorld {
    fn new(text: String) -> Self {
        Self {
            source: Source::detached(text),
        }
    }
}

impl World for GameWorld {
    fn library(&self) -> &LazyHash<Library> {
        library()
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &fonts().book
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
        fonts().fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Wrap a snippet with a shared preamble so target and user output are
/// rendered with identical page/text settings.
fn wrap(source: &str, kind: Kind) -> String {
    // `fill: none` keeps the page background transparent so the surrounding box
    // shows through (and dark-mode inversion only affects the glyphs).
    const PREAMBLE: &str = "#set page(width: auto, height: auto, margin: 6pt, fill: none)\n\
                            #set text(size: 18pt)\n";
    match kind {
        Kind::Math => format!("{PREAMBLE}$ {source} $"),
        Kind::Markup => format!("{PREAMBLE}{source}"),
    }
}

/// Compile a Typst snippet and return a standalone SVG string.
///
/// # Errors
///
/// Returns the collected compilation diagnostics as a human-readable
/// (HTML-escaped) string when the snippet fails to compile.
pub fn render_svg(source: &str, kind: Kind) -> Result<String, String> {
    /// How many compiles a memoized result may go unused before eviction. Keeps
    /// the cache bounded while preserving entries reused across keystrokes (e.g.
    /// the unchanged target recompiled by `matches` on every input).
    const EVICT_MAX_AGE: usize = 30;

    let world = GameWorld::new(wrap(source, kind));
    let result = typst::compile::<PagedDocument>(&world);
    let output = match result.output {
        Ok(document) => Ok(typst_svg::svg_merged(
            &document,
            &typst_svg::SvgOptions::default(),
            Abs::pt(0.0),
        )),
        Err(diagnostics) => Err(format_diagnostics(&diagnostics)),
    };

    // Typst memoizes compilation through `comemo`; evicting after each compile
    // bounds that cache while keeping recently-used (incremental) results.
    comemo::evict(EVICT_MAX_AGE);
    output
}

/// Judge whether `user` renders to the same output as `target`.
///
/// Both snippets are compiled and their SVGs compared after normalisation,
/// so any source producing the same visual result counts as correct.
#[must_use]
pub fn matches(target: &str, user: &str, kind: Kind) -> bool {
    match (render_svg(target, kind), render_svg(user, kind)) {
        (Ok(a), Ok(b)) => normalize(&a) == normalize(&b),
        _ => false,
    }
}

/// Syntax-highlight a snippet to an HTML fragment, using Typst's own parser.
///
/// The result is the inner `HTML` of a `<code>` element (styled `<span>`s); it is
/// rendered behind the editor as a colored overlay.
#[must_use]
pub fn highlight_html(source: &str, kind: Kind) -> String {
    let root = match kind {
        Kind::Math => typst::syntax::parse_math(source),
        Kind::Markup => typst::syntax::parse(source),
    };
    typst::syntax::highlight_html(&root)
}

/// Lowest points a single problem can be worth.
pub const MIN_DIFFICULTY_SCORE: u32 = 50;
/// Highest points a single problem can be worth. The server uses this to bound
/// submitted scores (`score <= solved * MAX_DIFFICULTY_SCORE`).
pub const MAX_DIFFICULTY_SCORE: u32 = 1500;

/// A heuristic structural complexity score for a problem, used to derive the
/// 1–5 star difficulty display (see `Problem::difficulty`).
///
/// Parses the math AST (so it reflects real structure rather than incidental
/// characters) and scores from three signals: the number of content leaves,
/// the weighted count of "complex" constructs (fractions, scripts, roots,
/// matrices, function calls, …), and the maximum nesting depth.
/// Rounded to a tidy multiple and clamped to a sensible range.
#[must_use]
pub fn difficulty_score(source: &str) -> u32 {
    let root = typst::syntax::parse_math(source);
    let mut stats = Stats::default();
    walk(&root, 0, &mut stats);

    let raw = MIN_DIFFICULTY_SCORE + stats.leaves * 4 + stats.weighted + stats.max_depth * 18;
    let rounded = (raw / 10) * 10;
    rounded.clamp(MIN_DIFFICULTY_SCORE, MAX_DIFFICULTY_SCORE)
}

#[derive(Default)]
struct Stats {
    /// Content leaves (identifiers, symbols, numbers, operators) — a length proxy.
    leaves: u32,
    /// Sum of per-node complexity weights.
    weighted: u32,
    /// Deepest nesting in the tree.
    max_depth: u32,
}

fn walk(node: &typst::syntax::SyntaxNode, depth: u32, stats: &mut Stats) {
    use typst::syntax::SyntaxKind as K;

    stats.max_depth = stats.max_depth.max(depth);
    stats.weighted += match node.kind() {
        K::MathFrac => 40,
        K::MathRoot => 35,
        K::FuncCall => 30,   // frac(), mat(), vec(), cases(), binom(), …
        K::MathAttach => 20, // sub/superscripts and big-operator limits
        K::MathDelimited | K::MathPrimes => 8, // ( … ), [ … ], lr( … ), primes
        _ => 0,
    };

    let mut has_child = false;
    for child in node.children() {
        has_child = true;
        walk(child, depth + 1, stats);
    }
    if !has_child
        && matches!(
            node.kind(),
            K::MathIdent | K::MathText | K::MathShorthand | K::Text | K::Str
        )
    {
        stats.leaves += 1;
    }
}

fn format_diagnostics<'a>(diagnostics: impl IntoIterator<Item = &'a SourceDiagnostic>) -> String {
    let joined = diagnostics
        .into_iter()
        .map(|d| d.message.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    html_escape(&joined)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Strip non-semantic noise from an SVG so visually-identical renders compare
/// equal: element `id`s, `href`/`xlink:href` cross-references, and whitespace.
/// The ordered sequence of glyph/path elements (which encodes the visual
/// result) is preserved. Also used by the snapshot gate in the `app` crate.
#[must_use]
pub fn normalize_svg(svg: &str) -> String {
    normalize(svg)
}

fn normalize(svg: &str) -> String {
    static IDS: OnceLock<Regex> = OnceLock::new();
    static WS: OnceLock<Regex> = OnceLock::new();
    let ids = IDS.get_or_init(|| Regex::new(r#"\s(?:xlink:href|href|id)="[^"]*""#).unwrap());
    let ws = WS.get_or_init(|| Regex::new(r"\s+").unwrap());

    let stripped = ids.replace_all(svg, "");
    ws.replace_all(&stripped, " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_math_to_svg() {
        let svg = render_svg("e^(i pi) + 1 = 0", Kind::Math).expect("should compile");
        assert!(svg.contains("<svg"), "expected an SVG document");
    }

    #[test]
    fn render_svg_is_stable_across_repeats() {
        // The cached library + comemo eviction must not affect output: many
        // repeated compiles of the same source are byte-identical, and other
        // sources still compile in between.
        let first = render_svg("e^(i pi) + 1 = 0", Kind::Math).expect("compiles");
        for _ in 0..40 {
            assert_eq!(
                render_svg("e^(i pi) + 1 = 0", Kind::Math).as_deref(),
                Ok(first.as_str())
            );
            assert!(render_svg("sum_(n=1)^oo 1/n^2", Kind::Math).is_ok());
        }
    }

    #[test]
    fn reports_errors_for_invalid_source() {
        // Calling an undefined function is a hard compilation error.
        let result = render_svg("#this_function_does_not_exist()", Kind::Markup);
        assert!(result.is_err(), "expected a diagnostic, got: {result:?}");
    }

    #[test]
    fn equivalent_sources_match() {
        // Different spacing / multiplication styling, same rendered output.
        assert!(matches("a b + c", "a  b  +  c", Kind::Math));
    }

    #[test]
    fn different_sources_do_not_match() {
        assert!(!matches("a + b", "a - b", Kind::Math));
    }

    #[test]
    fn difficulty_increases_with_complexity() {
        let simple = difficulty_score("a + b");
        let complex = difficulty_score("sum_(n=1)^oo 1/n^2 = pi^2/6");
        assert!(complex > simple);
    }

    #[test]
    fn highlights_to_spans() {
        let html = highlight_html("e^(i pi) + 1 = 0", Kind::Math);
        assert!(html.contains("<span") || html.contains("<code"));
    }
}
