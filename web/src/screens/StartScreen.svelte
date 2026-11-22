<script lang="ts">
  $effect(() => {
    document.title = 'Turtle'
  })
</script>

<main>
  <h1>Turtle</h1>
  <p>Open an Area in the sidebar, or search for a Page.</p>
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
