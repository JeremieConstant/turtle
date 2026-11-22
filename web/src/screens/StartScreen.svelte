<script lang="ts">
  import { navigation } from '../lib/navigation.svelte'

  $effect(() => {
    document.title = 'Turtle'
  })
</script>

<main>
  <h1>Turtle</h1>
  {#if navigation.areas?.length === 0}
    <p>You cannot see any Areas yet. Create one in the sidebar, or ask the Admin of an Area to give you a Role on it.</p>
  {:else}
    <p>Open an Area in the sidebar, or search for a Page.</p>
  {/if}
</main>

<style>
  main {
    max-width: var(--width-text);
    margin: 0 auto;
    padding: var(--space-12) var(--space-6);
  }

  h1 {
    margin: 0;
    font-size: var(--text-h1);
    letter-spacing: var(--tracking-display);
    line-height: var(--leading-tight);
  }

  p {
    color: var(--color-muted);
  }
</style>
