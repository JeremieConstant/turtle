//! What becomes of each kind of Confluence content, and the tally of what a
//! space holds.
//!
//! `ELEMENTS` is the one list of what the import does with each element and
//! macro it knows. The report of an import or a check is drawn from it, and
//! so can a public list of what comes across from Confluence.

use schemars::JsonSchema;
use serde::Serialize;
use std::collections::BTreeMap;

/// How well a kind of Confluence content comes across.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// It reads the same in Turtle.
    Exact,
    /// What it says comes across, but not all of how it looks or behaves.
    Approximate,
    /// A placeholder naming it shows where it was; what it held, if anything,
    /// follows the placeholder.
    Placeholder,
    /// It is left out.
    LeftOut,
}

/// A kind of Confluence content the import knows, what becomes of it, and
/// how well that comes across.
pub struct Known {
    pub name: &'static str,
    pub outcome: Outcome,
    pub becomes: &'static str,
}

const fn known(name: &'static str, outcome: Outcome, becomes: &'static str) -> Known {
    Known { name, outcome, becomes }
}

use Outcome::{Approximate, Exact, LeftOut};

/// Every kind of Confluence content the import knows. A macro not listed
/// here becomes a placeholder (`PLACEHOLDER_FOR_UNKNOWN`), and an element not listed
/// keeps its text (`TEXT_OF_UNKNOWN`).
pub const ELEMENTS: &[Known] = &[
    known("paragraph", Exact, "A paragraph."),
    known("heading", Exact, "A heading of the same level."),
    known("bold, italic, strikethrough or code", Exact, "The same formatting."),
    known("line break", Exact, "A line break."),
    known("bullet list", Exact, "A bullet list."),
    known("numbered list", Exact, "A numbered list."),
    known("task list", Exact, "A task list, each task checked off as it was."),
    known("table", Exact, "A table whose first row is its header."),
    known("table without a header row", Approximate, "A table whose first row becomes its header."),
    known("merged table cell", Approximate, "A cell followed by empty cells where it spanned several."),
    known("table cell with more than text", Approximate, "A cell holding its text, line by line; lists become lines starting with a bullet."),
    known("quote", Exact, "A quote."),
    known("horizontal rule", Exact, "A horizontal rule."),
    known("preformatted text", Exact, "A code block."),
    known("link", Exact, "A link."),
    known("link to a page", Exact, "A Page link."),
    known("link to a page with its own text", Approximate, "A Page link, shown with the Page's title instead of its own text."),
    known("link to a folder", Approximate, "Its text, without the link: Turtle links only to Pages."),
    known("link to a page outside the space", Approximate, "Its text, without the link: the page is not imported."),
    known("link to an attachment", Exact, "A link to the Attachment."),
    known("link to something else in Confluence", Approximate, "Its text, without the link."),
    known("image", Exact, "An image, its file an Attachment of the Page."),
    known("image from the web", Exact, "An image from its address."),
    known("image caption", Approximate, "A paragraph in italics after the image."),
    known("missing attachment", LeftOut, "Nothing: the file is not in the export."),
    known("mention", Approximate, "The person's name after an @, without a Mention."),
    known("emoticon", Exact, "The emoji."),
    known("date", Approximate, "The date as text."),
    known("underline", Approximate, "The text, without the underline."),
    known("subscript or superscript", Approximate, "The text, on the line."),
    known("coloured text", Approximate, "The text, without the colour."),
    known("inline comment", Approximate, "The text; the comment is left out."),
    known("instructional text", LeftOut, "Nothing: it only guided whoever wrote the page."),
    known("layout", Approximate, "Its columns one after another."),
    known("panel", Approximate, "A Callout of the kind closest to the panel's type; custom colours and icons are left out."),
    known("info macro", Exact, "A Callout of kind Note."),
    known("note macro", Exact, "A Callout of kind Warning."),
    known("tip macro", Exact, "A Callout of kind Tip."),
    known("warning macro", Exact, "A Callout of kind Danger."),
    known("panel macro", Approximate, "A Callout of kind Note; its colours are left out."),
    known("code macro", Exact, "A code block in the same language."),
    known("noformat macro", Exact, "A code block."),
    known("status macro", Exact, "A Status of the same colour."),
    known("expand macro", Approximate, "Its title in bold and what it held, always shown."),
    known("view-file macro", Approximate, "A link to the Attachment instead of a preview."),
    known("viewpdf macro", Approximate, "A link to the Attachment instead of a preview."),
    known("viewdoc macro", Approximate, "A link to the Attachment instead of a preview."),
    known("viewxls macro", Approximate, "A link to the Attachment instead of a preview."),
    known("viewppt macro", Approximate, "A link to the Attachment instead of a preview."),
    known("excerpt macro", Approximate, "What it held."),
    known("details macro", Approximate, "What it held."),
    known("anchor macro", LeftOut, "Nothing; links to it go to the Page."),
    known("unreadable page content", LeftOut, "Nothing: the Page is created empty."),
];

