//! Reads and writes Page content in the collaborative document.
//!
//! The layout is the one the editor produces: an XML fragment whose elements
//! are named after the editor's node types, with text formatting stored as
//! attributes on text ranges. The `the_editor_contract` tests here and
//! `web/scripts/editor-contract.test.ts` hold the two to each other through shared
//! fixtures in `tests/fixtures`.

use crate::diff::{align, common_edges, text_hunks, Fit, Step};
use crate::model::{normalize, Block, CalloutKind, Cell, Inline, Marks, Row, StatusColor, TaskItem};
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
/// The element a Page link is stored as, and its attribute naming the Page.
const PAGE_LINK: &str = "pageLink";
const PAGE_ID: &str = "pageId";
/// The elements a task list and its items are stored as, and whether an item is checked.
const TASK_LIST: &str = "taskList";
const TASK_ITEM: &str = "taskItem";
const CHECKED: &str = "checked";
/// The element a callout is stored as, and its attribute naming the kind.
const CALLOUT: &str = "callout";
const KIND: &str = "kind";
/// The elements a table is stored as: rows of cells, the first row's being
/// header cells. Each cell holds one paragraph.
const TABLE: &str = "table";
const TABLE_ROW: &str = "tableRow";
const TABLE_HEADER: &str = "tableHeader";
const TABLE_CELL: &str = "tableCell";
/// The elements a directive Turtle does not know is stored as, in running
/// text and as a block, and their attribute holding its source.
const UNKNOWN_DIRECTIVE: &str = "unknownDirective";
const UNKNOWN_DIRECTIVE_BLOCK: &str = "unknownDirectiveBlock";
const SOURCE: &str = "source";
/// The element a Status is stored as, and its attributes.
const STATUS: &str = "status";
const STATUS_TEXT: &str = "text";
const STATUS_COLOR: &str = "color";

/// The content of a Page from its stored state.
pub fn read_state(state: &[u8]) -> anyhow::Result<Vec<Block>> {
    use yrs::updates::decoder::Decode;
    let doc = Doc::new();
    doc.transact_mut().apply_update(yrs::Update::decode_v1(state)?)?;
    Ok(read(&doc))
}

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
                        Inline::Image { .. } => String::new(),
                        Inline::PageLink { .. } => String::new(),
                        Inline::UnknownDirective { source } => source,
                        Inline::Status { text, .. } => text,
                    })
                    .collect(),
            }),
            "blockquote" => blocks.push(Block::Blockquote(read_blocks(txn, element.children(txn)))),
            CALLOUT => blocks.push(Block::Callout {
                kind: callout_kind(txn, &element),
                content: read_blocks(txn, element.children(txn)),
            }),
            "bulletList" => blocks.push(Block::BulletList(blocks_of(read_items(txn, &element)))),
            "orderedList" => blocks.push(Block::OrderedList {
                start: number_attribute(txn, &element, "start").unwrap_or(1),
                items: blocks_of(read_items(txn, &element)),
            }),
            TASK_LIST => blocks.push(Block::TaskList(
                read_items(txn, &element)
                    .into_iter()
                    .map(|(checked, content)| TaskItem { checked, content })
                    .collect(),
            )),
            TABLE => blocks.push(Block::Table(read_rows(txn, &element))),
            "horizontalRule" => blocks.push(Block::HorizontalRule),
            UNKNOWN_DIRECTIVE_BLOCK => blocks.push(Block::UnknownDirective {
                source: string_attribute(txn, &element, SOURCE).unwrap_or_default(),
            }),
            // An element this version does not know keeps its content.
            _ => blocks.extend(read_blocks(txn, element.children(txn))),
        }
    }
    blocks
}

/// The items of a list, each with whether it is checked and what it holds.
fn read_items<T: ReadTxn>(txn: &T, list: &XmlElementRef) -> Vec<(bool, Vec<Block>)> {
    list.children(txn).map(|item| read_item(txn, &item)).collect()
}

fn read_item<T: ReadTxn>(txn: &T, item: &XmlOut) -> (bool, Vec<Block>) {
    match item {
        XmlOut::Element(item) => (is_checked(txn, item), read_blocks(txn, item.children(txn))),
        other => (false, read_blocks(txn, std::iter::once(other.clone()))),
    }
}

fn read_rows<T: ReadTxn>(txn: &T, table: &XmlElementRef) -> Vec<Row> {
    table
        .children(txn)
        .filter_map(|row| match row {
            XmlOut::Element(row) => Some(row.children(txn).map(|cell| read_cell(txn, &cell)).collect()),
            _ => None,
        })
        .collect()
}

/// What a cell holds. A cell holds one paragraph; should it hold more blocks,
/// their lines are kept, one after the other.
fn read_cell<T: ReadTxn>(txn: &T, cell: &XmlOut) -> Cell {
    let blocks = match cell {
        XmlOut::Element(cell) => read_blocks(txn, cell.children(txn)),
        other => read_blocks(txn, std::iter::once(other.clone())),
    };
    let mut inlines = Vec::new();
    for block in normalize(blocks) {
        if let Block::Paragraph(content) | Block::Heading { content, .. } = block {
            if !inlines.is_empty() {
                inlines.push(Inline::HardBreak);
            }
            inlines.extend(content);
        }
    }
    inlines
}

