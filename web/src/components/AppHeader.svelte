<script lang="ts">
  import type { Snippet } from 'svelte'
  import { router } from '../lib/router.svelte'
  import { session } from '../lib/session.svelte'

  const { children }: { children?: Snippet } = $props()

  function goHome(event: MouseEvent) {
    event.preventDefault()
    router.navigate('/')
  }

  async function signOut() {
    await session.signOut()
    router.navigate('/sign-in')
  }
</script>

<header>
  <a class="brand" href="/" onclick={goHome}>Turtle</a>
  {@render children?.()}
  <button type="button" onclick={signOut}>Sign out</button>
</header>

<style>
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: var(--line-width) solid var(--color-line);
  }

  .brand {
    font-weight: var(--weight-strong);
    text-decoration: none;
  }

  button {
    margin-left: auto;
  }
</style>
