# A ZIP to import is uploaded ahead of the operation that reads it

An operation takes its input as JSON (ADR-0005), so the ZIP an import reads was sent in base64 inside it. That holds the ZIP in memory several times over: the request, the parsed input, the decoded bytes, and then every Attachment read from it. A space of 155 MB took close to 900 MB, and a browser cannot build such a request at all once the ZIP has several hundred MB. A whole Confluence space with its attachments is often larger than that.

So a ZIP can be uploaded ahead of the import: sent as it is, as the body of `POST /api/uploads` with the content type `application/zip`, and written to a file in `uploads/` in the data directory as it arrives. The answer holds an `upload_id`, which the import operations take in place of `content`. The import reads the ZIP from its file, reads what describes the space as a stream, and takes each Attachment out of the ZIP only when it attaches it. Each Page it has written is saved and let go of, unless someone has it open. Memory then grows with the number of pages, not with the size of the ZIP.

The upload is a route of the HTTP adapter, not an operation: it carries bytes and decides nothing about them, as `GET /api/attachments/{id}` does in the other direction (ADR-0009). It is a second exception to the web app reaching the server only through the catalogue (ADR-0007). Who may import where, and what the ZIP holds, is decided by the import operation that reads it, the same whichever way the ZIP came; who may upload at all, how much and how long it is kept is decided in the operation layer too, which the route only calls. `content` in base64 stays, for a ZIP of at most 200 MB and no larger than a request may be, so an Agent can still import through MCP alone.

- **An upload belongs to whoever sent it.** Only they can import it; to anyone else it does not exist.
- **It does not stay.** An import uses it up, a check before importing leaves it, and one nobody imports is removed after an hour. What Turtle knows of uploads is held in memory, so none outlasts a restart, and `uploads/` is emptied at each start.
- **One at a time for each person.** A new upload takes the place of the last, and a second cannot begin while one is arriving; one that stops arriving for a minute is given up. So beside the ZIPs their running imports are still reading, a person's uploads take at most twice `TURTLE_MAX_IMPORT_MB` on disk.
- **Only someone who may change things uploads**: anyone signed in, a Guest included, but not through a read-only Token. Who they are is found out before any of the ZIP is taken. The route accepts only `application/zip` and `application/octet-stream`, which a form on another site cannot send, as operations accept only JSON.

## Considered Options

- **Raising the limit on `content`**: no new route, but memory is the limit, and it is the browser's as much as the server's.
- **A multipart form with the input and the file**: one request instead of two, but an operation would then have a second way of taking its input that only HTTP has, and a form is what other sites can post.
- **Uploading in pieces with operations** (`begin_upload`, `append_to_upload`): everything stays in the catalogue and works through MCP, but each piece is base64 again, and a 2 GB ZIP becomes hundreds of calls an Agent would have to make itself.

## Consequences

`TURTLE_MAX_IMPORT_MB` now bounds what an upload takes on disk rather than in memory, and its default is 2 GB. The data directory needs that much room while an import waits or runs, besides what the import brings in. A reverse proxy in front of Turtle has to accept a request as large.

An import still holds every page's content in memory from reading the space until it is written, about 25 MB for every 1,000 pages. A made-up space of 20,000 pages and 4,000 attachments, a ZIP of 1.9 GB, came in within 20 seconds holding 450 MB at most.

Whoever can sign in can put that much on the disk the database lies on, for an hour. So a Demo, which anyone can sign in to, takes 200 MB unless `TURTLE_MAX_IMPORT_MB` says otherwise; any other Turtle open to people who are not trusted with that much disk sets it lower.

An Agent that reaches Turtle only through MCP cannot upload, and so imports ZIPs up to what an MCP request may carry, which is the largest file that can be attached, 25 MB unless set otherwise. One that can make HTTP requests uploads with its Token as a person's browser does with its session.

The route's answer is not in the catalogue, so the web app's type for it is written by hand rather than generated, and has to be changed with it.
