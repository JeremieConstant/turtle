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

**Page**:
A document with content, placed in the tree of an Area; it may have Pages and Folders beneath it.
_Avoid_: Site, Seite, document, article

**Folder**:
A container in the tree of an Area that groups Pages and other Folders and has no content of its own.
_Avoid_: Ordner, directory, section

**Entry**:
A Page or Folder as it sits in the tree of an Area, among its siblings.
_Avoid_: Node, item, element

**Subtree**:
A Page or Folder together with everything beneath it in the tree.
_Avoid_: Subsite, Subbereich, sub-area, branch

**Attachment**:
An image or file that belongs to a Page.
_Avoid_: Upload, asset, media

**Version**:
A recorded state of a Page with its author, which can be compared with other Versions and restored.
_Avoid_: Revision, snapshot, history entry

**Trash**:
Where deleted Pages and Folders are kept so they can be brought back.
_Avoid_: Recycle bin, archive, Papierkorb

### People and agents

**Tag**:
A free-form label on a Page, shared across the whole Workspace.
_Avoid_: Label, category, keyword

**Member**:
A person who belongs to the organisation and has an account in the Workspace.
_Avoid_: User, account, Nutzer

**Guest**:
An outside person with an account who is invited to specific Areas or Subtrees, can hold Read or Write but never Admin, and is not counted among "all Members".
_Avoid_: External user, collaborator, visitor

**Workspace Admin**:
A Member who manages the other Members and can see every Area.
_Avoid_: Superuser, owner, root

**Agent**:
A non-human participant that reads and writes through the same permissions as a person and is always recognisable as non-human.
_Avoid_: Bot, service account, integration

**Token**:
A credential that lets an Agent act, either as the Member who owns it or as a standalone Agent, and never with more access than its owner has.
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
