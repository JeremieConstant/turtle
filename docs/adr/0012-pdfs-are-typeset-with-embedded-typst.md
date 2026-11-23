# PDFs are typeset with embedded Typst

A PDF Export turns an Area, Subtree or Page into one PDF with a cover, a table of contents with page numbers, running headers and footers, and the look of a PDF Style. Turtle is one binary that needs nothing else on disk (ADR-0002), and everything it does is an operation that the web app, the API and MCP all reach (ADR-0005). So the PDF is typeset inside the server, by Typst used as a Rust library, and fonts and the default look are built into the binary.

The Typst document is written straight from the document model (`Block` and `Inline`), not from the Page's Markdown, and every piece of Page content is escaped as Typst text, so nothing written in a Page runs as Typst code. A PDF Style holds only fixed fields (logo, colours, fonts, sizes, cover, table of contents, footer text, language) that fill in one layout Turtle owns; there is no free Typst or CSS in a PDF Style.

## Considered Options

- **Headless Chrome with Paged.js**, as the first prototype did: the layout is plain HTML and CSS, but every installation would need Chrome, or Turtle would need Node and Chrome beside it, against ADR-0002.
- **Chrome as an optional dependency**: PDF export only where an operator installed and configured Chrome, so the same Turtle behaves differently from one installation to the next.
- **Printing in the browser** with Paged.js and `window.print()`: nothing on the server, but Agents through the API and MCP could not export a PDF, against ADR-0005.
- **Free Typst or CSS in a PDF Style**: anything goes, but a Style would be tied to the engine, a broken one would only show on export, and a Workspace Admin's code would run in the server.

## Consequences

The binary grows by Typst and by the bundled fonts. Fonts a Workspace Admin uploads must be TTF or OTF, since Typst reads no WOFF or WOFF2. The prototype's look is carried over by hand into Turtle's Typst layout, not reused as code. Exports run synchronously like the Markdown Export; if large ones become too slow, they move to background jobs.
