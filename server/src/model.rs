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
    HorizontalRule,
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
        Block::HorizontalRule => Block::HorizontalRule,
    })
}

fn normalize_inlines(inlines: Vec<Inline>, in_heading: bool) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    for inline in inlines {
        let inline = match inline {
            Inline::HardBreak if in_heading => Inline::Text {
                text: " ".into(),
                marks: Marks::default(),
            },
            other => other,
        };
        match inline {
            Inline::HardBreak => out.push(Inline::HardBreak),
            Inline::Image { src, alt } => out.push(Inline::Image {
                src: src.replace(['\n', '\r'], ""),
                alt: alt.replace(['\n', '\r'], " "),
            }),
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

/// The words of the content without any formatting, one line per paragraph,
/// heading, list item or line of code. Search indexes this.
pub fn plain_text(blocks: &[Block]) -> String {
    fn inlines(content: &[Inline], out: &mut String) {
        for inline in content {
            match inline {
                Inline::Text { text, .. } => out.push_str(text),
                Inline::HardBreak => out.push('\n'),
                Inline::Image { alt, .. } => out.push_str(alt),
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
                Block::Blockquote(children) => blocks_into(children, out),
                Block::BulletList(items) | Block::OrderedList { items, .. } => {
                    for item in items {
                        blocks_into(item, out);
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
