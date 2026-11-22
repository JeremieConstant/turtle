# Attachment files live beside the database, named by their content

Attachments can be many megabytes each. Kept as blobs in SQLite (ADR-0004) they would grow the one database every operation waits on and make each backup copy all of them again. So the database keeps what an Attachment is (its Page, name, size and kind) and the file itself lies in `attachments/` in the data directory, under the SHA-256 of its content. A backup is still a copy of the data directory, and the same file attached twice is stored once. A file is written beside its place and then moved there, so a file under a hash is always complete.

A browser shows an image from an address and downloads a file from a link, neither of which can be a JSON operation. So besides the operations (`attach_file`, `list_attachments`, `read_attachment`, which carry the content in base64 for the API and MCP), the web app reaches the server through one more route: `GET /api/attachments/{id}`. It decides nothing itself: it asks `read_attachment`'s rule who may read the Attachment, as `/api/pages/{id}/markdown` does for a Page (ADR-0005, ADR-0007).

## Considered Options

- **Blobs in SQLite**: one place for everything, but at the cost above.
- **Files named by Attachment instead of content**: simpler to remove, but stores the same file once per Attachment.

## Consequences

Only PNG, JPEG, GIF and WebP are shown in a Page; every other file, SVG included, is served as a download in a sandbox, so nothing attached runs as part of Turtle. Files no Attachment refers to any more, once their Page is removed from the Trash for good, are not yet removed from the data directory.
