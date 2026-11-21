<script lang="ts">
  import type { Snippet } from 'svelte'
  import { router } from '../lib/router.svelte'
  import { session } from '../lib/session.svelte'
  import SearchBox from './SearchBox.svelte'

  /** `areaId` is the Area being looked at, which the search covers by default. */
  const { children, areaId }: { children?: Snippet; areaId?: string } = $props()

  function goHome(event: MouseEvent) {
    event.preventDefault()
    router.navigate('/')
  }

  function goToMembers(event: MouseEvent) {
    router.follow(event, '/settings/members')
  }

  async function signOut() {
    await session.signOut()
    router.navigate('/sign-in')
  }
</script>

<header>
  <a class="brand" href="/" onclick={goHome}>Turtle</a>
  {@render children?.()}
  <SearchBox {areaId} />
  {#if session.member?.workspace_admin}
    <a class="members" href="/settings/members" onclick={goToMembers}>Members</a>
  {/if}
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

  .members {
    margin-left: auto;
  }

  .members + button {
    margin-left: 0;
  }

  button {
    margin-left: auto;
  }
</style>
