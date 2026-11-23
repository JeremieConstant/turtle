//! Typesetting a PDF Export with Typst, run inside Turtle as a library
//! (ADR-0012).
//!
//! The layout is a fixed Typst file. What goes into it, the content and the
//! look, is handed to it as data in a JSON file it reads, so nothing from a
//! Page is ever read as Typst: it is always shown as written.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::visualize::{ExchangeFormat, RasterImage, SvgImage};
use typst::{Library, LibraryExt, World};

/// The fonts built into Turtle, so it needs nothing else on disk (ADR-0002).
/// Their licences are beside them in `server/fonts`.
const FONTS: &[&[u8]] = &[
    include_bytes!("../../fonts/inter/Inter-Regular.ttf"),
    include_bytes!("../../fonts/inter/Inter-Italic.ttf"),
    include_bytes!("../../fonts/inter/Inter-Medium.ttf"),
    include_bytes!("../../fonts/inter/Inter-SemiBold.ttf"),
    include_bytes!("../../fonts/inter/Inter-SemiBoldItalic.ttf"),
    include_bytes!("../../fonts/inter/Inter-Bold.ttf"),
    include_bytes!("../../fonts/inter/Inter-BoldItalic.ttf"),
    include_bytes!("../../fonts/source-serif-4/SourceSerif4-Regular.ttf"),
    include_bytes!("../../fonts/source-serif-4/SourceSerif4-It.ttf"),
    include_bytes!("../../fonts/source-serif-4/SourceSerif4-Semibold.ttf"),
    include_bytes!("../../fonts/source-serif-4/SourceSerif4-Bold.ttf"),
    include_bytes!("../../fonts/source-serif-4/SourceSerif4-BoldIt.ttf"),
    include_bytes!("../../fonts/jetbrains-mono/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../fonts/jetbrains-mono/JetBrainsMono-Italic.ttf"),
    include_bytes!("../../fonts/jetbrains-mono/JetBrainsMono-Bold.ttf"),
    include_bytes!("../../fonts/jetbrains-mono/JetBrainsMono-BoldItalic.ttf"),
];

/// The font families built in, by the names a PDF Style uses for them.
pub const FONT_FAMILIES: &[&str] = &["Inter", "Source Serif 4", "JetBrains Mono"];

const LAYOUT: &str = include_str!("layout.typ");

/// How a PDF looks: the values the fixed layout reads.
#[derive(Debug, Clone, Serialize)]
pub struct Look {
    /// Colours as `#rrggbb`.
    pub text_color: String,
    pub heading_color: String,
    pub accent_color: String,
    /// Font families.
    pub body_font: String,
    pub heading_font: String,
    pub code_font: String,
    /// Sizes in points: the body text, a chapter's title, and a Page's
    /// headings of the first two levels below it.
    pub body_size: f64,
    pub h1_size: f64,
    pub h2_size: f64,
    pub h3_size: f64,
    pub cover: bool,
    /// Whether there is a table of contents.
    pub toc: bool,
    /// 1 lists the chapters, 2 also their `##` Sections, 3 also `###`.
    pub toc_depth: u8,
    pub footer_text: Option<String>,
    pub language: Language,
}

/// The language of the words a PDF shows itself and of its date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Language {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "de")]
    German,
}

impl Language {
    /// The language's code, as stored and as the API names it.
    pub fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::German => "de",
        }
    }

    pub fn from_code(code: &str) -> Option<Language> {
        [Language::English, Language::German].into_iter().find(|language| language.code() == code)
    }

    fn labels(self) -> Labels {
        match self {
            Language::English => Labels {
                contents: "Contents",
                as_of: "As of",
                note: "Note",
                tip: "Tip",
                warning: "Warning",
                danger: "Danger",
            },
            Language::German => Labels {
                contents: "Inhalt",
                as_of: "Stand",
                note: "Hinweis",
                tip: "Tipp",
                warning: "Warnung",
                danger: "Gefahr",
            },
        }
    }

    fn date(self, date: Date) -> String {
        const MONTHS: [&str; 12] = [
            "January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
            "November", "December",
        ];
        const MONATE: [&str; 12] = [
            "Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober",
            "November", "Dezember",
        ];
        let month = usize::from(date.month - 1);
        match self {
            Language::English => format!("{} {} {}", date.day, MONTHS[month], date.year),
            Language::German => format!("{}. {} {}", date.day, MONATE[month], date.year),
        }
    }
}

