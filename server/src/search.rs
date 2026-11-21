//! The search index (Tantivy). It holds one document per Page and answers
//! queries, but knows nothing of the database or of who may read what: the
//! operations layer hands it the documents and, with every query, the access
//! it must stay within.

use anyhow::Context;
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tantivy::collector::TopDocs;
use tantivy::query::{
    BooleanQuery, ConstScoreQuery, DisjunctionMaxQuery, FuzzyTermQuery, Occur, Query, TermQuery, TermSetQuery,
};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, STORED, STRING,
};
use tantivy::tokenizer::{
    AsciiFoldingFilter, Language, LowerCaser, RemoveLongFilter, SimpleTokenizer, Stemmer,
    TextAnalyzer,
};
use tantivy::{IndexReader, IndexWriter, TantivyDocument, Term};

/// Changes whenever the schema or the analysis changes, so an index written
/// by an older Turtle is rebuilt instead of read wrongly.
const SCHEMA_VERSION: &str = "1";
const VERSION_FILE: &str = "turtle-search-version";
/// Present only while no Turtle has the index open and everything was
/// written before it closed. Without it, changes may have been lost in a
/// crash, so the index is rebuilt.
const CLOSED_CLEANLY_FILE: &str = "turtle-search-closed-cleanly";

/// What the index writer may hold in memory before it writes a segment.
const WRITER_MEMORY: usize = 20_000_000;

/// A Page as the index sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageDocument {
    pub page_id: String,
    pub area_id: String,
    /// The node whose Grants decide who may read the Page.
    pub access: String,
    pub title: String,
    /// The content as plain text.
    pub text: String,
}

/// What a query asks.
pub struct Search<'a> {
    pub text: &'a str,
    /// Only Pages of this Area; every Area if absent.
    pub area_id: Option<&'a str>,
    /// Only Pages whose `access` is one of these; every Page if absent.
    pub readable: Option<&'a [String]>,
    pub limit: usize,
}

/// A Page that matched, best first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub page_id: String,
    pub snippet: Vec<Fragment>,
}

/// A piece of a snippet.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct Fragment {
    pub text: String,
    /// Whether this piece is a word that matched.
    pub highlighted: bool,
}

/// About how many characters of the text a snippet shows, and how many of
/// them come before the first match.
const SNIPPET_LENGTH: usize = 200;
const SNIPPET_LEAD: usize = 60;

/// Pages whose document is out of date.
#[derive(Debug, Default)]
pub struct Pending {
    pub everything: bool,
    pub pages: HashSet<String>,
}

impl Pending {
    pub fn is_empty(&self) -> bool {
        !self.everything && self.pages.is_empty()
    }
}

/// New documents for what was pending.
pub struct Changes {
    /// Replaces the whole index, when everything was pending.
    pub everything: Option<Vec<PageDocument>>,
    /// Each Page's new document, or none when it is gone.
    pub pages: Vec<(String, Option<PageDocument>)>,
}

struct Fields {
    page_id: Field,
    area_id: Field,
    access: Field,
    title: Words,
    text: Words,
}

/// One piece of text, indexed three ways: as written (for typos and words
/// still being typed) and reduced to its English and German stems.
#[derive(Clone, Copy)]
struct Words {
    raw: Field,
    english: Field,
    german: Field,
}

const RAW: &str = "turtle_raw";
const ENGLISH: &str = "turtle_english";
const GERMAN: &str = "turtle_german";

pub struct Index {
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    fields: Fields,
    pending: Mutex<Pending>,
    closed_cleanly: PathBuf,
}

