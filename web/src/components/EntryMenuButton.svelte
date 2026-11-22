<script lang="ts" module>
  /**
   * Opens the Entry menu of a row on a right click, beside the row's ⋯
   * button, so both ways lead to the same place. A row without the button
   * has no menu and keeps the browser's own.
   */
  export function openFromRow(event: MouseEvent, open: (trigger: HTMLElement) => void) {
    const button = (event.currentTarget as HTMLElement).querySelector<HTMLElement>('[data-entry-menu]')
    if (!button) return
    event.preventDefault()
    open(button)
  }
</script>

<script lang="ts">
  import Icon from './Icon.svelte'

  /**
   * The ⋯ button that opens the Entry menu of a row. It shows on hovering the
   * row, on keyboard focus, and while its menu is open.
   */
  const {
    title,
    open,
    onopen,
  }: {
    /** The title of the Entry, for screen readers. */
    title: string
    /** Whether its menu is open. */
    open: boolean
    onopen: (trigger: HTMLElement) => void
  } = $props()
</script>

<button
  type="button"
  class="more"
  class:open
  data-entry-menu
  aria-label="Actions for {title}"
  aria-haspopup="menu"
  onclick={event => onopen(event.currentTarget)}
>
  <Icon name="more" size={14} />
</button>

<style>
  .more {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--size-chevron);
    height: var(--size-chevron);
    margin-left: var(--space-1);
    padding: 0;
    border: 0;
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--color-muted);
    opacity: 0;
    transition:
      opacity var(--dur-fast) var(--ease-out),
      background var(--dur-fast) var(--ease-out);
  }

  :global(:hover) > .more,
  .more.open,
  .more:focus-visible {
    opacity: 1;
  }

  .more:hover,
  .more.open {
    background: var(--color-line);
    color: var(--color-text);
  }
</style>
