//! Reading `entities.xml`, the heart of Confluence's XML export: every
//! object of the space, one after another, as Hibernate writes them.
//!
//! An object is written like this, with properties holding text or naming
//! another object, and collections naming several:
//!
//! ```xml
//! <object class="Page" package="…">
//!   <id name="id">141557967</id>
//!   <property name="title"><![CDATA[Backups]]></property>
//!   <property name="parent" class="Page" package="…"><id name="id">131398</id></property>
//!   <collection name="bodyContents" class="…"><element class="BodyContent" package="…"><id name="id">141689117</id></element></collection>
//! </object>
//! ```

use quick_xml::escape::resolve_html5_entity;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(super) struct Object {
    pub class: String,
    pub id: String,
    /// Properties holding text, by name.
    pub text: HashMap<String, String>,
    /// Properties naming another object, by name: that object's ID.
    pub refs: HashMap<String, String>,
    /// Collections, by name: the IDs of the objects in them.
    pub collections: HashMap<String, Vec<String>>,
}

impl Object {
    pub fn text(&self, name: &str) -> Option<&str> {
        self.text.get(name).map(String::as_str)
    }

    pub fn points_to(&self, name: &str) -> Option<&str> {
        self.refs.get(name).map(String::as_str)
    }
}

/// Every object `keep` keeps, in the order written, and how many objects
/// of each class there are; nothing when it is not well-formed. The file is
/// read as it comes, one object at a time: it can be far larger than what
/// is kept of it.
pub(super) fn read(xml: impl std::io::BufRead, keep: impl Fn(&Object) -> bool) -> Option<(Vec<Object>, HashMap<String, u32>)> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().check_end_names = true;
    let mut event = Vec::new();
    let mut objects = Vec::new();
    let mut counts: HashMap<String, u32> = HashMap::new();
    // The names of the elements open inside the current object, and the
    // name attribute of the property or collection open, if any.
    let mut path: Vec<String> = Vec::new();
    let mut current: Option<Object> = None;
    let mut field = String::new();
    let mut text = String::new();
    loop {
        event.clear();
        match reader.read_event_into(&mut event).ok()? {
            Event::Start(start) => {
                let name = start.name().as_ref().to_string();
                if name == "object" && current.is_none() {
                    let class = start.try_get_attribute("class").ok()??.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok()?;
                    current = Some(Object { class: class.into_owned(), ..Object::default() });
                    path.clear();
                    continue;
                }
                if current.is_some() {
                    if path.is_empty() && (name == "property" || name == "collection") {
                        let attribute = start.try_get_attribute("name").ok()??;
                        field = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok()?.into_owned();
                        if name == "collection" {
                            current.as_mut()?.collections.entry(field.clone()).or_default();
                        }
                    }
                    path.push(name);
                    text.clear();
                }
            }
            Event::End(end) => {
                let name = end.name().as_ref().to_string();
                if name == "object" && path.is_empty() {
                    if let Some(object) = current.take() {
                        *counts.entry(object.class.clone()).or_default() += 1;
                        if keep(&object) {
                            objects.push(object);
                        }
                    }
                    continue;
                }
                let Some(object) = current.as_mut() else { continue };
                let shape: Vec<&str> = path.iter().map(String::as_str).collect();
                match shape.as_slice() {
                    ["id"] => object.id = std::mem::take(&mut text),
                    ["property"] if !object.refs.contains_key(&field) => {
                        object.text.insert(field.clone(), std::mem::take(&mut text));
                    }
                    ["property", "id"] => {
                        object.refs.insert(field.clone(), std::mem::take(&mut text));
                    }
                    ["collection", "element", "id"] => {
                        object.collections.entry(field.clone()).or_default().push(std::mem::take(&mut text));
                    }
                    _ => {}
                }
                path.pop();
            }
            Event::Empty(start) => {
                // An empty collection is still a collection: a page with no
                // body is told apart from one without the collection.
                if current.is_some() && path.is_empty() && start.name().as_ref() == "collection" {
                    let attribute = start.try_get_attribute("name").ok()??;
                    let name = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok()?.into_owned();
                    current.as_mut()?.collections.entry(name).or_default();
                }
            }
            Event::Text(content) if current.is_some() => text.push_str(&content.xml10_content()),
            Event::CData(content) if current.is_some() => text.push_str(&content.xml10_content()),
            Event::GeneralRef(reference) if current.is_some() => match reference.resolve_char_ref().ok()? {
                Some(c) => text.push(c),
                // An entity no one knows is kept as written, as in a page's content.
                None => match resolve_html5_entity(&reference) {
                    Some(resolved) => text.push_str(resolved),
                    None => text.push_str(&format!("&{};", &*reference)),
                },
            },
            Event::Eof => break,
            _ => {}
        }
    }
    // An object left open means the file ends early.
    current.is_none().then_some((objects, counts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_are_read_with_their_text_what_they_point_to_and_collections() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<hibernate-generic datetime="2026-11-26 11:19:52.169">
<object class="Page" package="com.atlassian.confluence.pages">
<id name="id">7</id>
<property name="title"><![CDATA[Fish & Chips]]></property>
<property name="position">3</property>
<property name="originalVersionId"/>
<property name="parent" class="Page" package="com.atlassian.confluence.pages"><id name="id">1</id>
</property>
<collection name="bodyContents" class="java.util.Collection"><element class="BodyContent" package="com.atlassian.confluence.core"><id name="id">70</id>
</element>
</collection>
<collection name="labellings" class="java.util.Collection"></collection>
</object>
<object class="Notification" package="com.atlassian.confluence.mail.notification">
<id name="id">8</id>
</object>
</hibernate-generic>"#;

        let (objects, counts) = read(xml.as_bytes(), |object| object.class == "Page").unwrap();

        assert_eq!(objects.len(), 1);
        let page = &objects[0];
        assert_eq!((page.class.as_str(), page.id.as_str()), ("Page", "7"));
        assert_eq!(page.text("title"), Some("Fish & Chips"));
        assert_eq!(page.text("position"), Some("3"));
        assert_eq!(page.text("originalVersionId"), None);
        assert_eq!(page.points_to("parent"), Some("1"));
        assert_eq!(page.collections["bodyContents"], ["70"]);
        assert!(page.collections["labellings"].is_empty());
        assert_eq!(counts["Page"], 1);
        assert_eq!(counts["Notification"], 1);
    }
}