/// The words the layout shows itself.
#[derive(Serialize)]
struct Labels {
    contents: &'static str,
    as_of: &'static str,
    note: &'static str,
    tip: &'static str,
    warning: &'static str,
    danger: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub year: i64,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// The day a moment falls on, in UTC, from seconds since 1970.
    pub fn of(seconds: i64) -> Date {
        // Howard Hinnant's days-to-civil algorithm.
        let z = seconds.div_euclid(86_400) + 719_468;
        let era = z.div_euclid(146_097);
        let day_of_era = z - era * 146_097;
        let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let mp = (5 * day_of_year + 2) / 153;
        let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        Date { year, month, day }
    }
}

/// What a PDF holds.
pub struct Document {
    pub title: String,
    pub subtitle: Option<String>,
    pub as_of: Date,
    /// Folders and Pages in tree order.
    pub parts: Vec<Part>,
    /// The images the content shows, by the path an `Inline::Image` names.
    pub images: HashMap<String, Vec<u8>>,
    /// The PDF Style's logo, shown on the cover and in the footer.
    pub logo: Option<Logo>,
}

/// A logo `logo_format` found the layout can show.
pub struct Logo {
    pub format: LogoFormat,
    pub content: Vec<u8>,
}

/// The kinds of file a logo may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogoFormat {
    Png,
    Jpeg,
    Svg,
}

impl LogoFormat {
    /// The format's name, as stored.
    pub fn code(self) -> &'static str {
        match self {
            LogoFormat::Png => "png",
            LogoFormat::Jpeg => "jpeg",
            LogoFormat::Svg => "svg",
        }
    }

    pub fn from_code(code: &str) -> Option<LogoFormat> {
        [LogoFormat::Png, LogoFormat::Jpeg, LogoFormat::Svg].into_iter().find(|format| format.code() == code)
    }

    pub fn content_type(self) -> &'static str {
        match self {
            LogoFormat::Png => "image/png",
            LogoFormat::Jpeg => "image/jpeg",
            LogoFormat::Svg => "image/svg+xml",
        }
    }

    /// The path the layout reads the logo from. Typst tells the format by
    /// its extension.
    fn path(self) -> &'static str {
        match self {
            LogoFormat::Png => "/logo.png",
            LogoFormat::Jpeg => "/logo.jpg",
            LogoFormat::Svg => "/logo.svg",
        }
    }
}

/// A Folder, shown as a group heading, or a Page, shown as a chapter.
#[derive(Debug, Serialize)]
pub struct Part {
    pub kind: PartKind,
    /// The label content links to it by. Unique in the document.
    pub label: String,
    pub title: String,
    /// How many Folders of the PDF it is in.
    pub depth: usize,
    /// A Page's Sections, as the table of contents lists them.
    pub sections: Vec<Section>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    Folder,
    Page,
}

