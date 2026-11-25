<script lang="ts">
  /** Tells everyone in a Demo that what they change does not last. It cannot be dismissed. */
  let { resetsEveryHours }: { resetsEveryHours: number | null } = $props()

  const text = $derived(
    resetsEveryHours === null
      ? 'This is a demo. Everything you change will be reset.'
      : resetsEveryHours === 1
        ? 'This is a demo. Everything is reset every hour.'
        : `This is a demo. Everything is reset every ${resetsEveryHours} hours.`,
  )
</script>

<p class="demo-notice" role="note">{text}</p>

<style>
  .demo-notice {
    flex: none;
    margin: 0;
    padding: var(--space-1) var(--space-4);
    border-bottom: var(--line) solid var(--color-line);
    background: var(--color-sunken);
    color: var(--color-text-2);
    font-size: var(--text-s);
    text-align: center;
  }
</style>
