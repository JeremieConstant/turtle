//! The generic directive syntax Markdown is extended with (ADR-0013):
//! `:name[text]{key=value}` in running text and `:::name{key=value}` … `:::`
//! as a block.
//!
//! Turtle's own directives have bare names, such as `status`; anyone else's
//! carry a namespace, such as `acme.board`. Which directives Turtle knows is
//! decided where content is read from Markdown; here a directive is only its
//! syntax.

/// A directive as written: its name, the text in its brackets and its
/// attributes in the order written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub name: String,
    /// The text between the brackets, with its escapes read; `None` without brackets.
    pub label: Option<String>,
    pub attributes: Vec<(String, String)>,
}

impl Directive {
    /// Whether the name carries a namespace, as every directive that is not
    /// Turtle's own must.
    #[allow(dead_code)] // The first of Turtle's own directives, the Status, reads it.
    pub fn is_namespaced(&self) -> bool {
        self.name.contains('.')
    }

    /// The directive written for running text.
    #[allow(dead_code)] // Turtle's own directives, starting with the Status, write themselves with it.
    pub fn to_inline_markdown(&self) -> String {
        let mut out = format!(":{}", self.name);
        if let Some(label) = &self.label {
            out.push('[');
            for c in label.chars() {
                if matches!(c, '\\' | '[' | ']') {
                    out.push('\\');
                }
                // A directive in running text stays on one line.
                out.push(if c == '\n' || c == '\r' { ' ' } else { c });
            }
            out.push(']');
        }
        if !self.attributes.is_empty() {
            let pairs: Vec<String> = self
                .attributes
                .iter()
                .map(|(key, value)| {
                    let plain = !value.is_empty() && value.chars().all(is_unquoted_value_char);
                    if plain {
                        format!("{key}={value}")
                    } else {
                        format!("{key}=\"{}\"", value.replace(['"', '\n', '\r'], "'"))
                    }
                })
                .collect();
            out.push_str(&format!("{{{}}}", pairs.join(" ")));
        }
        out
    }
}

/// The directive written in running text at the start of `text`, and how
/// many bytes it takes. It has a name and brackets, braces or both, stays on
/// one line and holds no `|`, which would end a table cell.
pub fn inline_at(text: &str) -> Option<(Directive, usize)> {
    let rest = text.strip_prefix(':')?;
    let (directive, used) = name_label_attributes(rest)?;
    let len = 1 + used;
    if used == directive.name.len() {
        return None;
    }
    if text[..len].contains('|') {
        return None;
    }
    Some((directive, len))
}

/// The block directive at the start of `text`: from its opening line, such as
/// `:::acme.board{x=1}`, to its closing line of as many colons or more, which
/// is not counted with its line ending. Directives inside it may close with
/// their own lines. Without a closing line, there is no block directive.
pub fn block_at(text: &str) -> Option<(Directive, usize)> {
    let mut lines = lines_with_ends(text);
    let (first, _) = lines.next()?;
    let fence = first.chars().take_while(|c| *c == ':').count();
    if fence < 3 {
        return None;
    }
    let (directive, used) = name_label_attributes(&first[fence..])?;
    if !first[fence + used..].trim().is_empty() {
        return None;
    }
    let mut open: Vec<usize> = Vec::new();
    let mut code = CodeFences::default();
    for (line, start) in lines {
        if code.is_code(line) {
            continue;
        }
        let colons = line.chars().take_while(|c| *c == ':').count();
        if colons < 3 {
            continue;
        }
        if line[colons..].trim().is_empty() {
            match open.last() {
                Some(inner) if colons >= *inner => {
                    open.pop();
                }
                None if colons >= fence => return Some((directive, start + line.len())),
                _ => {}
            }
        } else if name_label_attributes(&line[colons..]).is_some() {
            open.push(colons);
        }
    }
    None
}

/// Follows the fenced code blocks in a text line by line.
#[derive(Default)]
pub struct CodeFences {
    /// The open fence, by its character and length.
    open: Option<(char, usize)>,
}

