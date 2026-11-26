# Roadmap

Where Turtle is heading, in three parts: what has been built, what comes next, and what Turtle will not become.

## Done: the MVP

The MVP is complete: Areas, Pages and Folders, live editing, Versions and the Trash, Search, Roles, Mail, Import and Export, and Agents through MCP. It was tracked in [#1](https://github.com/turtle-wiki/turtle/issues/1) and is described in [docs/mvp.md](docs/mvp.md). [Turtle 0.2.0](https://github.com/turtle-wiki/turtle/releases/tag/v0.2.0) is the first Release with all of it.

## Next: one milestone per Release

Each milestone below is one Release. Until 1.0, the data format and the API may change between them; Turtle brings an existing data directory up to date when it starts.

- **0.3: Moving in.** A team can bring its documentation from Confluence and learn Turtle without asking anyone.
  - Import a Confluence space ([#85](https://github.com/turtle-wiki/turtle/issues/85))
  - A first version of the user documentation
- **0.4: Sharing beyond the team.** Access can be narrower and wider than a whole Area.
  - Breaking Inheritance: a Page or Folder with Grants of its own
  - Guests, invited to chosen Areas or Subtrees
  - Share Links: read-only public access to an Area or Subtree
- **0.5: Larger teams.** Turtle fits an organisation's sign-in and language.
  - Groups as recipients of Grants
  - OIDC sign-in
  - A German interface ([#84](https://github.com/turtle-wiki/turtle/issues/84))
  - Hand over a deactivated Member's Personal Area ([#40](https://github.com/turtle-wiki/turtle/issues/40))
- **0.6: Working together.** Pages talk to each other and to the people writing them.
  - Comments, inline and at the end of a Page
  - Backlinks: which Pages link to this one
  - Templates, as ordinary Pages that are copied
  - The command palette: a leading `>` in the search runs the operations MCP exposes
- **1.0: Something to rely on.** No new features, but promises.
  - The data format and the API stay compatible within 1.x
  - Every update from 0.6 on is tested against real data directories
  - Backup, restore and updating are documented and tested
  - An outside look at security

## Never

- **No paid edition.** Every feature is in this repository under AGPL-3.0, with no enterprise edition and no CLA ([ADR-0003](docs/adr/0003-agpl-without-open-core.md)).
- **Nothing the editor cannot express as Markdown.** A Page holds only what survives export to Markdown and import back ([ADR-0006](docs/adr/0006-editor-is-limited-to-what-markdown-can-express.md)).

These two hold beside the list [Never](docs/mvp.md#never) in `docs/mvp.md`, which names the features Turtle leaves out.

To suggest something, open a [feature request](https://github.com/turtle-wiki/turtle/issues/new?template=feature_request.yml).
