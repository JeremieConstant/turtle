//! A Page's content in Confluence's storage format, made Turtle content.

use super::coverage::Tally;
use super::dom::{self, Element, Node};
use crate::model::{Block, CalloutKind, Inline, Marks, StatusColor, TaskItem};
use crate::ops::import::Target;
use std::collections::HashMap;

/// What the content of a Page can point at in the space it comes from.
#[derive(Debug, Default)]
pub(super) struct Lookup {
    /// The space's key, such as `KB`.
    pub key: String,
    /// The key of each page that becomes a Page, by its ID.
    pub pages: HashMap<String, String>,
    /// The ID of each page or folder, by its title, which is unique in a space.
    pub titles: HashMap<String, String>,
    /// The ID of each attachment, by the ID of its page and its file name.
    pub attachments: HashMap<(String, String), String>,
    /// The key of each attachment, by its ID.
    pub attachment_keys: HashMap<String, String>,
    /// The name of each person, by their key.
    pub people: HashMap<String, String>,
    /// The e-mail address of each person whose export tells it, by their key.
    pub emails: HashMap<String, String>,
    /// The ID of each Member or Guest the importer may see, by their e-mail
    /// address in lower case.
    pub people_in_turtle: HashMap<String, String>,
}

/// A Page's content, the links in it and what was found in it.
#[derive(Debug, Default)]
pub(super) struct Converted {
    pub content: Vec<Block>,
    /// What each link address in the content points to, as the Bundle
    /// takes it.
    pub links: HashMap<String, Target>,
    pub found: Tally,
}

/// The address a link to the page with this ID is written with until the
/// import points it at the Page.
pub(super) fn page_address(id: &str) -> String {
    format!("confluence:page/{id}")
}

/// The address an image or link to the attachment with this ID is written
/// with until the import points it at the Attachment.
pub(super) fn attachment_address(id: &str) -> String {
    format!("confluence:attachment/{id}")
}

/// Converts the content of the page with ID `page_id`; nothing when it is not
/// well-formed.
pub(super) fn convert(storage: &str, page_id: &str, lookup: &Lookup) -> Option<Converted> {
    let root = dom::parse(storage)?;
    let mut converter = Converter { lookup, page_id, links: HashMap::new(), found: Tally::default(), in_text: 0 };
    let content = converter.blocks(&root.children);
    Some(Converted { content, links: converter.links, found: converter.found })
}

struct Converter<'a> {
    lookup: &'a Lookup,
    page_id: &'a str,
    links: HashMap<String, Target>,
    found: Tally,
    /// How many paragraphs or headings the element being read is in: a
    /// macro outside any stands on its own.
    in_text: usize,
}

/// Blocks being made, with the running text of the paragraph not yet ended.
#[derive(Default)]
struct Out {
    blocks: Vec<Block>,
    line: Vec<Inline>,
}

impl Out {
    fn end_paragraph(&mut self) {
        if self.line.iter().any(|inline| !matches!(inline, Inline::Text { text, .. } if text.trim().is_empty())) {
            self.blocks.push(Block::Paragraph(std::mem::take(&mut self.line)));
        }
        self.line.clear();
    }

    fn push_block(&mut self, block: Block) {
        self.end_paragraph();
        self.blocks.push(block);
    }

    fn push_blocks(&mut self, blocks: Vec<Block>) {
        blocks.into_iter().for_each(|block| self.push_block(block));
    }

    fn text(&mut self, text: &str, marks: &Marks) {
        // Text between blocks, such as a line break between two paragraphs, is not content.
        if self.line.is_empty() && text.trim().is_empty() {
            return;
        }
        self.line.push(Inline::Text { text: collapse_spaces(text), marks: marks.clone() });
    }

    fn finish(mut self) -> Vec<Block> {
        self.end_paragraph();
        self.blocks
    }
}

