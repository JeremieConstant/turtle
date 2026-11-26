//! Reading a ZIP of Markdown files, as Turtle exports them or from anywhere else.

use super::{Bundle, Incoming, IncomingAttachment, Problem, Target};
use crate::model::{Block, Inline, Marks};
use crate::ops::tree::Kind;
use crate::ops::OpError;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Read;

/// Reads a ZIP sent in base64. It is taken whole and let go of as soon as
/// it is read, since an import holds it in memory, and each file's bytes are
/// handed on rather than copied.
pub(super) fn read(content: String) -> Result<Bundle, OpError> {
    let bytes = crate::ops::attachments::decode_content(&content)?;
    drop(content);
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| OpError::InvalidInput("Choose a ZIP file.".into()))?;
    let mut files = BTreeMap::new();
    let mut problems = Vec::new();
    for index in 0..archive.len() {
        let name = archive.name_for_index(index).and_then(Result::ok).map(|name| name.to_string()).unwrap_or_else(|| format!("file {}", index + 1));
        let mut content = Vec::new();
        let read = archive.by_index(index).and_then(|mut file| {
            if file.is_dir() {
                return Ok(None);
            }
            file.read_to_end(&mut content)?;
            Ok(Some(file.name().unwrap_or_default().to_string()))
        });
        let path = match read {
            Ok(Some(path)) if added_by_a_mac(&path) => continue,
            Ok(Some(path)) => path,
            Ok(None) => continue,
            Err(_) => {
                problems.push(Problem::new(name, "It could not be read from the ZIP, so it was left out."));
                continue;
            }
        };
        if path.ends_with(".md") && std::str::from_utf8(&content).is_err() {
            problems.push(Problem::new(path, "It is not Markdown in UTF-8, so it was left out."));
            continue;
        }
        files.insert(path, content);
    }
    drop(archive);
    let mut entries = entries_in("", &files);
    let mut attached = HashSet::new();
    collect_attached(&entries, &mut attached);
    attach_linked(&mut entries, &mut attached);
    hand_over_content(&mut entries, &mut files);
    Ok(Bundle { source: "a ZIP of Markdown files".into(), entries, problems })
}

/// Moves each Attachment's bytes out of the files, where each is found once.
fn hand_over_content(entries: &mut [Incoming], files: &mut BTreeMap<String, Vec<u8>>) {
    for entry in entries {
        for attachment in &mut entry.attachments {
            attachment.content = files.remove(&attachment.key).unwrap_or_default();
        }
        hand_over_content(&mut entry.children, files);
    }
}

/// Whether a file is one the Finder adds when it makes a ZIP, which holds
/// nothing of what was zipped.
fn added_by_a_mac(path: &str) -> bool {
    path.starts_with("__MACOSX/") || path.rsplit('/').next() == Some(".DS_Store")
}

fn collect_attached(entries: &[Incoming], attached: &mut HashSet<String>) {
    for entry in entries {
        attached.extend(entry.attachments.iter().map(|attachment| attachment.key.clone()));
        collect_attached(&entry.children, attached);
    }
}

/// Makes each file a Page links to, which is not an Attachment already,
/// an Attachment of the first Page in tree order that links to it.
fn attach_linked(entries: &mut [Incoming], attached: &mut HashSet<String>) {
    for entry in entries {
        let mut linked: Vec<&String> = entry
            .links
            .values()
            .filter_map(|target| match target {
                Target::Attachment(key) => Some(key),
                Target::Entry(_) => None,
            })
            .collect();
        linked.sort();
        linked.dedup();
        let new: Vec<IncomingAttachment> = linked
            .into_iter()
            .filter(|key| attached.insert((*key).clone()))
            .map(|key| {
                let name = key.rsplit('/').next().unwrap_or(key).to_string();
                IncomingAttachment { key: key.clone(), name, content: Vec::new() }
            })
            .collect();
        entry.attachments.extend(new);
        attach_linked(&mut entry.children, attached);
    }
}