/// What becomes of a Confluence macro the import does not know.
const PLACEHOLDER_FOR_UNKNOWN: &str = "A placeholder naming the macro, followed by what it held.";
/// What becomes of an element the import does not know.
const TEXT_OF_UNKNOWN: &str = "Its text.";

/// What becomes of the content called `name` in a tally.
pub fn known_as(name: &str) -> (Outcome, &'static str) {
    if let Some(known) = ELEMENTS.iter().find(|known| known.name == name) {
        return (known.outcome, known.becomes);
    }
    if name.ends_with(" macro") {
        (Outcome::Placeholder, PLACEHOLDER_FOR_UNKNOWN)
    } else {
        (Outcome::Approximate, TEXT_OF_UNKNOWN)
    }
}

/// How often each kind of content was found, by name.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tally(pub BTreeMap<String, u32>);

impl Tally {
    pub fn add(&mut self, name: &str) {
        *self.0.entry(name.to_string()).or_default() += 1;
    }

    /// Whether everything found comes across exactly.
    pub fn is_exact(&self) -> bool {
        self.0.keys().all(|name| known_as(name).0 == Outcome::Exact)
    }
}

/// How much of a Confluence space comes across into Turtle.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Coverage {
    /// The space's name, as Confluence has it.
    pub space: String,
    /// The pages that become Pages.
    pub pages: u32,
    /// The folders that become Folders.
    pub folders: u32,
    /// The attachments that become Attachments.
    pub attachments: u32,
    /// The Pages whose content comes across exactly.
    pub exact_pages: u32,
    /// Of all the elements and macros found in the pages, the share that
    /// comes across exactly, in whole percent, rounded down.
    pub exact_percent: u8,
    /// Of all the elements and macros found in the pages, the share that
    /// comes across exactly or approximately, in whole percent, rounded down.
    pub kept_percent: u8,
    /// Each kind of content found, the least well kept first.
    pub elements: Vec<Found>,
    /// What else the space holds that is not imported, such as comments.
    pub not_imported: Vec<NotImported>,
    /// The attachments larger than this Turtle accepts, which are left out.
    /// Whoever runs Turtle can raise the limit with TURTLE_MAX_ATTACHMENT_MB.
    pub too_large: Vec<TooLarge>,
    /// The largest attachment this Turtle accepts, in bytes.
    pub attachment_limit_bytes: u64,
}

/// An attachment larger than this Turtle accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TooLarge {
    /// The page's title and the attachment's name, as problems name it.
    pub file: String,
    /// Its size in bytes.
    pub bytes: u64,
}

/// A kind of content found in the pages, and what becomes of it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Found {
    /// What Confluence content it is, such as `table` or `info macro`.
    pub name: String,
    pub outcome: Outcome,
    /// What it becomes in Turtle, as a sentence.
    pub becomes: String,
    /// How often it was found.
    pub count: u32,
    /// The titles of the first few pages it was found on.
    pub pages: Vec<String>,
}

/// Something a space holds that is not imported, and how much of it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct NotImported {
    /// What it is, such as "Comments".
    pub what: String,
    pub count: u32,
}

/// How many page titles a Found names.
const PAGES_NAMED: usize = 5;

/// The report on a space: `found` holds what each page's content held, by
/// the page's title, in tree order.
pub fn report(space: String, counts: (u32, u32, u32), found: &[(String, Tally)], not_imported: Vec<NotImported>) -> Coverage {
    let (pages, folders, attachments) = counts;
    let mut by_name: BTreeMap<&str, (u32, Vec<String>)> = BTreeMap::new();
    for (title, tally) in found {
        for (name, count) in &tally.0 {
            let entry = by_name.entry(name).or_default();
            entry.0 += count;
            if entry.1.len() < PAGES_NAMED {
                entry.1.push(title.clone());
            }
        }
    }
    let mut elements: Vec<Found> = by_name
        .into_iter()
        .map(|(name, (count, pages))| {
            let (outcome, becomes) = known_as(name);
            Found { name: name.to_string(), outcome, becomes: becomes.to_string(), count, pages }
        })
        .collect();
    elements.sort_by(|a, b| b.outcome.cmp(&a.outcome).then(b.count.cmp(&a.count)).then(a.name.cmp(&b.name)));
    let total: u32 = elements.iter().map(|found| found.count).sum();
    let share = |keep: &dyn Fn(Outcome) -> bool| -> u8 {
        let kept: u32 = elements.iter().filter(|found| keep(found.outcome)).map(|found| found.count).sum();
        if total == 0 { 100 } else { (u64::from(kept) * 100 / u64::from(total)) as u8 }
    };
    Coverage {
        space,
        pages,
        folders,
        attachments,
        exact_pages: found.iter().filter(|(_, tally)| tally.is_exact()).count() as u32,
        exact_percent: share(&|outcome| outcome == Outcome::Exact),
        kept_percent: share(&|outcome| outcome <= Outcome::Approximate),
        elements,
        not_imported: not_imported.into_iter().filter(|item| item.count > 0).collect(),
        too_large: Vec::new(),
        attachment_limit_bytes: 0,
    }
}
