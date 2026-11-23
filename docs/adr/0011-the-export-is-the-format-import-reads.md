# The export is the format import reads, and it keeps the whole tree

An export (the `export` operation) must be readable without Turtle and must come back unchanged through a later import: the same tree, titles, order, content and Tags. So its layout is a contract with the import, not a detail of the export, and it is fixed here.

- **Every Entry is a file.** A Page or Folder is a Markdown file named after its title, and a directory of the same name holds what is beneath it. A Folder has a file too, holding only frontmatter, so its ID, title and position survive.
- **Frontmatter carries what a name cannot.** `id`, `kind` (`page` or `folder`), `title`, `position` (counted from 1 among the siblings in the export) and, for a Page, `tags`. File names are cleaned up for file systems and made unique among siblings ("Notes (2)"), so the title is read from the frontmatter, never from the name.
- **Attachments are apart.** A Page's Attachments are in `_attachments` inside its directory, never among its Pages. A Page beneath it that would be named `_attachments` gives way as `_attachments (2)`.
- **Links are relative paths.** A Page link to a Page in the export, and the address of one of its Attachments, become a relative path; a Page link to any other Page keeps its `turtle:` address.
- **What the exporter cannot read is left out**, and what they can read beneath it takes its place, as `get_page` leaves it out of a Page's path. Such a Page comes back under a different parent; it is the price of giving the exporter everything they may read.

## Considered Options

- **Attachments beside the Page's file**: closer to the words "beside their Page", but they would share names with the Page's siblings, and an import could not tell an attached `notes.md` from a Page.
- **Attachments among the Page's Pages**, as first built: an attached `Deploy.md` looked exactly like a Page named Deploy.
- **Folders as bare directories**: tidier to read, but a Folder's ID, its title as written and its place among its siblings were lost.
- **Leaving out everything beneath an unreadable Entry**: keeps the shape, but leaves out Pages the exporter may read.

## Consequences

The import (PRD #1, "Export and import") reads this layout: a `.md` file with `kind: folder` is a Folder, any other `.md` file is a Page, files in a `_attachments` directory are the Attachments of the Page whose directory it is, and a relative link to an exported file becomes a Page link again. A file without frontmatter, from any folder of Markdown, is a Page titled after its name. Changing this layout means changing the import with it.
