//! A Page's content as Confluence stores it, read into a tree of elements.
//!
//! Confluence's storage format is XHTML with elements of its own, such as
//! `ac:structured-macro`, and HTML's named entities, such as `&auml;`, which
//! XML alone does not know.

use quick_xml::escape::resolve_html5_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Element {
    /// The name as written, with its prefix, such as `p` or `ac:image`.
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    /// The child elements, without the text between them.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|element| element.name == name)
    }

    /// All the text inside, as written.
    pub fn text(&self) -> String {
        let mut out = String::new();
        fn collect(nodes: &[Node], out: &mut String) {
            for node in nodes {
                match node {
                    Node::Text(text) => out.push_str(text),
                    Node::Element(element) => collect(&element.children, out),
                }
            }
        }
        collect(&self.children, &mut out);
        out
    }
}

/// Reads a Page's content, held in an element of its own: nothing when it
/// is not well-formed.
pub(super) fn parse(storage: &str) -> Option<Element> {
    let wrapped = format!("<storage>{storage}</storage>");
    let mut reader = Reader::from_str(&wrapped);
    reader.config_mut().check_end_names = true;
    let mut stack: Vec<Element> = vec![Element::default()];
    loop {
        match reader.read_event().ok()? {
            Event::Start(start) => stack.push(element_of(&start)?),
            Event::Empty(start) => {
                let element = element_of(&start)?;
                stack.last_mut()?.children.push(Node::Element(element));
            }
            Event::End(_) => {
                let element = stack.pop()?;
                stack.last_mut()?.children.push(Node::Element(element));
            }
            Event::Text(text) => push_text(stack.last_mut()?, &text.xml10_content()),
            Event::CData(data) => push_text(stack.last_mut()?, &data.xml10_content()),
            Event::GeneralRef(reference) => {
                let text = match reference.resolve_char_ref().ok()? {
                    Some(c) => c.to_string(),
                    None => resolve_html5_entity(&reference).map(str::to_string)
                        // An entity no one knows is kept as written.
                        .unwrap_or_else(|| format!("&{};", &*reference)),
                };
                push_text(stack.last_mut()?, &text);
            }
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    let mut top = stack.pop()?;
    if !stack.is_empty() {
        return None;
    }
    match top.children.pop() {
        Some(Node::Element(storage)) if top.children.is_empty() => Some(storage),
        _ => None,
    }
}

fn element_of(start: &BytesStart) -> Option<Element> {
    let name = start.name().as_ref().to_string();
    let mut attributes = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.ok()?;
        let key = attribute.key.as_ref().to_string();
        let value = attribute.normalized_value_with(XmlVersion::Implicit1_0, 16, resolve_html5_entity).ok()?;
        attributes.push((key, value.into_owned()));
    }
    Some(Element { name, attributes, children: Vec::new() })
}

/// Adds text, joined to the text before it.
fn push_text(element: &mut Element, text: &str) {
    if let Some(Node::Text(previous)) = element.children.last_mut() {
        previous.push_str(text);
    } else {
        element.children.push(Node::Text(text.to_string()));
    }
}
