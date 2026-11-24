# Markdown is extended with generic directives

Some elements a Page needs have no Markdown of their own, such as the Status, and plugins from a store will add more later. So beyond what ADR-0006 lists, a Page's content may hold elements written in the generic directive syntax proposed for CommonMark (as remark-directive, MyST and Docusaurus read it): `:name[text]{key=value}` inline, `:::name{key=value}` … `:::` as a block. Turtle's own elements have bare names, such as `:status[ON TRACK]{color=green}`; a plugin's carry its namespace, such as `:acme.kanban{…}`. An element Turtle does not know stays in the content unchanged, is shown in the editor as a locked placeholder that can be moved and deleted but not edited, and is written back to Markdown exactly as it came, so a Page survives an installation that lacks the plugin. This extends ADR-0006: the editor offers what Markdown and directives can express without loss.

Where an element is at heart a link, it stays a Markdown link instead, so other readers still show something sensible: a Mention is `[@Name](turtle:member/<ID>)`, as a Page link is `[Title](turtle:<ID>)`, and Turtle draws it as a badge.

## Considered Options

- **A one-off syntax per element**, as the Callout borrows GitHub's `[!NOTE]`: fine while such syntax exists elsewhere, but there is none for a Status, and plugins would each invent their own.
- **HTML tags or comments** such as `<turtle-status>`: other Markdown readers hide or swallow them, and raw HTML in content invites escaping bugs.
- **A directive for the Mention too** (`:mention{id=…}`): uniform, but other readers would show raw syntax where a link reads well.

## Consequences

The Markdown reader and writer on the server and the editor's schema learn the directive syntax once, with a catch-all for unknown names; the round-trip test covers directives, including unknown ones. Built-in names are reserved, so plugins must be namespaced.
