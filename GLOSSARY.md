# Turtle

Turtle is an open-source, self-hosted knowledge base where humans and agents write documentation together in real time.

## Language

### Structure

**Workspace**:
The single organisation an installation serves; everything else lives inside it.
_Avoid_: Tenant, Mandant, organisation, instance

**Demo**:
A Turtle its settings declare a demo: every screen says so and how often the hosting resets it. Its demo Member is a Member without Workspace Admin whose email address and password the settings name and Turtle shows to everyone, so anyone can sign in as them.
_Avoid_: Sandbox, trial, playground

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

**Directive**:
A piece of a Page's content written in Markdown as `:name[text]{key=value}` in running text or as a block from `:::name{key=value}` to `:::`. Turtle's own have bare names, such as `status`; anyone else's carry a namespace, such as `acme.board`. One Turtle does not know is kept exactly as written and shown as a locked placeholder, "Unknown element: <name>", that can be moved and deleted but not edited.
_Avoid_: Shortcode, macro, extension

**Status**:
A short coloured marker in the running text of a Page's content, holding any text in one of six colours: grey, blue, green, yellow, red or purple.
_Avoid_: Chip, badge, lozenge, label

**Table**:
Rows of cells in a Page's content, the first row being the header; a cell holds text with formatting and links but no other blocks, and cells are never merged. In Markdown it is a table as GitHub writes them, with a `|` inside a cell written `\|`.
_Avoid_: Grid, spreadsheet, matrix

**Attachment**:
An image or file that belongs to a Page.
_Avoid_: Upload, asset, media

**Export**:
An Area or Subtree as a ZIP of Markdown files that is readable without Turtle: directories mirror the tree, every Page and Folder is a file named after its title with what an import needs as frontmatter, a Page's Attachments sit in its directory apart from its Pages, and Page links between its files are relative paths.
_Avoid_: Backup, download, dump

**Import**:
Pages, Folders and Attachments brought into an Area, or beneath a Page or Folder, from a ZIP: an Export, any folder of Markdown files, whose relative links become Page links, or a Confluence space exported as XML. What cannot come across is reported file by file, and the rest comes in. An import from Confluence also reports how much of the space came across: each kind of Confluence content found, whether it comes across exactly, approximately, as a placeholder naming it or not at all, and what the space holds that is not imported, such as comments and page history; this report can be had before importing, without importing. A file larger than the Turtle accepts is reported, before anything is created when the import is from Confluence. An import can also be started and then followed while it runs, as it reads the ZIP, creates the Pages and Folders and writes their content, by whoever started it.
_Avoid_: Upload, ingest

**PDF Export**:
An Area, Subtree or Page typeset as one PDF for reading and handing on: a cover, a table of contents with page numbers, the Pages as chapters in tree order and the Folders as groups, laid out by a PDF Style. Unlike an Export it is not read back by an import.
_Avoid_: Print, PDF download, report

**PDF Style**:
A named look for PDF Exports, shared across the Workspace and managed by Workspace Admins: a logo, colours, fonts, sizes, whether there is a cover and a table of contents, a footer text and the language of the PDF's fixed labels. It fills in fixed fields of one layout and holds no code; exactly one PDF Style is the default.
_Avoid_: Template, theme, layout, Vorlage

**Uploaded font**:
A TTF or OTF file holding one style of one font family, such as a corporate font's bold, that a Workspace Admin uploads so every PDF Style can use the family besides those built into Turtle. It belongs to the Workspace, not to a PDF Style; the last file of a family a PDF Style uses cannot be deleted.
_Avoid_: Custom font, Schriftart

**Version**:
A recorded state of a Page with its authors, the people whose changes arrived since the previous Version; it can be compared with other Versions and restored.
_Avoid_: Revision, snapshot, history entry

**Release**:
A numbered, published state of Turtle itself, such as 0.2.0, with its release notes.
_Avoid_: Version (that is a Page's), build

**Trash**:
Where deleted Pages, Folders and Areas are kept for 30 days so they can be brought back. Every Area has its own Trash for its Pages and Folders; a deleted Area is out of reach of its own Admins, so only a Workspace Admin brings it back.
_Avoid_: Recycle bin, archive, Papierkorb

**Tag**:
A free-form label on a Page, shared across the whole Workspace; it ignores upper and lower case and exists as long as one Page carries it.
_Avoid_: Label, category, keyword

**Page link**:
A link in a Page's content to another Page by its ID, shown with that Page's current title so renaming or moving it never breaks the link, and shown without title to someone who may not read it. In Markdown it is a link to the address `turtle:` followed by the Page's ID.
_Avoid_: Internal link, wiki link, reference

**Mention**:
A reference in a Page's content to a Member or Guest by their ID, shown with their current picture and name and leading to their Profile; an Agent cannot be mentioned. Mentioning someone does not notify them.
_Avoid_: User tag, user badge, @-link

### People and agents

**Member**:
A person who belongs to the organisation and has an account in the Workspace, and with it a Personal Area.
_Avoid_: User, account, Nutzer

**Guest**:
An outside person with an account who is invited to specific Areas or Subtrees, can hold Read or Write but never Admin, and is not counted among "all Members".
_Avoid_: External user, collaborator, visitor

**Profile**:
What Turtle shows others about a Member or Guest: their picture, name and email, and whether they are a Guest or a Workspace Admin. Only the person sets their picture, and a Workspace Admin may remove it; without one, their initials stand in. A Member sees everyone's Profile, a Guest only the Profiles of people who can reach an Area or Subtree the Guest can reach.
_Avoid_: Account page, user page, About me

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
A credential that lets an Agent act, either as the Member who owns it or as a standalone Agent, and never with more access than its owner has. A Member creates one by hand, or an app creates one through OAuth once the Member allows it, named after the app; the app then holds only short-lived access tokens for it. The "Connected apps" page lists a Member's Tokens of both kinds as apps, and disconnecting one there revokes its Token. In code, a Token made through OAuth is called a connection.
_Avoid_: API key, secret, access key

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
The passage of a Page's content shown with a search result, with the words that matched highlighted. When only the file name of one of the Page's Attachments matched, that file name.
_Avoid_: Excerpt, preview, summary