/// The kind of a callout; one of a kind this version does not know is a note.
fn callout_kind<T: ReadTxn>(txn: &T, element: &XmlElementRef) -> CalloutKind {
    string_attribute(txn, element, KIND)
        .and_then(|name| CalloutKind::named(&name))
        .unwrap_or(CalloutKind::Note)
}

fn blocks_of(items: Vec<(bool, Vec<Block>)>) -> Vec<Vec<Block>> {
    items.into_iter().map(|(_, blocks)| blocks).collect()
}

fn is_checked<T: ReadTxn>(txn: &T, element: &XmlElementRef) -> bool {
    match element.get_attribute(txn, CHECKED) {
        Some(Out::Any(Any::Bool(checked))) => checked,
        Some(Out::Any(Any::String(s))) => s.as_ref() == "true",
        _ => false,
    }
}

fn read_inlines<T: ReadTxn>(txn: &T, element: &XmlElementRef) -> Vec<Inline> {
    let mut inlines = Vec::new();
    for child in element.children(txn) {
        match child {
            XmlOut::Text(text) => inlines.extend(read_text(txn, &text)),
            XmlOut::Element(child) if child.tag().as_ref() == "hardBreak" => {
                inlines.push(Inline::HardBreak)
            }
            XmlOut::Element(child) if child.tag().as_ref() == "image" => inlines.push(read_image(txn, &child)),
            XmlOut::Element(child) if child.tag().as_ref() == PAGE_LINK => {
                if let Some(page_id) = string_attribute(txn, &child, PAGE_ID) {
                    inlines.push(Inline::PageLink { page_id });
                }
            }
            XmlOut::Element(child) if child.tag().as_ref() == UNKNOWN_DIRECTIVE => {
                if let Some(source) = string_attribute(txn, &child, SOURCE) {
                    inlines.push(Inline::UnknownDirective { source });
                }
            }
            XmlOut::Element(child) if child.tag().as_ref() == STATUS => inlines.push(read_status(txn, &child)),
            _ => {}
        }
    }
    inlines
}

/// A Status as the editor stores it: a `status` element with its text and
/// the name of its colour as attributes.
fn read_status<T: ReadTxn>(txn: &T, element: &XmlElementRef) -> Inline {
    Inline::Status {
        text: string_attribute(txn, element, STATUS_TEXT).unwrap_or_default(),
        color: string_attribute(txn, element, STATUS_COLOR).map(|name| StatusColor::named(&name)).unwrap_or_default(),
    }
}

/// Gives a stored Status the text and colour it does not have yet.
fn set_status(txn: &mut TransactionMut, element: &XmlElementRef, text: &str, color: StatusColor) {
    if string_attribute(txn, element, STATUS_TEXT).as_deref() != Some(text) {
        element.insert_attribute(txn, STATUS_TEXT, Any::from(text));
    }
    if string_attribute(txn, element, STATUS_COLOR).as_deref() != Some(color.name()) {
        element.insert_attribute(txn, STATUS_COLOR, Any::from(color.name()));
    }
}

