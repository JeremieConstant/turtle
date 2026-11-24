//! Markdown as a projection of Page content (ADR-0001).
//!
//! The contract: for any normalised content `c`,
//! `from_markdown(&to_markdown(&c)) == c`.
//!
//! Beyond CommonMark and GitHub's tables, task lists and strikethrough, it
//! reads and writes directives (ADR-0013), see `directive`. A directive
//! Turtle does not know is read as its source and written back unchanged.

use crate::directive;
use crate::model::{
    inline_directive, normalize, status_directive, turtle_link_of, Block, CalloutKind, Cell, Inline, Marks, Row,
    TaskItem, MEMBER_ADDRESS_SCHEME, PAGE_ADDRESS_SCHEME,
};
use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};

// ---------------------------------------------------------------------------
// Content -> Markdown
// ---------------------------------------------------------------------------

/// What a Page link or Mention points at, by ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linked<'a> {
    Page(&'a str),
    Person(&'a str),
}

/// The text a Page link or Mention shows: a Page's title, or None when the
/// reader may not read that Page; a person's current name, or None when
/// there is no such Member or Guest.
pub type Titles<'a> = &'a dyn Fn(Linked) -> Option<String>;

/// The Markdown of some content, with Page links and Mentions shown without
/// titles or names.
pub fn to_markdown(blocks: &[Block]) -> String {
    to_markdown_with_titles(blocks, &|_| None)
}

/// The Markdown of some content, with each Page link and Mention shown with
/// the text `titles` gives it. Reading it back ignores that text.
pub fn to_markdown_with_titles(blocks: &[Block], titles: Titles) -> String {
    let mut out = render_blocks(blocks, "\n\n", None, titles);
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// `parent_bullet` is the marker of the list item these blocks sit in, if any.
fn render_blocks(blocks: &[Block], separator: &str, parent_bullet: Option<char>, titles: Titles) -> String {
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
            Block::Paragraph(_) | Block::Heading { .. } => render_inline_block(block, titles),
            Block::CodeBlock { language, text } => render_code_block(language.as_deref(), text),
            Block::Blockquote(children) => quote(&render_blocks(children, "\n\n", None, titles)),
            Block::Callout { kind, content } => {
                let marker = format!("[!{}]", kind.name().to_ascii_uppercase());
                let inner = render_blocks(content, "\n\n", None, titles);
                // A paragraph continues on the marker's next line, as people
                // write callouts; anything else might not end the marker's
                // paragraph, so it follows an empty line.
                quote(&if inner.is_empty() {
                    marker
                } else if matches!(content.first(), Some(Block::Paragraph(_))) {
                    format!("{marker}\n{inner}")
                } else {
                    format!("{marker}\n\n{inner}")
                })
            }
            // A task list is a bullet list to Markdown, so the two take turns
            // in choosing their marker.
            Block::BulletList(_) | Block::TaskList(_) => {
                let marker = if previous_bullet == Some('-') { '+' } else { '-' };
                bullet = Some(marker);
                let items: Vec<_> = match block {
                    Block::TaskList(items) => items
                        .iter()
                        .map(|item| (Some(item.checked), item.content.as_slice()))
                        .collect(),
                    Block::BulletList(items) => items.iter().map(|item| (None, item.as_slice())).collect(),
                    _ => unreachable!(),
                };
                render_list(&items, Some(marker), |_| marker.to_string(), titles)
            }
            Block::OrderedList { start, items } => {
                let delim = if previous_delimiter == Some('.') { ')' } else { '.' };
                delimiter = Some(delim);
                let items: Vec<_> = items.iter().map(|item| (None, item.as_slice())).collect();
                render_list(&items, None, |i| format!("{}{}", start + i as u64, delim), titles)
            }
            Block::Table(rows) => render_table(rows, titles),
            Block::HorizontalRule => "***".to_string(),
            Block::UnknownDirective { source } => source.clone(),
        };
        previous_bullet = bullet;
        previous_delimiter = delimiter;
        parts.push(rendered);
    }
    parts.join(separator)
}

