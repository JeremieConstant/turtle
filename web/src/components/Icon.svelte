<script lang="ts" module>
  /* Icons only where a word would be worse: folding, Folders, the search, a
     few compact controls of the sidebar, the formatting toolbar, and the Agent
     mark. Drawn on 16 units in the text colour. */
  const PATHS = {
    chevron: 'M6 4l4 4-4 4',
    folder: 'M2.5 4.5a1 1 0 0 1 1-1h3l1.5 1.5h4.5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1z',
    plus: 'M8 3v10M3 8h10',
    close: 'M4.5 4.5l7 7M11.5 4.5l-7 7',
    trash: 'M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.5 8.5h6l.5-8.5M6.75 7v3.5M9.25 7v3.5',
    // A fold box in front of a second one: "all" of them, folded or unfolded.
    collapse: 'M3 5.5h7.5V13H3zM5.5 3h7.5v7.5M4.75 9.25h4',
    expand: 'M3 5.5h7.5V13H3zM5.5 3h7.5v7.5M4.75 9.25h4M6.75 7.25v4',
    search: 'M7 12A5 5 0 1 0 7 2a5 5 0 0 0 0 10zM10.5 10.5L14 14',
    menu: 'M2.5 4.5h11M2.5 8h11M2.5 11.5h11',
    agent: 'M5 3.5h6a1.5 1.5 0 0 1 1.5 1.5v5a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 10V5A1.5 1.5 0 0 1 5 3.5zM6.5 7v1M9.5 7v1M8 1.5v2M5.5 14h5',
    // Formatting, for the toolbar.
    text: 'M3.5 3.5h9M8 3.5v9',
    h1: 'M2.5 3.5v9M8 3.5v9M2.5 8H8M11 9l1.5-1v4.5',
    h2: 'M2.5 3.5v9M8 3.5v9M2.5 8H8M10.5 9.25a1.4 1.4 0 1 1 2.5.9L10.5 12.5h3',
    h3: 'M2.5 3.5v9M8 3.5v9M2.5 8H8M10.6 8.9a1.3 1.3 0 0 1 2.5.4c0 .7-.6 1.2-1.3 1.2.7 0 1.4.5 1.4 1.2a1.35 1.35 0 0 1-2.6.4',
    bold: 'M4.5 3h4a2.5 2.5 0 0 1 0 5h-4zM4.5 8h4.75a2.5 2.5 0 0 1 0 5H4.5z',
    italic: 'M7 3h5M4 13h5M9.5 3l-3 10',
    strike: 'M2.5 8h11M11 4.75C10.5 3.7 9.4 3 8 3 6.3 3 5 3.9 5 5.25c0 .9.5 1.5 1.3 1.9M5 11.25C5.5 12.3 6.6 13 8 13c1.7 0 3-.9 3-2.25 0-.5-.15-.9-.45-1.25',
    code: 'M6 4.5L2.5 8 6 11.5M10 4.5l3.5 3.5-3.5 3.5',
    link: 'M6.75 9.25a2.5 2.5 0 0 0 3.5 0l2-2a2.5 2.5 0 0 0-3.5-3.5l-.6.6M9.25 6.75a2.5 2.5 0 0 0-3.5 0l-2 2a2.5 2.5 0 0 0 3.5 3.5l.6-.6',
    bullets: 'M6.5 4.5h7M6.5 8h7M6.5 11.5h7M2.75 4.5h.5M2.75 8h.5M2.75 11.5h.5',
    numbers: 'M7 4.5h6.5M7 8h6.5M7 11.5h6.5M2.5 3.25l1-.5v3M2.4 9.4a1 1 0 0 1 1.85.5c0 .35-.2.6-.5.85l-1.35 1.25h1.9',
    quote: 'M3.5 3v10M7 4.5h6.5M7 8h6.5M7 11.5h4',
    codeblock: 'M2.5 3.5h11v9h-11zM6.5 6.25L4.75 8l1.75 1.75M9.5 6.25L11.25 8 9.5 9.75',
    rule: 'M2.5 8h11M2.5 4.5h4M2.5 11.5h4',
    undo: 'M5.5 3.5L2.5 6.5l3 3M2.5 6.5h7a3.5 3.5 0 0 1 0 7H7.5',
    redo: 'M10.5 3.5l3 3-3 3M13.5 6.5h-7a3.5 3.5 0 0 0 0 7h2',
    caret: 'M5 6.5l3 3 3-3',
    // Dots drawn as strokes of almost no length, so they need a thicker stroke to show.
    more: 'M3.5 8h.01M8 8h.01M12.5 8h.01',
  } as const
  export type IconName = keyof typeof PATHS
  /** Icons that need a stroke other than the usual 1.5 to read at the same weight. */
  const STROKES: Partial<Record<IconName, number>> = { bold: 1.9, more: 2.4 }
</script>

<script lang="ts">
  const { name, size = 16 }: { name: IconName; size?: number } = $props()
</script>

<svg width={size} height={size} viewBox="0 0 16 16" fill="none" aria-hidden="true">
  <path d={PATHS[name]} stroke="currentColor" stroke-width={STROKES[name] ?? 1.5} stroke-linecap="round" stroke-linejoin="round" />
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