/// An image as the editor stores it: an `image` element with its address and
/// alternative text as attributes.
fn read_image<T: ReadTxn>(txn: &T, element: &XmlElementRef) -> Inline {
    Inline::Image {
        src: string_attribute(txn, element, "src").unwrap_or_default(),
        alt: string_attribute(txn, element, "alt").unwrap_or_default(),
    }
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
        Block::Callout { .. } => CALLOUT,
        Block::BulletList(_) => "bulletList",
        Block::OrderedList { .. } => "orderedList",
        Block::TaskList(_) => TASK_LIST,
        Block::Table(_) => TABLE,
        Block::HorizontalRule => "horizontalRule",
        Block::UnknownDirective { .. } => UNKNOWN_DIRECTIVE_BLOCK,
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
    fn inlines(content: &[Inline], out: &mut Vec<char>) {
        for inline in content {
            match inline {
                Inline::Text { text, .. } => out.extend(text.chars()),
                Inline::HardBreak => out.push('\n'),
                Inline::Image { alt, .. } => out.extend(alt.chars()),
                Inline::PageLink { page_id } => out.extend(page_id.chars()),
                Inline::UnknownDirective { source } => out.extend(source.chars()),
                Inline::Status { text, .. } => out.extend(text.chars()),
            }
        }
    }
    fn collect(blocks: &[Block], out: &mut Vec<char>) {
        for block in blocks {
            match block {
                Block::Paragraph(content) | Block::Heading { content, .. } => inlines(content, out),
                Block::Table(rows) => {
                    for cell in rows.iter().flatten() {
                        inlines(cell, out);
                        out.push('\n');
                    }
                }
                Block::CodeBlock { text, .. } => out.extend(text.chars()),
                Block::Blockquote(children) | Block::Callout { content: children, .. } => collect(children, out),
                Block::BulletList(items) | Block::OrderedList { items, .. } => {
                    items.iter().for_each(|item| collect(item, out))
                }
                Block::TaskList(items) => items.iter().for_each(|item| collect(&item.content, out)),
                Block::HorizontalRule => {}
                Block::UnknownDirective { source } => out.extend(source.chars()),
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
        Block::Callout { kind, content } => {
            if string_attribute(txn, element, KIND).as_deref() != Some(kind.name()) {
                element.insert_attribute(txn, KIND, Any::from(kind.name()));
            }
            diff_blocks(txn, element, content);
            if element.len(txn) == 0 {
                element.push_back(txn, XmlElementPrelim::empty("paragraph"));
            }
        }
        Block::BulletList(items) => diff_items(txn, element, &list_items(items)),
        Block::OrderedList { start, items } => {
            if number_attribute(txn, element, "start") != Some(*start) {
                element.insert_attribute(txn, "start", Any::from(*start as f64));
            }
            diff_items(txn, element, &list_items(items));
        }
        Block::TaskList(items) => diff_items(txn, element, &task_items(items)),
        Block::Table(rows) => diff_rows(txn, element, rows),
        Block::HorizontalRule => {}
        Block::UnknownDirective { source } => {
            if string_attribute(txn, element, SOURCE).as_ref() != Some(source) {
                element.insert_attribute(txn, SOURCE, Any::from(source.as_str()));
            }
        }
    }
}

/// The element a cell is stored as: a header cell in the first row.
fn cell_tag(row: usize) -> &'static str {
    if row == 0 {
        TABLE_HEADER
    } else {
        TABLE_CELL
    }
}

/// A cell as it is now: whether it is stored as the element its row wants,
/// and what it holds.
fn current_cells<T: ReadTxn>(txn: &T, row: &XmlOut, wanted: &str) -> Vec<(bool, Cell)> {
    let XmlOut::Element(row) = row else { return Vec::new() };
    row.children(txn)
        .map(|cell| (tag_of(&cell) == Some(wanted), read_cell(txn, &cell)))
        .collect()
}

fn cells_text(cells: &[Cell]) -> Vec<char> {
    plain_text(&[Block::Table(vec![cells.to_vec()])])
}

/// Turns the rows of a table into `target`, the way `diff_blocks` does for blocks.
fn diff_rows(txn: &mut TransactionMut, table: &XmlElementRef, target: &[Row]) {
    let nodes: Vec<XmlOut> = table.children(txn).collect();
    let old_text: Vec<Vec<char>> = nodes
        .iter()
        .map(|node| {
            let cells: Vec<_> = current_cells(txn, node, TABLE_CELL).into_iter().map(|(_, cell)| cell).collect();
            cells_text(&cells)
        })
        .collect();
    let new_text: Vec<Vec<char>> = target.iter().map(|row| cells_text(row)).collect();
    // A row is the same when it holds the same cells, stored as its place wants them.
    let same = |txn: &TransactionMut, i: usize, j: usize| {
        let cells = current_cells(txn, &nodes[i], cell_tag(j));
        cells.len() == target[j].len() && cells.iter().zip(&target[j]).all(|((right, has), wanted)| *right && has == wanted)
    };
    let fits: Vec<Vec<Fit>> = (0..nodes.len())
        .map(|i| {
            (0..target.len())
                .map(|j| {
                    if same(txn, i, j) {
                        Fit::Same(2 + new_text[j].len() as u32)
                    } else if tag_of(&nodes[i]) == Some(TABLE_ROW) {
                        Fit::Similar(1 + common_edges(&old_text[i], &new_text[j]) as u32)
                    } else {
                        Fit::No
                    }
                })
                .collect()
        })
        .collect();
    let steps = align(nodes.len(), target.len(), |i, j| fits[i][j], |_| false);

    let mut at = 0;
    for step in steps {
        match step {
            Step::Keep(i, j) => {
                if let (false, XmlOut::Element(row)) = (matches!(fits[i][j], Fit::Same(_)), &nodes[i]) {
                    diff_cells(txn, row, &target[j], cell_tag(j));
                }
                at += 1;
            }
            Step::Remove(_) => table.remove_range(txn, at, 1),
            Step::Add(j) => {
                insert_row(txn, table, at, &target[j], cell_tag(j));
                at += 1;
            }
        }
    }
}

/// Turns the cells of a row into `target`, each stored as `tag`.
fn diff_cells(txn: &mut TransactionMut, row: &XmlElementRef, target: &[Cell], tag: &str) {
    let nodes: Vec<XmlOut> = row.children(txn).collect();
    let current = current_cells(txn, &XmlOut::Element(row.clone()), tag);
    let old_text: Vec<Vec<char>> = current.iter().map(|(_, cell)| cells_text(std::slice::from_ref(cell))).collect();
    let new_text: Vec<Vec<char>> = target.iter().map(|cell| cells_text(std::slice::from_ref(cell))).collect();

    let steps = align(
        nodes.len(),
        target.len(),
        |i, j| match &current[i] {
            (true, has) if *has == target[j] => Fit::Same(2 + new_text[j].len() as u32),
            (true, _) => Fit::Similar(1 + common_edges(&old_text[i], &new_text[j]) as u32),
            (false, _) => Fit::No,
        },
        |_| false,
    );

    let mut at = 0;
    for step in steps {
        match step {
            Step::Keep(i, j) => {
                if let (false, XmlOut::Element(cell)) = (current[i].1 == target[j], &nodes[i]) {
                    update_cell(txn, cell, &target[j]);
                }
                at += 1;
            }
            Step::Remove(_) => row.remove_range(txn, at, 1),
            Step::Add(j) => {
                insert_cell(txn, row, at, &target[j], tag);
                at += 1;
            }
        }
    }
}

/// Makes a cell hold exactly one paragraph with the given content.
fn update_cell(txn: &mut TransactionMut, cell: &XmlElementRef, content: &[Inline]) {
    let paragraph = match cell.get(txn, 0) {
        Some(XmlOut::Element(first)) if first.tag().as_ref() == "paragraph" => first,
        _ => cell.insert(txn, 0, XmlElementPrelim::empty("paragraph")),
    };
    let extra = cell.len(txn) - 1;
    if extra > 0 {
        cell.remove_range(txn, 1, extra);
    }
    diff_inlines(txn, &paragraph, content);
}

/// An item of a list as it is to be written: whether it is a task and
/// checked, and what it holds.
type ItemTarget<'a> = (Option<bool>, &'a [Block]);

