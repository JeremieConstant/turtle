# Team Knowledge Base

An open-source, self-hosted knowledge base where humans and agents write documentation together in real time.

## Language

### Structure

**Workspace**:
The single organisation an installation serves; everything else lives inside it.
_Avoid_: Tenant, Mandant, organisation, instance

**Area**:
A top-level container in the Workspace that holds one tree of Pages and Folders and is the unit permissions are granted on.
_Avoid_: Space, Bereich, project, wiki

**Page**:
A document with content, placed in the tree of an Area.
_Avoid_: Site, Seite, document, article

**Folder**:
A container in the tree of an Area that groups Pages and other Folders and has no content of its own.
_Avoid_: Ordner, directory, section

**Subtree**:
A Page or Folder together with everything beneath it in the tree.
_Avoid_: Subsite, Subbereich, sub-area, branch

### People and agents

**Agent**:
A non-human participant that reads and writes through the same permissions as a person and is always recognisable as non-human.
_Avoid_: Bot, service account, integration

### Permissions

**Role**:
The level of access someone holds on an Area: Read, Write, or Admin, each including the one before it.
_Avoid_: Permission level, access level, Berechtigung
