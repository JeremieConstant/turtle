<script lang="ts">
  import { router } from '../lib/router.svelte'

  function goHome(event: MouseEvent) {
    event.preventDefault()
    router.navigate('/')
  }

  $effect(() => {
    document.title = 'Not found · Turtle'
  })
</script>

<main>
  <h1>Nothing here</h1>
  <p>This address does not lead anywhere in Turtle.</p>
  <a href="/" onclick={goHome}>Go to the start</a>
</main>

<style>
  main {
    max-width: var(--width-editor);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
  }

  h1 {
    line-height: var(--leading-tight);
  }

  p {
    color: var(--color-muted);
  }
</style>