/// Runs of white space become one space, as a browser shows them.
fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() && c != '\u{a0}' {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

impl Converter<'_> {
    fn blocks(&mut self, nodes: &[Node]) -> Vec<Block> {
        let mut out = Out::default();
        self.walk(nodes, &Marks::default(), &mut out);
        out.finish()
    }

    fn walk(&mut self, nodes: &[Node], marks: &Marks, out: &mut Out) {
        for node in nodes {
            match node {
                Node::Text(text) => out.text(text, marks),
                Node::Element(element) => self.element(element, marks, out),
            }
        }
    }

    fn element(&mut self, element: &Element, marks: &Marks, out: &mut Out) {
        let children = &element.children;
        match element.name.as_str() {
            "p" => {
                self.found.add("paragraph");
                out.end_paragraph();
                self.in_text += 1;
                self.walk(children, marks, out);
                self.in_text -= 1;
                out.end_paragraph();
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.found.add("heading");
                let level = element.name[1..].parse().unwrap_or(1);
                self.in_text += 1;
                let mut blocks = self.blocks(children).into_iter();
                self.in_text -= 1;
                let content = match blocks.next() {
                    Some(Block::Paragraph(content)) => content,
                    Some(other) => {
                        out.push_block(other);
                        Vec::new()
                    }
                    None => Vec::new(),
                };
                if !content.is_empty() {
                    out.push_block(Block::Heading { level, content });
                }
                out.push_blocks(blocks.collect());
            }
            "strong" | "b" => self.walk(children, &Marks { bold: true, ..marks.clone() }, out),
            "em" | "i" => self.walk(children, &Marks { italic: true, ..marks.clone() }, out),
            "s" | "del" | "strike" => self.walk(children, &Marks { strike: true, ..marks.clone() }, out),
            "code" => self.walk(children, &Marks { code: true, ..marks.clone() }, out),
            "u" => {
                self.found.add("underline");
                self.walk(children, marks, out);
            }
            "sub" | "sup" => {
                self.found.add("subscript or superscript");
                self.walk(children, marks, out);
            }
            "span" => {
                if element.attribute("style").is_some_and(|style| style.contains("color")) {
                    self.found.add("coloured text");
                }
                self.walk(children, marks, out);
            }
            "br" => out.line.push(Inline::HardBreak),
            "a" => self.link(element, marks, out),
            "ul" | "ol" => {
                let items: Vec<Vec<Block>> = element.elements().filter(|e| e.name == "li").map(|li| self.blocks(&li.children)).collect();
                if element.name == "ul" {
                    self.found.add("bullet list");
                    out.push_block(Block::BulletList(items));
                } else {
                    self.found.add("numbered list");
                    let start = element.attribute("start").and_then(|start| start.parse().ok()).unwrap_or(1);
                    out.push_block(Block::OrderedList { start, items });
                }
            }
            "ac:task-list" => {
                self.found.add("task list");
                let items = element
                    .elements()
                    .filter(|e| e.name == "ac:task")
                    .map(|task| TaskItem {
                        checked: task.child("ac:task-status").is_some_and(|status| status.text().trim() == "complete"),
                        content: task.child("ac:task-body").map(|body| self.blocks(&body.children)).unwrap_or_default(),
                    })
                    .collect();
                out.push_block(Block::TaskList(items));
            }
            "table" => {
                let table = self.table(element);
                out.push_block(table);
            }
            "pre" => {
                self.found.add("preformatted text");
                out.push_block(Block::CodeBlock { language: None, text: element.text() });
            }
            "blockquote" => {
                self.found.add("quote");
                let content = self.blocks(children);
                out.push_block(Block::Blockquote(content));
            }
            "hr" => {
                self.found.add("horizontal rule");
                out.push_block(Block::HorizontalRule);
            }
            "ac:layout" => {
                self.found.add("layout");
                self.walk(children, marks, out);
            }
            "ac:structured-macro" | "ac:macro" => self.structured(element, marks, out),
            "ac:adf-extension" => self.extension(element, marks, out),
            "ac:image" => self.image(element, marks, out),
            "img" => {
                self.found.add("image from the web");
                let src = element.attribute("src").unwrap_or_default().to_string();
                out.line.push(Inline::Image { src, alt: element.attribute("alt").unwrap_or_default().to_string() });
            }
            "ac:link" => self.confluence_link(element, marks, out),
            "ac:emoticon" => {
                self.found.add("emoticon");
                let emoji = element
                    .attribute("ac:emoji-fallback")
                    .map(str::to_string)
                    .unwrap_or_else(|| emoji_named(element.attribute("ac:name").unwrap_or_default()));
                out.text(&emoji, marks);
            }
            "time" => {
                self.found.add("date");
                out.text(element.attribute("datetime").unwrap_or_default(), marks);
            }
            "ac:inline-comment-marker" => {
                self.found.add("inline comment");
                self.walk(children, marks, out);
            }
            "ac:placeholder" => self.found.add("instructional text"),
            // Parts of what holds them, read there, or wrappers with nothing of their own.
            "div" | "section" | "tbody" | "thead" | "tfoot" | "ac:layout-section" | "ac:layout-cell" | "ac:rich-text-body"
            | "ac:link-body" | "ac:adf-fallback" => self.walk(children, marks, out),
            "colgroup" | "col" | "ac:parameter" | "ac:caption" => {}
            other => {
                self.found.add(&format!("{other} element"));
                self.walk(children, marks, out);
            }
        }
    }

    /// An HTML link, which becomes a Page link when it points at a page of
    /// the space.
    fn link(&mut self, element: &Element, marks: &Marks, out: &mut Out) {
        let href = element.attribute("href").unwrap_or_default().to_string();
        match linked_page_id(&href).filter(|id| self.lookup.pages.contains_key(id)) {
            Some(id) => {
                self.found.add(self.page_link_kind(&id, &element.text()));
                self.links.insert(href.clone(), Target::Entry(self.lookup.pages[&id].clone()));
            }
            None => self.found.add("link"),
        }
        let linked = Marks { link: (!href.is_empty()).then_some(href), ..marks.clone() };
        self.walk(&element.children, &linked, out);
    }

    /// A link in Confluence's own form, to a page, an attachment, a person
    /// or something else in Confluence.
    fn confluence_link(&mut self, element: &Element, marks: &Marks, out: &mut Out) {
        let body = element.child("ac:link-body").or_else(|| element.child("ac:plain-text-link-body"));
        let target = element.elements().find(|e| e.name.starts_with("ri:"));
        let (address, text) = match target {
            Some(page) if page.name == "ri:page" => {
                let title = page.attribute("ri:content-title").unwrap_or_default();
                let in_space = page.attribute("ri:space-key").is_none_or(|key| key == self.lookup.key);
                let id = self.lookup.titles.get(title).filter(|_| in_space);
                match id.and_then(|id| Some((id, self.lookup.pages.get(id)?))) {
                    Some((id, key)) => {
                        let text = body.map(Element::text).unwrap_or_default();
                        self.found.add(self.page_link_kind(id, &text));
                        let address = page_address(id);
                        self.links.insert(address.clone(), Target::Entry(key.clone()));
                        (Some(address), title.to_string())
                    }
                    None if id.is_some() => {
                        self.found.add("link to a folder");
                        (None, title.to_string())
                    }
                    None => {
                        self.found.add("link to a page outside the space");
                        (None, title.to_string())
                    }
                }
            }
            Some(attachment) if attachment.name == "ri:attachment" => {
                let file = attachment.attribute("ri:filename").unwrap_or_default();
                (self.attachment_link(attachment), file.to_string())
            }
            Some(user) if user.name == "ri:user" => {
                let key = user.attribute("ri:account-id").or_else(|| user.attribute("ri:userkey")).unwrap_or_default();
                let name = self.lookup.people.get(key).map(String::as_str);
                // The person's e-mail address, where the export tells it or,
                // as Confluence Cloud does, names them by it, finds the
                // Member or Guest with the same one.
                let email = self.lookup.emails.get(key).map(String::as_str).or(name.filter(|name| name.contains('@')));
                if let Some(person_id) = email.and_then(|email| self.lookup.people_in_turtle.get(&email.to_lowercase())) {
                    self.found.add("mention of a Member or Guest");
                    out.line.push(Inline::Mention { person_id: person_id.clone() });
                    return;
                }
                self.found.add("mention");
                // An address is not written into the Page, only what comes before its @.
                let shown = name.map(|name| name.split('@').next().unwrap_or(name)).filter(|name| !name.is_empty());
                (None, format!("@{}", shown.unwrap_or("someone in Confluence")))
            }
            Some(url) if url.name == "ri:url" => {
                self.found.add("link");
                let value = url.attribute("ri:value").unwrap_or_default();
                (Some(value.to_string()), value.to_string())
            }
            Some(other) => {
                self.found.add("link to something else in Confluence");
                let title = ["ri:content-title", "ri:space-key", "ri:filename"].into_iter().find_map(|name| other.attribute(name));
                (None, title.unwrap_or_default().to_string())
            }
            None => {
                self.found.add("link to something else in Confluence");
                (None, element.attribute("ac:anchor").unwrap_or_default().to_string())
            }
        };
        let linked = Marks { link: address, ..marks.clone() };
        match body {
            Some(body) if !body.text().trim().is_empty() => self.walk(&body.children, &linked, out),
            _ => out.text(&text, &linked),
        }
    }

    /// What a link to the page with this ID is counted as: a link whose
    /// text is not the page's title loses its text to the title.
    fn page_link_kind(&self, id: &str, text: &str) -> &'static str {
        let text = text.trim();
        let titled = self.lookup.titles.get(text).is_some_and(|titled| titled == id);
        if text.is_empty() || titled { "link to a page" } else { "link to a page with its own text" }
    }

    /// The address of the attachment a `ri:attachment` names, counting it
    /// as linked to or as missing.
    fn attachment_link(&mut self, attachment: &Element) -> Option<String> {
        match self.attachment_id(attachment) {
            Some(id) => {
                self.found.add("link to an attachment");
                Some(self.attachment(&id))
            }
            None => {
                self.found.add("missing attachment");
                None
            }
        }
    }

    /// The ID of the attachment a `ri:attachment` names: on the page it
    /// names, or else on this one.
    fn attachment_id(&self, attachment: &Element) -> Option<String> {
        let file = attachment.attribute("ri:filename")?;
        let page_id = match attachment.child("ri:page") {
            Some(page) => self.lookup.titles.get(page.attribute("ri:content-title")?)?.as_str(),
            None => self.page_id,
        };
        self.lookup.attachments.get(&(page_id.to_string(), file.to_string())).cloned()
    }

    /// The address of the attachment with this ID, which the import points
    /// at its Attachment.
    fn attachment(&mut self, id: &str) -> String {
        let address = attachment_address(id);
        if let Some(key) = self.lookup.attachment_keys.get(id) {
            self.links.insert(address.clone(), Target::Attachment(key.clone()));
        }
        address
    }

    fn image(&mut self, element: &Element, marks: &Marks, out: &mut Out) {
        let alt = element.attribute("ac:alt").unwrap_or_default().to_string();
        if let Some(url) = element.child("ri:url") {
            self.found.add("image from the web");
            out.line.push(Inline::Image { src: url.attribute("ri:value").unwrap_or_default().to_string(), alt });
        } else if let Some(id) = element.child("ri:attachment").and_then(|attachment| self.attachment_id(attachment)) {
            self.found.add("image");
            let src = self.attachment(&id);
            out.line.push(Inline::Image { src, alt });
        } else {
            self.found.add("missing attachment");
            let file = element.child("ri:attachment").and_then(|attachment| attachment.attribute("ri:filename")).unwrap_or_default();
            out.text(&format!("Not imported: {file}"), &Marks { italic: true, ..marks.clone() });
        }
        if let Some(caption) = element.child("ac:caption") {
            self.found.add("image caption");
            out.end_paragraph();
            self.walk(&caption.children, &Marks { italic: true, ..marks.clone() }, out);
            out.end_paragraph();
        }
    }

    fn table(&mut self, table: &Element) -> Block {
        let mut rows_of_cells = Vec::new();
        fn collect_rows<'e>(element: &'e Element, rows: &mut Vec<&'e Element>) {
            for child in element.elements() {
                match child.name.as_str() {
                    "tr" => rows.push(child),
                    "thead" | "tbody" | "tfoot" => collect_rows(child, rows),
                    _ => {}
                }
            }
        }
        collect_rows(table, &mut rows_of_cells);
        let header = rows_of_cells.first().is_some_and(|row| row.elements().all(|cell| cell.name == "th"));
        self.found.add(if header { "table" } else { "table without a header row" });
        // The rows a merged cell still reaches below, by column.
        let mut reaching: HashMap<usize, u32> = HashMap::new();
        let mut rows = Vec::new();
        for tr in rows_of_cells {
            let mut row = Vec::new();
            let fill = |row: &mut Vec<Vec<Inline>>, reaching: &mut HashMap<usize, u32>| {
                while let Some(left) = reaching.get_mut(&row.len()).filter(|left| **left > 0) {
                    *left -= 1;
                    row.push(Vec::new());
                }
            };
            for cell in tr.elements().filter(|cell| cell.name == "td" || cell.name == "th") {
                fill(&mut row, &mut reaching);
                let span = |name| cell.attribute(name).and_then(|n| n.parse::<usize>().ok()).unwrap_or(1).max(1);
                let (columns, rows_down) = (span("colspan"), span("rowspan"));
                if columns > 1 || rows_down > 1 {
                    self.found.add("merged table cell");
                }
                let content = self.cell(cell);
                for column in 0..columns {
                    if rows_down > 1 {
                        reaching.insert(row.len(), rows_down as u32 - 1);
                    }
                    row.push(if column == 0 { content.clone() } else { Vec::new() });
                }
            }
            fill(&mut row, &mut reaching);
            rows.push(row);
        }
        Block::Table(rows)
    }

    /// What a table cell holds, as inline content: a cell holding more than
    /// paragraphs has it line by line.
    fn cell(&mut self, cell: &Element) -> Vec<Inline> {
        let blocks = self.blocks(&cell.children);
        if blocks.iter().any(|block| !matches!(block, Block::Paragraph(_))) {
            self.found.add("table cell with more than text");
        }
        let mut lines = Vec::new();
        lines_of(&blocks, "", &mut lines);
        let mut out = Vec::new();
        for line in lines {
            if !out.is_empty() {
                out.push(Inline::HardBreak);
            }
            out.extend(line);
        }
        out
    }

    /// A Confluence macro, `ac:structured-macro`: one the import knows
    /// becomes its Turtle counterpart, any other a placeholder.
    fn structured(&mut self, element: &Element, marks: &Marks, out: &mut Out) {
        let name = element.attribute("ac:name").unwrap_or_default();
        let parameter = |key: &str| {
            parameter_of(element, key).map(|e| e.text().trim().to_string()).filter(|text| !text.is_empty())
        };
        let rich = element.child("ac:rich-text-body");
        let plain = element.child("ac:plain-text-body").map(Element::text);
        let callout = match name {
            "info" => Some(CalloutKind::Note),
            "note" => Some(CalloutKind::Warning),
            "tip" => Some(CalloutKind::Tip),
            "warning" => Some(CalloutKind::Danger),
            "panel" => Some(CalloutKind::Note),
            _ => None,
        };
        self.found.add(&format!("{name} macro"));
        if let Some(kind) = callout {
            let mut content = title_paragraph(parameter("title"));
            content.extend(rich.map(|body| self.blocks(&body.children)).unwrap_or_default());
            out.push_block(Block::Callout { kind, content });
            return;
        }
        match name {
            "code" | "noformat" => {
                out.push_blocks(title_paragraph(parameter("title")));
                let language = if name == "code" { parameter("language") } else { None };
                out.push_block(Block::CodeBlock { language, text: plain.unwrap_or_default() });
            }
            "status" => {
                let color = StatusColor::named(&parameter("colour").unwrap_or_default().to_lowercase());
                out.line.push(Inline::Status { text: parameter("title").unwrap_or_default(), color });
            }
            "expand" => {
                out.push_blocks(title_paragraph(parameter("title")));
                if let Some(body) = rich {
                    let content = self.blocks(&body.children);
                    out.push_blocks(content);
                }
            }
            "view-file" | "viewpdf" | "viewdoc" | "viewxls" | "viewppt" => {
                let attachment = parameter_of(element, "name").and_then(|parameter| parameter.child("ri:attachment"));
                let file = attachment.and_then(|a| a.attribute("ri:filename")).unwrap_or_default().to_string();
                let address = attachment.and_then(|attachment| self.attachment_link(attachment));
                out.text(&file, &Marks { link: address, ..marks.clone() });
            }
            "excerpt" | "details" => {
                if let Some(body) = rich {
                    self.walk(&body.children, marks, out);
                }
            }
            "anchor" => {}
            _ => {
                if self.in_text == 0 {
                    out.push_block(Block::Paragraph(vec![placeholder(name, element)]));
                } else {
                    out.line.push(placeholder(name, element));
                }
                if let Some(body) = rich {
                    out.end_paragraph();
                    self.walk(&body.children, marks, out);
                } else if let Some(text) = plain.filter(|text| !text.trim().is_empty()) {
                    out.push_block(Block::CodeBlock { language: None, text });
                }
            }
        }
    }

    /// An element of Confluence Cloud's own, of which only panels are known.
    fn extension(&mut self, element: &Element, marks: &Marks, out: &mut Out) {
        let node = element.child("ac:adf-node");
        match node.and_then(|node| node.attribute("type")) {
            Some("panel") => {
                self.found.add("panel");
                let node = node.expect("a panel is a node");
                let kind = node
                    .elements()
                    .find(|e| e.name == "ac:adf-attribute" && e.attribute("key") == Some("panel-type"))
                    .map(|e| match e.text().trim() {
                        "success" => CalloutKind::Tip,
                        "warning" => CalloutKind::Warning,
                        "error" => CalloutKind::Danger,
                        _ => CalloutKind::Note,
                    })
                    .unwrap_or(CalloutKind::Note);
                let content = node.child("ac:adf-content").map(|c| self.blocks(&c.children)).unwrap_or_default();
                out.push_block(Block::Callout { kind, content });
            }
            other => {
                self.found.add(&format!("{} element", other.unwrap_or("ac:adf-extension")));
                if let Some(fallback) = element.child("ac:adf-fallback") {
                    self.walk(&fallback.children, marks, out);
                }
            }
        }
    }
}

