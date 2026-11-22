<script lang="ts" module>
  /* Icons only where a word would be worse: disclosure, Folder, search, and
     a few compact tree and Entry controls. 16px, 1.5px strokes, currentColor. */
  const PATHS = {
    chevron: 'M6 4l4 4-4 4',
    folder: 'M2.5 4.5a1 1 0 0 1 1-1h3l1.5 1.5h4.5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1z',
    search: 'M7 12A5 5 0 1 0 7 2a5 5 0 0 0 0 10zM10.5 10.5L14 14',
    plus: 'M8 3v10M3 8h10',
    // A tree toggle box in front of a second one: "all" of them, closed or opened.
    collapse: 'M3 5.5h7.5V13H3zM5.5 3h7.5v7.5M4.75 9.25h4',
    expand: 'M3 5.5h7.5V13H3zM5.5 3h7.5v7.5M4.75 9.25h4M6.75 7.25v4',
    more: 'M3.5 8h.01M8 8h.01M12.5 8h.01',
    menu: 'M2.5 4.5h11M2.5 8h11M2.5 11.5h11',
    move: 'M3 8h9M9 5l3 3-3 3',
    close: 'M4 4l8 8M12 4l-8 8',
    agent: 'M5 3.5h6a1.5 1.5 0 0 1 1.5 1.5v5a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 10V5A1.5 1.5 0 0 1 5 3.5zM6.5 7v1M9.5 7v1M8 1.5v2M5.5 14h5',
    page: 'M4.5 2.5h4.5l3 3v8h-7.5zM9 2.5v3h3',
    // Formatting, for the toolbar and the "/" menu.
    text: 'M3.5 3.5h9M8 3.5v9',
    h2: 'M2.5 3.5v9M8 3.5v9M2.5 8H8M10.5 9.25a1.4 1.4 0 1 1 2.5.9L10.5 12.5h3',
    h3: 'M2.5 3.5v9M8 3.5v9M2.5 8H8M10.6 8.9a1.3 1.3 0 0 1 2.5.4c0 .7-.6 1.2-1.3 1.2.7 0 1.4.5 1.4 1.2a1.35 1.35 0 0 1-2.6.4',
    bold: 'M4.5 3h4a2.5 2.5 0 0 1 0 5h-4zM4.5 8h4.75a2.5 2.5 0 0 1 0 5H4.5z',
    italic: 'M7 3h5M4 13h5M9.5 3l-3 10',
    strike: 'M2.5 8h11M11 4.75C10.5 3.7 9.4 3 8 3 6.3 3 5 3.9 5 5.25c0 .9.5 1.5 1.3 1.9M5 11.25C5.5 12.3 6.6 13 8 13c1.7 0 3-.9 3-2.25 0-.5-.15-.9-.45-1.25',
    code: 'M6 4.5L2.5 8 6 11.5M10 4.5l3.5 3.5-3.5 3.5',
    link: 'M6.75 9.25a2.5 2.5 0 0 0 3.5 0l2-2a2.5 2.5 0 0 0-3.5-3.5l-.6.6M9.25 6.75a2.5 2.5 0 0 0-3.5 0l-2 2a2.5 2.5 0 0 0 3.5 3.5l.6-.6',
    bullets: 'M6.5 4.5h7M6.5 8h7M6.5 11.5h7M2.75 4.5h.5M2.75 8h.5M2.75 11.5h.5',
    numbers: 'M7 4.5h6.5M7 8h6.5M7 11.5h6.5M2.5 3.25l1-.5v3M2.4 9.4a1 1 0 0 1 1.85.5c0 .35-.2.6-.5.85l-1.35 1.25h1.9',
    tasks: 'M2.5 3h3v3h-3zM2.5 10.5l1.1 1.1L5.75 9.5M8 4.5h5.5M8 10.5h5.5',
    quote: 'M3.5 3v10M7 4.5h6.5M7 8h6.5M7 11.5h4',
    codeblock: 'M2.5 3.5h11v9h-11zM6.5 6.25L4.75 8l1.75 1.75M9.5 6.25L11.25 8 9.5 9.75',
    callout: 'M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12zM8 7.5v3.25M8 5.25h.01',
    table: 'M2.5 3.5h11v9h-11zM2.5 6.5h11M2.5 9.5h11M6.5 6.5v6',
    image: 'M2.5 3.5h11v9h-11zM2.5 10.5l3-3 3 3 2-2 3 3M10.25 6.25h.01',
    undo: 'M5.5 3.5L2.5 6.5l3 3M2.5 6.5h7a3.5 3.5 0 0 1 0 7H7.5',
    redo: 'M10.5 3.5l3 3-3 3M13.5 6.5h-7a3.5 3.5 0 0 0 0 7h2',
    caret: 'M5 6.5l3 3 3-3',
  } as const
  export type IconName = keyof typeof PATHS
</script>

<script lang="ts">
  let { name, size = 16 }: { name: IconName; size?: number } = $props()
</script>

<svg width={size} height={size} viewBox="0 0 16 16" fill="none" aria-hidden="true">
  <path d={PATHS[name]} stroke="currentColor" stroke-width={name === 'more' ? 2.4 : name === 'bold' ? 1.9 : 1.5} stroke-linecap="round" stroke-linejoin="round" />
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
