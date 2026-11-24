//! The content of a Page, independent of how it is stored or serialised.
//!
//! This is the common ground between the collaborative document (see `ydoc`)
//! and Markdown (see `markdown`). It can only hold what Markdown can express
//! (ADR-0006), extended by directives (ADR-0013).

use crate::directive::{self, Directive};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Marks {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text { text: String, marks: Marks },
    HardBreak,
    /// An image, shown where it stands in the text. Formatting does not
    /// apply to it.
    Image { src: String, alt: String },
    /// A link to another Page by its ID. It holds no text: it is shown with
    /// the Page's current title, so renaming or moving the Page never breaks
    /// it, and with nothing to someone who may not read the Page.
    PageLink { page_id: String },
    /// A directive in the running text that Turtle does not know (ADR-0013),
    /// such as `:acme.demo[hi]{x=1}`. It is kept exactly as it was written,
    /// so a Page survives an installation that lacks the plugin it is from.
    UnknownDirective { source: String },
    /// A Status: a short coloured marker, written `:status[ON TRACK]{color=green}`.
    /// Its text is kept as typed and shown in uppercase. It takes no
    /// formatting, stays on one line and holds no `|`, which would end a
    /// table cell.
    Status { text: String, color: StatusColor },
}

/// The colours a Status comes in. They are named in the content; each theme
/// and the PDF give them their own shades.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusColor {
    #[default]
    Grey,
    Blue,
    Green,
    Yellow,
    Red,
    Purple,
}

impl StatusColor {
    pub const ALL: [StatusColor; 6] = [Self::Grey, Self::Blue, Self::Green, Self::Yellow, Self::Red, Self::Purple];

    /// The name the colour is written and stored by.
    pub fn name(self) -> &'static str {
        match self {
            Self::Grey => "grey",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Red => "red",
            Self::Purple => "purple",
        }
    }

    /// The colour by its name; any other name is grey.
    pub fn named(name: &str) -> Self {
        Self::ALL.into_iter().find(|c| c.name() == name).unwrap_or_default()
    }
}

/// The name of the Status directive.
const STATUS: &str = "status";

/// A Status written as a directive.
pub fn status_directive(text: &str, color: StatusColor) -> Directive {
    Directive {
        name: STATUS.to_string(),
        label: Some(text.to_string()),
        attributes: vec![("color".to_string(), color.name().to_string())],
    }
}

/// How a Page link is written as a link address: `turtle:` and the Page's ID.
pub const PAGE_ADDRESS_SCHEME: &str = "turtle:";

/// The inline element a directive written in running text stands for: one
/// of Turtle's own, or one it does not know, kept as written. Nothing when
/// the source is not exactly one directive.
pub fn inline_directive(source: &str) -> Option<Inline> {
    let (directive, len) = directive::inline_at(source)?;
    (len == source.len())
        .then(|| known_inline(&directive).unwrap_or_else(|| Inline::UnknownDirective { source: source.to_string() }))
}

/// The element one of Turtle's own directives in running text stands for.
/// Their names are bare and reserved (ADR-0013); each is matched here by
/// name. A namespaced one is never Turtle's.
fn known_inline(directive: &Directive) -> Option<Inline> {
    if directive.is_namespaced() {
        return None;
    }
    match directive.name.as_str() {
        STATUS => {
            let color = directive.attributes.iter().find(|(key, _)| key == "color");
            Some(Inline::Status {
                text: directive.label.clone().unwrap_or_default(),
                color: color.map(|(_, name)| StatusColor::named(name)).unwrap_or_default(),
            })
        }
        _ => None,
    }
}

/// Whether the source is exactly one block directive Turtle does not know.
pub fn is_unknown_block_directive(source: &str) -> bool {
    directive::block_at(source).is_some_and(|(_, len)| len == source.len())
}

/// The ID of the Page a link address points at, when it is a Page link.
pub fn page_id_of(address: &str) -> Option<&str> {
    let id = address.strip_prefix(PAGE_ADDRESS_SCHEME)?;
    let valid = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then_some(id)
}

// The variants are named as Markdown and the editor name them, so a code
// block is `CodeBlock` even though the enum is `Block`.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading { level: u8, content: Vec<Inline> },
    CodeBlock { language: Option<String>, text: String },
    Blockquote(Vec<Block>),
    BulletList(Vec<Vec<Block>>),
    OrderedList { start: u64, items: Vec<Vec<Block>> },
    /// A list whose items can each be checked off.
    TaskList(Vec<TaskItem>),
    /// A highlighted passage of a given kind; to Markdown a quote whose first
    /// line names the kind, such as `[!NOTE]`.
    Callout { kind: CalloutKind, content: Vec<Block> },
    /// Rows of cells, the first row being the header. Every row has the same
    /// number of cells, and a cell holds inline content only.
    Table(Vec<Row>),
    HorizontalRule,
    /// A block directive Turtle does not know (ADR-0013), from its opening
    /// line such as `:::acme.board{x=1}` to its closing `:::`, kept exactly
    /// as it was written. It stands only at the top of a Page's content,
    /// never inside a list, quote or Callout.
    UnknownDirective { source: String },
}

