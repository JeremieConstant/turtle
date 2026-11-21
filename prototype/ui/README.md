# Turtle interface prototype

A throwaway prototype to settle how Turtle looks and feels. It does not talk to the server: all data comes from `src/lib/mock/`, and changes live in memory until the page reloads.

```bash
npm install
npm run dev
```

## What carries over to `web/`

- `src/styles/tokens.css`: every colour, distance, type setting, radius and motion duration. Reduced motion sets the durations to 0.
- The structure: Svelte 5 single-page app without SvelteKit, plain component-scoped CSS, `<dialog>` and `popover` for dialogs and menus (ADR-0007).
- `src/lib/mock/ops.svelte.ts`: every action in the interface is one of these functions, named after the operation an Agent would call (`list_areas`, `get_tree`, `move_entry`, `create_page`, `create_quick_note`, `search`, ...). Swapping them for the generated operation calls is the main step to make it real. They are synchronous here; the real ones are not.
- `src/editor/`: TipTap limited to what Markdown can express (ADR-0006), and the look of other people's cursors and an Agent's change.

## Where to look

- `/`: search and Quick Notes
- `/p/p-deploy/deploying-to-production`: two Members editing with live cursors; the Docs Agent adds a paragraph after about four seconds
- `/p/p-demo/demo-script`: a Page the Member can only read (Read Role on Sales)
- `/a/archive/archive`: an empty Area
- `/search?q=rollback`: search results

## What is simulated

`src/lib/mock/live.ts` stands in for the editing connection: it moves the two cursors, types Lena's sentence and inserts the Agent's paragraph. Quick Notes have a `list_quick_notes` operation that the real catalogue does not have yet.