/// Puts each line of some Markdown in a quote.
fn quote(inner: &str) -> String {
    if inner.is_empty() {
        return ">".to_string();
    }
    inner
        .lines()
        .map(|line| if line.is_empty() { ">".to_string() } else { format!("> {line}") })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The kind of callout a line of Markdown marks, when it is nothing but a marker such as `[!NOTE]`.
fn callout_marker(line: &str) -> Option<CalloutKind> {
    let name = line.trim_end().strip_prefix("[!")?.strip_suffix(']')?;
    CalloutKind::named(name)
}

const EMPTY_ITEM: &str = "<!-- -->";

/// Renders list items, each with whether it is checked when it is a task.
fn render_list(
    items: &[(Option<bool>, &[Block])],
    bullet: Option<char>,
    marker: impl Fn(usize) -> String,
    titles: Titles,
) -> String {
    let tight = items.iter().all(|(_, item)| is_tight_item(item));
    let separator = if tight { "\n" } else { "\n\n" };
    let rendered: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(i, (task, item))| {
            let marker = marker(i);
            let mut body = render_blocks(item, separator, bullet, titles);
            if let Some(checked) = task {
                let checkbox = if *checked { "[x]" } else { "[ ]" };
                // The checkbox only counts in front of a paragraph, so an item
                // without one starts with a comment that adds no content.
                body = if matches!(item.first(), Some(Block::Paragraph(_))) {
                    format!("{checkbox} {body}")
                } else if body.is_empty() {
                    format!("{checkbox} {EMPTY_ITEM}")
                } else {
                    format!("{checkbox} {EMPTY_ITEM}\n\n{body}")
                };
            }
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
    rendered.join(separator)
}

/// Renders a table as GitHub writes them: a header row, a row of dashes, and
/// the other rows.
fn render_table(rows: &[Row], titles: Titles) -> String {
    let line = |cells: Vec<String>| format!("| {} |", cells.join(" | "));
    let mut lines: Vec<String> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        lines.push(line(row.iter().map(|cell| render_cell(cell, titles)).collect()));
        if i == 0 {
            lines.push(line(vec!["---".to_string(); row.len()]));
        }
    }
    lines.join("\n")
}

/// Renders the content of a cell, which has to stay on one line: a line
/// break is `<br>`, and every `|` is escaped so it does not end the cell,
/// even inside code or a link address. Like a paragraph, it falls back to
/// HTML tags for formatting the markers cannot express.
fn render_cell(cell: &[Inline], titles: Titles) -> String {
    let render = |html| render_inlines(cell, InlinePlace::Cell, html, titles).replace('|', "\\|");
    let markdown = render(false);
    let read = from_markdown(&format!("| {markdown} |\n| --- |"));
    if read == [Block::Table(vec![vec![cell.to_vec()]])] {
        markdown
    } else {
        render(true)
    }
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
        // A task item always has content: its checkbox.
        [Block::Paragraph(_), Block::TaskList(_)] => true,
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
fn render_inline_block(block: &Block, titles: Titles) -> String {
    let markdown = render_inline_block_with(block, false, titles);
    if from_markdown(&markdown).as_slice() == std::slice::from_ref(block) {
        markdown
    } else {
        render_inline_block_with(block, true, titles)
    }
}

fn render_inline_block_with(block: &Block, html: bool, titles: Titles) -> String {
    match block {
        Block::Paragraph(content) => render_inlines(content, InlinePlace::Paragraph, html, titles),
        Block::Heading { level, content } => {
            let hashes = "#".repeat(*level as usize);
            let inner = render_inlines(content, InlinePlace::Heading, html, titles);
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

/// Where inline content is written, which decides how a line break is
/// written and what has to be escaped.
#[derive(Clone, Copy, PartialEq)]
enum InlinePlace {
    Paragraph,
    Heading,
    Cell,
}

fn render_inlines(inlines: &[Inline], place: InlinePlace, html: bool, titles: Titles) -> String {
    let mut out = String::new();
    let mut open: Vec<Mark> = Vec::new();
    let mut at_line_start = true;

    for (i, inline) in inlines.iter().enumerate() {
        match inline {
            Inline::HardBreak if place == InlinePlace::Cell => {
                out.push_str("<br>");
                at_line_start = true;
            }
            Inline::HardBreak => {
                out.push_str("\\\n");
                at_line_start = true;
            }
            // Formatting does not apply to an image, so the marks around it
            // stay open: `**a ![b](c) d**` keeps both texts bold.
            Inline::Image { src, alt } => {
                let alt = escape_text(alt, false, false, place);
                out.push_str(&format!("![{alt}](<{}>)", escape_url(src)));
                at_line_start = false;
            }
            // Like an image, it takes no formatting and leaves the marks around it open.
            Inline::UnknownDirective { source } => {
                out.push_str(source);
                at_line_start = false;
            }
            Inline::Status { text, color } => {
                out.push_str(&status_directive(text, *color).to_inline_markdown());
                at_line_start = false;
            }
            Inline::PageLink { .. } | Inline::Mention { .. } => {
                // A Page link or Mention carries no formatting of its own.
                while let Some(mark) = open.pop() {
                    out.push_str(&close_mark(&mark, html));
                }
                let (shown, address) = match inline {
                    Inline::PageLink { page_id } => {
                        (titles(Linked::Page(page_id)).unwrap_or_default(), format!("{PAGE_ADDRESS_SCHEME}{page_id}"))
                    }
                    Inline::Mention { person_id } => {
                        let name = titles(Linked::Person(person_id)).unwrap_or_default();
                        (format!("@{name}"), format!("{MEMBER_ADDRESS_SCHEME}{person_id}"))
                    }
                    _ => unreachable!("matched above"),
                };
                let shown = escape_text(&shown.replace(['\n', '\r'], " "), false, false, place);
                out.push_str(&format!("[{shown}]({address})"));
                at_line_start = false;
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
                    let at_line_end = matches!(inlines.get(i + 1), None | Some(Inline::HardBreak));
                    out.push_str(&escape_text(text, at_line_start, at_line_end, place));
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
fn escape_text(text: &str, at_line_start: bool, at_line_end: bool, place: InlinePlace) -> String {
    let in_heading = place == InlinePlace::Heading;
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
            // A `:` would start a directive where a name follows it, and a
            // block directive at the start of a line.
            ':' => first_visible || starts_directive_name(&chars[i + 1..]),
            '.' | ')' => list_delimiter_at == Some(i),
            // A `!` directly before a link would turn it into an image.
            '!' => i + 1 == chars.len(),
            // Lines with a `|` could be read as a table. A cell escapes every
            // `|` of its whole rendering itself.
            '|' => place != InlinePlace::Cell,
            _ => false,
        };
        if escape {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Whether text after a `:` might be read as a directive's name: a name
/// that runs to the end of the text, where anything might follow it, or to
/// brackets or braces.
fn starts_directive_name(after: &[char]) -> bool {
    if !after.first().is_some_and(char::is_ascii_alphabetic) {
        return false;
    }
    let name = after
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .count();
    matches!(after.get(name), None | Some('[' | '{'))
}

// ---------------------------------------------------------------------------
// Markdown -> Content
// ---------------------------------------------------------------------------

enum Frame {
    Root(Vec<Block>),
    /// A quote, and the kind of callout it is if its first line says so.
    Blockquote(Vec<Block>, Option<CalloutKind>),
    List {
        start: Option<u64>,
        items: Vec<(Vec<Block>, Option<Checkbox>)>,
    },
    Item(Vec<Block>, Option<Checkbox>),
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
    /// A table and its rows so far.
    Table(Vec<Row>),
    Cell(Cell),
}

/// The checkbox in front of a list item, and how it was written.
struct Checkbox {
    checked: bool,
    written: String,
}

/// A list is a task list when every item has a checkbox. Otherwise the
/// checkboxes it has are kept as the text they were written as.
fn list_block(start: Option<u64>, items: Vec<(Vec<Block>, Option<Checkbox>)>) -> Block {
    if start.is_none() && items.iter().all(|(_, checkbox)| checkbox.is_some()) {
        return Block::TaskList(
            items
                .into_iter()
                .map(|(content, checkbox)| TaskItem {
                    checked: checkbox.is_some_and(|c| c.checked),
                    content,
                })
                .collect(),
        );
    }
    let items = items
        .into_iter()
        .map(|(mut blocks, checkbox)| {
            if let Some(Checkbox { written, .. }) = checkbox {
                let text = Inline::Text {
                    text: format!("{written} "),
                    marks: Marks::default(),
                };
                match blocks.first_mut() {
                    Some(Block::Paragraph(content)) => content.insert(0, text),
                    _ => blocks.insert(0, Block::Paragraph(vec![text])),
                }
            }
            blocks
        })
        .collect();
    match start {
        Some(start) => Block::OrderedList { start, items },
        None => Block::BulletList(items),
    }
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
    /// The image being read, if any: where it points and its text so far.
    image: Option<(String, String)>,
    /// The Page link or Mention being read. What the link says is left out,
    /// because it shows the Page's current title or the person's name instead.
    turtle_link: Option<Inline>,
    /// Where the line that made a quote a callout ends. Its marker is not text.
    callout_marker_end: Option<usize>,
    /// The directives in running text, which the Markdown being read holds
    /// as `stand_in`, their number, and `stand_in` again.
    directives: Vec<String>,
    stand_in: char,
}

impl Builder {
    fn add_block(&mut self, block: Block) {
        match self.stack.last_mut() {
            Some(Frame::Root(blocks)) | Some(Frame::Blockquote(blocks, _)) | Some(Frame::Item(blocks, _)) => {
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
            Some(Frame::Paragraph { .. }) | Some(Frame::Heading { .. }) | Some(Frame::Cell(_))
        );
        if !has_target {
            self.stack.push(Frame::Paragraph {
                content: Vec::new(),
                implicit: true,
            });
        }
        match self.stack.last_mut() {
            Some(Frame::Paragraph { content, .. }) | Some(Frame::Heading { content, .. }) | Some(Frame::Cell(content)) => {
                content
            }
            _ => unreachable!(),
        }
    }

    fn text(&mut self, text: &str, code: bool) {
        let pieces = stood_in(text, self.stand_in, &self.directives);
        // Whatever Markdown puts inside an image is its alternative text.
        if let Some((_, alt)) = &mut self.image {
            pieces.iter().for_each(|(piece, _)| alt.push_str(piece));
            return;
        }
        if let Some(Frame::CodeBlock { text: body, .. }) = self.stack.last_mut() {
            pieces.iter().for_each(|(piece, _)| body.push_str(piece));
            return;
        }
        if self.turtle_link.is_some() {
            return;
        }
        let marks = self.marks.current(code);
        for (piece, is_directive) in pieces {
            let directive = (is_directive && !code).then(|| inline_directive(&piece)).flatten();
            let inline = directive.unwrap_or(Inline::Text { text: piece, marks: marks.clone() });
            self.inline_target().push(inline);
        }
    }

    fn hard_break(&mut self) {
        if self.turtle_link.is_none() {
            self.inline_target().push(Inline::HardBreak);
        }
    }

    fn html(&mut self, html: &str) {
        if self.image.is_some() {
            return self.text(html, false);
        }
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
            "<br>" | "<br/>" | "<br />" => self.hard_break(),
            // Any other HTML is kept as visible text instead of being dropped.
            _ => self.text(html, false),
        }
    }
}

/// Splits a text of the Markdown being read into its pieces, each with
/// whether it is the source of a directive that a stand-in stood for.
fn stood_in(text: &str, stand_in: char, directives: &[String]) -> Vec<(String, bool)> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(stand_in) {
        let after = &rest[start + stand_in.len_utf8()..];
        let number = after.find(stand_in).and_then(|end| Some((after[..end].parse::<usize>().ok()?, end)));
        let Some((source, end)) = number.and_then(|(n, end)| Some((directives.get(n)?, end))) else {
            break;
        };
        if start > 0 {
            pieces.push((rest[..start].to_string(), false));
        }
        pieces.push((source.clone(), true));
        rest = &after[end + stand_in.len_utf8()..];
    }
    if !rest.is_empty() || pieces.is_empty() {
        pieces.push((rest.to_string(), false));
    }
    pieces
}

/// Part of the Markdown at the top level: Markdown, or a block directive.
enum TopLevel<'a> {
    Markdown(&'a str),
    BlockDirective(&'a str),
}

/// The Markdown split around its block directives. One starts a line at
/// the top level where Markdown would start a paragraph or heading.
fn top_level(markdown: &str) -> Vec<TopLevel<'_>> {
    let mut parts = Vec::new();
    let mut rest = markdown;
    'parts: loop {
        let mut depth = 0;
        for (event, range) in Parser::new_ext(rest, OPTIONS).into_offset_iter() {
            match event {
                Event::Start(tag) => {
                    if depth == 0 && matches!(tag, Tag::Paragraph | Tag::Heading { .. }) {
                        for (line, at) in directive::lines_with_ends(&rest[range.clone()]) {
                            let start = range.start + at;
                            let line_start = start == 0 || rest.as_bytes()[start - 1] == b'\n';
                            if !line_start || !line.starts_with(":::") {
                                continue;
                            }
                            if let Some((_, len)) = directive::block_at(&rest[start..]) {
                                parts.push(TopLevel::Markdown(&rest[..start]));
                                parts.push(TopLevel::BlockDirective(&rest[start..start + len]));
                                rest = &rest[start + len..];
                                continue 'parts;
                            }
                        }
                    }
                    depth += 1;
                }
                Event::End(_) => depth -= 1,
                _ => {}
            }
        }
        parts.push(TopLevel::Markdown(rest));
        return parts;
    }
}

pub fn from_markdown(markdown: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    for part in top_level(markdown) {
        match part {
            TopLevel::Markdown(markdown) => blocks.extend(read_markdown(markdown)),
            TopLevel::BlockDirective(source) => blocks.push(Block::UnknownDirective { source: source.to_string() }),
        }
    }
    normalize(blocks)
}

const OPTIONS: Options = Options::ENABLE_STRIKETHROUGH
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_TABLES);

/// Where the Markdown has directives in running text. A directive starts
/// with a `:` that is text, so not in code, an address or HTML, and that is
/// not escaped.
fn inline_directives(markdown: &str) -> Vec<std::ops::Range<usize>> {
    let mut found: Vec<std::ops::Range<usize>> = Vec::new();
    let mut in_code = false;
    let mut in_image = 0;
    let mut in_autolink = 0;
    for (event, range) in Parser::new_ext(markdown, OPTIONS).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => in_code = true,
            Event::End(TagEnd::CodeBlock) => in_code = false,
            Event::Start(Tag::Image { .. }) => in_image += 1,
            Event::End(TagEnd::Image) => in_image -= 1,
            Event::Start(Tag::Link { link_type: LinkType::Autolink | LinkType::Email, .. }) => in_autolink += 1,
            Event::End(TagEnd::Link) if in_autolink > 0 => in_autolink -= 1,
            Event::Text(text) if !in_code && in_image == 0 && in_autolink == 0 && markdown[range.clone()] == *text => {
                for (i, _) in text.match_indices(':') {
                    let at = range.start + i;
                    let escapes = markdown[..at].bytes().rev().take_while(|b| *b == b'\\').count();
                    if escapes % 2 == 1 || found.last().is_some_and(|last| at < last.end) {
                        continue;
                    }
                    if let Some((_, len)) = directive::inline_at(&markdown[at..]) {
                        found.push(at..at + len);
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// Reads Markdown without block directives. Each directive in running text
/// is read as a stand-in, so that what it holds is not read as Markdown.
fn read_markdown(markdown: &str) -> Vec<Block> {
    let stand_in = ('\u{E000}'..='\u{F8FF}').find(|c| !markdown.contains(*c)).unwrap_or('\u{E000}');
    let mut replaced = markdown.to_string();
    let mut directives = Vec::new();
    // What a directive holds, such as a backtick, may hide a later one from
    // the parser, so the Markdown is searched again once they stand in.
    loop {
        let found: Vec<_> = inline_directives(&replaced)
            .into_iter()
            .filter(|range| !replaced[range.clone()].contains(stand_in))
            .collect();
        if found.is_empty() {
            break;
        }
        let mut next = String::with_capacity(replaced.len());
        let mut at = 0;
        for range in found {
            next.push_str(&replaced[at..range.start]);
            next.push_str(&format!("{stand_in}{}{stand_in}", directives.len()));
            directives.push(replaced[range.clone()].to_string());
            at = range.end;
        }
        next.push_str(&replaced[at..]);
        replaced = next;
    }
    let markdown = replaced.as_str();

    let mut b = Builder {
        stack: vec![Frame::Root(Vec::new())],
        marks: ActiveMarks::default(),
        image: None,
        turtle_link: None,
        callout_marker_end: None,
        directives,
        stand_in,
    };

    for (event, range) in Parser::new_ext(markdown, OPTIONS).into_offset_iter() {
        if let Some(end) = b.callout_marker_end {
            if range.start > end {
                b.callout_marker_end = None;
            } else if matches!(event, Event::Text(_) | Event::SoftBreak | Event::HardBreak) {
                continue;
            }
        }
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    b.close_implicit_paragraph();
                    // The marker is read from what was written, so a quote that
                    // only shows a marker once escapes are read stays a quote.
                    if let Some(Frame::Blockquote(children, kind @ None)) = b.stack.last_mut() {
                        let line = markdown[range.start..].split('\n').next().unwrap_or_default();
                        if children.is_empty() {
                            if let Some(marked) = callout_marker(line) {
                                *kind = Some(marked);
                                b.callout_marker_end = Some(range.start + line.len());
                            }
                        }
                    }
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
                    b.stack.push(Frame::Blockquote(Vec::new(), None));
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
                Tag::Item => b.stack.push(Frame::Item(Vec::new(), None)),
                Tag::HtmlBlock => b.close_implicit_paragraph(),
                Tag::Table(_) => {
                    b.close_implicit_paragraph();
                    b.stack.push(Frame::Table(Vec::new()));
                }
                Tag::TableHead | Tag::TableRow => {
                    if let Some(Frame::Table(rows)) = b.stack.last_mut() {
                        rows.push(Vec::new());
                    }
                }
                Tag::TableCell => b.stack.push(Frame::Cell(Vec::new())),
                Tag::Emphasis => b.marks.italic += 1,
                Tag::Strong => b.marks.bold += 1,
                Tag::Strikethrough => b.marks.strike += 1,
                Tag::Link { dest_url, .. } => match turtle_link_of(&dest_url) {
                    Some(link) => b.turtle_link = Some(link),
                    None => b.marks.links.push(dest_url.to_string()),
                },
                Tag::Image { dest_url, .. } => b.image = Some((dest_url.to_string(), String::new())),
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
                    match b.stack.pop() {
                        Some(Frame::Blockquote(content, Some(kind))) => b.add_block(Block::Callout { kind, content }),
                        Some(Frame::Blockquote(children, None)) => b.add_block(Block::Blockquote(children)),
                        _ => {}
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
                    if let Some(Frame::Item(blocks, checkbox)) = b.stack.pop() {
                        if let Some(Frame::List { items, .. }) = b.stack.last_mut() {
                            items.push((blocks, checkbox));
                        }
                    }
                }
                TagEnd::List(_) => {
                    if let Some(Frame::List { start, items }) = b.stack.pop() {
                        b.add_block(list_block(start, items));
                    }
                }
                TagEnd::HtmlBlock => b.close_implicit_paragraph(),
                TagEnd::TableCell => {
                    if let Some(Frame::Cell(content)) = b.stack.pop() {
                        if let Some(Frame::Table(rows)) = b.stack.last_mut() {
                            rows.last_mut().expect("a cell is in a row").push(content);
                        }
                    }
                }
                TagEnd::Table => {
                    if let Some(Frame::Table(rows)) = b.stack.pop() {
                        b.add_block(Block::Table(rows));
                    }
                }
                TagEnd::Emphasis => b.marks.italic = b.marks.italic.saturating_sub(1),
                TagEnd::Strong => b.marks.bold = b.marks.bold.saturating_sub(1),
                TagEnd::Strikethrough => b.marks.strike = b.marks.strike.saturating_sub(1),
                // Links do not nest, so a link that ends inside a Page link or Mention is that one.
                TagEnd::Link => match b.turtle_link.take() {
                    Some(link) => b.inline_target().push(link),
                    None => {
                        b.marks.links.pop();
                    }
                },
                TagEnd::Image => {
                    if let Some((src, alt)) = b.image.take() {
                        // Like the rest of what a Page link says, an image inside one is left out.
                        if b.turtle_link.is_none() {
                            b.inline_target().push(Inline::Image { src, alt });
                        }
                    }
                }
                _ => {}
            },
            Event::Text(text) => b.text(&text, false),
            Event::Code(text) => b.text(&text, true),
            Event::SoftBreak => b.text(" ", false),
            Event::HardBreak if b.image.is_some() => b.text(" ", false),
            Event::HardBreak => b.hard_break(),
            Event::Html(html) | Event::InlineHtml(html) => b.html(&html),
            Event::TaskListMarker(checked) => {
                let item = b.stack.iter_mut().rev().find(|frame| matches!(frame, Frame::Item(..)));
                if let Some(Frame::Item(_, checkbox)) = item {
                    *checkbox = Some(Checkbox {
                        checked,
                        written: markdown[range].to_string(),
                    });
                }
            }
            Event::Rule => {
                b.close_implicit_paragraph();
                b.add_block(Block::HorizontalRule);
            }
            _ => {}
        }
    }

    b.close_implicit_paragraph();
    match b.stack.into_iter().next() {
        Some(Frame::Root(blocks)) => blocks,
        _ => Vec::new(),
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use crate::model::{Block, CalloutKind, Inline, Marks, StatusColor, TaskItem};

    /// Directives Turtle does not know, holding what Markdown would read otherwise.
    const INLINE_DIRECTIVES: &[&str] = &[
        ":acme.demo[hi]{x=1}",
        ":acme.x{}",
        ":note[*a* `b ]c \\] <d> &amp;]",
        ":acme.y[**]{k=\"v * w\" f}",
        ":acme.z[a](not a link)",
        ":acme.w[`]",
    ];
    const BLOCK_DIRECTIVES: &[&str] = &[
        ":::acme.board{x=1}\n:::",
        ":::acme.board\n# not a heading\n\n- [ ] *odd*   spacing\n:::",
        "::::chart[x]\n:::inner\n```\n:::\n```\n:::\n::::",
    ];

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
                    match self.next(10) {
                        0 => return Inline::HardBreak,
                        1 => {
                            let page_id = ["a1b2", "c3d4", "e5"][self.next(3)].to_string();
                            return Inline::PageLink { page_id };
                        }
                        2 if self.next(3) == 0 => {
                            let source = INLINE_DIRECTIVES[self.next(INLINE_DIRECTIVES.len())].to_string();
                            return Inline::UnknownDirective { source };
                        }
                        3 => {
                            let person_id = ["m1", "m2"][self.next(2)].to_string();
                            return Inline::Mention { person_id };
                        }
                        4 if self.next(3) == 0 => {
                            let color = StatusColor::ALL[self.next(StatusColor::ALL.len())];
                            let text = if self.next(4) == 0 { String::new() } else { self.text() };
                            return Inline::Status { text, color };
                        }
                        _ => {}
                    }
                    if self.next(8) == 0 {
                        let alt = if self.next(3) == 0 { String::new() } else { self.text() };
                        return Inline::Image {
                            src: format!("/api/attachments/{}", self.text()),
                            alt,
                        };
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
            (0..len)
                .map(|_| {
                    // Block directives stand only at the top.
                    if depth == 0 && self.next(8) == 0 {
                        let source = BLOCK_DIRECTIVES[self.next(BLOCK_DIRECTIVES.len())].to_string();
                        return Block::UnknownDirective { source };
                    }
                    self.block(depth)
                })
                .collect()
        }

        pub fn block(&mut self, depth: usize) -> Block {
            // The first five kinds hold no other blocks, so they end the nesting.
            let kinds = if depth >= 3 { 5 } else { 10 };
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
                4 => {
                    let width = 1 + self.next(3);
                    Block::Table((0..1 + self.next(3)).map(|_| (0..width).map(|_| self.inlines()).collect()).collect())
                }
                8 => Block::Blockquote(self.blocks(depth + 1)),
                7 => Block::Callout {
                    kind: CalloutKind::ALL[self.next(CalloutKind::ALL.len())],
                    content: self.blocks(depth + 1),
                },
                5 => Block::BulletList((0..1 + self.next(3)).map(|_| self.blocks(depth + 1)).collect()),
                6 => Block::TaskList(
                    (0..1 + self.next(3))
                        .map(|_| TaskItem {
                            checked: self.next(2) == 0,
                            content: self.blocks(depth + 1),
                        })
                        .collect(),
                ),
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
    use crate::model::StatusColor;
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
    fn images_are_written_as_markdown_images_and_read_back() {
        let blocks = vec![para(vec![
            text("See "),
            Inline::Image {
                src: "/api/attachments/a1".into(),
                alt: "the [new] logo".into(),
            },
            text(" here."),
        ])];
        assert_eq!(
            to_markdown(&blocks),
            "See ![the \\[new\\] logo](</api/attachments/a1>) here.\n"
        );
        assert_round_trip(blocks);
        assert_eq!(
            from_markdown("**bold ![logo](x.png)** [![linked](y.png)](https://example.org)"),
            vec![para(vec![
                marked("bold ", |m| m.bold = true),
                Inline::Image { src: "x.png".into(), alt: "logo".into() },
                text(" "),
                Inline::Image { src: "y.png".into(), alt: "linked".into() },
            ])],
        );
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
    fn paragraph_lines_that_look_like_a_table_round_trip() {
        for lines in [["|a", "|-"], ["a|b", "-|-"], ["a | b", "--- | ---"]] {
            assert_round_trip(vec![para(vec![
                text("x"),
                Inline::HardBreak,
                text(lines[0]),
                Inline::HardBreak,
                text(lines[1]),
            ])]);
            assert_round_trip(vec![para(vec![
                text(lines[0]),
                Inline::HardBreak,
                text(lines[1]),
            ])]);
        }
        assert_round_trip(vec![para(vec![
            marked("a|b", |m| m.code = true),
            Inline::HardBreak,
            text("-|-"),
        ])]);
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

    fn page_link(id: &str) -> Inline {
        Inline::PageLink { page_id: id.into() }
    }

    #[test]
    fn a_page_link_is_a_turtle_address_shown_with_the_current_title() {
        let blocks = vec![para(vec![text("See "), page_link("a1b2"), text(" first.")])];
        let titles = |linked: Linked| (linked == Linked::Page("a1b2")).then(|| "Setup [draft] *v2*".to_string());

        let markdown = to_markdown_with_titles(&blocks, &titles);

        assert_eq!(markdown, "See [Setup \\[draft\\] \\*v2\\*](turtle:a1b2) first.\n");
        assert_eq!(from_markdown(&markdown), blocks);
    }

    #[test]
    fn a_page_link_whose_title_may_not_be_shown_has_no_text() {
        let blocks = vec![para(vec![page_link("a1b2")])];

        assert_eq!(to_markdown_with_titles(&blocks, &|_| None), "[](turtle:a1b2)\n");
        assert_eq!(to_markdown(&blocks), "[](turtle:a1b2)\n");
        assert_eq!(from_markdown("[](turtle:a1b2)"), blocks);
    }

    #[test]
    fn reading_a_turtle_address_makes_a_page_link_whatever_its_text_says() {
        assert_eq!(
            from_markdown("**[An *old* title](turtle:a1b2)** and <turtle:c3>"),
            vec![para(vec![page_link("a1b2"), text(" and "), page_link("c3")])]
        );
        // An address that names no Page stays an ordinary link.
        assert_eq!(
            from_markdown("[odd](turtle:a/b)"),
            vec![para(vec![marked("odd", |m| m.link = Some("turtle:a/b".into()))])]
        );
    }

    #[test]
    fn text_linked_to_a_turtle_address_is_a_page_link_once_normalised() {
        let linked = |s: &str, bold: bool| {
            marked(s, |m| {
                m.link = Some("turtle:a1b2".into());
                m.bold = bold
            })
        };
        assert_eq!(
            normalize(vec![para(vec![text("x"), linked("Set", true), linked("up", false), text("y")])]),
            vec![para(vec![text("x"), page_link("a1b2"), text("y")])]
        );
    }

    fn mention(id: &str) -> Inline {
        Inline::Mention { person_id: id.into() }
    }

    #[test]
    fn a_mention_is_a_member_address_shown_with_the_current_name() {
        let blocks = vec![para(vec![text("Ask "), mention("m1"), text(".")])];
        let names = |linked: Linked| (linked == Linked::Person("m1")).then(|| "Ada [the] *first*".to_string());

        let markdown = to_markdown_with_titles(&blocks, &names);

        assert_eq!(markdown, "Ask [@Ada \\[the\\] \\*first\\*](turtle:member/m1).\n");
        assert_eq!(from_markdown(&markdown), blocks);
    }

    #[test]
    fn reading_a_member_address_makes_a_mention_whatever_its_text_says() {
        assert_eq!(
            from_markdown("**[@Old *name*](turtle:member/m1)** and [](turtle:member/m2)"),
            vec![para(vec![mention("m1"), text(" and "), mention("m2")])]
        );
        // An address that names nobody stays an ordinary link.
        assert_eq!(
            from_markdown("[odd](turtle:member/a/b)"),
            vec![para(vec![marked("odd", |m| m.link = Some("turtle:member/a/b".into()))])]
        );
    }

    #[test]
    fn a_mention_of_nobody_known_is_an_at_sign() {
        let blocks = vec![para(vec![mention("m1")])];
        assert_eq!(to_markdown(&blocks), "[@](turtle:member/m1)\n");
        assert_eq!(from_markdown("[@](turtle:member/m1)"), blocks);
    }

    #[test]
    fn mentions_beside_formatting_and_in_table_cells_round_trip() {
        assert_round_trip(vec![
            para(vec![marked("bold", |m| m.bold = true), mention("m1"), marked("bold!", |m| m.bold = true)]),
            Block::Heading { level: 2, content: vec![mention("m1"), mention("m1"), page_link("m1")] },
            Block::Table(vec![vec![vec![text("Owner")]], vec![vec![mention("m1"), text(" and "), mention("m2")]]]),
        ]);
    }

    #[test]
    fn page_links_beside_formatting_round_trip() {
        assert_round_trip(vec![
            para(vec![marked("bold", |m| m.bold = true), page_link("a"), marked("bold!", |m| m.bold = true)]),
            Block::Heading { level: 2, content: vec![page_link("a"), page_link("a"), text(" #")] },
            para(vec![text("wow!"), page_link("b"), Inline::HardBreak, page_link("c")]),
        ]);
    }

    fn task(checked: bool, blocks: Vec<Block>) -> TaskItem {
        TaskItem { checked, content: blocks }
    }

    #[test]
    fn a_task_list_is_a_list_with_checkboxes() {
        let blocks = vec![Block::TaskList(vec![
            task(false, vec![para(vec![text("Write the notes")])]),
            task(true, vec![para(vec![text("Ship it")])]),
        ])];

        let markdown = to_markdown(&blocks);

        assert_eq!(markdown, "- [ ] Write the notes\n- [x] Ship it\n");
        assert_eq!(from_markdown(&markdown), blocks);
        assert_eq!(from_markdown("* [X] done\n* [ ] open"), vec![Block::TaskList(vec![
            task(true, vec![para(vec![text("done")])]),
            task(false, vec![para(vec![text("open")])]),
        ])]);
    }

    #[test]
    fn a_list_where_only_some_items_have_checkboxes_keeps_them_as_text() {
        assert_eq!(
            from_markdown("- [x] done\n- plain"),
            vec![Block::BulletList(vec![
                vec![para(vec![text("[x] done")])],
                vec![para(vec![text("plain")])],
            ])]
        );
        assert_eq!(
            from_markdown("1. [ ] first"),
            vec![Block::OrderedList { start: 1, items: vec![vec![para(vec![text("[ ] first")])]] }]
        );
    }

    #[test]
    fn awkward_task_items_round_trip() {
        assert_round_trip(vec![
            Block::TaskList(vec![
                task(true, vec![]),
                task(false, vec![
                    para(vec![text("parent")]),
                    Block::TaskList(vec![task(true, vec![para(vec![text("child")])])]),
                ]),
            ]),
            Block::TaskList(vec![task(false, vec![para(vec![text("[x] a second list")])])]),
            Block::BulletList(vec![vec![para(vec![text("[ ] not a task")])]]),
            Block::TaskList(vec![
                task(false, vec![Block::CodeBlock { language: None, text: "code first".into() }]),
                task(true, vec![para(vec![text("one")]), para(vec![text("two")])]),
            ]),
        ]);
    }

    #[test]
    fn a_callout_is_a_quote_whose_first_line_names_its_kind() {
        let blocks = vec![
            Block::Callout { kind: CalloutKind::Note, content: vec![para(vec![text("Read this first.")])] },
            Block::Callout { kind: CalloutKind::Tip, content: vec![] },
            Block::Callout {
                kind: CalloutKind::Warning,
                content: vec![Block::BulletList(vec![vec![para(vec![text("one")])]])],
            },
            Block::Callout { kind: CalloutKind::Danger, content: vec![para(vec![text("Hot.")])] },
        ];

        let markdown = to_markdown(&blocks);

        assert_eq!(
            markdown,
            "> [!NOTE]\n> Read this first.\n\n\
             > [!TIP]\n\n\
             > [!WARNING]\n>\n> - one\n\n\
             > [!DANGER]\n> Hot.\n"
        );
        assert_eq!(from_markdown(&markdown), blocks);
        assert_eq!(
            from_markdown("> [!note]  \n> quiet"),
            vec![Block::Callout { kind: CalloutKind::Note, content: vec![para(vec![text("quiet")])] }]
        );
    }

    #[test]
    fn a_quote_that_merely_starts_like_a_callout_stays_a_quote() {
        for markdown in ["> [!NOTE] same line", "> [!CAUTION]\n> unknown kind", "> \\[!NOTE\\]\n> escaped", "> *[!NOTE]*\n> x"] {
            let blocks = from_markdown(markdown);
            assert!(matches!(blocks.as_slice(), [Block::Blockquote(_)]), "{markdown:?} gave {blocks:?}");
        }
        assert_eq!(
            from_markdown("> [!NOTE] same line"),
            vec![Block::Blockquote(vec![para(vec![text("[!NOTE] same line")])])]
        );
        assert_round_trip(vec![
            Block::Blockquote(vec![para(vec![text("[!NOTE]")])]),
            Block::Blockquote(vec![para(vec![text("[!TIP]"), Inline::HardBreak, text("x")])]),
            Block::Blockquote(vec![Block::Callout { kind: CalloutKind::Tip, content: vec![] }]),
            Block::Callout {
                kind: CalloutKind::Note,
                content: vec![para(vec![text("[!WARNING]")]), Block::HorizontalRule],
            },
        ]);
    }

    fn cell(s: &str) -> Vec<Inline> {
        if s.is_empty() { vec![] } else { vec![text(s)] }
    }

    fn table(rows: &[&[&str]]) -> Block {
        Block::Table(rows.iter().map(|row| row.iter().map(|s| cell(s)).collect()).collect())
    }

    #[test]
    fn a_table_is_written_with_its_first_row_as_the_header() {
        let blocks = vec![table(&[&["Name", "Role"], &["Ada", "Writer"], &["Grace", ""]])];

        let markdown = to_markdown(&blocks);

        assert_eq!(markdown, "| Name | Role |\n| --- | --- |\n| Ada | Writer |\n| Grace |  |\n");
        assert_eq!(from_markdown(&markdown), blocks);
    }

    #[test]
    fn a_pipe_in_a_cell_is_escaped_so_it_stays_in_the_cell() {
        let blocks = vec![table(&[&["a | b", "x"], &["|", "\\|"]])];

        let markdown = to_markdown(&blocks);

        assert_eq!(markdown, "| a \\| b | x |\n| --- | --- |\n| \\| | \\\\\\| |\n");
        assert_eq!(from_markdown(&markdown), blocks);
        assert_round_trip(vec![Block::Table(vec![vec![
            vec![marked("a|b", |m| m.code = true)],
            vec![marked("a\\|b`|", |m| m.code = true)],
            vec![marked("or | link", |m| m.link = Some("https://x.org/?q=a|b".into()))],
            vec![Inline::Image { src: "/a|b.png".into(), alt: "c|d".into() }],
        ]])]);
    }

    #[test]
    fn a_cell_holds_formatting_links_and_line_breaks() {
        let row = vec![
            vec![marked("bold", |m| m.bold = true), text(" and "), page_link("a1b2")],
            vec![text("one"), Inline::HardBreak, text("two")],
        ];
        let blocks = vec![Block::Table(vec![row.clone(), row])];

        let markdown = to_markdown(&blocks);

        assert_eq!(
            markdown,
            "| **bold** and [](turtle:a1b2) | one<br>two |\n| --- | --- |\n| **bold** and [](turtle:a1b2) | one<br>two |\n"
        );
        assert_eq!(from_markdown(&markdown), blocks);
    }

    #[test]
    fn a_table_inside_a_list_or_a_callout_round_trips() {
        assert_round_trip(vec![
            Block::BulletList(vec![vec![para(vec![text("item")]), table(&[&["a"], &["b"]])]]),
            Block::Callout { kind: CalloutKind::Tip, content: vec![table(&[&["", ""]])] },
            table(&[&["only a header"]]),
            para(vec![text("a | b")]),
        ]);
    }

    #[test]
    fn rows_of_a_table_read_from_markdown_have_as_many_cells_as_its_header() {
        assert_eq!(from_markdown("| a | b |\n|:-|-:|\n| 1 |\n| 1 | 2 | 3 |"), vec![table(&[&["a", "b"], &["1", ""], &["1", "2"]])]);
    }

    fn unknown(source: &str) -> Inline {
        Inline::UnknownDirective { source: source.into() }
    }

    #[test]
    fn an_unknown_inline_directive_is_kept_byte_for_byte() {
        let markdown = "Before :acme.demo[hi *there*]{x=1 y=\"a b\"} after.\n";

        let blocks = from_markdown(markdown);

        assert_eq!(
            blocks,
            vec![para(vec![text("Before "), unknown(":acme.demo[hi *there*]{x=1 y=\"a b\"}"), text(" after.")])]
        );
        assert_eq!(to_markdown(&blocks), markdown);
    }

    #[test]
    fn an_unknown_block_directive_is_kept_byte_for_byte() {
        let board = ":::acme.board{columns=\"To do,Done\"}\n- [ ] *odd*   spacing\n\n```\n:::\n```\n::::acme.lane\n::::\n:::";
        let markdown = format!("Intro\n\n{board}\n\n:::chart\n:::\n\nAfter :acme.demo[hi]{{x=1}}.\n");

        let blocks = from_markdown(&markdown);

        assert_eq!(
            blocks,
            vec![
                para(vec![text("Intro")]),
                Block::UnknownDirective { source: board.into() },
                // An unnamespaced name Turtle does not know is kept all the same.
                Block::UnknownDirective { source: ":::chart\n:::".into() },
                para(vec![text("After "), unknown(":acme.demo[hi]{x=1}"), text(".")]),
            ]
        );
        assert_eq!(to_markdown(&blocks), markdown);
    }

    #[test]
    fn what_merely_looks_like_a_directive_stays_text() {
        for s in [":acme.x[a]{b=1}", ":::acme.x", "a:b", "see :x", ":x{", "\\:x[a]", "::x[a]{}", "time 10:30", ":x"] {
            assert_round_trip(vec![para(vec![text(s)])]);
            assert_round_trip(vec![para(vec![text("x"), Inline::HardBreak, text(s)])]);
        }
        assert_round_trip(vec![para(vec![text("a :b"), page_link("c")]), para(vec![text("::"), unknown(":x[y]")])]);
        assert_eq!(
            from_markdown("`:acme.x[a]{}` and <https://x.org/:a{b}> and ![:acme.x[a]{}](y.png)\n\n```\n:acme.x[a]{}\n```"),
            vec![
                para(vec![
                    marked(":acme.x[a]{}", |m| m.code = true),
                    text(" and "),
                    marked("https://x.org/:a{b}", |m| m.link = Some("https://x.org/:a{b}".into())),
                    text(" and "),
                    Inline::Image { src: "y.png".into(), alt: ":acme.x[a]{}".into() },
                ]),
                Block::CodeBlock { language: None, text: ":acme.x[a]{}".into() },
            ]
        );
        // Without a closing line, a block directive is the text it says.
        assert_eq!(from_markdown(":::acme.x\nnever closed"), vec![para(vec![text(":::acme.x never closed")])]);
    }

    #[test]
    fn a_block_directive_away_from_the_top_is_kept_as_its_text() {
        assert_eq!(
            normalize(vec![Block::Blockquote(vec![Block::UnknownDirective { source: ":::acme.x\none\n:::".into() }])]),
            vec![Block::Blockquote(vec![para(vec![
                text(":::acme.x"),
                Inline::HardBreak,
                text("one"),
                Inline::HardBreak,
                text(":::"),
            ])])]
        );
    }

    fn status(text: &str, color: StatusColor) -> Inline {
        Inline::Status { text: text.into(), color }
    }

    #[test]
    fn a_status_is_read_and_written_as_a_directive() {
        let markdown = "Build :status[On track]{color=green} today.\n";

        let blocks = from_markdown(markdown);

        assert_eq!(blocks, vec![para(vec![text("Build "), status("On track", StatusColor::Green), text(" today.")])]);
        assert_eq!(to_markdown(&blocks), markdown);
    }

    #[test]
    fn a_status_without_a_known_colour_is_grey() {
        assert_eq!(from_markdown(":status[Idle]"), vec![para(vec![status("Idle", StatusColor::Grey)])]);
        assert_eq!(from_markdown(":status[Idle]{color=orange}"), vec![para(vec![status("Idle", StatusColor::Grey)])]);
        assert_eq!(to_markdown(&[para(vec![status("Idle", StatusColor::Grey)])]), ":status[Idle]{color=grey}\n");
        for color in ["grey", "blue", "green", "yellow", "red", "purple"] {
            let markdown = format!(":status[x]{{color={color}}}\n");
            assert_eq!(to_markdown(&from_markdown(&markdown)), markdown);
        }
    }

    #[test]
    fn a_status_stands_wherever_running_text_does() {
        let markdown = "# Plan :status[Draft]{color=blue}\n\n| Task | State |\n| --- | --- |\n| Build | **:status[Late]{color=red}** |\n";

        let blocks = from_markdown(markdown);

        assert_eq!(
            blocks,
            vec![
                Block::Heading { level: 1, content: vec![text("Plan "), status("Draft", StatusColor::Blue)] },
                Block::Table(vec![
                    vec![vec![text("Task")], vec![text("State")]],
                    vec![vec![text("Build")], vec![status("Late", StatusColor::Red)]],
                ]),
            ]
        );
    }

    #[test]
    fn a_status_keeps_its_text_as_typed() {
        for typed in ["on Track", "a [b] c\\d", "*not bold* `x`", "50% & <ok>", "", "  spaced  "] {
            assert_round_trip(vec![para(vec![text("x "), status(typed, StatusColor::Purple)])]);
            assert_eq!(normalize(vec![para(vec![status(typed, StatusColor::Purple)])]), vec![para(vec![status(typed, StatusColor::Purple)])]);
        }
        // A directive in running text stays on one line and holds no `|`, which would end a table cell.
        assert_eq!(
            normalize(vec![para(vec![status("a|b\nc", StatusColor::Green)])]),
            vec![para(vec![status("a/b c", StatusColor::Green)])]
        );
        // Another's directive of the same name is not a Status.
        assert_eq!(from_markdown(":acme.status[x]"), vec![para(vec![unknown(":acme.status[x]")])]);
    }

    const SEEDS: u64 = 20_000;

    #[test]
    fn random_content_round_trips() {
        let mut failures: Vec<(u64, String)> = Vec::new();
        for seed in 0..SEEDS {
            let mut rng = Rng(seed);
            let blocks = normalize(rng.blocks(0));
            // Page links shown with a title must read back the same as without one.
            let title = format!("t `x` ](y) <br> *z*\\ !{}", rng.text());
            for markdown in [to_markdown(&blocks), to_markdown_with_titles(&blocks, &|_| Some(title.clone()))] {
                if from_markdown(&markdown) != blocks {
                    failures.push((seed, markdown));
                }
            }
        }
        failures.sort_by_key(|(_, markdown)| markdown.len());
        for (seed, markdown) in failures.iter().take(6) {
            eprintln!("seed {seed}\n--- markdown ---\n{markdown}\n---");
        }
        assert!(failures.is_empty(), "{} of {SEEDS} failed", failures.len());
    }
}
