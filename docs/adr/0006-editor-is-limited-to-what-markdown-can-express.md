# The editor is limited to what Markdown can express

People edit in a rich-text editor and never see Markdown unless they ask for it, so Markdown is produced by conversion (ADR-0001). To keep the promise that every Page is fully available as Markdown, the editor offers only elements that convert to Markdown and back without loss: headings, lists, task lists, simple tables, code blocks, quotes, callouts, links, images and Attachments. Text colours, merged table cells and column layouts are left out on purpose, and a round-trip test covers every element. Adding an element that Markdown cannot express would silently break export, import and agent editing.

## Considered Options

- **Markdown text as the stored content, with a live-preview editor**: lossless by construction and simplest for agents, but rejected because people would see Markdown syntax.
- **Unrestricted rich text with best-effort Markdown export**: rejected because the export would lose content.

## Consequences

Agents read and write Markdown. The server converts what they send and applies only the difference to the stored document, so edits a person is making at the same moment survive.

ADR-0013 extends this: elements Markdown has no syntax for are written as generic directives.
