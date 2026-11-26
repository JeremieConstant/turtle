# Coding standards

Read during review. These are the judgement calls; anything a tool can decide is in `scripts/check.sh` instead.

## Domain language

Names in code, API, UI text and tests use the terms in `GLOSSARY.md`, in the sense it defines. A concept the glossary lacks is a gap to record there, not a synonym to coin. Table and column names (`nodes`, `grants`) may stay storage words, but they stay out of operation names and anything a person or Agent reads.

## Operations decide, adapters translate

Every capability is an operation in `server/src/ops` (ADR-0005), and every permission decision is made there. HTTP handlers, the editing WebSocket and later MCP find the caller, call into `ops`, and turn the outcome into their own protocol, without choosing who may do what themselves. A rule that holds for more than one operation lives in one function in `ops` that each of them calls.

## Hidden is not found

Something the caller may not read answers exactly like something that does not exist: same error kind, same message, and nothing in the message that came from it.

This covers people as much as Pages: a Profile, a person's name and their picture reach only those who may see that Profile, except where the spec names the exception. A comment in the code explaining a wider rule does not make one.

`permissions::role_on` is where reading is decided. A reader that decides it some other way, such as the search index through `readable_through` and `governing_node`, or `list_tags`, applies every exclusion `role_on` makes itself, such as leaving out what is in the Trash.

## Messages are for people

Error messages are whole sentences that say what to do next ("Enter a title."), name things with glossary terms, and never expose internals. Internal failures are logged on the server, not described to the caller.

## An import reports, it does not stop

An import brings in all it can. What is wrong with one file, Page, Attachment, Tag or link becomes a Problem naming it, and the rest comes in; only what concerns the whole import, such as a missing Role or a ZIP that is no export, ends it. A `?` on a call made per imported item is the place to look: it ends the import part way, with some of the tree already created.

## A Version holds exactly its authors' changes

People's edits arrive at an open Page at any moment. Code that reads a Page's content, changes it, and takes its unrecorded changes for a Version does all three under one `state` lock in `live.rs`, so an edit arriving in between can neither be undone nor credited to the wrong author.

## Comments say why

A comment explains a reason, a constraint or a decision the code cannot show. Doc comments on operations and their input and output say what a caller needs to know, because they become the API and MCP descriptions.

## A finding shows its evidence

A review finding that says the code does something wrong names where it happens, as a file and line, or the test that fails. A suspicion the reviewer could not trace to a line is reported as a question, not a finding.

## Tests go through the public seam

Tests exercise an operation, an HTTP or WebSocket endpoint, or a module's exports. Expected values come from the spec or a worked example, never recomputed the way the code computes them.

Page content has three seams: Markdown (`markdown::{to_markdown, from_markdown}`), the collaborative document (`ydoc::{write, read}`), and the document's stored layout, the element and attribute names the editor reads. A document test may state the content it expects as Markdown, since the Markdown reader is independent of the document code, and may read the stored layout directly, since that layout is the contract with the editor rather than an internal detail.
