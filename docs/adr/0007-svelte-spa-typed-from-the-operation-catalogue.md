# The web app is a Svelte single-page app, typed from the operation catalogue

The web app grows from one editor page into sign-in, Area lists, the tree, Grants and search. Built without a framework, each of these screens would be put together differently, and adopting a framework later would mean rewriting the whole interface. We therefore use Svelte 5 as a plain single-page app built with Vite, without SvelteKit: the server is Rust, nothing is rendered on the server, and the built files are embedded in the binary. Svelte compiles away most of its runtime, which fits a minimal, light interface, and the editor keeps using TipTap's core directly.

The web app reaches the server only through the operation catalogue (ADR-0005) and the editing WebSocket. TypeScript types for every operation are generated from the catalogue at build time, so a changed operation breaks the web app's type check instead of drifting silently.

Styling is plain component-scoped CSS. Colours, spacing and type are CSS custom properties defined in one place; there is no utility framework and no component library, and dialogs and menus build on the browser's own elements. A later design pass is meant to change values, not structure. Toasts are the one exception to building on the browser's own elements: they come from svelte-sonner, dressed in the same custom properties, because stacking, timing and announcing them to screen readers is more than the browser offers.

## Addresses

Addresses are hard to change once people bookmark and share them:

- `/p/{id}/{title}` for a Page and `/a/{id}/{name}` for an Area. A Page's address does not contain its Area, so moving it to another Area breaks no link.
- The title part is only for readability. An address with an outdated or missing title still works and is redirected to the current one.
- `/search`, `/settings/...` and `/setup` for the remaining screens.
- `/join/{secret}` and `/reset-password/{secret}` for the single-use links a Workspace Admin passes on.

## Considered Options

- **React**: official TipTap bindings and the largest ecosystem, but much more JavaScript than the interface needs.
- **Vue**: official TipTap bindings, between Svelte and React in size; no reason to prefer it over Svelte here.
- **Solid**: small and fast, but it has just moved to a new major version, and most available knowledge, including that of coding agents, describes the previous one.
- **No framework**: what the first slice used; rejected because every further screen would invent its own structure.
