<script lang="ts">
  import type { Snippet } from 'svelte'
  import { router } from '../lib/router.svelte'
  import { session } from '../lib/session.svelte'

  const { children }: { children?: Snippet } = $props()

  function goHome(event: MouseEvent) {
    event.preventDefault()
    router.navigate('/')
  }

  function goToMembers(event: MouseEvent) {
    router.follow(event, '/settings/members')
  }

  function goToTokens(event: MouseEvent) {
    router.follow(event, '/settings/tokens')
  }

  async function signOut() {
    await session.signOut()
    router.navigate('/sign-in')
  }
</script>

<header>
  <a class="brand" href="/" onclick={goHome}>Turtle</a>
  {@render children?.()}
  <nav>
    {#if session.member?.workspace_admin}
      <a href="/settings/members" onclick={goToMembers}>Members</a>
    {/if}
    <a href="/settings/tokens" onclick={goToTokens}>Tokens</a>
  </nav>
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

  nav {
    display: flex;
    gap: var(--space-3);
    margin-left: auto;
  }
</style>