impl Index {
    /// Opens the index in `dir`. One that is missing, unreadable or of
    /// another schema is created anew and everything is marked pending, so
    /// the next catch-up rebuilds it from the database. Everything is also
    /// pending when the last Turtle to use it did not close it.
    pub fn open(dir: &Path) -> anyhow::Result<Index> {
        let (schema, fields) = schema();
        let (index, fresh) = match open_existing(dir) {
            Some(index) => (index, false),
            None => {
                if dir.exists() {
                    std::fs::remove_dir_all(dir)
                        .with_context(|| format!("removing the old search index {}", dir.display()))?;
                }
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("creating the search index {}", dir.display()))?;
                let index = tantivy::Index::create_in_dir(dir, schema)?;
                std::fs::write(version_file(dir), SCHEMA_VERSION)?;
                (index, true)
            }
        };
        let closed_cleanly = dir.join(CLOSED_CLEANLY_FILE);
        let complete = !fresh && closed_cleanly.exists();
        if closed_cleanly.exists() {
            std::fs::remove_file(&closed_cleanly)?;
        }
        index.tokenizers().register(RAW, analyzer(None));
        index.tokenizers().register(ENGLISH, analyzer(Some(Language::English)));
        index.tokenizers().register(GERMAN, analyzer(Some(Language::German)));
        let writer = index.writer_with_num_threads(1, WRITER_MEMORY)?;
        let reader = index.reader()?;
        Ok(Index {
            reader,
            writer: Mutex::new(writer),
            fields,
            pending: Mutex::new(Pending { everything: !complete, pages: HashSet::new() }),
            closed_cleanly,
        })
    }

    /// Notes that the index holds every change, so the next start need not
    /// rebuild it. Called last, after the final catch-up.
    pub fn close(&self) -> anyhow::Result<()> {
        std::fs::write(&self.closed_cleanly, "")?;
        Ok(())
    }

    /// Marks a Page whose title, content or place changed.
    pub fn page_changed(&self, page_id: &str) {
        self.pending.lock().unwrap().pages.insert(page_id.to_string());
    }

    /// Takes what is pending, to be turned into `Changes`.
    pub fn take_pending(&self) -> Pending {
        std::mem::take(&mut *self.pending.lock().unwrap())
    }

    /// Puts back what could not be applied, so the next catch-up tries again.
    pub fn restore_pending(&self, pending: Pending) {
        let mut current = self.pending.lock().unwrap();
        current.everything |= pending.everything;
        current.pages.extend(pending.pages);
    }

    /// Writes the changes and makes them searchable.
    pub fn apply(&self, changes: Changes) -> anyhow::Result<()> {
        let mut writer = self.writer.lock().unwrap();
        if let Some(documents) = changes.everything {
            writer.delete_all_documents()?;
            for document in &documents {
                writer.add_document(self.document(document))?;
            }
        }
        for (page_id, document) in &changes.pages {
            writer.delete_term(Term::from_field_text(self.fields.page_id, page_id));
            if let Some(document) = document {
                writer.add_document(self.document(document))?;
            }
        }
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Matches a Page that contains the word in its title or text. The
    /// score decides the order: a word in the title as written or as a word
    /// form beats a word in the title with a typo or still being typed, and
    /// either beats any number of matches in the text. Among equals, the
    /// usual relevance applies.
    fn word_query(&self, word: &Word, being_typed: bool) -> Box<dyn Query> {
        // The best of the three analyses counts, not their sum, which keeps
        // a match in the text far below the fixed score of a title match:
        // relevance grows only with the logarithm of the number of Pages.
        let exact = |words: Words| -> Box<dyn Query> {
            Box::new(DisjunctionMaxQuery::new(vec![
                term(words.raw, &word.raw),
                term(words.english, &word.english),
                term(words.german, &word.german),
            ]))
        };
        let loose = |words: Words| -> Box<dyn Query> {
            let typos = typos_allowed(&word.raw);
            let term = Term::from_field_text(words.raw, &word.raw);
            let mut alternatives: Vec<Box<dyn Query>> = Vec::new();
            if typos > 0 {
                alternatives.push(Box::new(FuzzyTermQuery::new(term.clone(), typos, true)));
            }
            if being_typed {
                alternatives.push(Box::new(FuzzyTermQuery::new_prefix(term, typos, true)));
            }
            Box::new(BooleanQuery::union(alternatives))
        };
        let (title, text) = (self.fields.title, self.fields.text);
        Box::new(BooleanQuery::union(vec![
            Box::new(ConstScoreQuery::new(exact(title), 1000.0)),
            // Breaks ties between title matches: the shorter title is closer.
            exact(title),
            Box::new(ConstScoreQuery::new(loose(title), 500.0)),
            exact(text),
            Box::new(ConstScoreQuery::new(loose(text), 0.5)),
        ]))
    }

    fn document(&self, page: &PageDocument) -> TantivyDocument {
        let fields = &self.fields;
        let mut document = TantivyDocument::default();
        document.add_text(fields.page_id, &page.page_id);
        document.add_text(fields.area_id, &page.area_id);
        document.add_text(fields.access, &page.access);
        for (words, text) in [(fields.title, &page.title), (fields.text, &page.text)] {
            document.add_text(words.raw, text);
            document.add_text(words.english, text);
            document.add_text(words.german, text);
        }
        document
    }

    pub fn search(&self, query: &Search) -> anyhow::Result<Vec<Hit>> {
        let words = words(query.text);
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        for (index, word) in words.iter().enumerate() {
            let being_typed = index == words.len() - 1;
            clauses.push((Occur::Must, self.word_query(word, being_typed)));
        }
        // Filters add nothing to the score.
        let mut filter = |query: Box<dyn Query>| {
            clauses.push((Occur::Must, Box::new(ConstScoreQuery::new(query, 0.0))));
        };
        if let Some(readable) = query.readable {
            if readable.is_empty() {
                return Ok(Vec::new());
            }
            let terms = readable.iter().map(|node| Term::from_field_text(self.fields.access, node));
            filter(Box::new(TermSetQuery::new(terms)));
        }
        if let Some(area_id) = query.area_id {
            filter(term(self.fields.area_id, area_id));
        }
        let matching = BooleanQuery::new(clauses);
        let searcher = self.reader.searcher();
        let top = searcher.search(&matching, &TopDocs::with_limit(query.limit.max(1)).order_by_score())?;
        let mut hits = Vec::new();
        for (_, address) in top {
            let document: TantivyDocument = searcher.doc(address)?;
            let page_id = document
                .get_first(self.fields.page_id)
                .and_then(|value| value.as_str())
                .context("a search document without a Page")?;
            let text = document.get_first(self.fields.text.raw).and_then(|value| value.as_str()).unwrap_or("");
            hits.push(Hit { page_id: page_id.to_string(), snippet: snippet(text, &words) });
        }
        Ok(hits)
    }
}

