<script lang="ts" module>
  /* Icons only where a word would be worse: disclosure, Folder, search, and
     a few compact tree controls. 16px, 1.5px strokes, currentColor. */
  const PATHS = {
    chevron: 'M6 4l4 4-4 4',
    folder: 'M2.5 4.5a1 1 0 0 1 1-1h3l1.5 1.5h4.5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1z',
    search: 'M7 12A5 5 0 1 0 7 2a5 5 0 0 0 0 10zM10.5 10.5L14 14',
    plus: 'M8 3v10M3 8h10',
    collapse: 'M4 6.5l4-3 4 3M4 12.5l4-3 4 3',
    expand: 'M4 3.5l4 3 4-3M4 9.5l4 3 4-3',
    menu: 'M2.5 4.5h11M2.5 8h11M2.5 11.5h11',
    move: 'M3 8h9M9 5l3 3-3 3',
    close: 'M4 4l8 8M12 4l-8 8',
    agent: 'M5 3.5h6a1.5 1.5 0 0 1 1.5 1.5v5a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 10V5A1.5 1.5 0 0 1 5 3.5zM6.5 7v1M9.5 7v1M8 1.5v2M5.5 14h5',
    page: 'M4.5 2.5h4.5l3 3v8h-7.5zM9 2.5v3h3',
  } as const
  export type IconName = keyof typeof PATHS
</script>

<script lang="ts">
  let { name, size = 16 }: { name: IconName; size?: number } = $props()
</script>

<svg width={size} height={size} viewBox="0 0 16 16" fill="none" aria-hidden="true">
  <path d={PATHS[name]} stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