impl CodeFences {
    /// Whether the next line opens, holds or closes fenced code.
    pub fn is_code(&mut self, line: &str) -> bool {
        let indent = line.len() - line.trim_start_matches(' ').len();
        let unindented = &line[indent..];
        let fence_char = unindented.chars().next().filter(|c| *c == '`' || *c == '~');
        let run = fence_char.map_or(0, |c| unindented.chars().take_while(|x| *x == c).count());
        if let Some((open_char, open_len)) = self.open {
            if indent < 4 && fence_char == Some(open_char) && run >= open_len && unindented[run..].trim().is_empty() {
                self.open = None;
            }
            return true;
        }
        match fence_char {
            // A backtick fence's info string holds no backtick.
            Some(c) if indent < 4 && run >= 3 && !(c == '`' && unindented[run..].contains('`')) => {
                self.open = Some((c, run));
                true
            }
            _ => false,
        }
    }
}

/// The lines of a text without their line endings, each with where it starts.
pub fn lines_with_ends(text: &str) -> impl Iterator<Item = (&str, usize)> {
    let mut start = 0;
    text.split_inclusive('\n').map(move |line| {
        let at = start;
        start += line.len();
        (line.trim_end_matches(['\n', '\r']), at)
    })
}

/// The name of the directive a source starts with, after its colons.
pub fn name_of(source: &str) -> Option<&str> {
    let rest = source.trim_start_matches(':');
    let len = name_len(rest)?;
    Some(&rest[..len])
}

/// Reads a name, then the optional brackets and braces that follow it.
fn name_label_attributes(text: &str) -> Option<(Directive, usize)> {
    let name_end = name_len(text)?;
    let name = text[..name_end].to_string();
    let mut at = name_end;
    let label = if text[at..].starts_with('[') {
        let (label, used) = label(&text[at..])?;
        at += used;
        Some(label)
    } else {
        None
    };
    let attributes = if text[at..].starts_with('{') {
        let (attributes, used) = attributes(&text[at..])?;
        at += used;
        attributes
    } else {
        Vec::new()
    };
    Some((Directive { name, label, attributes }, at))
}

/// How long the name at the start of `text` is: words of letters, digits,
/// `-` and `_` joined by dots, each starting with a letter.
fn name_len(text: &str) -> Option<usize> {
    let mut len = 0;
    let mut segment_start = true;
    for c in text.chars() {
        if segment_start {
            if !c.is_ascii_alphabetic() {
                break;
            }
            segment_start = false;
        } else if c == '.' {
            segment_start = true;
        } else if !(c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            break;
        }
        len += c.len_utf8();
    }
    // A name ends in a letter or digit, not in a dot.
    let len = text[..len].trim_end_matches('.').len();
    (len > 0).then_some(len)
}

