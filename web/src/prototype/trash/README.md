# Trash prototype (issue #14)

Throwaway. It answers one question: where does the Trash live in the interface, and what does a deleted Subtree look like with restoring and the 30-day limit?

Three variants on one route, inside the real shell, switchable with the floating bar, the arrow keys or `?variant=`:

- **A, Trash per Area** (`/prototype/trash?variant=A`): a "Trash" row at the end of an Area's tree in the sidebar; a screen per Area that lists deleted Subtrees, each unfolding to what went with it.
- **B, One Trash for the Workspace** (`?variant=B`): one row at the foot of the sidebar; a table of everything, grouped by how soon it is gone, narrowed by Area, with rows ticked and restored together.
- **C, Deleted stay in the tree** (`?variant=C`): no Trash screen; a "Show deleted" switch on the Area screen shows deleted Entries where they were.

Run Turtle as usual (`scripts/preview.sh`, then `scripts/seed.sh` on a fresh data directory), sign in and open `/prototype/trash`. The bar shows on a dev server or a loopback address only.

The deleted Entries are made up and live in memory (`state.svelte.ts`); restoring and deleting for good are stubs. The Areas and the live tree in C are real.