/// The parameter of a macro with this name.
fn parameter_of<'e>(element: &'e Element, name: &str) -> Option<&'e Element> {
    element.elements().find(|e| e.name == "ac:parameter" && e.attribute("ac:name") == Some(name))
}

/// A macro's title as a paragraph in bold, if it has one.
fn title_paragraph(title: Option<String>) -> Vec<Block> {
    title
        .map(|title| Block::Paragraph(vec![Inline::Text { text: title, marks: Marks { bold: true, ..Marks::default() } }]))
        .into_iter()
        .collect()
}

/// A placeholder for a macro Turtle does not know: a directive naming it
/// with its simple parameters (ADR-0013), which the editor shows locked.
fn placeholder(name: &str, element: &Element) -> Inline {
    let clean = |text: &str| -> String {
        let cleaned: String = text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
        if cleaned.starts_with(|c: char| c.is_ascii_alphabetic()) { cleaned } else { format!("m{cleaned}") }
    };
    let attributes = element
        .elements()
        .filter(|e| e.name == "ac:parameter" && e.elements().next().is_none())
        .filter_map(|e| {
            let key = clean(e.attribute("ac:name")?);
            let value = e.text().trim().replace('|', "/");
            (!value.is_empty()).then_some((key, value))
        })
        .collect();
    let directive = crate::directive::Directive {
        name: format!("confluence.{}", clean(name)),
        label: Some(name.replace('|', "/")),
        attributes,
    };
    Inline::UnknownDirective { source: directive.to_inline_markdown() }
}