/// The text between brackets at the start of `text`, which may hold
/// balanced brackets and escapes, and how many bytes it takes.
fn label(text: &str) -> Option<(String, usize)> {
    let mut depth = 0usize;
    let mut out = String::new();
    let mut chars = text.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        match c {
            '\n' | '\r' => return None,
            '\\' => match chars.next() {
                Some((_, escaped)) if escaped.is_ascii_punctuation() => out.push(escaped),
                Some((_, '\n' | '\r')) | None => return None,
                Some((_, other)) => {
                    out.push('\\');
                    out.push(other);
                }
            },
            '[' => {
                depth += 1;
                out.push(c);
            }
            ']' if depth == 0 => return Some((out, i + 1)),
            ']' => {
                depth -= 1;
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    None
}

fn is_unquoted_value_char(c: char) -> bool {
    !c.is_whitespace() && !matches!(c, '"' | '\'' | '=' | '<' | '>' | '`' | '{' | '}')
}

/// The attributes between braces at the start of `text`: `key=value`,
/// `key="value"`, `key='value'` or a bare `key`, apart by spaces.
fn attributes(text: &str) -> Option<(Vec<(String, String)>, usize)> {
    let bytes = text.as_bytes();
    let mut at = 1;
    let mut out = Vec::new();
    loop {
        while at < bytes.len() && (bytes[at] == b' ' || bytes[at] == b'\t') {
            at += 1;
        }
        match bytes.get(at)? {
            b'}' => return Some((out, at + 1)),
            _ => {
                let key_len = text[at..]
                    .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')))
                    .unwrap_or(text.len() - at);
                if key_len == 0 || !bytes[at].is_ascii_alphabetic() && bytes[at] != b'_' {
                    return None;
                }
                let key = text[at..at + key_len].to_string();
                at += key_len;
                if bytes.get(at) != Some(&b'=') {
                    out.push((key, String::new()));
                    continue;
                }
                at += 1;
                let value = match bytes.get(at)? {
                    quote @ (b'"' | b'\'') => {
                        let end = text[at + 1..].find(*quote as char)?;
                        let value = &text[at + 1..at + 1 + end];
                        if value.contains(['\n', '\r']) {
                            return None;
                        }
                        at += end + 2;
                        value.to_string()
                    }
                    _ => {
                        let len = text[at..].find(|c| !is_unquoted_value_char(c)).unwrap_or(text.len() - at);
                        if len == 0 {
                            return None;
                        }
                        let value = text[at..at + len].to_string();
                        at += len;
                        value
                    }
                };
                // Attributes are set apart by spaces.
                if !matches!(bytes.get(at), Some(b' ' | b'\t' | b'}')) {
                    return None;
                }
                out.push((key, value));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directive(name: &str, label: Option<&str>, attributes: &[(&str, &str)]) -> Directive {
        Directive {
            name: name.into(),
            label: label.map(Into::into),
            attributes: attributes.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    #[test]
    fn an_inline_directive_has_a_name_brackets_and_braces() {
        assert_eq!(
            inline_at(":status[ON TRACK]{color=green} more"),
            Some((directive("status", Some("ON TRACK"), &[("color", "green")]), 30))
        );
        let written = ":acme.demo[a \\] [b]]{x=\"1 2\" y='z' flag}";
        assert_eq!(
            inline_at(&format!("{written}, then text")),
            Some((directive("acme.demo", Some("a ] [b]"), &[("x", "1 2"), ("y", "z"), ("flag", "")]), written.len()))
        );
        assert_eq!(inline_at(":acme.demo{}"), Some((directive("acme.demo", None, &[]), 12)));
    }

    #[test]
    fn text_that_merely_has_a_colon_is_no_directive() {
        for text in [":", ":x", "::x[a]", ":1x[a]", ":x[a", ":x{a", ":x{a=}", ":x{=a}", ":x[a\nb]", ":x{a=\"b}", ":x[a|b]", ":x{a=1b=2}"] {
            assert_eq!(inline_at(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_directive_written_reads_back_as_itself() {
        for written in [
            directive("status", Some("ON [TRACK] \\ ok"), &[("color", "green")]),
            directive("acme.demo", None, &[("x", "a b"), ("y", ""), ("z", "1")]),
            directive("acme.demo", Some(""), &[]),
        ] {
            let markdown = written.to_inline_markdown();
            assert_eq!(inline_at(&markdown), Some((written, markdown.len())), "{markdown}");
        }
    }

    #[test]
    fn a_block_directive_runs_to_its_closing_line() {
        let text = ":::acme.board{x=1}\nOne\n\n::::acme.lane\n:::\n::::\n\n:::\nafter";
        assert_eq!(block_at(text), Some((directive("acme.board", None, &[("x", "1")]), text.len() - "\nafter".len())));
        let long = "::::acme.x\n:::\nstill in\n::::";
        assert_eq!(block_at(&format!("{long}\n")), Some((directive("acme.x", None, &[]), long.len())));
        assert_eq!(block_at(":::acme.x\nnever closed\n"), None);
        assert_eq!(block_at(":::acme.x trailing words\n:::"), None);
        assert_eq!(block_at("::acme.x\n::"), None);
    }

    #[test]
    fn the_name_and_its_namespace() {
        assert_eq!(name_of(":::acme.board{x=1}"), Some("acme.board"));
        assert_eq!(name_of(":status[x]"), Some("status"));
        assert!(directive("acme.board", None, &[]).is_namespaced());
        assert!(!directive("status", None, &[]).is_namespaced());
    }
}