/// The Pages and Folders in the directory `within`, given as a path ending
/// in `/`, or empty for the top. A Markdown file is a Page and a directory
/// of the same name holds what is beneath it; a directory without one is a
/// Folder.
fn entries_in(within: &str, files: &BTreeMap<String, Vec<u8>>) -> Vec<Incoming> {
    let mut names = BTreeSet::new();
    for path in files.keys() {
        let Some(rest) = path.strip_prefix(within) else { continue };
        // A Page's Attachments are its own, not Pages beneath it.
        if rest.starts_with(&format!("{ATTACHMENTS}/")) && files.contains_key(&format!("{}.md", within.trim_end_matches('/'))) {
            continue;
        }
        match rest.split_once('/') {
            // A directory without Markdown in it holds files for Pages to
            // refer to, not Pages.
            Some((directory, _)) if holds_markdown(&format!("{within}{directory}/"), files) => {
                names.insert(directory.to_string())
            }
            Some(_) => false,
            None => match rest.strip_suffix(".md") {
                Some(stem) => names.insert(stem.to_string()),
                None => false,
            },
        };
    }
    // Names come in order, so files without a position stay in name order
    // after those that have one.
    let mut placed: Vec<(Option<u64>, Incoming)> = names
        .into_iter()
        .map(|name| {
            let children = entries_in(&format!("{within}{name}/"), files);
            let Some(file) = files.get(&format!("{within}{name}.md")) else {
                let key = format!("{within}{name}/");
                let folder = Incoming { key, kind: Kind::Folder, title: name, id: None, content: Vec::new(), tags: Vec::new(), links: HashMap::new(), attachments: Vec::new(), children };
                return (None, folder);
            };
            let key = format!("{within}{name}.md");
            let text = String::from_utf8_lossy(file);
            let (frontmatter, body) = Frontmatter::split(&text);
            let kind = match frontmatter.scalar("kind") {
                Some("folder") => Kind::Folder,
                _ => Kind::Page,
            };
            let (mut content, tags) = match kind {
                Kind::Page => (crate::markdown::from_markdown(body), frontmatter.list("tags")),
                Kind::Folder => (Vec::new(), Vec::new()),
            };
            let links = links_in(&mut content, within, files);
            let title = frontmatter.scalar("title").map_or_else(|| name.clone(), str::to_string);
            let position = frontmatter.scalar("position").and_then(|position| position.parse().ok());
            let attachments = match kind {
                Kind::Page => attachments_in(&format!("{within}{name}/{ATTACHMENTS}/"), files),
                Kind::Folder => Vec::new(),
            };
            let id = frontmatter.scalar("id").map(str::to_string);
            (position, Incoming { key, kind, title, id, content, tags, links, attachments, children })
        })
        .collect();
    placed.sort_by_key(|(position, _)| (position.is_none(), *position));
    placed.into_iter().map(|(_, incoming)| incoming).collect()
}

/// The frontmatter of a Markdown file: the lines between a first line of
/// `---` and the next. Turtle writes its values as JSON, but files from
/// elsewhere carry YAML, so plain and single-quoted values, `[a, b]` lists
/// and lists of `- item` lines are read too. Keys it does not know are kept
/// and ignored.
#[derive(Default)]
struct Frontmatter(BTreeMap<String, Value>);

enum Value {
    Scalar(String),
    List(Vec<String>),
}

impl Frontmatter {
    /// The frontmatter of a file, and what follows it.
    fn split(text: &str) -> (Frontmatter, &str) {
        let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) else {
            return (Frontmatter::default(), text);
        };
        let mut end = None;
        let mut offset = 0;
        for line in rest.split_inclusive('\n') {
            if line.trim_end() == "---" {
                end = Some((offset, offset + line.len()));
                break;
            }
            offset += line.len();
        }
        let Some((head_end, body_start)) = end else { return (Frontmatter::default(), text) };
        (Frontmatter::parse(&rest[..head_end]), &rest[body_start..])
    }

    fn parse(head: &str) -> Frontmatter {
        let mut values = BTreeMap::new();
        let mut list_of: Option<String> = None;
        for line in head.lines() {
            let trimmed = line.trim();
            if let (Some(key), Some(item)) = (&list_of, trimmed.strip_prefix("- ")) {
                if let Some(Value::List(items)) = values.get_mut(key) {
                    items.push(unquoted(item));
                }
                continue;
            }
            list_of = None;
            let Some((key, value)) = line.split_once(':') else { continue };
            let (key, value) = (key.trim().to_string(), value.trim());
            if value.is_empty() {
                values.insert(key.clone(), Value::List(Vec::new()));
                list_of = Some(key);
            } else if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                let items = serde_json::from_str::<Vec<String>>(value).unwrap_or_else(|_| {
                    inner.split(',').map(unquoted).filter(|item| !item.is_empty()).collect()
                });
                values.insert(key, Value::List(items));
            } else {
                values.insert(key, Value::Scalar(unquoted(value)));
            }
        }
        Frontmatter(values)
    }

    fn list(&self, key: &str) -> Vec<String> {
        match self.0.get(key) {
            Some(Value::List(items)) => items.clone(),
            _ => Vec::new(),
        }
    }

    fn scalar(&self, key: &str) -> Option<&str> {
        match self.0.get(key) {
            Some(Value::Scalar(value)) => Some(value),
            _ => None,
        }
    }
}

