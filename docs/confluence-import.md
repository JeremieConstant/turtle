# Importing from Confluence

Turtle imports a Confluence space from its export: the pages and folders as Pages and Folders in the same tree and order, with their content, attachments and labels. This page says how to import a space and how much of it comes across.

Confluence can hold more than Turtle does. Turtle keeps a Page to what Markdown can express ([ADR-0006](adr/0006-editor-is-limited-to-what-markdown-can-express.md)), so some content comes across only approximately and some macros not at all. Nothing is dropped silently: every import says what it found and what became of it.

## Import a space

1. In Confluence, export the space as **CSV** or **XML**, with its attachments. Confluence Cloud recommends CSV. Both give a ZIP.
2. In Turtle, open the menu of the Area, or of the Page or Folder to import beneath, and choose **Import from Confluence**. This needs the Write Role there.
3. Choose the ZIP. Turtle shows how far the import has got while it runs.

When the import is done, an **Import report** lies beside what it created, as a Page. It tells how much of the space came across, lists each Page that holds content to check, with what to look for, and names every file that was left out. Check a Page off in the list once you have looked at it.

To undo an import, choose **Imports** in the menu of the Area and then **Undo**. Everything the import created moves to the Trash, from where each Page can be restored.

To see the report without importing anything, call the operation `check_confluence_import` with the ZIP, through the HTTP API or as an Agent through MCP.

## What comes across

Beside the content of each page:

- **The tree.** Pages and folders keep their place and order. The XML export does not tell folders from pages, so a page without any content becomes a Folder; the CSV export tells them apart.
- **Attachments.** The current version of each attachment becomes an Attachment of its Page. Images show where they showed, and links to a file go to the Attachment.
- **Links between pages.** A link to a page of the same space becomes a Page link, which follows the Page when it is renamed or moved.
- **Labels.** The global labels of a page become its Tags.
- **Mentions.** A mention becomes a Mention of the Member or Guest with the same e-mail address, if the person importing can see them. This needs the XML export: the CSV export of Confluence Cloud names people only by an account ID, so there a mention becomes text.

<!-- what comes across: generated from the import's own list, see the end of this page -->
Of the 54 kinds of Confluence content the import knows, 25 come across exactly, 25 approximately and 4 are left out. A macro it does not know becomes a placeholder naming the macro, followed by what the macro held. Any other element it does not know keeps its text.

### Comes across exactly

| Confluence content | What it becomes in Turtle |
| --- | --- |
| Paragraph | A paragraph. |
| Heading | A heading of the same level. |
| Bold, italic, strikethrough or code | The same formatting. |
| Line break | A line break. |
| Bullet list | A bullet list. |
| Numbered list | A numbered list. |
| Task list | A task list, each task checked off as it was. |
| Table | A table whose first row is its header. |
| Quote | A quote. |
| Horizontal rule | A horizontal rule. |
| Preformatted text | A code block. |
| Link | A link. |
| Link to a page | A Page link. |
| Link to an attachment | A link to the Attachment. |
| Image | An image, its file an Attachment of the Page. |
| Image from the web | An image from its address. |
| Mention of a Member or Guest | A Mention of the Member or Guest with the same e-mail address. |
| Emoticon | The emoji. |
| Info macro | A Callout of kind Note. |
| Note macro | A Callout of kind Warning. |
| Tip macro | A Callout of kind Tip. |
| Warning macro | A Callout of kind Danger. |
| Code macro | A code block in the same language. |
| Noformat macro | A code block. |
| Status macro | A Status of the same colour. |

### Comes across approximately