/// A word of the query, as written and as each language stems it.
struct Word {
    raw: String,
    english: String,
    german: String,
}

fn words(text: &str) -> Vec<Word> {
    let (mut english, mut german) = (analyzer(Some(Language::English)), analyzer(Some(Language::German)));
    // Each word is stemmed as written, as the documents are: before its
    // letters are folded to plain ASCII.
    token_spans(&mut analyzer(None), text)
        .into_iter()
        .map(|(raw, span)| Word {
            english: tokens(&mut english, &text[span.clone()]).concat(),
            german: tokens(&mut german, &text[span]).concat(),
            raw,
        })
        .collect()
}

/// How many typos a word may contain and still match: none in short words,
/// where one changed letter makes another word, more in long ones. Swapping
/// two neighbouring letters counts as one.
fn typos_allowed(word: &str) -> u8 {
    match word.chars().count() {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    }
}

/// The part of the text around the first word that matches the query, with
/// every matching word in it highlighted. A word matches in the snippet as it
/// does in the query: as written, as a word form, with typos, or as the
/// beginning of a longer word when it is the last one.
fn snippet(text: &str, query: &[Word]) -> Vec<Fragment> {
    // The three analyses split alike and drop the same over-long words, so
    // their tokens line up.
    let spans = token_spans(&mut analyzer(None), text);
    let english = tokens(&mut analyzer(Some(Language::English)), text);
    let german = tokens(&mut analyzer(Some(Language::German)), text);
    let matched: Vec<Range<usize>> = spans
        .iter()
        .zip(english.iter().zip(&german))
        .filter(|((raw, _), (english, german))| {
            query.iter().enumerate().any(|(index, word)| {
                let being_typed = index == query.len() - 1;
                word.matches(raw, english, german, being_typed)
            })
        })
        .map(|((_, span), _)| span.clone())
        .collect();

    let first = matched.first().map_or(0, |span| span.start);
    let lead = chars_back(text, first, SNIPPET_LEAD);
    // Begin and end on whole words.
    let start = if lead == 0 {
        0
    } else {
        spans.iter().map(|(_, span)| span.start).find(|&start| start >= lead).unwrap_or(first)
    };
    let reach = chars_forward(text, start, SNIPPET_LENGTH);
    let end = if reach == text.len() {
        reach
    } else {
        spans.iter().map(|(_, span)| span.end).rfind(|&end| end <= reach).unwrap_or(reach)
    };

    let mut pieces = Vec::new();
    let mut push = |text: &str, highlighted: bool| {
        let text = text.replace(['\n', '\r'], " ");
        if text.is_empty() {
            return;
        }
        match pieces.last_mut() {
            Some(Fragment { text: last, highlighted: was }) if *was == highlighted => last.push_str(&text),
            _ => pieces.push(Fragment { text, highlighted }),
        }
    };
    if start > 0 {
        push("…", false);
    }
    let mut cursor = start;
    for span in matched.iter().filter(|span| span.start >= start && span.end <= end) {
        push(&text[cursor..span.start], false);
        push(&text[span.clone()], true);
        cursor = span.end;
    }
    push(&text[cursor..end], false);
    if end < text.len() {
        push("…", false);
    }
    pieces
}