#[derive(Debug, Serialize)]
pub struct Section {
    pub title: String,
    pub label: String,
    /// 2 for a `#` or `##` heading, 3 for `###`.
    pub rank: u8,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Paragraph { inlines: Vec<Inline> },
    /// `rank` is the heading's level within a chapter, from 2, since the
    /// chapter's title is the one heading above.
    Heading { rank: u8, label: Option<String>, inlines: Vec<Inline> },
    Code { language: Option<String>, text: String },
    Quote { blocks: Vec<Block> },
    /// `kind` is "note", "tip", "warning" or "danger".
    Callout { kind: &'static str, blocks: Vec<Block> },
    Bullets { items: Vec<Vec<Block>> },
    Numbered { start: u32, items: Vec<Vec<Block>> },
    Tasks { items: Vec<Task> },
    /// The first row is the header; every row has as many cells as it.
    Table { rows: Vec<Vec<Vec<Inline>>> },
    Rule,
}

#[derive(Debug, Serialize)]
pub struct Task {
    pub checked: bool,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Inline {
    Text { text: String, bold: bool, italic: bool, strike: bool, code: bool, link: Option<Link> },
    Break,
    /// One of the document's images.
    Image { path: String, alt: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    /// The label of a part of the document.
    Part(String),
    Url(String),
}

/// Whether the layout can show these bytes as an image. A file it cannot
/// would fail the whole PDF, so only these are handed to it.
pub fn is_image(bytes: &[u8]) -> bool {
    ExchangeFormat::detect(bytes).is_some_and(|format| RasterImage::plain(Bytes::new(bytes.to_vec()), format).is_ok())
}

/// What kind of logo these bytes are, if they are a PNG, JPEG or SVG the
/// layout can show. Each is typeset on a page of its own to find out: an SVG
/// can be read and still fail the PDF, such as one that names a file
/// elsewhere, and a logo that fails would fail every PDF made with it.
pub fn logo_format(content: &[u8]) -> Option<LogoFormat> {
    let format = match ExchangeFormat::detect(content) {
        Some(ExchangeFormat::Png) => LogoFormat::Png,
        Some(ExchangeFormat::Jpg) => LogoFormat::Jpeg,
        Some(_) => return None,
        None if SvgImage::new(Bytes::new(content.to_vec())).is_ok() => LogoFormat::Svg,
        None => return None,
    };
    let files = HashMap::from([(file_id(format.path()), Bytes::new(content.to_vec()))]);
    let world = PdfWorld::new(&format!("#image({:?}, width: 40mm)", format.path()), files);
    let shown = typeset_world(&world).is_ok();
    typst::comemo::evict(0);
    shown.then_some(format)
}

/// Typesets the document in the look as a PDF.
pub fn typeset(document: Document, look: &Look) -> anyhow::Result<Vec<u8>> {
    #[derive(Serialize)]
    struct Data<'a> {
        look: &'a Look,
        labels: Labels,
        title: String,
        subtitle: Option<String>,
        date: String,
        /// The path of the logo, if there is one.
        logo: Option<&'static str>,
        parts: Vec<Part>,
    }
    let data = Data {
        look,
        labels: look.language.labels(),
        title: document.title,
        subtitle: document.subtitle,
        date: look.language.date(document.as_of),
        logo: document.logo.as_ref().map(|logo| logo.format.path()),
        parts: document.parts,
    };
    let mut files: HashMap<FileId, Bytes> =
        document.images.into_iter().map(|(path, bytes)| (file_id(&path), Bytes::new(bytes))).collect();
    if let Some(logo) = document.logo {
        files.insert(file_id(logo.format.path()), Bytes::new(logo.content));
    }
    files.insert(file_id("/data.json"), Bytes::new(serde_json::to_vec(&data)?));
    let world = PdfWorld::new(LAYOUT, files);
    let result = typeset_world(&world);
    // Typst keeps what it worked out for reuse; a server would only gather it.
    typst::comemo::evict(0);
    result
}

fn typeset_world(world: &PdfWorld) -> anyhow::Result<Vec<u8>> {
    let messages = |errors: &[typst::diag::SourceDiagnostic]| {
        errors.iter().map(|error| error.message.to_string()).collect::<Vec<_>>().join("; ")
    };
    let document = typst::compile::<typst_layout::PagedDocument>(world)
        .output
        .map_err(|errors| anyhow::anyhow!("typesetting a PDF failed: {}", messages(&errors)))?;
    typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .map_err(|errors| anyhow::anyhow!("writing a PDF failed: {}", messages(&errors)))
}

fn file_id(path: &str) -> FileId {
    RootedPath::new(VirtualRoot::Project, VirtualPath::new(path).expect("a valid path")).intern()
}

struct PdfWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: Source,
    files: HashMap<FileId, Bytes>,
}

impl PdfWorld {
    /// A world whose main file is `main`.
    fn new(main: &str, files: HashMap<FileId, Bytes>) -> PdfWorld {
        let fonts: Vec<Font> = FONTS.iter().flat_map(|data| Font::iter(Bytes::new(*data))).collect();
        PdfWorld {
            library: LazyHash::new(Library::builder().build()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
            main: Source::new(file_id("/main.typ"), main.into()),
            files,
        }
    }
}

impl World for PdfWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.get(&id).cloned().ok_or_else(|| FileError::NotFound(id.vpath().get_without_slash().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    /// The layout is given the date; it never asks for today's.
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moment_falls_on_its_day_in_utc() {
        assert_eq!(Date::of(0), Date { year: 1970, month: 1, day: 1 });
        assert_eq!(Date::of(1_700_000_000), Date { year: 2023, month: 11, day: 14 });
        assert_eq!(Date::of(951_825_600), Date { year: 2000, month: 2, day: 29 });
        assert_eq!(Date::of(-1), Date { year: 1969, month: 12, day: 31 });
    }
}
