# Roadmap

Where Turtle is heading, in three parts: what is being built now, what comes after, and what Turtle will not become.

## Now: the MVP

Turtle is building its MVP, tracked in [#1](https://github.com/turtle-wiki/turtle/issues/1) and described in [docs/mvp.md](docs/mvp.md) under [In the MVP](docs/mvp.md#in-the-mvp).

Two milestones mark the way:

- **[First public showing](https://github.com/turtle-wiki/turtle/milestone/1)**: what Turtle needs before it is shown to people outside the project, such as a README, a contribution guide, a security policy and releases with readable notes.
- **[1.0](https://github.com/turtle-wiki/turtle/milestone/2)**: the MVP complete, so that a team can rely on Turtle for its documentation.

## Next: after the MVP

What follows the MVP, in roughly that order, is the list [After the MVP](docs/mvp.md#after-the-mvp) in `docs/mvp.md`.

## Never

- **No paid edition.** Every feature is in this repository under AGPL-3.0, with no enterprise edition and no CLA ([ADR-0003](docs/adr/0003-agpl-without-open-core.md)).
- **Nothing the editor cannot express as Markdown.** A Page holds only what survives export to Markdown and import back ([ADR-0006](docs/adr/0006-editor-is-limited-to-what-markdown-can-express.md)).

These two hold beside the list [Never](docs/mvp.md#never) in `docs/mvp.md`, which names the features Turtle leaves out.

To suggest something, open a [feature request](https://github.com/turtle-wiki/turtle/issues/new?template=feature_request.yml).
