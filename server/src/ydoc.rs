//! Reads and writes Page content in the collaborative document.
//!
//! The layout is the one the editor produces: an XML fragment whose elements
//! are named after the editor's node types, with text formatting stored as
//! attributes on text ranges.

use crate::model::{normalize, Block, Inline, Marks};
use std::collections::HashMap;
use std::sync::Arc;
use yrs::types::text::YChange;
use yrs::types::Attrs;
use yrs::{
    Any, Doc, Number, Out, ReadTxn, Text, Transact, TransactionMut, Xml, XmlElementPrelim, XmlElementRef,
    XmlFragment, XmlOut, XmlTextPrelim, XmlTextRef,
};

/// The name of the fragment the editor binds to.
const FRAGMENT: &str = "default";

pub fn read(doc: &Doc) -> Vec<Block> {
    let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
    let txn = doc.transact();
    normalize(read_blocks(&txn, fragment.children(&txn)))
}

/// Replaces the whole content and returns the update that describes the change,
/// ready to be sent to connected editors.
pub fn write(doc: &Doc, blocks: &[Block]) -> Vec<u8> {
    let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
    let mut txn = doc.transact_mut();
    let len = fragment.len(&txn);
    if len > 0 {
        fragment.remove_range(&mut txn, 0, len);
    }
    write_blocks(&mut txn, &fragment, blocks);
    if blocks.is_empty() {
        fragment.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
    }
    txn.encode_update_v1()
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
// Writing
// ---------------------------------------------------------------------------

fn write_blocks<P: XmlFragment>(txn: &mut TransactionMut, parent: &P, blocks: &[Block]) {
    for block in blocks {
        match block {
            Block::Paragraph(content) => {
                let element = parent.push_back(txn, XmlElementPrelim::empty("paragraph"));
                write_inlines(txn, &element, content);
            }
            Block::Heading { level, content } => {
                let element = parent.push_back(txn, XmlElementPrelim::empty("heading"));
                element.insert_attribute(txn, "level", Any::from(*level as f64));
                write_inlines(txn, &element, content);
            }
            Block::CodeBlock { language, text } => {
                let element = parent.push_back(txn, XmlElementPrelim::empty("codeBlock"));
                if let Some(language) = language {
                    element.insert_attribute(txn, "language", Any::from(language.as_str()));
                }
                if !text.is_empty() {
                    element.push_back(txn, XmlTextPrelim::new(text.as_str()));
                }
            }
            Block::Blockquote(children) => {
                let element = parent.push_back(txn, XmlElementPrelim::empty("blockquote"));
                write_blocks(txn, &element, children);
                if children.is_empty() {
                    element.push_back(txn, XmlElementPrelim::empty("paragraph"));
                }
            }
            Block::BulletList(items) => {
                let element = parent.push_back(txn, XmlElementPrelim::empty("bulletList"));
                write_items(txn, &element, items);
            }
            Block::OrderedList { start, items } => {
                let element = parent.push_back(txn, XmlElementPrelim::empty("orderedList"));
                element.insert_attribute(txn, "start", Any::from(*start as f64));
                write_items(txn, &element, items);
            }
            Block::HorizontalRule => {
                parent.push_back(txn, XmlElementPrelim::empty("horizontalRule"));
            }
        }
    }
}

fn write_items(txn: &mut TransactionMut, list: &XmlElementRef, items: &[Vec<Block>]) {
    for item in items {
        let element = list.push_back(txn, XmlElementPrelim::empty("listItem"));
        // The editor requires every list item to start with a paragraph.
        if !matches!(item.first(), Some(Block::Paragraph(_))) {
            element.push_back(txn, XmlElementPrelim::empty("paragraph"));
        }
        write_blocks(txn, &element, item);
    }
}

fn write_inlines(txn: &mut TransactionMut, element: &XmlElementRef, inlines: &[Inline]) {
    let mut current: Option<XmlTextRef> = None;
    for inline in inlines {
        match inline {
            Inline::HardBreak => {
                element.push_back(txn, XmlElementPrelim::empty("hardBreak"));
                current = None;
            }
            Inline::Text { text, marks } => {
                let target = match &current {
                    Some(target) => target.clone(),
                    None => {
                        let created = element.push_back(txn, XmlTextPrelim::new(""));
                        current = Some(created.clone());
                        created
                    }
                };
                let end = target.len(txn);
                // Attributes are always passed, even when empty: a plain insert
                // would inherit the formatting of the text before it.
                target.insert_with_attributes(txn, end, text, write_marks(marks));
            }
        }
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
    use crate::markdown::{from_markdown, to_markdown};
    use yrs::updates::decoder::Decode;

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

        // Writing again replaces the content instead of appending to it.
        write(&doc, &blocks);
        assert_eq!(read(&doc), blocks);
    }

    #[test]
    fn update_from_a_write_reproduces_the_content_elsewhere() {
        let blocks = from_markdown("Hello **world**\n\n- a\n- b\n");
        let source = Doc::new();
        let update = write(&source, &blocks);

        let replica = Doc::new();
        replica
            .transact_mut()
            .apply_update(yrs::Update::decode_v1(&update).unwrap())
            .unwrap();
        assert_eq!(read(&replica), blocks);
    }
}
