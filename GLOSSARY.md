# Turtle

Turtle is an open-source, self-hosted knowledge base where humans and agents write documentation together in real time.

## Language

### Structure

**Workspace**:
The single organisation an installation serves; everything else lives inside it.
_Avoid_: Tenant, Mandant, organisation, instance

**Area**:
A top-level container in the Workspace that holds one tree of Pages and Folders and is the unit permissions are granted on.
_Avoid_: Space, Bereich, project, wiki

**Personal Area**:
The Area that belongs to one Member alone: only that Member sees it, and nothing in it can be granted or shared; to share something, the Member moves it to another Area.
_Avoid_: Private space, inbox, drafts, home

**Page**:
A document with content, placed in the tree of an Area; it may have Pages and Folders beneath it.
_Avoid_: Site, Seite, document, article

**Quick Note**:
A Page a Member writes straight from the start screen, without first finding a place for it; it lands at the top of their Personal Area unless they pick another Area they may write in, can be moved afterwards, and stays known as that Member's Quick Note wherever it sits.
_Avoid_: Note, draft, scratch, memo

**Folder**:
A container in the tree of an Area that groups Pages and other Folders and has no content of its own.
_Avoid_: Ordner, directory

**Entry**:
A Page or Folder as it sits in the tree of an Area, among its siblings.
_Avoid_: Node, item, element

**Subtree**:
A Page or Folder together with everything beneath it in the tree.
_Avoid_: Subsite, Subbereich, sub-area, branch

**Section**:
A heading of a Page with everything after it up to the next heading of the same or a higher level.
_Avoid_: Chapter, block, Abschnitt

**Task list**:
A list in a Page's content whose items each have a checkbox that anyone who can write the Page checks or unchecks. In Markdown its items start with `[ ]` or `[x]`.
_Avoid_: Checklist, to-do list, todo

**Callout**:
A highlighted passage in a Page's content of one of four kinds: note, tip, warning or danger. In Markdown it is a quote whose first line is the kind's marker alone, such as `[!NOTE]`.
_Avoid_: Admonition, alert, info box, panel

**Attachment**:
An image or file that belongs to a Page.
_Avoid_: Upload, asset, media

**Version**:
A recorded state of a Page with its authors, the people whose changes arrived since the previous Version; it can be compared with other Versions and restored.
_Avoid_: Revision, snapshot, history entry

**Trash**:
Where deleted Pages, Folders and Areas are kept for 30 days so they can be brought back. Every Area has its own Trash for its Pages and Folders; a deleted Area is out of reach of its own Admins, so only a Workspace Admin brings it back.
_Avoid_: Recycle bin, archive, Papierkorb

**Tag**:
A free-form label on a Page, shared across the whole Workspace; it ignores upper and lower case and exists as long as one Page carries it.
_Avoid_: Label, category, keyword

**Page link**:
A link in a Page's content to another Page by its ID, shown with that Page's current title so renaming or moving it never breaks the link, and shown without title to someone who may not read it. In Markdown it is a link to the address `turtle:` followed by the Page's ID.
_Avoid_: Internal link, wiki link, reference, mention

### People and agents

**Member**:
A person who belongs to the organisation and has an account in the Workspace, and with it a Personal Area.
_Avoid_: User, account, Nutzer

**Guest**:
An outside person with an account who is invited to specific Areas or Subtrees, can hold Read or Write but never Admin, and is not counted among "all Members".
_Avoid_: External user, collaborator, visitor

**Workspace Admin**:
A Member who manages the other Members and can see every Area except the Personal Areas of other active Members.
_Avoid_: Superuser, owner, root

**Agent**:
A non-human participant that reads and writes through the same permissions as a person and is always recognisable as non-human. It is either a standalone Agent or an Agent acting for a Member.
_Avoid_: Bot, service account, integration

**Standalone Agent**:
An Agent with its own name and Tokens, created by a Workspace Admin, that holds only the Roles given to it by name; a Grant to all Members does not include it.
_Avoid_: Service account, bot user

**Agent acting for a Member**:
An Agent that acts as a Member through one of the Member's Tokens, with no identity of its own, and is named after that Token.
_Avoid_: Personal agent, delegate

**Token**:
A credential that lets an Agent act, either as the Member who owns it or as a standalone Agent, and never with more access than its owner has.
_Avoid_: API key, secret, access key

**Connected app**:
An Agent acting for a Member as the Member sees it on their Connected apps page, named after its Token: an app they allowed by connecting it through OAuth, or a script or client they gave a Token by hand. Disconnecting it revokes the Token.
_Avoid_: Integration, authorized application

### Permissions

**Grant**:
The assignment of a Role on an Area or a Subtree to a Member, a Guest, an Agent, or all Members at once.
_Avoid_: Permission entry, ACL entry, share

**Inheritance**:
The rule that a Page or Folder takes its Grants from what is above it; breaking it gives that Subtree its own Grants, which start as a copy and then change independently.
_Avoid_: Cascading, propagation, Vererbung

**Share Link**:
A public, unguessable address that lets anyone read an Area or Subtree without an account, shown either as a navigable site or as one continuous document with a table of contents.
_Avoid_: Public link, external share, publish

**Role**:
The level of access someone holds on an Area or Subtree: Read, Write, or Admin, each including the one before it.
_Avoid_: Permission level, access level, Berechtigung

### Search

**Snippet**:
The passage of a Page's content shown with a search result, with the words that matched highlighted.
_Avoid_: Excerpt, preview, summary
