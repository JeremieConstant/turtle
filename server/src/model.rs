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
}

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