/// Blocks as lines of inline content, each list item a line of its own
/// starting with its marker after `indent`.
fn lines_of(blocks: &[Block], indent: &str, lines: &mut Vec<Vec<Inline>>) {
    let plain = |text: String| Inline::Text { text, marks: Marks::default() };
    for block in blocks {
        match block {
            Block::Paragraph(content) | Block::Heading { content, .. } => lines.push(content.clone()),
            Block::BulletList(items) => {
                for item in items {
                    item_lines(item, &format!("{indent}• "), lines);
                }
            }
            Block::OrderedList { start, items } => {
                for (n, item) in items.iter().enumerate() {
                    item_lines(item, &format!("{indent}{}. ", start + n as u64), lines);
                }
            }
            Block::TaskList(items) => {
                for item in items {
                    item_lines(&item.content, &format!("{indent}{} ", if item.checked { "☑" } else { "☐" }), lines);
                }
            }
            Block::Blockquote(content) | Block::Callout { content, .. } => lines_of(content, indent, lines),
            Block::CodeBlock { text, .. } => {
                for line in text.lines() {
                    lines.push(vec![Inline::Text { text: line.to_string(), marks: Marks { code: true, ..Marks::default() } }]);
                }
            }
            Block::Table(rows) => {
                for row in rows {
                    lines.push(vec![plain(row.iter().map(|cell| crate::model::plain_text(&[Block::Paragraph(cell.clone())])).collect::<Vec<_>>().join(" / "))]);
                }
            }
            Block::HorizontalRule => lines.push(vec![plain("—".into())]),
            Block::UnknownDirective { source } => lines.push(vec![plain(source.clone())]),
        }
    }
}