/// The cells of a table row, left to right.
pub type Row = Vec<Cell>;
/// What a table cell holds: inline content only.
pub type Cell = Vec<Inline>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalloutKind {
    Note,
    Tip,
    Warning,
    Danger,
}

impl CalloutKind {
    pub const ALL: [CalloutKind; 4] = [CalloutKind::Note, CalloutKind::Tip, CalloutKind::Warning, CalloutKind::Danger];

    /// The name of the kind as the editor stores it; in capitals it is the
    /// kind's Markdown marker.
    pub fn name(self) -> &'static str {
        match self {
            CalloutKind::Note => "note",
            CalloutKind::Tip => "tip",
            CalloutKind::Warning => "warning",
            CalloutKind::Danger => "danger",
        }
    }

    /// The kind with the given name, in any case.
    pub fn named(name: &str) -> Option<CalloutKind> {
        CalloutKind::ALL
            .into_iter()
            .find(|kind| kind.name().eq_ignore_ascii_case(name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskItem {
    pub checked: bool,
    pub content: Vec<Block>,
}

/// Brings content into the one canonical shape that survives a Markdown round
/// trip. Everything dropped here is something Markdown has no way to express:
/// empty paragraphs, line breaks at the edge of a block or inside a heading,
/// and formatting combined with inline code.
pub fn normalize(blocks: Vec<Block>) -> Vec<Block> {
    blocks.into_iter().filter_map(|block| normalize_block(block, true)).collect()
}

/// As `normalize`, for the blocks inside a list, quote or Callout.
fn normalize_nested(blocks: Vec<Block>) -> Vec<Block> {
    blocks.into_iter().filter_map(|block| normalize_block(block, false)).collect()
}

fn normalize_block(block: Block, at_top: bool) -> Option<Block> {
    Some(match block {
        // Markdown can only hold a block directive at the top of the content,
        // so anywhere else, or when it is not one, its lines are kept as text.
        Block::UnknownDirective { source } if at_top && is_unknown_block_directive(&source) => {
            Block::UnknownDirective { source }
        }
        Block::UnknownDirective { source } => {
            let mut lines = Vec::new();
            for line in source.lines() {
                if !lines.is_empty() {
                    lines.push(Inline::HardBreak);
                }
                lines.push(Inline::Text { text: line.to_string(), marks: Marks::default() });
            }
            return normalize_block(Block::Paragraph(lines), at_top);
        }
        Block::Paragraph(content) => {
            let content = normalize_inlines(content, false);
            if content.is_empty() {
                return None;
            }
            Block::Paragraph(content)
        }
        Block::Heading { level, content } => Block::Heading {
            level: level.clamp(1, 6),
            content: normalize_inlines(content, true),
        },
        Block::CodeBlock { language, text } => Block::CodeBlock {
            language: language
                .and_then(|l| l.split_whitespace().next().map(|s| s.replace('`', "")))
                .filter(|l| !l.is_empty()),
            text: text.replace("\r\n", "\n").replace('\r', "\n"),
        },
        Block::Blockquote(children) => Block::Blockquote(normalize_nested(children)),
        Block::Callout { kind, content } => Block::Callout { kind, content: normalize_nested(content) },
        Block::BulletList(items) => {
            if items.is_empty() {
                return None;
            }
            Block::BulletList(items.into_iter().map(normalize_nested).collect())
        }
        Block::OrderedList { start, items } => {
            if items.is_empty() {
                return None;
            }
            Block::OrderedList {
                start: start.min(999_999_999),
                items: items.into_iter().map(normalize_nested).collect(),
            }
        }
        Block::TaskList(items) => {
            if items.is_empty() {
                return None;
            }
            Block::TaskList(
                items
                    .into_iter()
                    .map(|item| TaskItem { checked: item.checked, content: normalize_nested(item.content) })
                    .collect(),
            )
        }
        Block::Table(rows) => {
            // As wide as its widest row, so a short row gains empty cells
            // instead of a long one losing what it holds.
            let width = rows.iter().map(Vec::len).max().unwrap_or(0);
            if width == 0 {
                return None;
            }
            Block::Table(
                rows.into_iter()
                    .map(|row| {
                        let mut row: Row =
                            row.into_iter().map(|cell| normalize_inlines(cell, false)).collect();
                        row.resize(width, Vec::new());
                        row
                    })
                    .collect(),
            )
        }
        Block::HorizontalRule => Block::HorizontalRule,
    })
}

fn normalize_inlines(inlines: Vec<Inline>, in_heading: bool) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    // The Page that the text just before links to, so one link stays one
    // Page link where its formatting changes partway.
    let mut linked_page: Option<String> = None;
    for inline in inlines {
        let inline = match inline {
            Inline::HardBreak if in_heading => Inline::Text {
                text: " ".into(),
                marks: Marks::default(),
            },
            // What is not exactly one directive is the text it says.
            Inline::UnknownDirective { source } => {
                inline_directive(&source).unwrap_or(Inline::Text { text: source, marks: Marks::default() })
            }
            other => other,
        };
        match inline {
            Inline::HardBreak => {
                linked_page = None;
                out.push(Inline::HardBreak)
            }
            Inline::PageLink { page_id } => {
                linked_page = None;
                out.push(Inline::PageLink { page_id })
            }
            Inline::UnknownDirective { source } => {
                linked_page = None;
                out.push(Inline::UnknownDirective { source })
            }
            // A directive in running text stays on one line and holds no `|`.
            Inline::Status { text, color } => {
                linked_page = None;
                out.push(Inline::Status { text: text.replace(['\n', '\r'], " ").replace('|', "/"), color })
            }
            Inline::Image { src, alt } => {
                linked_page = None;
                out.push(Inline::Image {
                    src: src.replace(['\n', '\r'], ""),
                    alt: alt.replace(['\n', '\r'], " "),
                })
            }
            Inline::Text { text, mut marks } => {
                let text = text.replace(['\n', '\r'], " ");
                if text.is_empty() {
                    continue;
                }
                if marks.code {
                    marks = Marks {
                        code: true,
                        ..Marks::default()
                    };
                }
                // Text linked to a Page is that Page link, however it is
                // formatted; its words give way to the Page's title.
                if let Some(page_id) = marks.link.as_deref().and_then(page_id_of) {
                    if linked_page.as_deref() != Some(page_id) {
                        out.push(Inline::PageLink { page_id: page_id.to_string() });
                        linked_page = Some(page_id.to_string());
                    }
                    continue;
                }
                linked_page = None;
                match out.last_mut() {
                    Some(Inline::Text {
                        text: prev,
                        marks: prev_marks,
                    }) if *prev_marks == marks => prev.push_str(&text),
                    _ => out.push(Inline::Text { text, marks }),
                }
            }
        }
    }
    while matches!(out.first(), Some(Inline::HardBreak)) {
        out.remove(0);
    }
    while matches!(out.last(), Some(Inline::HardBreak)) {
        out.pop();
    }
    out
}

/// The IDs of the Pages the content links to, each once, in order.
pub fn linked_pages(blocks: &[Block]) -> Vec<String> {
    fn inlines(content: &[Inline], out: &mut Vec<String>) {
        for inline in content {
            if let Inline::PageLink { page_id } = inline {
                if !out.contains(page_id) {
                    out.push(page_id.clone());
                }
            }
        }
    }
    fn collect(blocks: &[Block], out: &mut Vec<String>) {
        for block in blocks {
            match block {
                Block::Paragraph(content) | Block::Heading { content, .. } => inlines(content, out),
                Block::Blockquote(children) | Block::Callout { content: children, .. } => collect(children, out),
                Block::BulletList(items) | Block::OrderedList { items, .. } => {
                    items.iter().for_each(|item| collect(item, out))
                }
                Block::TaskList(items) => items.iter().for_each(|item| collect(&item.content, out)),
                Block::Table(rows) => rows.iter().flatten().for_each(|cell| inlines(cell, out)),
                Block::CodeBlock { .. } | Block::HorizontalRule | Block::UnknownDirective { .. } => {}
            }
        }
    }
    let mut out = Vec::new();
    collect(blocks, &mut out);
    out
}

/// The words of the content without any formatting, one line per paragraph,
/// heading, list item or line of code. Search indexes this.
pub fn plain_text(blocks: &[Block]) -> String {
    fn inlines(content: &[Inline], out: &mut String) {
        for inline in content {
            match inline {
                Inline::Text { text, .. } => out.push_str(text),
                Inline::HardBreak => out.push('\n'),
                Inline::Image { alt, .. } => out.push_str(alt),
                // The linked Page's title is not this Page's to give away.
                Inline::PageLink { .. } => out.push(' '),
                // What a directive Turtle does not know means is unknown.
                Inline::UnknownDirective { .. } => out.push(' '),
                Inline::Status { text, .. } => {
                    out.push(' ');
                    out.push_str(text);
                    out.push(' ');
                }
            }
        }
        out.push('\n');
    }
    fn blocks_into(blocks: &[Block], out: &mut String) {
        for block in blocks {
            match block {
                Block::Paragraph(content) | Block::Heading { content, .. } => inlines(content, out),
                Block::CodeBlock { text, .. } => {
                    out.push_str(text);
                    out.push('\n');
                }
                Block::Blockquote(children) | Block::Callout { content: children, .. } => blocks_into(children, out),
                Block::BulletList(items) | Block::OrderedList { items, .. } => {
                    for item in items {
                        blocks_into(item, out);
                    }
                }
                Block::TaskList(items) => {
                    for item in items {
                        blocks_into(&item.content, out);
                    }
                }
                Block::Table(rows) => rows.iter().flatten().for_each(|cell| inlines(cell, out)),
                Block::HorizontalRule | Block::UnknownDirective { .. } => {}
            }
        }
    }
    let mut out = String::new();
    blocks_into(blocks, &mut out);
    out.trim_end().to_string()
}