| Confluence content | What it becomes in Turtle |
| --- | --- |
| Table without a header row | A table whose first row becomes its header. |
| Merged table cell | A cell followed by empty cells where it spanned several. |
| Table cell with more than text | A cell holding its text, line by line; lists become lines starting with a bullet. |
| Link to a page with its own text | A Page link, shown with the Page's title instead of its own text. |
| Link to a folder | Its text, without the link: Turtle links only to Pages. |
| Link to a page outside the space | Its text, without the link: the page is not imported. |
| Link to something else in Confluence | Its text, without the link. |
| Image caption | A paragraph in italics after the image. |
| Mention | The person's name after an @, as text, without an e-mail address: no Member or Guest has their address, or the export holds none. |
| Date | The date as text. |
| Underline | The text, without the underline. |
| Subscript or superscript | The text, on the line. |
| Coloured text | The text, without the colour. |
| Inline comment | The text; the comment is left out. |
| Layout | Its columns one after another. |
| Panel | A Callout of the kind closest to the panel's type; custom colours and icons are left out. |
| Panel macro | A Callout of kind Note; its colours are left out. |
| Expand macro | Its title in bold and what it held, always shown. |
| View-file macro | A link to the Attachment instead of a preview. |
| Viewpdf macro | A link to the Attachment instead of a preview. |
| Viewdoc macro | A link to the Attachment instead of a preview. |
| Viewxls macro | A link to the Attachment instead of a preview. |
| Viewppt macro | A link to the Attachment instead of a preview. |
| Excerpt macro | What it held. |
| Details macro | What it held. |

### Is left out

| Confluence content | What it becomes in Turtle |
| --- | --- |
| Missing attachment | Its file name in italics, saying it was not imported: the file is not in the export. |
| Instructional text | Nothing: it only guided whoever wrote the page. |
| Anchor macro | Nothing; links to it go to the Page. |
| Unreadable page content | Nothing: the Page is created empty. |

### Is not imported

A space holds more than its pages. The import counts these in its report and leaves them in Confluence:

- Comments
- Earlier versions of pages
- Drafts
- Archived pages
- Pages in Confluence's trash
- Earlier versions of attachments
- Personal labels
- Team labels and other labels that are not global
- Space permissions
- Page restrictions
- Blog posts
- Whiteboards
- Databases
<!-- end of what comes across -->

## Limits

- **One space per export.** An export holding several spaces is refused. A link to a page in another space keeps its text and loses its address.
- **Size.** A ZIP may be up to 2 GB and each attachment up to 25 MB, unless whoever runs Turtle sets other limits with `TURTLE_MAX_IMPORT_MB` and `TURTLE_MAX_ATTACHMENT_MB`. A larger attachment is left out and named in the report, before anything is created. [Large spaces](#large-spaces) says what a large import needs.
- **Access.** Space permissions and page restrictions are not imported. Who may read and change the imported Pages is decided by the Grants of the Area they are imported into.
- **Who wrote what.** Every imported Page is created by whoever imports it, at the time of the import. Authors, dates and earlier versions stay in Confluence.

## Large spaces

The web app uploads the ZIP as a file, and Turtle reads it from there: it never holds the ZIP in memory, and takes each attachment out of it only to attach it. What a large space needs is room in the data directory for the ZIP, until it is imported, and for the attachments. Memory grows with the number of pages, about 25 MB for every 1,000 pages, whatever the attachments weigh.

Two made-up spaces, each page with four earlier versions, imported on a laptop (Apple M5 Pro, internal SSD, release build) on 2026-11-27:

| Space | ZIP | Time | Most memory held |
| --- | --- | --- | --- |
| 5,000 pages and folders, 600 attachments | 155 MB | 4 s | 127 MB from XML, 119 MB from CSV |
| 20,000 pages and folders, 4,000 attachments | 1.9 GB | 19 s | 450 MB from XML, 421 MB from CSV |

The times are for reading the ZIP and creating everything, without the upload. To try a space of your own this way, see `import_a_real_export` in [`server/src/ops/import/confluence/mod.rs`](../server/src/ops/import/confluence/mod.rs).

A ZIP sent in base64 with the operation, instead of uploaded, is held in memory. It may be 200 MB at most over the HTTP API, and through MCP as large as a file that can be attached, which is how an Agent without HTTP access imports ([ADR-0014](adr/0014-a-zip-to-import-is-uploaded-ahead-of-the-operation.md)).

## How this page is kept true

The lists between "Comes across exactly" and "Is not imported" are written from the importer's own list of what it does with each kind of content, `ELEMENTS` in [`server/src/ops/import/confluence/coverage.rs`](../server/src/ops/import/confluence/coverage.rs). A test fails when the page and the importer disagree, so the page cannot promise more than the import does. The Import report of each import is drawn from the same list.

If something in your space does not come across as it should, [open an issue](https://github.com/turtle-wiki/turtle/issues/new/choose) and name the macro or element; the report names each one it found.