/// A YAML scalar without its quotes: JSON's double quotes, as Turtle writes
/// them, or single quotes, in which `''` stands for one.
fn unquoted(value: &str) -> String {
    let value = value.trim();
    if value.starts_with('"') {
        if let Ok(text) = serde_json::from_str::<String>(value) {
            return text;
        }
    }
    match value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        Some(inner) => inner.replace("''", "'"),
        None => value.to_string(),
    }
}

/// Whether there is a Markdown file somewhere in the directory `within`,
/// other than among a Page's Attachments.
fn holds_markdown(within: &str, files: &BTreeMap<String, Vec<u8>>) -> bool {
    files.keys().any(|path| {
        path.strip_prefix(within)
            .is_some_and(|rest| rest.ends_with(".md") && !rest.split('/').any(|part| part == ATTACHMENTS))
    })
}

/// The directory in a Page's directory that holds its Attachments.
const ATTACHMENTS: &str = "_attachments";

/// The files directly in the directory `within`, as Attachments whose
/// content is handed over once all are known.
fn attachments_in(within: &str, files: &BTreeMap<String, Vec<u8>>) -> Vec<IncomingAttachment> {
    files
        .keys()
        .filter_map(|path| {
            let name = path.strip_prefix(within).filter(|name| !name.contains('/'))?;
            Some(IncomingAttachment { key: path.clone(), name: name.to_string(), content: Vec::new() })
        })
        .collect()
}

/// What the relative links in a file in the directory `within` point to
/// among the ZIP's files, by the address they are written with.
fn links_in(content: &mut [Block], within: &str, files: &BTreeMap<String, Vec<u8>>) -> HashMap<String, Target> {
    let mut links = HashMap::new();
    crate::ops::export::for_each_inline(content, &mut |inline| {
        let (Inline::Image { src: address, .. } | Inline::Text { marks: Marks { link: Some(address), .. }, .. }) = inline else {
            return;
        };
        let Some(path) = resolve(within, address) else { return };
        if !files.contains_key(&path) {
            return;
        }
        let target = if path.ends_with(".md") && !path.contains(&format!("/{ATTACHMENTS}/")) {
            Target::Entry(path)
        } else {
            Target::Attachment(path)
        };
        links.insert(address.clone(), target);
    });
    links
}

/// The path in the ZIP a relative address in a file in the directory
/// `within` leads to, without what follows a `#`. None for an address with
/// a scheme, such as `https:` or `turtle:`, or from the top of a site.
fn resolve(within: &str, address: &str) -> Option<String> {
    let address = address.split('#').next().unwrap_or_default();
    let has_scheme = address.split_once(':').is_some_and(|(scheme, _)| !scheme.contains('/'));
    if address.is_empty() || has_scheme || address.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = within.split('/').filter(|part| !part.is_empty()).collect();
    let decoded = percent_decoded(address);
    for part in decoded.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// The address with each `%` and two hex digits turned into the byte they
/// stand for, as the export encodes `%` and `#` in names.
fn percent_decoded(address: &str) -> String {
    let bytes = address.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = bytes.get(index + 1..index + 3).and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        match (bytes[index], hex) {
            (b'%', Some(byte)) => {
                decoded.push(byte);
                index += 3;
            }
            (byte, _) => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}