/// A list item's lines, the first starting with its marker.
fn item_lines(item: &[Block], marker: &str, lines: &mut Vec<Vec<Inline>>) {
    let first = lines.len();
    let nested_indent: String = " ".repeat(marker.chars().count());
    lines_of(item, &nested_indent, lines);
    match lines.get_mut(first) {
        Some(line) => line.insert(0, Inline::Text { text: marker.to_string(), marks: Marks::default() }),
        None => lines.push(vec![Inline::Text { text: marker.trim_end().to_string(), marks: Marks::default() }]),
    }
}

/// The ID of the Confluence page a web address points at, if it is one:
/// `…/pages/<ID>…`, `…?pageId=<ID>` or a short link `…/x/<code>`.
fn linked_page_id(href: &str) -> Option<String> {
    let digits = |text: &str| -> Option<String> {
        let id: String = text.chars().take_while(char::is_ascii_digit).collect();
        (!id.is_empty()).then_some(id)
    };
    if let Some((_, rest)) = href.split_once("/pages/") {
        return digits(rest);
    }
    if let Some((_, rest)) = href.split_once("pageId=") {
        return digits(rest);
    }
    let (_, code) = href.split_once("/x/")?;
    let code: String = code.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).collect();
    short_link_id(&code)
}

/// The page ID a Confluence short link stands for: the ID's bytes, least
/// significant first, in base64 with `-` and `_` for `/` and `+`, without
/// trailing `A`s.
fn short_link_id(code: &str) -> Option<String> {
    use base64::Engine;
    if code.is_empty() || code.len() > 11 {
        return None;
    }
    let mut padded: String = code.chars().map(|c| match c { '-' => '/', '_' => '+', c => c }).collect();
    while padded.len() < 12 {
        padded.push('A');
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(padded).ok()?;
    let mut id: u64 = 0;
    for (i, byte) in bytes.iter().take(8).enumerate() {
        id |= u64::from(*byte) << (8 * i);
    }
    (id > 0).then(|| id.to_string())
}

/// The emoji for an emoticon of Confluence Data Center, which names them.
fn emoji_named(name: &str) -> String {
    let emoji = match name {
        "smile" => "🙂",
        "sad" => "🙁",
        "cheeky" => "😛",
        "laugh" => "😀",
        "wink" => "😉",
        "thumbs-up" => "👍",
        "thumbs-down" => "👎",
        "information" => "ℹ️",
        "tick" => "✅",
        "cross" => "❌",
        "warning" => "⚠️",
        "plus" => "➕",
        "minus" => "➖",
        "question" => "❓",
        "light-on" => "💡",
        "light-off" => "💡",
        "yellow-star" | "red-star" | "green-star" | "blue-star" => "⭐",
        "heart" => "❤️",
        "broken-heart" => "💔",
        other => return format!(":{other}:"),
    };
    emoji.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A space with the page "Home" (ID 1), on which the attachment
    /// "chart.png" (ID 10), the page "Runbook" (ID 2) and the folder
    /// "Guides" (ID 3).
    fn space() -> Lookup {
        let mut space = Lookup { key: "KB".into(), ..Lookup::default() };
        for (id, title) in [("1", "Home"), ("2", "Runbook")] {
            space.pages.insert(id.into(), format!("page:{id}"));
            space.titles.insert(title.into(), id.into());
        }
        space.titles.insert("Guides".into(), "3".into());
        space.attachments.insert(("1".into(), "chart.png".into()), "10".into());
        space.attachment_keys.insert("10".into(), "attachment:10".into());
        space.people.insert("712020:abc".into(), "Grace Hopper".into());
        space
    }

    /// The content of the page "Home" as Markdown, with what was found.
    fn converted(storage: &str) -> (String, Vec<(String, u32)>) {
        let converted = convert(storage, "1", &space()).expect("well-formed");
        let markdown = crate::markdown::to_markdown(&crate::model::normalize(converted.content));
        (markdown, converted.found.0.into_iter().collect())
    }

    fn markdown(storage: &str) -> String {
        converted(storage).0
    }

    fn found(storage: &str) -> Vec<(String, u32)> {
        converted(storage).1
    }

    fn tally(entries: &[(&str, u32)]) -> Vec<(String, u32)> {
        entries.iter().map(|(name, count)| (name.to_string(), *count)).collect()
    }

    #[test]
    fn paragraphs_keep_their_text_with_html_entities_read() {
        assert_eq!(
            converted("<p>Gr&uuml;&szlig;e &amp; mehr&nbsp;hier</p><p>Zweiter&#160;Absatz</p>"),
            ("Grüße \\& mehr\u{a0}hier\n\nZweiter\u{a0}Absatz\n".to_string(), tally(&[("paragraph", 2)]))
        );
    }

    #[test]
    fn headings_formatting_and_line_breaks_come_across() {
        assert_eq!(
            converted("<h2>Setup</h2><p><strong>Bold</strong>, <em>italic</em>, <s>gone</s> and <code>cmd</code><br />next line</p>"),
            (
                "## Setup\n\n**Bold**, *italic*, ~~gone~~ and `cmd`\\\nnext line\n".to_string(),
                tally(&[("heading", 1), ("paragraph", 1)])
            )
        );
    }

    #[test]
    fn lists_and_task_lists_keep_their_nesting_and_checks() {
        let storage = "<ul><li><p>One</p><ol start=\"3\"><li><p>Three</p></li></ol></li></ul>\
            <ac:task-list><ac:task><ac:task-id>1</ac:task-id><ac:task-status>complete</ac:task-status><ac:task-body>Done</ac:task-body></ac:task>\
            <ac:task><ac:task-id>2</ac:task-id><ac:task-status>incomplete</ac:task-status><ac:task-body><span class=\"placeholder-inline-tasks\">Open</span></ac:task-body></ac:task></ac:task-list>";
        assert_eq!(markdown(storage), "- One\n\n  3. Three\n\n+ [x] Done\n+ [ ] Open\n");
        assert_eq!(found(storage), tally(&[("bullet list", 1), ("numbered list", 1), ("paragraph", 2), ("task list", 1)]));
    }

    #[test]
    fn confluence_info_note_tip_and_warning_become_callouts_with_their_title() {
        let panel = |name: &str, body: &str| {
            format!("<ac:structured-macro ac:name=\"{name}\" ac:schema-version=\"1\"><ac:parameter ac:name=\"title\">Heads up</ac:parameter><ac:rich-text-body><p>{body}</p></ac:rich-text-body></ac:structured-macro>")
        };
        let storage = [panel("info", "i"), panel("note", "n"), panel("tip", "t"), panel("warning", "w")].concat();
        assert_eq!(
            markdown(&storage),
            "> [!NOTE]\n> **Heads up**\n>\n> i\n\n> [!WARNING]\n> **Heads up**\n>\n> n\n\n> [!TIP]\n> **Heads up**\n>\n> t\n\n> [!DANGER]\n> **Heads up**\n>\n> w\n"
        );
        assert_eq!(found(&storage), tally(&[("info macro", 1), ("note macro", 1), ("paragraph", 4), ("tip macro", 1), ("warning macro", 1)]));
    }

    #[test]
    fn confluence_code_becomes_code_blocks_in_its_language() {
        let storage = "<ac:structured-macro ac:name=\"code\"><ac:parameter ac:name=\"language\">bash</ac:parameter><ac:plain-text-body><![CDATA[echo \"<hi>\" && exit]]></ac:plain-text-body></ac:structured-macro>\
            <pre>plain  text</pre>";
        assert_eq!(markdown(storage), "```bash\necho \"<hi>\" && exit\n```\n\n```\nplain  text\n```\n");
        assert_eq!(found(storage), tally(&[("code macro", 1), ("preformatted text", 1)]));
    }

    #[test]
    fn a_confluence_status_becomes_a_status_of_the_same_colour() {
        let storage = "<p>State: <ac:structured-macro ac:name=\"status\"><ac:parameter ac:name=\"title\">Same day</ac:parameter><ac:parameter ac:name=\"colour\">Red</ac:parameter></ac:structured-macro></p>";
        assert_eq!(markdown(storage), "State: :status[Same day]{color=red}\n");
    }

    #[test]
    fn links_to_pages_of_the_space_become_page_links_and_others_keep_their_text_and_own_text_is_counted() {
        let storage = "<p><ac:link><ri:page ri:content-title=\"Runbook\" /><ac:link-body>the runbook</ac:link-body></ac:link>, \
            <ac:link><ri:page ri:space-key=\"OPS\" ri:content-title=\"Elsewhere\" /></ac:link>, \
            <a href=\"https://wiki.example/wiki/spaces/KB/pages/2/Runbook\">web</a>, \
            <a href=\"https://wiki.example/wiki/x/AgAAAA\">short</a>, <ac:link><ri:page ri:content-title=\"Guides\" /></ac:link> \
            and <a href=\"https://example.org\">out</a> <ac:link><ri:page ri:content-title=\"Runbook\" /></ac:link></p>";
        let converted = convert(storage, "1", &space()).unwrap();
        assert_eq!(
            crate::markdown::to_markdown(&crate::model::normalize(converted.content)),
            "[the runbook](<confluence:page/2>), Elsewhere, [web](<https://wiki.example/wiki/spaces/KB/pages/2/Runbook>), [short](<https://wiki.example/wiki/x/AgAAAA>), Guides and [out](<https://example.org>) [Runbook](<confluence:page/2>)\n"
        );
        let to_runbook = Target::Entry("page:2".into());
        assert_eq!(converted.links.get("confluence:page/2"), Some(&to_runbook));
        assert_eq!(converted.links.get("https://wiki.example/wiki/spaces/KB/pages/2/Runbook"), Some(&to_runbook));
        assert_eq!(converted.links.get("https://wiki.example/wiki/x/AgAAAA"), Some(&to_runbook));
        assert_eq!(converted.links.len(), 3);
        assert_eq!(
            converted.found.0.into_iter().collect::<Vec<_>>(),
            tally(&[
                ("link", 1),
                ("link to a folder", 1),
                ("link to a page", 1),
                ("link to a page outside the space", 1),
                ("link to a page with its own text", 3),
                ("paragraph", 1)
            ])
        );
    }

    #[test]
    fn images_and_links_to_attachments_point_at_the_attachment_and_a_missing_one_is_counted() {
        let storage = "<p><ac:image ac:alt=\"Chart\"><ri:attachment ri:filename=\"chart.png\" /><ac:caption><p>Q3</p></ac:caption></ac:image></p>\
            <p><ac:link><ri:attachment ri:filename=\"chart.png\" /></ac:link> \
            <ac:image><ri:attachment ri:filename=\"gone.png\" /></ac:image>\
            <ac:image><ri:url ri:value=\"https://example.org/a.png\" /></ac:image></p>";
        let converted = convert(storage, "1", &space()).unwrap();
        assert_eq!(
            crate::markdown::to_markdown(&crate::model::normalize(converted.content)),
            "![Chart](<confluence:attachment/10>)\n\n*Q3*\n\n[chart.png](<confluence:attachment/10>) *Not imported: gone.png![](<https://example.org/a.png>)*\n"
        );
        assert_eq!(converted.links.get("confluence:attachment/10"), Some(&Target::Attachment("attachment:10".into())));
        assert_eq!(
            converted.found.0.into_iter().collect::<Vec<_>>(),
            tally(&[("image", 1), ("image caption", 1), ("image from the web", 1), ("link to an attachment", 1), ("missing attachment", 1), ("paragraph", 3)])
        );
    }

    #[test]
    fn mentions_emoticons_and_dates_become_text() {
        let storage = "<p><ac:link><ri:user ri:userkey=\"712020:abc\" /></ac:link> <ac:emoticon ac:name=\"smile\" ac:emoji-fallback=\"🙂\" /> \
            <ac:emoticon ac:name=\"tick\" /> <time datetime=\"2026-11-09\" /></p>";
        assert_eq!(markdown(storage), "@Grace Hopper 🙂 ✅ 2026-11-09\n");
        assert_eq!(found(storage), tally(&[("date", 1), ("emoticon", 2), ("mention", 1), ("paragraph", 1)]));
    }

    #[test]
    fn tables_keep_their_rows_and_say_what_had_to_give() {
        let storage = "<table><colgroup><col /></colgroup><tbody>\
            <tr><th><p>A</p></th><th><p>B</p></th><th><p>C</p></th></tr>\
            <tr><td colspan=\"2\"><p>wide</p></td><td rowspan=\"2\"><ul><li><p>x</p></li><li><p>y</p></li></ul></td></tr>\
            <tr><td><p>1</p><p>2</p></td><td>3</td></tr></tbody></table>\
            <table><tbody><tr><td>no</td><td>header</td></tr></tbody></table>";
        assert_eq!(
            markdown(storage),
            "| A | B | C |\n| --- | --- | --- |\n| wide |  | • x<br>• y |\n| 1<br>2 | 3 |  |\n\n| no | header |\n| --- | --- |\n"
        );
        assert_eq!(
            found(storage),
            tally(&[("bullet list", 1), ("merged table cell", 2), ("paragraph", 8), ("table", 1), ("table cell with more than text", 1), ("table without a header row", 1)])
        );
    }

    #[test]
    fn panels_expands_and_layouts_keep_what_they_hold() {
        let storage = "<ac:layout><ac:layout-section ac:type=\"two_equal\"><ac:layout-cell><p>Left</p></ac:layout-cell><ac:layout-cell><p>Right</p></ac:layout-cell></ac:layout-section></ac:layout>\
            <ac:structured-macro ac:name=\"expand\"><ac:parameter ac:name=\"title\">More</ac:parameter><ac:rich-text-body><p>Hidden</p></ac:rich-text-body></ac:structured-macro>\
            <ac:structured-macro ac:name=\"panel\"><ac:parameter ac:name=\"bgColor\">#E6FCFF</ac:parameter><ac:rich-text-body><p>Boxed</p></ac:rich-text-body></ac:structured-macro>\
            <ac:adf-extension><ac:adf-node type=\"panel\"><ac:adf-attribute key=\"panel-type\">error</ac:adf-attribute><ac:adf-content><p>Careful</p></ac:adf-content></ac:adf-node>\
            <ac:adf-fallback><div class=\"panel\"><p>Careful</p></div></ac:adf-fallback></ac:adf-extension>";
        assert_eq!(markdown(storage), "Left\n\nRight\n\n**More**\n\nHidden\n\n> [!NOTE]\n> Boxed\n\n> [!DANGER]\n> Careful\n");
        assert_eq!(
            found(storage),
            tally(&[("expand macro", 1), ("layout", 1), ("panel", 1), ("panel macro", 1), ("paragraph", 5)])
        );
    }

    #[test]
    fn what_confluence_offers_and_turtle_does_not_know_becomes_a_placeholder_naming_it_followed_by_what_it_held() {
        let storage = "<ac:structured-macro ac:name=\"toc\"><ac:parameter ac:name=\"maxLevel\">3</ac:parameter></ac:structured-macro>\
            <ac:structured-macro ac:name=\"jira\"><ac:parameter ac:name=\"key\">OPS-1 | x</ac:parameter><ac:parameter ac:name=\"server\">System JIRA</ac:parameter></ac:structured-macro>\
            <ac:structured-macro ac:name=\"section\"><ac:rich-text-body><p>Kept</p></ac:rich-text-body></ac:structured-macro>";
        assert_eq!(
            markdown(storage),
            ":confluence.toc[toc]{maxLevel=3}\n\n:confluence.jira[jira]{key=\"OPS-1 / x\" server=\"System JIRA\"}\n\n:confluence.section[section]\n\nKept\n"
        );
        assert_eq!(found(storage), tally(&[("jira macro", 1), ("paragraph", 1), ("section macro", 1), ("toc macro", 1)]));
    }

    #[test]
    fn an_element_turtle_does_not_know_keeps_its_text_and_is_counted_by_name() {
        let storage = "<p>a<ac:inline-comment-marker ac:ref=\"x\">b</ac:inline-comment-marker><u>c</u><blink>d</blink></p>";
        assert_eq!(markdown(storage), "abcd\n");
        assert_eq!(found(storage), tally(&[("blink element", 1), ("inline comment", 1), ("paragraph", 1), ("underline", 1)]));
    }

    #[test]
    fn content_that_is_not_well_formed_is_not_converted() {
        assert!(convert("<p>open", "1", &space()).is_none());
    }
}