fn list_items(items: &[Vec<Block>]) -> Vec<ItemTarget<'_>> {
    items.iter().map(|item| (None, item.as_slice())).collect()
}

fn task_items(items: &[TaskItem]) -> Vec<ItemTarget<'_>> {
    items.iter().map(|item| (Some(item.checked), item.content.as_slice())).collect()
}

/// Turns the items of a list into `target`, the way `diff_blocks` does for blocks.
fn diff_items(txn: &mut TransactionMut, list: &XmlElementRef, target: &[ItemTarget]) {
    let tag = item_tag(target.first().and_then(|(checked, _)| *checked));
    let nodes: Vec<XmlOut> = list.children(txn).collect();
    let current: Vec<(bool, Vec<Block>)> = nodes
        .iter()
        .map(|node| {
            let (checked, blocks) = read_item(txn, node);
            (checked, normalize(blocks))
        })
        .collect();
    let old_text: Vec<Vec<char>> = current.iter().map(|(_, blocks)| plain_text(blocks)).collect();
    let new_text: Vec<Vec<char>> = target.iter().map(|(_, blocks)| plain_text(blocks)).collect();
    let same = |i: usize, j: usize| {
        let (checked, blocks) = &current[i];
        blocks.as_slice() == target[j].1 && target[j].0.is_none_or(|wanted| wanted == *checked)
    };

    let steps = align(
        nodes.len(),
        target.len(),
        |i, j| {
            if same(i, j) {
                Fit::Same(2 + new_text[j].len() as u32)
            } else if tag_of(&nodes[i]) == Some(tag) {
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
                if let (false, XmlOut::Element(item)) = (same(i, j), &nodes[i]) {
                    let (checked, blocks) = target[j];
                    if let Some(checked) = checked.filter(|checked| *checked != current[i].0) {
                        item.insert_attribute(txn, CHECKED, Any::Bool(checked));
                    }
                    diff_blocks(txn, item, blocks);
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
                insert_item(txn, list, at, target[j]);
                at += 1;
            }
        }
    }
}

/// The element an item is stored as: a task item when it has a checkbox.
fn item_tag(checked: Option<bool>) -> &'static str {
    if checked.is_some() {
        TASK_ITEM
    } else {
        "listItem"
    }
}

/// What an element holds between two line breaks: text in one or more formats.
type Run<'a> = Vec<(&'a str, &'a Marks)>;

