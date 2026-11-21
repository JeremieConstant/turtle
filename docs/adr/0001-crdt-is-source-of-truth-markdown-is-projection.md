# The CRDT document is the source of truth; Markdown is a projection

Real-time co-authoring with live cursors needs a CRDT, and a CRDT and Markdown files on disk cannot both be authoritative without drifting apart. We store each Page as a CRDT document and treat Markdown as a projection that is always available through export, import, the API and MCP. "Your data belongs to you" therefore means every Page can be exported to and imported from Markdown at any time, not that `.md` files sit on the server.

## Considered Options

- **Markdown files as the truth, CRDT only during an editing session**: rejected because loading and flushing on every session creates a second truth and loses concurrent edits when they disagree.
- **No CRDT, locking or last-write-wins**: rejected because co-authoring is the most important feature.
