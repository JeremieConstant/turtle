//! The content of a Page, independent of how it is stored or serialised.
//!
//! This is the common ground between the collaborative document (see `ydoc`)
//! and Markdown (see `markdown`). It can only hold what Markdown can express
//! (ADR-0006).

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
}

/// How a Page link is written as a link address: `turtle:` and the Page's ID.
pub const PAGE_ADDRESS_SCHEME: &str = "turtle:";

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
    HorizontalRule,
}

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
    blocks.into_iter().filter_map(normalize_block).collect()
}

fn normalize_block(block: Block) -> Option<Block> {
    Some(match block {
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
        Block::Blockquote(children) => Block::Blockquote(normalize(children)),
        Block::Callout { kind, content } => Block::Callout { kind, content: normalize(content) },
        Block::BulletList(items) => {
            if items.is_empty() {
                return None;
            }
            Block::BulletList(items.into_iter().map(normalize).collect())
        }
        Block::OrderedList { start, items } => {
            if items.is_empty() {
                return None;
            }
            Block::OrderedList {
                start: start.min(999_999_999),
                items: items.into_iter().map(normalize).collect(),
            }
        }
        Block::TaskList(items) => {
            if items.is_empty() {
                return None;
            }
            Block::TaskList(
                items
                    .into_iter()
                    .map(|item| TaskItem { checked: item.checked, content: normalize(item.content) })
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
    fn collect(blocks: &[Block], out: &mut Vec<String>) {
        for block in blocks {
            match block {
                Block::Paragraph(content) | Block::Heading { content, .. } => {
                    for inline in content {
                        if let Inline::PageLink { page_id } = inline {
                            if !out.contains(page_id) {
                                out.push(page_id.clone());
                            }
                        }
                    }
                }
                Block::Blockquote(children) | Block::Callout { content: children, .. } => collect(children, out),
                Block::BulletList(items) | Block::OrderedList { items, .. } => {
                    items.iter().for_each(|item| collect(item, out))
                }
                Block::TaskList(items) => items.iter().for_each(|item| collect(&item.content, out)),
                Block::CodeBlock { .. } | Block::HorizontalRule => {}
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
                Block::HorizontalRule => {}
            }
        }
    }
    let mut out = String::new();
    blocks_into(blocks, &mut out);
    out.trim_end().to_string()
}