enum Part<'a> {
    Run(Run<'a>),
    Break,
    Image { src: &'a str, alt: &'a str },
    PageLink(&'a str),
    UnknownDirective(&'a str),
    Status { text: &'a str, color: StatusColor },
}

fn parts(inlines: &[Inline]) -> Vec<Part<'_>> {
    let mut parts: Vec<Part> = Vec::new();
    for inline in inlines {
        match (inline, parts.last_mut()) {
            (Inline::HardBreak, _) => parts.push(Part::Break),
            (Inline::Image { src, alt }, _) => parts.push(Part::Image { src, alt }),
            (Inline::PageLink { page_id }, _) => parts.push(Part::PageLink(page_id)),
            (Inline::UnknownDirective { source }, _) => parts.push(Part::UnknownDirective(source)),
            (Inline::Status { text, color }, _) => parts.push(Part::Status { text, color: *color }),
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
    /// Always an `Inline::Image`.
    Image(Inline),
    /// A Page link, by the ID of its Page; without one it is not a Page link
    /// anyone can follow, and is removed like any other unwanted child.
    PageLink(Option<String>),
    /// A directive Turtle does not know, by its source.
    UnknownDirective(Option<String>),
    /// A Status, changed in place when its text or colour changes.
    Status(XmlElementRef),
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
            Part::Break | Part::Image { .. } | Part::PageLink(_) | Part::UnknownDirective(_) | Part::Status { .. } => {
                Vec::new()
            }
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
            XmlOut::Element(child) if child.tag().as_ref() == "image" => Child::Image(read_image(txn, &child)),
            XmlOut::Element(child) if child.tag().as_ref() == PAGE_LINK => {
                Child::PageLink(string_attribute(txn, &child, PAGE_ID))
            }
            XmlOut::Element(child) if child.tag().as_ref() == UNKNOWN_DIRECTIVE => {
                Child::UnknownDirective(string_attribute(txn, &child, SOURCE))
            }
            XmlOut::Element(child) if child.tag().as_ref() == STATUS => Child::Status(child),
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
            (Child::Image(Inline::Image { src, alt }), Part::Image { src: wanted_src, alt: wanted_alt })
                if src == wanted_src && alt == wanted_alt =>
            {
                Fit::Same(1)
            }
            (Child::PageLink(Some(has)), Part::PageLink(wanted)) if has == wanted => Fit::Same(1),
            (Child::UnknownDirective(Some(has)), Part::UnknownDirective(wanted)) if has == wanted => Fit::Same(1),
            (Child::Status(status), Part::Status { text, color }) => {
                if read_status(txn, status) == (Inline::Status { text: text.to_string(), color: *color }) {
                    Fit::Same(1)
                } else {
                    Fit::Similar(1)
                }
            }
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
                if let (Child::Status(status), Part::Status { text, color }) = (&children[i], &target[j]) {
                    set_status(txn, status, text, *color);
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
                    Part::Image { src, alt } => {
                        let image = element.insert(txn, at, XmlElementPrelim::empty("image"));
                        image.insert_attribute(txn, "src", Any::from(*src));
                        image.insert_attribute(txn, "alt", Any::from(*alt));
                    }
                    Part::PageLink(page_id) => {
                        let link = element.insert(txn, at, XmlElementPrelim::empty(PAGE_LINK));
                        link.insert_attribute(txn, PAGE_ID, Any::from(*page_id));
                    }
                    Part::UnknownDirective(source) => {
                        let directive = element.insert(txn, at, XmlElementPrelim::empty(UNKNOWN_DIRECTIVE));
                        directive.insert_attribute(txn, SOURCE, Any::from(*source));
                    }
                    Part::Status { text, color } => {
                        let status = element.insert(txn, at, XmlElementPrelim::empty(STATUS));
                        set_status(txn, &status, text, *color);
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
        Block::Blockquote(children) => insert_children(txn, &element, children),
        Block::Callout { kind, content } => {
            element.insert_attribute(txn, KIND, Any::from(kind.name()));
            insert_children(txn, &element, content);
        }
        Block::BulletList(items) => insert_items(txn, &element, &list_items(items)),
        Block::OrderedList { start, items } => {
            element.insert_attribute(txn, "start", Any::from(*start as f64));
            insert_items(txn, &element, &list_items(items));
        }
        Block::TaskList(items) => insert_items(txn, &element, &task_items(items)),
        Block::Table(rows) => {
            for (at, row) in rows.iter().enumerate() {
                insert_row(txn, &element, at as u32, row, cell_tag(at));
            }
        }
        Block::HorizontalRule => {}
        Block::UnknownDirective { source } => {
            element.insert_attribute(txn, SOURCE, Any::from(source.as_str()));
        }
    }
}

fn insert_row(txn: &mut TransactionMut, table: &XmlElementRef, index: u32, cells: &[Cell], tag: &str) {
    let row = table.insert(txn, index, XmlElementPrelim::empty(TABLE_ROW));
    for (at, cell) in cells.iter().enumerate() {
        insert_cell(txn, &row, at as u32, cell, tag);
    }
}

fn insert_cell(txn: &mut TransactionMut, row: &XmlElementRef, index: u32, content: &[Inline], tag: &str) {
    let cell = row.insert(txn, index, XmlElementPrelim::empty(tag));
    let paragraph = cell.push_back(txn, XmlElementPrelim::empty("paragraph"));
    diff_inlines(txn, &paragraph, content);
}

/// Fills a quote or callout, which the editor requires to hold at least one block.
fn insert_children(txn: &mut TransactionMut, element: &XmlElementRef, children: &[Block]) {
    for (at, child) in children.iter().enumerate() {
        insert_block(txn, element, at as u32, child);
    }
    if children.is_empty() {
        element.push_back(txn, XmlElementPrelim::empty("paragraph"));
    }
}

fn insert_items(txn: &mut TransactionMut, list: &XmlElementRef, items: &[ItemTarget]) {
    for (at, item) in items.iter().enumerate() {
        insert_item(txn, list, at as u32, *item);
    }
}

fn insert_item(txn: &mut TransactionMut, list: &XmlElementRef, index: u32, (checked, item): ItemTarget) {
    let element = list.insert(txn, index, XmlElementPrelim::empty(item_tag(checked)));
    if let Some(checked) = checked {
        element.insert_attribute(txn, CHECKED, Any::Bool(checked));
    }
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
    fn an_image_is_the_editors_image_node() {
        // The editor stores an image as an `image` element in the paragraph,
        // with its address and alternative text as attributes.
        let doc = Doc::new();
        {
            let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
            let mut txn = doc.transact_mut();
            let paragraph = fragment.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
            paragraph.push_back(&mut txn, XmlTextPrelim::new("Logo: "));
            let image = paragraph.push_back(&mut txn, XmlElementPrelim::empty("image"));
            image.insert_attribute(&mut txn, "src", Any::from("/api/attachments/a1"));
            image.insert_attribute(&mut txn, "alt", Any::from("logo.png"));
        }
        assert_eq!(to_markdown(&read(&doc)), "Logo: ![logo.png](</api/attachments/a1>)\n");

        let written = Doc::new();
        write(&written, &read(&doc));
        let fragment = written.get_or_insert_xml_fragment(FRAGMENT);
        let txn = written.transact();
        let Some(XmlOut::Element(paragraph)) = fragment.get(&txn, 0) else { panic!("a paragraph") };
        let Some(XmlOut::Element(image)) = paragraph.get(&txn, 1) else { panic!("an image") };
        assert_eq!(image.tag().as_ref(), "image");
        assert_eq!(string_attribute(&txn, &image, "src").as_deref(), Some("/api/attachments/a1"));
        assert_eq!(string_attribute(&txn, &image, "alt").as_deref(), Some("logo.png"));
    }

    #[test]
    fn a_page_link_is_stored_as_the_editors_page_link_node_naming_the_page() {
        let blocks = from_markdown("See [Setup](turtle:a1b2) first.");
        let doc = Doc::new();
        write(&doc, &blocks);

        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let txn = doc.transact();
        let Some(XmlOut::Element(paragraph)) = fragment.get(&txn, 0) else { panic!("no paragraph") };
        let Some(XmlOut::Element(link)) = paragraph.get(&txn, 1) else { panic!("no Page link") };
        assert_eq!(link.tag().as_ref(), "pageLink");
        assert_eq!(string_attribute(&txn, &link, "pageId").as_deref(), Some("a1b2"));
        drop(txn);
        assert_eq!(read(&doc), blocks);
    }

    #[test]
    fn a_status_is_stored_as_the_editors_status_with_its_text_and_colour() {
        let blocks = from_markdown("| Task | State |\n| - | - |\n| Build | :status[On track]{color=green} |\n");
        let doc = Doc::new();
        write(&doc, &blocks);

        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let txn = doc.transact();
        let status = (|| {
            let XmlOut::Element(table) = fragment.get(&txn, 0)? else { return None };
            let XmlOut::Element(row) = table.get(&txn, 1)? else { return None };
            let XmlOut::Element(cell) = row.get(&txn, 1)? else { return None };
            let XmlOut::Element(paragraph) = cell.get(&txn, 0)? else { return None };
            let XmlOut::Element(status) = paragraph.get(&txn, 0)? else { return None };
            Some(status)
        })()
        .expect("no status in the cell");
        assert_eq!(status.tag().as_ref(), "status");
        assert_eq!(string_attribute(&txn, &status, "text").as_deref(), Some("On track"));
        assert_eq!(string_attribute(&txn, &status, "color").as_deref(), Some("green"));
        drop(txn);
        assert_eq!(read(&doc), blocks);
    }

    #[test]
    fn a_status_changed_by_an_agent_keeps_what_a_person_types_beside_it() {
        let server = Doc::new();
        write(&server, &from_markdown("Build :status[On track]{color=green} now\n"));
        let editor = replica(&server);

        let typed = type_into(&editor, 0, "Build ".len() as u32, "it ");
        let written = write(&server, &from_markdown("Build :status[Late]{color=red} now\n")).unwrap();
        apply(&server, &typed);
        apply(&editor, &written);

        let expected = "Build it :status[Late]{color=red} now\n";
        assert_eq!(to_markdown(&read(&server)), expected);
        assert_eq!(to_markdown(&read(&editor)), expected);
    }

    #[test]
    fn an_unknown_directive_is_stored_as_the_editors_placeholder_holding_its_source() {
        let blocks = from_markdown("See :acme.demo[hi]{x=1} here.\n\n:::acme.board{x=1}\n- a\n:::\n");
        let doc = Doc::new();
        write(&doc, &blocks);

        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let txn = doc.transact();
        let Some(XmlOut::Element(paragraph)) = fragment.get(&txn, 0) else { panic!("no paragraph") };
        let Some(XmlOut::Element(inline)) = paragraph.get(&txn, 1) else { panic!("no inline directive") };
        assert_eq!(inline.tag().as_ref(), "unknownDirective");
        assert_eq!(string_attribute(&txn, &inline, "source").as_deref(), Some(":acme.demo[hi]{x=1}"));
        let Some(XmlOut::Element(block)) = fragment.get(&txn, 1) else { panic!("no block directive") };
        assert_eq!(block.tag().as_ref(), "unknownDirectiveBlock");
        assert_eq!(string_attribute(&txn, &block, "source").as_deref(), Some(":::acme.board{x=1}\n- a\n:::"));
        drop(txn);
        assert_eq!(read(&doc), blocks);
    }

    #[test]
    fn unknown_directives_stay_while_an_agent_and_a_person_edit_around_them() {
        let server = Doc::new();
        let before = "Plan :acme.demo[hi]{x=1} for today.\n\n:::acme.board\n:::\n\nNotes\n";
        write(&server, &from_markdown(before));
        let editor = replica(&server);

        // A person types in the last paragraph while an Agent rewrites the first.
        let typed = type_into(&editor, 2, "Notes".len() as u32, " and more");
        let written =
            write(&server, &from_markdown("Our plan :acme.demo[hi]{x=1} for tomorrow.\n\n:::acme.board\n:::\n\nNotes\n"))
                .unwrap();
        apply(&server, &typed);
        apply(&editor, &written);

        let expected = "Our plan :acme.demo[hi]{x=1} for tomorrow.\n\n:::acme.board\n:::\n\nNotes and more\n";
        assert_eq!(to_markdown(&read(&server)), expected);
        assert_eq!(to_markdown(&read(&editor)), expected);
    }

    #[test]
    fn a_task_list_is_stored_as_the_editors_task_items_with_their_checkboxes() {
        let blocks = from_markdown("- [ ] open\n- [x] done\n");
        let doc = Doc::new();
        write(&doc, &blocks);

        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let txn = doc.transact();
        let Some(XmlOut::Element(list)) = fragment.get(&txn, 0) else { panic!("no list") };
        assert_eq!(list.tag().as_ref(), "taskList");
        let checked: Vec<_> = list
            .children(&txn)
            .map(|item| match item {
                XmlOut::Element(item) => (item.tag().to_string(), item.get_attribute(&txn, "checked")),
                _ => panic!("an item is not an element"),
            })
            .collect();
        assert_eq!(
            checked,
            vec![
                ("taskItem".to_string(), Some(Out::Any(Any::Bool(false)))),
                ("taskItem".to_string(), Some(Out::Any(Any::Bool(true)))),
            ]
        );
        drop(txn);
        assert_eq!(read(&doc), blocks);
    }

    #[test]
    fn a_callout_is_stored_as_the_editors_callout_naming_its_kind() {
        let doc = Doc::new();
        write(&doc, &from_markdown("> [!WARNING]\n> Mind the gap.\n"));
        let kind = |doc: &Doc| {
            let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
            let txn = doc.transact();
            let Some(XmlOut::Element(callout)) = fragment.get(&txn, 0) else { panic!("no callout") };
            (callout.tag().to_string(), string_attribute(&txn, &callout, "kind"))
        };
        assert_eq!(kind(&doc), ("callout".to_string(), Some("warning".to_string())));
        let before = block_ids(&doc);

        write(&doc, &from_markdown("> [!DANGER]\n> Mind the gap.\n"));

        assert_eq!(kind(&doc), ("callout".to_string(), Some("danger".to_string())));
        assert_eq!(block_ids(&doc), before, "the kind changes in place");
        assert_eq!(read(&doc), from_markdown("> [!DANGER]\n> Mind the gap.\n"));
    }

    #[test]
    fn checking_an_item_changes_only_its_checkbox() {
        let doc = Doc::new();
        write(&doc, &from_markdown("- [ ] open\n- [ ] also open\n"));
        let elsewhere = replica(&doc);
        let text = |doc: &Doc| {
            let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
            let txn = doc.transact();
            let Some(XmlOut::Element(list)) = fragment.get(&txn, 0) else { panic!("no list") };
            let Some(XmlOut::Element(item)) = list.get(&txn, 1) else { panic!("no item") };
            let Some(XmlOut::Element(paragraph)) = item.get(&txn, 0) else { panic!("no paragraph") };
            let Some(XmlOut::Text(text)) = paragraph.get(&txn, 0) else { panic!("no text") };
            text
        };
        let before = text(&doc);

        let update = write(&doc, &from_markdown("- [ ] open\n- [x] also open\n")).unwrap();
        apply(&elsewhere, &update);

        assert_eq!(text(&doc), before);
        assert_eq!(read(&elsewhere), from_markdown("- [ ] open\n- [x] also open\n"));
    }

    /// Each row of the table at the top of the document, as the elements its cells are stored as.
    fn table_layout(doc: &Doc) -> Vec<Vec<String>> {
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let txn = doc.transact();
        let Some(XmlOut::Element(table)) = fragment.get(&txn, 0) else { panic!("no table") };
        assert_eq!(table.tag().as_ref(), "table");
        table
            .children(&txn)
            .map(|row| {
                let XmlOut::Element(row) = row else { panic!("a row is not an element") };
                assert_eq!(row.tag().as_ref(), "tableRow");
                row.children(&txn)
                    .map(|cell| {
                        let XmlOut::Element(cell) = cell else { panic!("a cell is not an element") };
                        let Some(XmlOut::Element(paragraph)) = cell.get(&txn, 0) else { panic!("no paragraph") };
                        assert_eq!((paragraph.tag().as_ref(), cell.len(&txn)), ("paragraph", 1));
                        cell.tag().to_string()
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_table_is_stored_as_the_editors_rows_of_header_and_body_cells() {
        let markdown = "| Name | Role |\n| --- | --- |\n| Ada | *Writer* |\n";
        let doc = Doc::new();
        write(&doc, &from_markdown(markdown));

        let header = vec!["tableHeader".to_string(); 2];
        let body = vec!["tableCell".to_string(); 2];
        assert_eq!(table_layout(&doc), vec![header.clone(), body.clone()]);
        assert_eq!(to_markdown(&read(&doc)), markdown);

        // Taking away the header row makes the next row the header.
        write(&doc, &from_markdown("| Ada | *Writer* |\n| --- | --- |\n"));
        assert_eq!(table_layout(&doc), vec![header]);
        assert_eq!(to_markdown(&read(&doc)), "| Ada | *Writer* |\n| --- | --- |\n");
    }

    #[test]
    fn changing_a_cell_changes_only_that_cell() {
        let doc = Doc::new();
        write(&doc, &from_markdown("| a | b |\n| --- | --- |\n| c | d |\n"));
        let elsewhere = replica(&doc);
        let cell_ids = |doc: &Doc| {
            let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
            let txn = doc.transact();
            let Some(XmlOut::Element(table)) = fragment.get(&txn, 0) else { panic!("no table") };
            let ids: Vec<_> = table
                .children(&txn)
                .flat_map(|row| match row {
                    XmlOut::Element(row) => row.children(&txn).map(|cell| cell.id()).collect::<Vec<_>>(),
                    _ => panic!("a row is not an element"),
                })
                .collect();
            ids
        };
        let before = cell_ids(&doc);

        let update = write(&doc, &from_markdown("| a | b |\n| --- | --- |\n| c | **d!** |\n")).unwrap();
        apply(&elsewhere, &update);

        assert_eq!(cell_ids(&doc), before);
        assert_eq!(read(&elsewhere), from_markdown("| a | b |\n| --- | --- |\n| c | **d!** |\n"));
    }

    // The editor contract: see web/scripts/editor-contract.test.ts, which shares
    // these fixtures and says how to write them anew.
    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    fn contract_markdown() -> String {
        std::fs::read_to_string(fixture("editor-contract.md")).unwrap()
    }

    #[test]
    fn the_editor_contract_a_document_the_editor_made_reads_as_its_markdown() {
        let stored = std::fs::read(fixture("from-editor.bin")).unwrap();
        let blocks = read_state(&stored).unwrap();
        assert_eq!(to_markdown(&blocks), contract_markdown());
    }

    /// The layout of a document: each element with its attributes and each
    /// text with its formatting, sorted, so two documents with the same
    /// layout compare equal however their bytes were encoded.
    fn layout(doc: &Doc) -> String {
        fn attributes(pairs: impl Iterator<Item = (String, String)>) -> String {
            let mut pairs: Vec<_> = pairs.map(|(k, v)| format!("{k}={v}")).collect();
            pairs.sort();
            pairs.join(" ")
        }
        fn dump<T: ReadTxn>(txn: &T, node: XmlOut, depth: usize, out: &mut String) {
            let indent = "  ".repeat(depth);
            match node {
                XmlOut::Element(element) => {
                    let attrs = attributes(element.attributes(txn).map(|(k, v)| (k.to_string(), format!("{v:?}"))));
                    out.push_str(&format!("{indent}<{} {attrs}>\n", element.tag()));
                    for child in element.children(txn) {
                        dump(txn, child, depth + 1, out);
                    }
                }
                XmlOut::Text(text) => {
                    for chunk in text.diff(txn, YChange::identity) {
                        let marks = chunk.attributes.map(|a| {
                            attributes(a.iter().map(|(k, v)| (k.to_string(), format!("{v:?}"))))
                        });
                        out.push_str(&format!("{indent}{:?} [{}]\n", chunk.insert, marks.unwrap_or_default()));
                    }
                }
                XmlOut::Fragment(fragment) => fragment.children(txn).for_each(|child| dump(txn, child, depth, out)),
            }
        }
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let txn = doc.transact();
        let mut out = String::new();
        fragment.children(&txn).for_each(|child| dump(&txn, child, 0, &mut out));
        out
    }

    #[test]
    fn the_editor_contract_the_markdown_written_is_the_fixture_the_editor_reads() {
        let doc = Doc::with_client_id(1);
        write(&doc, &from_markdown(&contract_markdown()));
        let path = fixture("from-server.bin");
        if std::env::var("UPDATE_CONTRACT").as_deref() == Ok("1") {
            let stored = doc.transact().encode_state_as_update_v1(&yrs::StateVector::default());
            std::fs::write(&path, stored).unwrap();
        }
        let fixture = Doc::new();
        apply(&fixture, &std::fs::read(&path).unwrap());
        assert_eq!(
            layout(&doc),
            layout(&fixture),
            "from-server.bin is out of date; write it anew as web/scripts/editor-contract.test.ts says"
        );
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
            Block::Callout { kind, content } => Block::Callout {
                kind: if rng.next(4) == 0 { CalloutKind::Danger } else { *kind },
                content: mutate(rng, content),
            },
            Block::BulletList(items) => {
                Block::BulletList(items.iter().map(|item| mutate(rng, item)).collect())
            }
            Block::OrderedList { start, items } => Block::OrderedList {
                start: *start,
                items: items.iter().map(|item| mutate(rng, item)).collect(),
            },
            Block::TaskList(items) => Block::TaskList(
                items
                    .iter()
                    .map(|item| TaskItem {
                        checked: item.checked != (rng.next(4) == 0),
                        content: mutate(rng, &item.content),
                    })
                    .collect(),
            ),
            Block::Table(rows) => {
                let mut rows: Vec<Row> = rows
                    .iter()
                    .map(|row| row.iter().map(|cell| mutate_inlines(rng, cell)).collect())
                    .collect();
                let width = rows[0].len();
                match rng.next(6) {
                    0 => rows.insert(rng.next(rows.len() + 1), vec![Vec::new(); width]),
                    1 if rows.len() > 1 => {
                        rows.remove(rng.next(rows.len()));
                    }
                    2 => {
                        let at = rng.next(width + 1);
                        rows.iter_mut().for_each(|row| row.insert(at, rng.inlines()));
                    }
                    3 if width > 1 => {
                        let at = rng.next(width);
                        rows.iter_mut().for_each(|row| {
                            row.remove(at);
                        });
                    }
                    _ => {}
                }
                Block::Table(rows)
            }
            Block::HorizontalRule => Block::HorizontalRule,
            Block::UnknownDirective { source } => Block::UnknownDirective { source: source.clone() },
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
