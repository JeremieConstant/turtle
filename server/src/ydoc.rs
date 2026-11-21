//! Reads and writes Page content in the collaborative document.
//!
//! The layout is the one the editor produces: an XML fragment whose elements
//! are named after the editor's node types, with text formatting stored as
//! attributes on text ranges.

use crate::diff::{align, common_edges, text_hunks, Fit, Step};
use crate::model::{normalize, Block, Inline, Marks};
use std::collections::HashMap;
use std::sync::Arc;
use yrs::types::text::YChange;
use yrs::types::Attrs;
use yrs::{
    Any, Doc, Number, OffsetKind, Out, ReadTxn, Text, Transact, TransactionMut, Xml,
    XmlElementPrelim, XmlElementRef, XmlFragment, XmlOut, XmlTextPrelim, XmlTextRef,
};

/// The name of the fragment the editor binds to.
const FRAGMENT: &str = "default";

pub fn read(doc: &Doc) -> Vec<Block> {
    let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
    let txn = doc.transact();
    normalize(read_blocks(&txn, fragment.children(&txn)))
}

/// Makes the content equal to the given blocks by changing only what
/// differs, so that edits made elsewhere at the same moment survive. Returns
/// the update that describes the change, ready to be sent to connected
/// editors, or nothing when the content already was what was asked for.
pub fn write(doc: &Doc, blocks: &[Block]) -> Option<Vec<u8>> {
    let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
    let mut txn = doc.transact_mut();
    diff_blocks(&mut txn, &fragment, blocks);
    // The editor requires a document to hold at least one block.
    if fragment.len(&txn) == 0 {
        fragment.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
    }
    txn.has_changes().then(|| txn.encode_update_v1())
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

fn read_blocks<T: ReadTxn>(txn: &T, nodes: impl Iterator<Item = XmlOut>) -> Vec<Block> {
    let mut blocks = Vec::new();
    for node in nodes {
        let element = match node {
            XmlOut::Element(element) => element,
            XmlOut::Text(text) => {
                blocks.push(Block::Paragraph(read_text(txn, &text)));
                continue;
            }
            XmlOut::Fragment(fragment) => {
                blocks.extend(read_blocks(txn, fragment.children(txn)));
                continue;
            }
        };
        match element.tag().as_ref() {
            "paragraph" => blocks.push(Block::Paragraph(read_inlines(txn, &element))),
            "heading" => blocks.push(Block::Heading {
                level: number_attribute(txn, &element, "level").unwrap_or(1) as u8,
                content: read_inlines(txn, &element),
            }),
            "codeBlock" => blocks.push(Block::CodeBlock {
                language: string_attribute(txn, &element, "language"),
                text: read_inlines(txn, &element)
                    .into_iter()
                    .map(|inline| match inline {
                        Inline::Text { text, .. } => text,
                        Inline::HardBreak => "\n".to_string(),
                    })
                    .collect(),
            }),
            "blockquote" => blocks.push(Block::Blockquote(read_blocks(txn, element.children(txn)))),
            "bulletList" => blocks.push(Block::BulletList(read_items(txn, &element))),
            "orderedList" => blocks.push(Block::OrderedList {
                start: number_attribute(txn, &element, "start").unwrap_or(1),
                items: read_items(txn, &element),
            }),
            "horizontalRule" => blocks.push(Block::HorizontalRule),
            // An element this version does not know keeps its content.
            _ => blocks.extend(read_blocks(txn, element.children(txn))),
        }
    }
    blocks
}

fn read_items<T: ReadTxn>(txn: &T, list: &XmlElementRef) -> Vec<Vec<Block>> {
    list.children(txn)
        .map(|item| match item {
            XmlOut::Element(item) => read_blocks(txn, item.children(txn)),
            other => read_blocks(txn, std::iter::once(other)),
        })
        .collect()
}

fn read_inlines<T: ReadTxn>(txn: &T, element: &XmlElementRef) -> Vec<Inline> {
    let mut inlines = Vec::new();
    for child in element.children(txn) {
        match child {
            XmlOut::Text(text) => inlines.extend(read_text(txn, &text)),
            XmlOut::Element(child) if child.tag().as_ref() == "hardBreak" => {
                inlines.push(Inline::HardBreak)
            }
            _ => {}
        }
    }
    inlines
}

fn read_text<T: ReadTxn>(txn: &T, text: &XmlTextRef) -> Vec<Inline> {
    text.diff(txn, YChange::identity)
        .into_iter()
        .filter_map(|chunk| match chunk.insert {
            Out::Any(Any::String(s)) => Some(Inline::Text {
                text: s.to_string(),
                marks: chunk.attributes.map(|a| read_marks(&a)).unwrap_or_default(),
            }),
            _ => None,
        })
        .collect()
}

fn read_marks(attributes: &Attrs) -> Marks {
    let mut marks = Marks::default();
    for (key, value) in attributes {
        if matches!(value, Any::Null | Any::Undefined | Any::Bool(false)) {
            continue;
        }
        // Marks that may overlap themselves are stored as `name--hash`.
        match key.split("--").next().unwrap_or_default() {
            "bold" => marks.bold = true,
            "italic" => marks.italic = true,
            "strike" => marks.strike = true,
            "code" => marks.code = true,
            "link" => {
                if let Any::Map(map) = value {
                    if let Some(Any::String(href)) = map.get("href") {
                        marks.link = Some(href.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    marks
}

fn string_attribute<T: ReadTxn>(txn: &T, element: &XmlElementRef, name: &str) -> Option<String> {
    match element.get_attribute(txn, name)? {
        Out::Any(Any::String(s)) => Some(s.to_string()),
        _ => None,
    }
}

fn number_attribute<T: ReadTxn>(txn: &T, element: &XmlElementRef, name: &str) -> Option<u64> {
    match element.get_attribute(txn, name)? {
        Out::Any(Any::Number(Number::Int(n))) => Some(n.max(0) as u64),
        Out::Any(Any::Number(Number::Float(n))) => Some(n.max(0.0) as u64),
        Out::Any(Any::String(s)) => s.parse().ok(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Writing only what differs
// ---------------------------------------------------------------------------

/// Turns the blocks inside `parent` into `target`. A block that already is
/// what is wanted stays untouched, one of the same type is changed in place,
/// and only the rest is removed and written anew. Blocks Markdown cannot
/// express, such as the empty paragraph someone is about to type in, are
/// never removed.
fn diff_blocks<P: XmlFragment>(txn: &mut TransactionMut, parent: &P, target: &[Block]) {
    let nodes: Vec<XmlOut> = parent.children(txn).collect();
    let current: Vec<Vec<Block>> = nodes
        .iter()
        .map(|node| normalize(read_blocks(txn, std::iter::once(node.clone()))))
        .collect();
    let old_text: Vec<Vec<char>> = current.iter().map(|blocks| plain_text(blocks)).collect();
    let new_text: Vec<Vec<char>> = target
        .iter()
        .map(|block| plain_text(std::slice::from_ref(block)))
        .collect();
    let same = |i: usize, j: usize| current[i].as_slice() == std::slice::from_ref(&target[j]);

    let steps = align(
        nodes.len(),
        target.len(),
        |i, j| {
            if same(i, j) {
                Fit::Same(2 + new_text[j].len() as u32)
            } else if tag_of(&nodes[i]) == Some(tag_for(&target[j])) {
                Fit::Similar(1 + common_edges(&old_text[i], &new_text[j]) as u32)
            } else {
                Fit::No
            }
        },
        |i| current[i].is_empty(),
    );

    // Where in the parent the next step applies.
    let mut at = 0;
    for step in steps {
        match step {
            Step::Keep(i, j) => {
                if let (false, XmlOut::Element(element)) = (same(i, j), &nodes[i]) {
                    update_block(txn, element, &target[j]);
                }
                at += 1;
            }
            Step::Remove(i) if current[i].is_empty() => at += 1,
            Step::Remove(_) => parent.remove_range(txn, at, 1),
            Step::Add(j) => {
                insert_block(txn, parent, at, &target[j]);
                at += 1;
            }
        }
    }
}

/// The name of the element a block is stored as.
fn tag_for(block: &Block) -> &'static str {
    match block {
        Block::Paragraph(_) => "paragraph",
        Block::Heading { .. } => "heading",
        Block::CodeBlock { .. } => "codeBlock",
        Block::Blockquote(_) => "blockquote",
        Block::BulletList(_) => "bulletList",
        Block::OrderedList { .. } => "orderedList",
        Block::HorizontalRule => "horizontalRule",
    }
}

fn tag_of(node: &XmlOut) -> Option<&str> {
    match node {
        XmlOut::Element(element) => Some(element.tag().as_ref()),
        _ => None,
    }
}

/// All text of some blocks in reading order, to judge how alike two are.
fn plain_text(blocks: &[Block]) -> Vec<char> {
    fn collect(blocks: &[Block], out: &mut Vec<char>) {
        for block in blocks {
            match block {
                Block::Paragraph(content) | Block::Heading { content, .. } => {
                    for inline in content {
                        match inline {
                            Inline::Text { text, .. } => out.extend(text.chars()),
                            Inline::HardBreak => out.push('\n'),
                        }
                    }
                }
                Block::CodeBlock { text, .. } => out.extend(text.chars()),
                Block::Blockquote(children) => collect(children, out),
                Block::BulletList(items) | Block::OrderedList { items, .. } => {
                    items.iter().for_each(|item| collect(item, out))
                }
                Block::HorizontalRule => {}
            }
            out.push('\n');
        }
    }
    let mut out = Vec::new();
    collect(blocks, &mut out);
    out
}

/// Turns an element into the given block of its own type.
fn update_block(txn: &mut TransactionMut, element: &XmlElementRef, block: &Block) {
    match block {
        Block::Paragraph(content) => diff_inlines(txn, element, content),
        Block::Heading { level, content } => {
            if number_attribute(txn, element, "level") != Some(*level as u64) {
                element.insert_attribute(txn, "level", Any::from(*level as f64));
            }
            diff_inlines(txn, element, content);
        }
        Block::CodeBlock { language, text } => {
            if string_attribute(txn, element, "language") != *language {
                match language {
                    Some(language) => {
                        element.insert_attribute(txn, "language", Any::from(language.as_str()));
                    }
                    None => element.remove_attribute(txn, &"language"),
                }
            }
            let content = Inline::Text {
                text: text.clone(),
                marks: Marks::default(),
            };
            let content = if text.is_empty() {
                &[][..]
            } else {
                std::slice::from_ref(&content)
            };
            diff_inlines(txn, element, content);
        }
        Block::Blockquote(children) => {
            diff_blocks(txn, element, children);
            if element.len(txn) == 0 {
                element.push_back(txn, XmlElementPrelim::empty("paragraph"));
            }
        }
        Block::BulletList(items) => diff_items(txn, element, items),
        Block::OrderedList { start, items } => {
            if number_attribute(txn, element, "start") != Some(*start) {
                element.insert_attribute(txn, "start", Any::from(*start as f64));
            }
            diff_items(txn, element, items);
        }
        Block::HorizontalRule => {}
    }
}

/// Turns the items of a list into `target`, the way `diff_blocks` does for blocks.
fn diff_items(txn: &mut TransactionMut, list: &XmlElementRef, target: &[Vec<Block>]) {
    let nodes: Vec<XmlOut> = list.children(txn).collect();
    let current: Vec<Vec<Block>> = nodes
        .iter()
        .map(|node| match node {
            XmlOut::Element(item) => normalize(read_blocks(txn, item.children(txn))),
            other => normalize(read_blocks(txn, std::iter::once(other.clone()))),
        })
        .collect();
    let old_text: Vec<Vec<char>> = current.iter().map(|blocks| plain_text(blocks)).collect();
    let new_text: Vec<Vec<char>> = target.iter().map(|blocks| plain_text(blocks)).collect();

    let steps = align(
        nodes.len(),
        target.len(),
        |i, j| {
            if current[i] == target[j] {
                Fit::Same(2 + new_text[j].len() as u32)
            } else if tag_of(&nodes[i]) == Some("listItem") {
                Fit::Similar(1 + common_edges(&old_text[i], &new_text[j]) as u32)
            } else {
                Fit::No
            }
        },
        |_| false,
    );

    let mut at = 0;
    for step in steps {
        match step {
            Step::Keep(i, j) => {
                if let (false, XmlOut::Element(item)) = (current[i] == target[j], &nodes[i]) {
                    diff_blocks(txn, item, &target[j]);
                    let starts_with_paragraph = matches!(
                        item.get(txn, 0),
                        Some(XmlOut::Element(first)) if first.tag().as_ref() == "paragraph"
                    );
                    // The editor requires every list item to start with a paragraph.
                    if !starts_with_paragraph {
                        item.insert(txn, 0, XmlElementPrelim::empty("paragraph"));
                    }
                }
                at += 1;
            }
            Step::Remove(_) => list.remove_range(txn, at, 1),
            Step::Add(j) => {
                insert_item(txn, list, at, &target[j]);
                at += 1;
            }
        }
    }
}

/// What an element holds between two line breaks: text in one or more formats.
type Run<'a> = Vec<(&'a str, &'a Marks)>;

enum Part<'a> {
    Run(Run<'a>),
    Break,
}

fn parts(inlines: &[Inline]) -> Vec<Part<'_>> {
    let mut parts: Vec<Part> = Vec::new();
    for inline in inlines {
        match (inline, parts.last_mut()) {
            (Inline::HardBreak, _) => parts.push(Part::Break),
            (Inline::Text { text, marks }, Some(Part::Run(run))) => run.push((text, marks)),
            (Inline::Text { text, marks }, _) => parts.push(Part::Run(vec![(text, marks)])),
        }
    }
    parts
}

enum Child {
    /// A text and what it says. Without the latter it holds something other
    /// than text and cannot be changed in place.
    Text(XmlTextRef, Option<Vec<char>>),
    Break,
    /// Something this version does not know. It is left alone.
    Other,
}

/// Turns the content of a paragraph, heading or code block into `target`,
/// changing only the text ranges and formats that differ.
fn diff_inlines(txn: &mut TransactionMut, element: &XmlElementRef, target: &[Inline]) {
    let target = parts(target);
    let new_text: Vec<Vec<char>> = target
        .iter()
        .map(|part| match part {
            Part::Run(run) => run.iter().flat_map(|(text, _)| text.chars()).collect(),
            Part::Break => Vec::new(),
        })
        .collect();
    let children: Vec<Child> = element
        .children(txn)
        .map(|node| match node {
            XmlOut::Text(text) => {
                let said = text_chunks(txn, &text)
                    .map(|chunks| chunks.iter().flat_map(|(text, _)| text.chars()).collect());
                Child::Text(text, said)
            }
            XmlOut::Element(child) if child.tag().as_ref() == "hardBreak" => Child::Break,
            _ => Child::Other,
        })
        .collect();

    let steps = align(
        children.len(),
        target.len(),
        |i, j| match (&children[i], &target[j]) {
            (Child::Text(_, Some(said)), Part::Run(_)) => {
                Fit::Similar(1 + common_edges(said, &new_text[j]) as u32)
            }
            (Child::Break, Part::Break) => Fit::Same(1),
            _ => Fit::No,
        },
        |i| matches!(children[i], Child::Other),
    );

    let mut at = 0;
    for step in steps {
        match step {
            Step::Keep(i, j) => {
                if let (Child::Text(text, Some(said)), Part::Run(run)) = (&children[i], &target[j])
                {
                    diff_text(txn, text, said, run, &new_text[j]);
                }
                at += 1;
            }
            Step::Remove(i) if matches!(children[i], Child::Other) => at += 1,
            Step::Remove(_) => element.remove_range(txn, at, 1),
            Step::Add(j) => {
                match &target[j] {
                    Part::Run(run) => {
                        let text = element.insert(txn, at, XmlTextPrelim::new(""));
                        diff_text(txn, &text, &[], run, &new_text[j]);
                    }
                    Part::Break => {
                        element.insert(txn, at, XmlElementPrelim::empty("hardBreak"));
                    }
                }
                at += 1;
            }
        }
    }
}

/// The pieces of a text with their formatting, or nothing when the text
/// holds something other than text.
fn text_chunks<T: ReadTxn>(
    txn: &T,
    text: &XmlTextRef,
) -> Option<Vec<(String, Option<Box<Attrs>>)>> {
    text.diff(txn, YChange::identity)
        .into_iter()
        .map(|chunk| match chunk.insert {
            Out::Any(Any::String(s)) => Some((s.to_string(), chunk.attributes)),
            _ => None,
        })
        .collect()
}

/// How long a text is in the unit the document counts positions in.
fn length(kind: OffsetKind, text: &[char]) -> u32 {
    text.iter()
        .map(|character| match kind {
            OffsetKind::Bytes => character.len_utf8(),
            OffsetKind::Utf16 => character.len_utf16(),
        })
        .sum::<usize>() as u32
}

/// Turns a text that says `old` into the run `new`: first the words, then
/// the formatting of what stayed.
fn diff_text(txn: &mut TransactionMut, text: &XmlTextRef, old: &[char], run: &Run, new: &[char]) {
    let kind = txn.doc().offset_kind();

    // Back to front, so the positions of earlier ranges stay valid.
    for hunk in text_hunks(old, new).into_iter().rev() {
        let mut at = length(kind, &old[..hunk.old.start]);
        if !hunk.old.is_empty() {
            text.remove_range(txn, at, length(kind, &old[hunk.old.clone()]));
        }
        let mut run_start = 0;
        for (piece, marks) in run {
            let run_end = run_start + piece.chars().count();
            let (from, to) = (hunk.new.start.max(run_start), hunk.new.end.min(run_end));
            if from < to {
                let added: String = new[from..to].iter().collect();
                // Attributes are always passed, even when empty: a plain insert
                // would inherit the formatting of the text before it.
                text.insert_with_attributes(txn, at, &added, write_marks(marks));
                at += length(kind, &new[from..to]);
            }
            run_start = run_end;
        }
    }

    // The text now says what is wanted; what stayed may still be formatted differently.
    let Some(chunks) = text_chunks(txn, text) else {
        return;
    };
    let mut wanted = run
        .iter()
        .flat_map(|(piece, marks)| piece.chars().map(move |character| (character, *marks)))
        .peekable();
    let mut at = 0;
    for (said, attributes) in chunks {
        let has = attributes.as_deref().map(read_marks).unwrap_or_default();
        let mut rest = said.chars().peekable();
        while rest.peek().is_some() {
            let Some(&(_, marks)) = wanted.peek() else {
                return;
            };
            // As far as the same format is wanted.
            let mut stretch = Vec::new();
            while let (Some(_), Some(&(_, next))) = (rest.peek(), wanted.peek()) {
                if next != marks {
                    break;
                }
                stretch.extend(rest.next());
                wanted.next();
            }
            let len = length(kind, &stretch);
            if canonical(has.clone()) != *marks {
                text.format(
                    txn,
                    at,
                    len,
                    marks_change(attributes.as_deref(), &has, marks),
                );
            }
            at += len;
        }
    }
}

/// Marks as they survive a Markdown round trip: inline code carries no other format.
fn canonical(marks: Marks) -> Marks {
    if marks.code {
        Marks {
            code: true,
            ..Marks::default()
        }
    } else {
        marks
    }
}

/// The attributes that turn the formatting a text has into the one wanted.
fn marks_change(attributes: Option<&Attrs>, has: &Marks, wanted: &Marks) -> Attrs {
    let mut change = Attrs::new();
    for (key, value) in attributes.into_iter().flatten() {
        if matches!(value, Any::Null | Any::Undefined | Any::Bool(false)) {
            continue;
        }
        let stays = match key.split("--").next().unwrap_or_default() {
            "bold" => wanted.bold,
            "italic" => wanted.italic,
            "strike" => wanted.strike,
            "code" => wanted.code,
            "link" => wanted.link.is_some() && wanted.link == has.link,
            // Formatting this version does not know is not its to remove.
            _ => true,
        };
        if !stays {
            change.insert(key.clone(), Any::Null);
        }
    }
    let missing = Marks {
        bold: wanted.bold && !has.bold,
        italic: wanted.italic && !has.italic,
        strike: wanted.strike && !has.strike,
        code: wanted.code && !has.code,
        link: wanted.link.clone().filter(|_| wanted.link != has.link),
    };
    change.extend(write_marks(&missing));
    change
}

// ---------------------------------------------------------------------------
// Writing new content
// ---------------------------------------------------------------------------

fn insert_block<P: XmlFragment>(txn: &mut TransactionMut, parent: &P, index: u32, block: &Block) {
    let element = parent.insert(txn, index, XmlElementPrelim::empty(tag_for(block)));
    match block {
        Block::Paragraph(content) => diff_inlines(txn, &element, content),
        Block::Heading { level, content } => {
            element.insert_attribute(txn, "level", Any::from(*level as f64));
            diff_inlines(txn, &element, content);
        }
        Block::CodeBlock { language, text } => {
            if let Some(language) = language {
                element.insert_attribute(txn, "language", Any::from(language.as_str()));
            }
            if !text.is_empty() {
                element.push_back(txn, XmlTextPrelim::new(text.as_str()));
            }
        }
        Block::Blockquote(children) => {
            for (at, child) in children.iter().enumerate() {
                insert_block(txn, &element, at as u32, child);
            }
            if children.is_empty() {
                element.push_back(txn, XmlElementPrelim::empty("paragraph"));
            }
        }
        Block::BulletList(items) => {
            for (at, item) in items.iter().enumerate() {
                insert_item(txn, &element, at as u32, item);
            }
        }
        Block::OrderedList { start, items } => {
            element.insert_attribute(txn, "start", Any::from(*start as f64));
            for (at, item) in items.iter().enumerate() {
                insert_item(txn, &element, at as u32, item);
            }
        }
        Block::HorizontalRule => {}
    }
}

fn insert_item(txn: &mut TransactionMut, list: &XmlElementRef, index: u32, item: &[Block]) {
    let element = list.insert(txn, index, XmlElementPrelim::empty("listItem"));
    // The editor requires every list item to start with a paragraph.
    if !matches!(item.first(), Some(Block::Paragraph(_))) {
        element.push_back(txn, XmlElementPrelim::empty("paragraph"));
    }
    for block in item {
        let at = element.len(txn);
        insert_block(txn, &element, at, block);
    }
}

fn write_marks(marks: &Marks) -> Attrs {
    let flag = || Any::Map(Arc::new(HashMap::new()));
    let mut attributes = Attrs::new();
    if marks.bold {
        attributes.insert("bold".into(), flag());
    }
    if marks.italic {
        attributes.insert("italic".into(), flag());
    }
    if marks.strike {
        attributes.insert("strike".into(), flag());
    }
    if marks.code {
        attributes.insert("code".into(), flag());
    }
    if let Some(href) = &marks.link {
        let mut link = HashMap::new();
        link.insert("href".to_string(), Any::from(href.as_str()));
        attributes.insert("link".into(), Any::Map(Arc::new(link)));
    }
    attributes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::testing::Rng;
    use crate::markdown::{from_markdown, to_markdown};
    use yrs::types::Delta;
    use yrs::updates::decoder::Decode;
    use yrs::Observable;

    #[test]
    fn content_survives_the_collaborative_document() {
        let markdown = "# Title\n\n\
            Some **bold**, *italic*, ~~struck~~ and `code` with a [link](<https://example.org>).\\\n\
            Second line.\n\n\
            - one\n- two\n  1. nested\n  2. list\n\n\
            > quoted\n\n\
            ```rust\nfn main() {}\n```\n\n\
            ***\n\n\
            3. three\n4. four\n";
        let blocks = from_markdown(markdown);

        let doc = Doc::new();
        write(&doc, &blocks);
        assert_eq!(read(&doc), blocks);
        assert_eq!(to_markdown(&read(&doc)), to_markdown(&blocks));
    }

    #[test]
    fn writing_the_current_content_changes_nothing() {
        let blocks = from_markdown("# Title\n\nSome **bold** text.\n\n- one\n- two\n");
        let doc = Doc::new();
        assert!(write(&doc, &blocks).is_some());

        assert_eq!(write(&doc, &blocks), None);
        assert_eq!(read(&doc), blocks);
    }

    #[test]
    fn update_from_a_write_reproduces_the_content_elsewhere() {
        let blocks = from_markdown("Hello **world**\n\n- a\n- b\n");
        let source = Doc::new();
        let update = write(&source, &blocks).unwrap();

        let replica = Doc::new();
        replica
            .transact_mut()
            .apply_update(yrs::Update::decode_v1(&update).unwrap())
            .unwrap();
        assert_eq!(read(&replica), blocks);
    }

    /// Applies an update made on one document to another.
    fn apply(doc: &Doc, update: &[u8]) {
        doc.transact_mut()
            .apply_update(yrs::Update::decode_v1(update).unwrap())
            .unwrap();
    }

    fn replica(of: &Doc) -> Doc {
        let doc = Doc::new();
        apply(
            &doc,
            &of.transact()
                .encode_state_as_update_v1(&yrs::StateVector::default()),
        );
        doc
    }

    fn block_ids(doc: &Doc) -> Vec<yrs::branch::BranchID> {
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let txn = doc.transact();
        let ids = fragment.children(&txn).map(|node| node.id()).collect();
        ids
    }

    /// The text of the block at `index`, which holds a single text.
    fn text_of(doc: &Doc, index: u32) -> XmlTextRef {
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let txn = doc.transact();
        let Some(XmlOut::Element(element)) = fragment.get(&txn, index) else {
            panic!("no block at {index}");
        };
        let Some(XmlOut::Text(text)) = element.get(&txn, 0) else {
            panic!("block {index} holds no text");
        };
        text
    }

    /// Types `typed` at `at` in the text of the block at `index`, as an editor would.
    fn type_into(doc: &Doc, index: u32, at: u32, typed: &str) -> Vec<u8> {
        let text = text_of(doc, index);
        let mut txn = doc.transact_mut();
        text.insert(&mut txn, at, typed);
        txn.encode_update_v1()
    }

    #[test]
    fn unchanged_blocks_keep_their_identity() {
        let doc = Doc::new();
        write(
            &doc,
            &from_markdown("# Title\n\nFirst.\n\nSecond.\n\n- a\n- b\n"),
        );
        let before = block_ids(&doc);

        write(
            &doc,
            &from_markdown("# Title\n\nFirst.\n\nInserted.\n\nSecond.\n\n- a\n- b\n"),
        );
        let mut after = block_ids(&doc);
        assert_eq!(after.len(), 5);
        after.remove(2);
        assert_eq!(after, before);

        write(&doc, &from_markdown("# Title\n\nSecond.\n\n- a\n- b\n"));
        let mut without_first = before.clone();
        without_first.remove(1);
        assert_eq!(block_ids(&doc), without_first);
    }

    #[test]
    fn only_the_differing_text_of_a_changed_block_is_replaced() {
        let doc = Doc::new();
        write(&doc, &from_markdown("# A title\n\nThe quick brown fox.\n"));
        let before = block_ids(&doc);

        // What a connected editor is told about each text.
        let changes = Arc::new(std::sync::Mutex::new(Vec::new()));
        for index in 0..2u32 {
            let changes = changes.clone();
            text_of(&doc, index).observe(
                "test",
                move |txn, event: &yrs::types::xml::XmlTextEvent| {
                    changes
                        .lock()
                        .unwrap()
                        .push((index, event.delta(txn).to_vec()));
                },
            );
        }

        write(
            &doc,
            &from_markdown("# A new title\n\nThe slow brown fox.\n"),
        );
        assert_eq!(
            block_ids(&doc),
            before,
            "changed blocks are edited in place"
        );

        let inserted = |s: &str| Delta::Inserted(Out::Any(Any::from(s)), None);
        let mut changes = changes.lock().unwrap().clone();
        changes.sort_by_key(|(index, _)| *index);
        assert_eq!(
            changes,
            vec![
                (0, vec![Delta::Retain(2, None), inserted("new ")]),
                (
                    1,
                    vec![Delta::Retain(4, None), Delta::Deleted(5), inserted("slow")]
                ),
            ]
        );
        assert_eq!(
            read(&doc),
            from_markdown("# A new title\n\nThe slow brown fox.\n")
        );
    }

    #[test]
    fn formatting_changes_without_replacing_the_text() {
        let doc = Doc::new();
        write(&doc, &from_markdown("Some plain words here.\n"));
        let text = text_of(&doc, 0);

        let target = from_markdown("Some **plain** words [here](https://x.org).\n");
        write(&doc, &target);
        assert_eq!(read(&doc), target);
        assert_eq!(
            text_of(&doc, 0),
            text,
            "the same text holds the new formatting"
        );

        let target = from_markdown("Some plain words [here](https://y.org).\n");
        write(&doc, &target);
        assert_eq!(read(&doc), target);
    }

    #[test]
    fn writing_the_difference_between_any_two_contents_yields_the_second() {
        let mut failures = Vec::new();
        for seed in 0..2_000u64 {
            let first = normalize(Rng(seed).blocks(0));
            let second = normalize(Rng(seed + 1_000_003).blocks(0));
            let doc = Doc::new();
            write(&doc, &first);
            let elsewhere = replica(&doc);
            let update = write(&doc, &second);
            if let Some(update) = &update {
                apply(&elsewhere, update);
            }
            if read(&doc) != second || read(&elsewhere) != second {
                failures.push(seed);
            }
            if write(&doc, &second).is_some() {
                failures.push(seed);
            }
        }
        assert!(
            failures.is_empty(),
            "failed for seeds {:?}",
            &failures[..failures.len().min(10)]
        );
    }

    /// A copy of some blocks with a few small changes: words edited,
    /// formatting toggled, blocks added, dropped or replaced.
    fn mutate(rng: &mut Rng, blocks: &[Block]) -> Vec<Block> {
        let mut out = Vec::new();
        for block in blocks {
            match rng.next(10) {
                0 => {}
                1 => out.push(rng.block(1)),
                2 => {
                    out.push(rng.block(1));
                    out.push(block.clone());
                }
                3..=5 => out.push(mutate_block(rng, block)),
                _ => out.push(block.clone()),
            }
        }
        if rng.next(4) == 0 {
            out.push(rng.block(1));
        }
        out
    }

    fn mutate_block(rng: &mut Rng, block: &Block) -> Block {
        match block {
            Block::Paragraph(content) => Block::Paragraph(mutate_inlines(rng, content)),
            Block::Heading { level, content } => Block::Heading {
                level: if rng.next(5) == 0 {
                    1 + rng.next(6) as u8
                } else {
                    *level
                },
                content: mutate_inlines(rng, content),
            },
            Block::CodeBlock { language, text } => Block::CodeBlock {
                language: language.clone(),
                text: format!("{text}{}", rng.text()),
            },
            Block::Blockquote(children) => Block::Blockquote(mutate(rng, children)),
            Block::BulletList(items) => {
                Block::BulletList(items.iter().map(|item| mutate(rng, item)).collect())
            }
            Block::OrderedList { start, items } => Block::OrderedList {
                start: *start,
                items: items.iter().map(|item| mutate(rng, item)).collect(),
            },
            Block::HorizontalRule => Block::HorizontalRule,
        }
    }

    fn mutate_inlines(rng: &mut Rng, inlines: &[Inline]) -> Vec<Inline> {
        let mut out = Vec::new();
        for inline in inlines {
            match (inline, rng.next(6)) {
                (Inline::Text { text, marks }, 0) => {
                    let mut chars: Vec<char> = text.chars().collect();
                    let at = rng.next(chars.len() + 1);
                    chars.splice(at..at, rng.text().chars());
                    out.push(Inline::Text {
                        text: chars.into_iter().collect(),
                        marks: marks.clone(),
                    });
                }
                (Inline::Text { text, marks }, 1) => out.push(Inline::Text {
                    text: text.clone(),
                    marks: Marks {
                        bold: !marks.bold,
                        ..marks.clone()
                    },
                }),
                (_, 2) => {}
                (inline, _) => out.push(inline.clone()),
            }
        }
        out
    }

    #[test]
    fn writing_a_small_change_yields_the_changed_content() {
        let (mut failures, mut changed) = (Vec::new(), 0);
        for seed in 0..3_000u64 {
            let mut rng = Rng(seed);
            let first = normalize(rng.blocks(0));
            let second = normalize(mutate(&mut rng, &first));
            let doc = Doc::new();
            write(&doc, &first);
            let elsewhere = replica(&doc);
            if let Some(update) = write(&doc, &second) {
                apply(&elsewhere, &update);
                changed += 1;
            }
            if read(&doc) != second || read(&elsewhere) != second || write(&doc, &second).is_some()
            {
                failures.push(seed);
            }
        }
        assert!(
            failures.is_empty(),
            "failed for seeds {:?}",
            &failures[..failures.len().min(10)]
        );
        // Most pairs differ, so the test exercises the write and not only its shortcut.
        assert!(changed > 1_500, "only {changed} of 3000 pairs differed");
    }

    #[test]
    fn a_concurrent_edit_in_an_unchanged_block_survives() {
        let server = Doc::new();
        write(
            &server,
            &from_markdown("# Title\n\nKeep this.\n\nChange this.\n"),
        );
        let editor = replica(&server);

        let typed = type_into(&editor, 1, "Keep".len() as u32, " typing");
        let written = write(
            &server,
            &from_markdown("# Title\n\nKeep this.\n\nChanged that.\n"),
        )
        .unwrap();
        apply(&server, &typed);
        apply(&editor, &written);

        let expected = from_markdown("# Title\n\nKeep typing this.\n\nChanged that.\n");
        assert_eq!(read(&server), expected);
        assert_eq!(read(&editor), expected);
    }

    #[test]
    fn a_concurrent_edit_beside_a_changed_range_survives() {
        let server = Doc::new();
        write(&server, &from_markdown("The quick brown fox jumps.\n"));
        let editor = replica(&server);

        // Someone types at the end while the write changes the start.
        let typed = type_into(
            &editor,
            0,
            "The quick brown fox jumps".len() as u32,
            " high",
        );
        let written = write(&server, &from_markdown("The slow brown fox jumps.\n")).unwrap();
        apply(&server, &typed);
        apply(&editor, &written);

        let expected = from_markdown("The slow brown fox jumps high.\n");
        assert_eq!(read(&server), expected);
        assert_eq!(read(&editor), expected);
    }

    #[test]
    fn an_empty_paragraph_someone_is_typing_in_stays() {
        let doc = Doc::new();
        write(&doc, &from_markdown("One.\n\nTwo.\n"));
        {
            let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
            let mut txn = doc.transact_mut();
            fragment.insert(&mut txn, 1, XmlElementPrelim::empty("paragraph"));
        }
        let before = block_ids(&doc);
        assert!(write(&doc, &from_markdown("One.\n\nTwo!\n")).is_some());
        assert_eq!(block_ids(&doc), before);
        assert_eq!(read(&doc), from_markdown("One.\n\nTwo!\n"));
    }
}
