# Turtle MVP

Turtle is an open-source, self-hosted alternative to Confluence. It keeps the core (writing together, finding things, structure, permissions) and drops the rest. Agents can do everything a person can, and every Page is always available as Markdown.

Terms in capitals are defined in [GLOSSARY.md](../GLOSSARY.md). The reasons behind the foundational decisions are in [adr/](adr/).

## Principles

- **Open source without tiers.** AGPL-3.0, one repository, no paid edition (ADR-0003).
- **Your data is yours.** Every Page can be exported to and imported from Markdown without loss (ADR-0001, ADR-0006).
- **Agents are first-class.** Every capability is available through MCP (ADR-0005).
- **Light to run.** One binary, one node, embedded SQLite, no external services (ADR-0002, ADR-0004).

## In the MVP

### Structure

- One Workspace per installation.
- Areas, each holding one tree of Pages and Folders. A Page may have children; a Folder has no content.
- Order in the tree is manual; new entries go to the end.
- A Page or Folder can be moved, also into another Area if the person has Write in both. It then takes the permissions of the target Area.
- Every Page has a fixed ID. Titles need not be unique. The URL contains the ID and a readable title.
- Deleted Pages and Folders go to the Trash and are removed for good after 30 days.

### Editing

- Rich-text editor; people never see Markdown unless they switch a Page to Markdown view.
- Several people edit the same Page at once and see each other's cursors.
- Elements: headings, lists, task lists, simple tables, code blocks, quotes, callouts, links, images, Attachments. Nothing that Markdown cannot express.
- Tables are created through a small wizard that asks for rows and columns.
- Links to other Pages point at the Page ID and show its current title, so renaming and moving never break them.
- Images and files can be attached to a Page.
- No offline editing.

### Versions

- A Version is recorded when the last editor leaves the Page, and every 2 minutes during a longer session.
- Every write by an Agent is its own Version, so each can be undone on its own.
- Versions show their author (person or Agent), can be compared, and can be restored.

### Search

- Searches titles, Page content and Attachment file names. Attachment content is not searched.
- Inside an Area the search covers that Area; elsewhere it covers everything. A toggle switches to everything.
- Tolerates typos, understands word stems in English and German, ranks title matches higher, shows a snippet per result, shows results while typing.
- A change is findable within a few seconds.
- Results contain only what the searcher may read, including titles and snippets. This is enforced in the backend for UI, API and MCP alike.

### Permissions

- Workspace level: Workspace Admin and Member. Any Member may create an Area and becomes its Admin.
- Area level: the Roles Read, Write and Admin, given through Grants to a Member, an Agent, or all Members.
- A new Area is private to its creator. An Area someone cannot read is invisible to them, including its name.
- The data model already allows Grants on Subtrees and Grants to groups, although neither is offered yet.

### Accounts

- Local accounts with password.
- Invitations and password resets are sent by email over SMTP. Without SMTP configured, the system produces a link for the admin to pass on.
- The interface is English only.

### Agents and MCP

- The MCP server is part of the binary (Streamable HTTP, Token authentication).
- Every operation has an MCP tool; a CI test fails otherwise.
- A personal Token acts as its owner and never exceeds the owner's access. It can be limited to read-only and to chosen Areas, can expire, and can be revoked on its own.
- Standalone Agents are created by a Workspace Admin and receive Grants like a person.
- Agents read and write Markdown. They can replace a whole Page, replace a section identified by its heading, or append. The server applies only the difference, so simultaneous edits by people survive.
- People see Agent edits live, with the Agent's name. Agents have no cursor.

### Export and import

- Export of an Area or Subtree is a ZIP whose folders mirror the tree: one `.md` file per Page, ID and title as frontmatter, Attachments alongside, Page links as relative paths.
- Import reads this format, and also any plain folder of Markdown files.

## After the MVP

In roughly this order:

1. **Breaking Inheritance.** A Page or Folder gets its own Grants for its Subtree, starting as a copy of the inherited ones. They can narrow or widen access. Area Admins can never be locked out. Someone with access to only a Subtree sees the Area by name with that Subtree as its top entry; ancestors and siblings stay invisible, including their titles.
2. **Groups** as recipients of Grants.
3. **Guests.** Invited by an Area Admin to specific Areas or Subtrees with Read or Write.
4. **Share Links.** Read-only public access to an Area or Subtree, as a navigable site or as one document with a table of contents. Revocable, optional password and expiry, hidden from search engines.
5. **Tags** on Pages: Workspace-wide, created by typing, set by anyone with Write, usable as a search filter, included in export frontmatter.
6. **Backlinks**: which Pages link to this one.
7. **Command palette.** Cmd/Ctrl+K opens the search as a palette; a leading `>` switches to commands, which are the same operations MCP exposes.
8. **Comments**, inline and at the end of a Page.
9. **Templates** as ordinary Pages that are copied.
10. **OIDC sign-in**, then translations starting with German.

## Never

Macros, blogs, whiteboards, databases, likes, task management, a plugin marketplace, multi-tenancy, clustering.

## Technology

- Backend in Rust, with `yrs` for the collaborative documents, SQLite for storage and Tantivy for search.
- Frontend: a small single-page app in TypeScript with TipTap and Yjs, embedded in the binary. Its looks do not matter for the MVP.
- Live editing over WebSocket. Open documents are held in memory and written to SQLite in batches.

## Open risks

- **500 people working at once** is the target but unmeasured. A load test must confirm it.
- **Lossless Markdown round trip** and **merging Agent edits into a live document** are the hardest parts and are proven first.

## First step

A narrow end-to-end slice of the riskiest part: two browsers edit the same Page live with cursors, the state survives a restart, and the Page converts to Markdown and back without loss. Tree, permissions, search and MCP follow once this holds.
