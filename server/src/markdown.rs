//! Markdown as a projection of Page content (ADR-0001).
//!
//! The contract: for any normalised content `c`,
//! `from_markdown(&to_markdown(&c)) == c`.

use crate::model::{normalize, Block, Inline, Marks};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

// ---------------------------------------------------------------------------
// Content -> Markdown
// ---------------------------------------------------------------------------

pub fn to_markdown(blocks: &[Block]) -> String {
    let mut out = render_blocks(blocks, "\n\n", None);
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// `parent_bullet` is the marker of the list item these blocks sit in, if any.
fn render_blocks(blocks: &[Block], separator: &str, parent_bullet: Option<char>) -> String {
    let mut parts: Vec<String> = Vec::new();
    // Two lists of the same kind in a row would merge into one when read back,
    // so the second one switches to the other marker. A list that opens an
    // item does the same against its parent, because `- - -` is a rule.
    let mut previous_bullet: Option<char> = parent_bullet;
    let mut previous_delimiter: Option<char> = None;
    for block in blocks {
        let mut bullet = None;
        let mut delimiter = None;
        let rendered = match block {
            Block::Paragraph(_) | Block::Heading { .. } => render_inline_block(block),
            Block::CodeBlock { language, text } => render_code_block(language.as_deref(), text),
            Block::Blockquote(children) => {
                let inner = render_blocks(children, "\n\n", None);
                if inner.is_empty() {
                    ">".to_string()
                } else {
                    inner
                        .lines()
                        .map(|line| {
                            if line.is_empty() {
                                ">".to_string()
                            } else {
                                format!("> {line}")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            Block::BulletList(items) => {
                let marker = if previous_bullet == Some('-') { '+' } else { '-' };
                bullet = Some(marker);
                render_list(items, Some(marker), |_| marker.to_string())
            }
            Block::OrderedList { start, items } => {
                let delim = if previous_delimiter == Some('.') { ')' } else { '.' };
                delimiter = Some(delim);
                render_list(items, None, |i| format!("{}{}", start + i as u64, delim))
            }
            Block::HorizontalRule => "***".to_string(),
        };
        previous_bullet = bullet;
        previous_delimiter = delimiter;
        parts.push(rendered);
    }
    parts.join(separator)
}

const EMPTY_ITEM: &str = "<!-- -->";

fn render_list(
    items: &[Vec<Block>],
    bullet: Option<char>,
    marker: impl Fn(usize) -> String,
) -> String {
    let tight = items.iter().all(|item| is_tight_item(item));
    let rendered: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let marker = marker(i);
            let body = render_blocks(item, if tight { "\n" } else { "\n\n" }, bullet);
            if body.is_empty() {
                // pulldown-cmark loses track of the enclosing list item when a
                // loose list inside a quote ends with an empty item. A comment
                // gives that item a body without adding content.
                if !tight && i + 1 == items.len() {
                    return format!("{marker} {EMPTY_ITEM}");
                }
                return marker;
            }
            let indent = " ".repeat(marker.len() + 1);
            let mut lines = body.lines();
            let mut out = format!("{} {}", marker, lines.next().unwrap_or(""));
            for line in lines {
                out.push('\n');
                if !line.is_empty() {
                    out.push_str(&indent);
                    out.push_str(line);
                }
            }
            out
        })
        .collect();
    rendered.join(if tight { "\n" } else { "\n\n" })
}

/// An item can be written without blank lines when it is a single paragraph,
/// optionally followed by a nested list that is allowed to interrupt it: one
/// whose first item has content and, if ordered, starts at 1.
fn is_tight_item(item: &[Block]) -> bool {
    let first_item_has_content = |items: &[Vec<Block>]| items.first().is_some_and(|i| !i.is_empty());
    match item {
        [] => true,
        [Block::Paragraph(_)] => true,
        [Block::Paragraph(_), Block::BulletList(items)] => first_item_has_content(items),
        [Block::Paragraph(_), Block::OrderedList { start: 1, items }] => first_item_has_content(items),
        _ => false,
    }
}

fn render_code_block(language: Option<&str>, text: &str) -> String {
    let fence = "`".repeat(longest_run(text, '`').max(2) + 1);
    let mut out = format!("{}{}\n", fence, language.unwrap_or(""));
    out.push_str(text);
    out.push('\n');
    out.push_str(&fence);
    out
}

fn longest_run(text: &str, needle: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for c in text.chars() {
        if c == needle {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

/// Renders a paragraph or heading. Markdown's emphasis markers only work next
/// to certain characters (`**bold **x` does not parse as bold), so the result
/// is read back and, if it does not match, written again with HTML tags.
fn render_inline_block(block: &Block) -> String {
    let markdown = render_inline_block_with(block, false);
    if from_markdown(&markdown).as_slice() == std::slice::from_ref(block) {
        markdown
    } else {
        render_inline_block_with(block, true)
    }
}

fn render_inline_block_with(block: &Block, html: bool) -> String {
    match block {
        Block::Paragraph(content) => render_inlines(content, false, html),
        Block::Heading { level, content } => {
            let hashes = "#".repeat(*level as usize);
            let inner = render_inlines(content, true, html);
            if inner.is_empty() {
                hashes
            } else {
                format!("{hashes} {inner}")
            }
        }
        _ => unreachable!("only paragraphs and headings hold inline content"),
    }
}

#[derive(Clone, PartialEq)]
enum Mark {
    Link(String),
    Bold,
    Italic,
    Strike,
}

fn wanted_marks(marks: &Marks) -> Vec<Mark> {
    let mut wanted = Vec::new();
    if let Some(url) = &marks.link {
        wanted.push(Mark::Link(url.clone()));
    }
    if marks.bold {
        wanted.push(Mark::Bold);
    }
    if marks.italic {
        wanted.push(Mark::Italic);
    }
    if marks.strike {
        wanted.push(Mark::Strike);
    }
    wanted
}

fn open_mark(mark: &Mark, html: bool) -> String {
    match (mark, html) {
        (Mark::Link(_), _) => "[".into(),
        (Mark::Bold, false) => "**".into(),
        (Mark::Italic, false) => "*".into(),
        (Mark::Strike, false) => "~~".into(),
        (Mark::Bold, true) => "<strong>".into(),
        (Mark::Italic, true) => "<em>".into(),
        (Mark::Strike, true) => "<del>".into(),
    }
}

fn close_mark(mark: &Mark, html: bool) -> String {
    match (mark, html) {
        (Mark::Link(url), _) => format!("](<{}>)", escape_url(url)),
        (Mark::Bold, false) => "**".into(),
        (Mark::Italic, false) => "*".into(),
        (Mark::Strike, false) => "~~".into(),
        (Mark::Bold, true) => "</strong>".into(),
        (Mark::Italic, true) => "</em>".into(),
        (Mark::Strike, true) => "</del>".into(),
    }
}

fn render_inlines(inlines: &[Inline], in_heading: bool, html: bool) -> String {
    let mut out = String::new();
    let mut open: Vec<Mark> = Vec::new();
    let mut at_line_start = true;

    for (i, inline) in inlines.iter().enumerate() {
        match inline {
            Inline::HardBreak => {
                out.push_str("\\\n");
                at_line_start = true;
            }
            Inline::Text { text, marks } => {
                let wanted = if marks.code {
                    Vec::new()
                } else {
                    wanted_marks(marks)
                };
                let shared = open
                    .iter()
                    .zip(&wanted)
                    .take_while(|(a, b)| a == b)
                    .count();
                while open.len() > shared {
                    out.push_str(&close_mark(&open.pop().unwrap(), html));
                }
                for mark in &wanted[shared..] {
                    out.push_str(&open_mark(mark, html));
                    open.push(mark.clone());
                }
                if marks.code {
                    out.push_str(&render_code_span(text));
                } else {
                    let at_line_end = !matches!(inlines.get(i + 1), Some(Inline::Text { .. }));
                    out.push_str(&escape_text(text, at_line_start, at_line_end, in_heading));
                }
                at_line_start = false;
            }
        }
    }
    while let Some(mark) = open.pop() {
        out.push_str(&close_mark(&mark, html));
    }
    out
}

fn render_code_span(text: &str) -> String {
    let ticks = "`".repeat(longest_run(text, '`') + 1);
    let all_spaces = text.chars().all(|c| c == ' ');
    let needs_padding = text.starts_with('`')
        || text.ends_with('`')
        || (text.starts_with(' ') && text.ends_with(' ') && !all_spaces);
    if needs_padding {
        format!("{ticks} {text} {ticks}")
    } else {
        format!("{ticks}{text}{ticks}")
    }
}

fn escape_url(url: &str) -> String {
    let mut out = String::new();
    for c in url.chars() {
        match c {
            '\\' | '<' | '>' | '&' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// Escapes text so Markdown reads it back literally. Whitespace at the edge of
/// a line would be stripped, so it is written as a character reference.
fn escape_text(text: &str, at_line_start: bool, at_line_end: bool, in_heading: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let is_edge_space = |c: char| c == ' ' || c == '\t';
    let leading = if at_line_start {
        chars.iter().take_while(|c| is_edge_space(**c)).count()
    } else {
        0
    };
    let trailing = if at_line_end {
        chars.iter().rev().take_while(|c| is_edge_space(**c)).count()
    } else {
        0
    };
    // In a run of digits at the start of a line, the `.` or `)` that follows
    // would turn the line into an ordered list.
    let list_delimiter_at = if at_line_start {
        let digits = chars[leading..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        (digits > 0).then_some(leading + digits)
    } else {
        None
    };

    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let on_edge = i < leading || i >= chars.len().saturating_sub(trailing);
        if on_edge && is_edge_space(c) {
            out.push_str(if c == ' ' { "&#32;" } else { "&#9;" });
            continue;
        }
        let first_visible = at_line_start && i == leading;
        let escape = match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '&' | '~' => true,
            '#' => in_heading || first_visible,
            '-' | '+' | '=' => first_visible,
            '.' | ')' => list_delimiter_at == Some(i),
            // A `!` directly before a link would turn it into an image.
            '!' => i + 1 == chars.len(),
            _ => false,
        };
        if escape {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------------------
// Markdown -> Content
// ---------------------------------------------------------------------------

enum Frame {
    Root(Vec<Block>),
    Blockquote(Vec<Block>),
    List {
        start: Option<u64>,
        items: Vec<Vec<Block>>,
    },
    Item(Vec<Block>),
    /// `implicit` marks text that sits directly in a list item without a
    /// paragraph around it, which is how Markdown writes tight lists.
    Paragraph {
        content: Vec<Inline>,
        implicit: bool,
    },
    Heading {
        level: u8,
        content: Vec<Inline>,
    },
    CodeBlock {
        language: Option<String>,
        text: String,
    },
}

#[derive(Default)]
struct ActiveMarks {
    bold: u32,
    italic: u32,
    strike: u32,
    links: Vec<String>,
}

impl ActiveMarks {
    fn current(&self, code: bool) -> Marks {
        Marks {
            bold: self.bold > 0,
            italic: self.italic > 0,
            strike: self.strike > 0,
            code,
            link: self.links.last().cloned(),
        }
    }
}

struct Builder {
    stack: Vec<Frame>,
    marks: ActiveMarks,
}

impl Builder {
    fn add_block(&mut self, block: Block) {
        match self.stack.last_mut() {
            Some(Frame::Root(blocks)) | Some(Frame::Blockquote(blocks)) | Some(Frame::Item(blocks)) => {
                blocks.push(block)
            }
            // Markdown never nests a block directly in these; if it happens
            // the content goes to the nearest container instead of being lost.
            _ => {
                let frame = self.stack.pop().expect("root frame is never popped");
                self.add_block(block);
                self.stack.push(frame);
            }
        }
    }

    fn close_implicit_paragraph(&mut self) {
        if let Some(Frame::Paragraph { implicit: true, .. }) = self.stack.last() {
            if let Some(Frame::Paragraph { content, .. }) = self.stack.pop() {
                self.add_block(Block::Paragraph(content));
            }
        }
    }

    fn inline_target(&mut self) -> &mut Vec<Inline> {
        let has_target = matches!(
            self.stack.last(),
            Some(Frame::Paragraph { .. }) | Some(Frame::Heading { .. })
        );
        if !has_target {
            self.stack.push(Frame::Paragraph {
                content: Vec::new(),
                implicit: true,
            });
        }
        match self.stack.last_mut() {
            Some(Frame::Paragraph { content, .. }) | Some(Frame::Heading { content, .. }) => content,
            _ => unreachable!(),
        }
    }

    fn text(&mut self, text: &str, code: bool) {
        if let Some(Frame::CodeBlock { text: body, .. }) = self.stack.last_mut() {
            body.push_str(text);
            return;
        }
        let marks = self.marks.current(code);
        self.inline_target().push(Inline::Text {
            text: text.to_string(),
            marks,
        });
    }

    fn html(&mut self, html: &str) {
        let trimmed = html.trim();
        if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
            return;
        }
        match trimmed.to_ascii_lowercase().as_str() {
            "<strong>" => self.marks.bold += 1,
            "</strong>" => self.marks.bold = self.marks.bold.saturating_sub(1),
            "<em>" => self.marks.italic += 1,
            "</em>" => self.marks.italic = self.marks.italic.saturating_sub(1),
            "<del>" => self.marks.strike += 1,
            "</del>" => self.marks.strike = self.marks.strike.saturating_sub(1),
            "<br>" | "<br/>" | "<br />" => self.inline_target().push(Inline::HardBreak),
            // Any other HTML is kept as visible text instead of being dropped.
            _ => self.text(html, false),
        }
    }
}

pub fn from_markdown(markdown: &str) -> Vec<Block> {
    let mut b = Builder {
        stack: vec![Frame::Root(Vec::new())],
        marks: ActiveMarks::default(),
    };

    for event in Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    b.close_implicit_paragraph();
                    b.stack.push(Frame::Paragraph {
                        content: Vec::new(),
                        implicit: false,
                    });
                }
                Tag::Heading { level, .. } => {
                    b.close_implicit_paragraph();
                    b.stack.push(Frame::Heading {
                        level: level as u8,
                        content: Vec::new(),
                    });
                }
                Tag::BlockQuote(_) => {
                    b.close_implicit_paragraph();
                    b.stack.push(Frame::Blockquote(Vec::new()));
                }
                Tag::CodeBlock(kind) => {
                    b.close_implicit_paragraph();
                    let language = match kind {
                        CodeBlockKind::Fenced(info) => Some(info.to_string()),
                        CodeBlockKind::Indented => None,
                    };
                    b.stack.push(Frame::CodeBlock {
                        language,
                        text: String::new(),
                    });
                }
                Tag::List(start) => {
                    b.close_implicit_paragraph();
                    b.stack.push(Frame::List {
                        start,
                        items: Vec::new(),
                    });
                }
                Tag::Item => b.stack.push(Frame::Item(Vec::new())),
                Tag::HtmlBlock => b.close_implicit_paragraph(),
                Tag::Emphasis => b.marks.italic += 1,
                Tag::Strong => b.marks.bold += 1,
                Tag::Strikethrough => b.marks.strike += 1,
                Tag::Link { dest_url, .. } => b.marks.links.push(dest_url.to_string()),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if let Some(Frame::Paragraph { content, .. }) = b.stack.pop() {
                        b.add_block(Block::Paragraph(content));
                    }
                }
                TagEnd::Heading(_) => {
                    if let Some(Frame::Heading { level, content }) = b.stack.pop() {
                        b.add_block(Block::Heading { level, content });
                    }
                }
                TagEnd::BlockQuote(_) => {
                    b.close_implicit_paragraph();
                    if let Some(Frame::Blockquote(children)) = b.stack.pop() {
                        b.add_block(Block::Blockquote(children));
                    }
                }
                TagEnd::CodeBlock => {
                    if let Some(Frame::CodeBlock { language, mut text }) = b.stack.pop() {
                        if text.ends_with('\n') {
                            text.pop();
                        }
                        b.add_block(Block::CodeBlock { language, text });
                    }
                }
                TagEnd::Item => {
                    b.close_implicit_paragraph();
                    if let Some(Frame::Item(blocks)) = b.stack.pop() {
                        if let Some(Frame::List { items, .. }) = b.stack.last_mut() {
                            items.push(blocks);
                        }
                    }
                }
                TagEnd::List(_) => {
                    if let Some(Frame::List { start, items }) = b.stack.pop() {
                        b.add_block(match start {
                            Some(start) => Block::OrderedList { start, items },
                            None => Block::BulletList(items),
                        });
                    }
                }
                TagEnd::HtmlBlock => b.close_implicit_paragraph(),
                TagEnd::Emphasis => b.marks.italic = b.marks.italic.saturating_sub(1),
                TagEnd::Strong => b.marks.bold = b.marks.bold.saturating_sub(1),
                TagEnd::Strikethrough => b.marks.strike = b.marks.strike.saturating_sub(1),
                TagEnd::Link => {
                    b.marks.links.pop();
                }
                _ => {}
            },
            Event::Text(text) => b.text(&text, false),
            Event::Code(text) => b.text(&text, true),
            Event::SoftBreak => b.text(" ", false),
            Event::HardBreak => b.inline_target().push(Inline::HardBreak),
            Event::Html(html) | Event::InlineHtml(html) => b.html(&html),
            Event::Rule => {
                b.close_implicit_paragraph();
                b.add_block(Block::HorizontalRule);
            }
            _ => {}
        }
    }

    b.close_implicit_paragraph();
    match b.stack.into_iter().next() {
        Some(Frame::Root(blocks)) => normalize(blocks),
        _ => Vec::new(),
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use crate::model::{Block, Inline, Marks};

    // A small deterministic generator, so the property tests need no extra
    // dependency and failures are reproducible from the seed.
    pub struct Rng(pub u64);

    impl Rng {
        pub fn next(&mut self, bound: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % bound
        }

        pub fn text(&mut self) -> String {
            const ALPHABET: &[&str] = &[
                "a", "b", "word", " ", " ", "  ", "*", "_", "`", "[", "]", "(", ")", "<", ">", "&",
                "~", "#", "-", "+", "=", "1", ".", "\\", "!", "|", ":", "\"", "é", "ü", "\t",
            ];
            let len = 1 + self.next(8);
            (0..len).map(|_| ALPHABET[self.next(ALPHABET.len())]).collect()
        }

        pub fn inlines(&mut self) -> Vec<Inline> {
            let len = self.next(5);
            (0..len)
                .map(|_| {
                    if self.next(8) == 0 {
                        return Inline::HardBreak;
                    }
                    let marks = Marks {
                        bold: self.next(4) == 0,
                        italic: self.next(4) == 0,
                        strike: self.next(6) == 0,
                        code: self.next(8) == 0,
                        link: (self.next(8) == 0).then(|| format!("https://x.org/{}", self.text())),
                    };
                    Inline::Text {
                        text: self.text(),
                        marks,
                    }
                })
                .collect()
        }

        pub fn blocks(&mut self, depth: usize) -> Vec<Block> {
            let len = self.next(4);
            (0..len).map(|_| self.block(depth)).collect()
        }

        pub fn block(&mut self, depth: usize) -> Block {
            let kinds = if depth >= 3 { 4 } else { 7 };
            match self.next(kinds) {
                0 => Block::Paragraph(self.inlines()),
                1 => Block::Heading {
                    level: 1 + self.next(6) as u8,
                    content: self.inlines(),
                },
                2 => Block::CodeBlock {
                    language: (self.next(2) == 0).then(|| "rs".to_string()),
                    text: (0..self.next(4))
                        .map(|_| self.text())
                        .collect::<Vec<_>>()
                        .join("\n"),
                },
                3 => Block::HorizontalRule,
                4 => Block::Blockquote(self.blocks(depth + 1)),
                5 => Block::BulletList((0..1 + self.next(3)).map(|_| self.blocks(depth + 1)).collect()),
                _ => Block::OrderedList {
                    start: self.next(12) as u64,
                    items: (0..1 + self.next(3)).map(|_| self.blocks(depth + 1)).collect(),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::Rng;
    use super::*;

    fn text(s: &str) -> Inline {
        Inline::Text {
            text: s.into(),
            marks: Marks::default(),
        }
    }

    fn marked(s: &str, f: impl Fn(&mut Marks)) -> Inline {
        let mut marks = Marks::default();
        f(&mut marks);
        Inline::Text {
            text: s.into(),
            marks,
        }
    }

    fn para(inlines: Vec<Inline>) -> Block {
        Block::Paragraph(inlines)
    }

    fn assert_round_trip(blocks: Vec<Block>) {
        let blocks = normalize(blocks);
        let markdown = to_markdown(&blocks);
        let back = from_markdown(&markdown);
        assert_eq!(back, blocks, "\n--- markdown ---\n{markdown}\n---");
    }

    #[test]
    fn writes_readable_markdown() {
        let blocks = vec![
            Block::Heading {
                level: 1,
                content: vec![text("Release notes")],
            },
            para(vec![
                text("Turtle is "),
                marked("fast", |m| m.bold = true),
                text(" and "),
                marked("small", |m| m.italic = true),
                text(". See "),
                marked("the docs", |m| m.link = Some("https://example.org/a".into())),
                text("."),
            ]),
            Block::BulletList(vec![
                vec![para(vec![text("one")])],
                vec![
                    para(vec![text("two")]),
                    Block::BulletList(vec![vec![para(vec![text("nested")])]]),
                ],
            ]),
            Block::CodeBlock {
                language: Some("rust".into()),
                text: "fn main() {}".into(),
            },
        ];
        assert_eq!(
            to_markdown(&blocks),
            "# Release notes\n\n\
             Turtle is **fast** and *small*. See [the docs](<https://example.org/a>).\n\n\
             - one\n- two\n  - nested\n\n\
             ```rust\nfn main() {}\n```\n"
        );
        assert_round_trip(blocks);
    }

    #[test]
    fn every_block_type_round_trips() {
        assert_round_trip(vec![
            Block::Heading {
                level: 3,
                content: vec![text("Title # with hash #")],
            },
            Block::Heading {
                level: 2,
                content: vec![],
            },
            para(vec![text("line one"), Inline::HardBreak, text("line two")]),
            Block::HorizontalRule,
            Block::Blockquote(vec![
                para(vec![text("quoted")]),
                Block::CodeBlock {
                    language: None,
                    text: "a\n\nb".into(),
                },
            ]),
            Block::Blockquote(vec![]),
            Block::OrderedList {
                start: 7,
                items: vec![vec![para(vec![text("seven")])], vec![]],
            },
            Block::OrderedList {
                start: 1,
                items: vec![vec![
                    para(vec![text("first")]),
                    Block::OrderedList {
                        start: 3,
                        items: vec![vec![para(vec![text("deep")])]],
                    },
                ]],
            },
            Block::BulletList(vec![vec![
                Block::CodeBlock {
                    language: Some("sh".into()),
                    text: "ls\n".into(),
                },
                Block::HorizontalRule,
                Block::Blockquote(vec![para(vec![text("in item")])]),
            ]]),
            Block::BulletList(vec![vec![para(vec![text("separate list")])]]),
            Block::CodeBlock {
                language: None,
                text: "```\nfence inside\n````".into(),
            },
            Block::CodeBlock {
                language: None,
                text: String::new(),
            },
        ]);
    }

    #[test]
    fn text_that_looks_like_markdown_stays_text() {
        for s in [
            "# not a heading",
            "- not a list",
            "+ nor this",
            "1. not ordered",
            "12) not ordered",
            "> not a quote",
            "*not emphasis* _nor_ this",
            "`not code`",
            "[not](a link)",
            "<b>not html</b> &amp; not an entity",
            "---",
            "===",
            "    not an indented code block",
            "trailing spaces   ",
            "back\\slash",
            "~~not struck~~",
        ] {
            assert_round_trip(vec![para(vec![text(s)])]);
            assert_round_trip(vec![para(vec![text("x"), Inline::HardBreak, text(s)])]);
        }
    }

    #[test]
    fn awkward_formatting_round_trips() {
        assert_round_trip(vec![para(vec![
            text("a"),
            marked("\"quoted\"", |m| m.bold = true),
            text("b"),
        ])]);
        assert_round_trip(vec![para(vec![
            marked("bold ", |m| m.bold = true),
            text("then plain"),
        ])]);
        assert_round_trip(vec![para(vec![marked(" ", |m| m.italic = true), text("x")])]);
        assert_round_trip(vec![para(vec![
            marked("both", |m| {
                m.bold = true;
                m.italic = true
            }),
            marked("bold", |m| m.bold = true),
            marked("struck", |m| m.strike = true),
        ])]);
        assert_round_trip(vec![para(vec![
            marked("` tick", |m| m.code = true),
            text(" "),
            marked(" padded ", |m| m.code = true),
            text(" "),
            marked("  ", |m| m.code = true),
        ])]);
        assert_round_trip(vec![para(vec![marked("link <with> (odd) &chars;", |m| {
            m.link = Some("https://example.org/?a=1&b=<2>\\".into())
        })])]);
    }

    const SEEDS: u64 = 20_000;

    #[test]
    fn random_content_round_trips() {
        let mut failures: Vec<(u64, String)> = Vec::new();
        for seed in 0..SEEDS {
            let mut rng = Rng(seed);
            let blocks = normalize(rng.blocks(0));
            let markdown = to_markdown(&blocks);
            if from_markdown(&markdown) != blocks {
                failures.push((seed, markdown));
            }
        }
        failures.sort_by_key(|(_, markdown)| markdown.len());
        for (seed, markdown) in failures.iter().take(6) {
            eprintln!("seed {seed}\n--- markdown ---\n{markdown}\n---");
        }
        assert!(failures.is_empty(), "{} of {SEEDS} failed", failures.len());
    }
}
