<script lang="ts" module>
  /* Icons only where a word would be worse: folding, Folders, the search, and
     a few compact controls of the sidebar. Drawn on 16 units in the text colour. */
  const PATHS = {
    chevron: 'M6 4l4 4-4 4',
    folder: 'M2.5 4.5a1 1 0 0 1 1-1h3l1.5 1.5h4.5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1z',
    plus: 'M8 3v10M3 8h10',
    // A fold box in front of a second one: "all" of them, folded or unfolded.
    collapse: 'M3 5.5h7.5V13H3zM5.5 3h7.5v7.5M4.75 9.25h4',
    expand: 'M3 5.5h7.5V13H3zM5.5 3h7.5v7.5M4.75 9.25h4M6.75 7.25v4',
    search: 'M7 12A5 5 0 1 0 7 2a5 5 0 0 0 0 10zM10.5 10.5L14 14',
    menu: 'M2.5 4.5h11M2.5 8h11M2.5 11.5h11',
    // Dots drawn as strokes of almost no length, so they need a thicker stroke to show.
    more: 'M3.5 8h.01M8 8h.01M12.5 8h.01',
  } as const
  export type IconName = keyof typeof PATHS
</script>

<script lang="ts">
  const { name, size = 16 }: { name: IconName; size?: number } = $props()
</script>

<svg width={size} height={size} viewBox="0 0 16 16" fill="none" aria-hidden="true">
  <path d={PATHS[name]} stroke="currentColor" stroke-width={name === 'more' ? 2.4 : 1.5} stroke-linecap="round" stroke-linejoin="round" />
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