/// Where the text is `count` characters before `from`, or its beginning.
fn chars_back(text: &str, from: usize, count: usize) -> usize {
    match count {
        0 => from,
        _ => text[..from].char_indices().rev().nth(count - 1).map_or(0, |(index, _)| index),
    }
}

/// Where the text is `count` characters after `from`, or its end.
fn chars_forward(text: &str, from: usize, count: usize) -> usize {
    text[from..].char_indices().nth(count).map_or(text.len(), |(index, _)| from + index)
}

impl Word {
    /// Whether a word of the text, in its three analyses, matches this one.
    fn matches(&self, raw: &str, english: &str, german: &str, being_typed: bool) -> bool {
        let typos = typos_allowed(&self.raw) as usize;
        raw == self.raw
            || english == self.english
            || german == self.german
            || (typos > 0 && distance(raw, &self.raw) <= typos)
            || (being_typed && begins_with(raw, &self.raw, typos))
    }
}

/// Whether `word` begins with `beginning`, give or take `typos`.
fn begins_with(word: &str, beginning: &str, typos: usize) -> bool {
    let word: Vec<char> = word.chars().collect();
    let length = beginning.chars().count();
    (length.saturating_sub(typos)..=length + typos)
        .filter(|&end| end <= word.len())
        .any(|end| distance(&word[..end].iter().collect::<String>(), beginning) <= typos)
}

/// The number of letters to insert, delete, change or swap with their
/// neighbour to turn one word into the other.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut rows = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in rows[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let change = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (rows[i - 1][j] + 1).min(rows[i][j - 1] + 1).min(rows[i - 1][j - 1] + change);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = best;
        }
    }
    rows[a.len()][b.len()]
}

/// Each token with where it is in the text.
fn token_spans(analyzer: &mut TextAnalyzer, text: &str) -> Vec<(String, Range<usize>)> {
    let mut stream = analyzer.token_stream(text);
    let mut out = Vec::new();
    while let Some(token) = stream.next() {
        out.push((token.text.clone(), token.offset_from..token.offset_to));
    }
    out
}

fn tokens(analyzer: &mut TextAnalyzer, text: &str) -> Vec<String> {
    let mut stream = analyzer.token_stream(text);
    let mut out = Vec::new();
    while let Some(token) = stream.next() {
        out.push(token.text.clone());
    }
    out
}

fn term(field: Field, text: &str) -> Box<dyn Query> {
    Box::new(TermQuery::new(Term::from_field_text(field, text), IndexRecordOption::WithFreqs))
}

fn analyzer(language: Option<Language>) -> TextAnalyzer {
    let builder = TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(40))
        .filter(LowerCaser)
        .dynamic();
    match language {
        Some(language) => builder.filter_dynamic(Stemmer::new(language)),
        None => builder,
    }
    .filter_dynamic(AsciiFoldingFilter)
    .build()
}

fn schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let text = |builder: &mut tantivy::schema::SchemaBuilder, name: &str| {
        let words = |tokenizer: &str| {
            TextOptions::default().set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer(tokenizer)
                    .set_index_option(IndexRecordOption::WithFreqs),
            )
        };
        // The text as written is kept too, for snippets.
        Words {
            raw: builder.add_text_field(&format!("{name}_raw"), words(RAW).set_stored()),
            english: builder.add_text_field(&format!("{name}_english"), words(ENGLISH)),
            german: builder.add_text_field(&format!("{name}_german"), words(GERMAN)),
        }
    };
    let title = text(&mut builder, "title");
    let body = text(&mut builder, "text");
    let fields = Fields {
        page_id: builder.add_text_field("page_id", STRING | STORED),
        area_id: builder.add_text_field("area_id", STRING),
        access: builder.add_text_field("access", STRING),
        title,
        text: body,
    };
    (builder.build(), fields)
}

fn version_file(dir: &Path) -> PathBuf {
    dir.join(VERSION_FILE)
}

fn open_existing(dir: &Path) -> Option<tantivy::Index> {
    let version = std::fs::read_to_string(version_file(dir)).ok()?;
    if version != SCHEMA_VERSION {
        return None;
    }
    tantivy::Index::open_in_dir(dir).ok()
}
