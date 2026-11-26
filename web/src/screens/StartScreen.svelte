<script lang="ts">
  /*
   * The start screen answers two questions: what do I want to write down,
   * and where was I working. Finding anything else is the search at the top
   * of the sidebar, the same on every screen.
   */
  import QuickNotes from '../components/QuickNotes.svelte'
  import RecentPages from '../components/RecentPages.svelte'
  import { navigation } from '../lib/navigation.svelte'
  import { session } from '../lib/session.svelte'

  // Quick Notes land in the Personal Area, so whoever has none gets no box.
  const personal = $derived(navigation.areas?.find(area => area.personal))
  const mac = /Mac|iPhone|iPad/.test(navigator.platform)
  let recent = $state<RecentPages>()

  $effect(() => {
    document.title = 'Turtle'
  })
</script>

<main>
  {#if personal && session.member}
    <QuickNotes {personal} memberId={session.member.id} onsaved={() => recent?.load()} />
  {:else if navigation.areas}
    <p class="hint">Open an Area in the sidebar, or press {mac ? '⌘K' : 'Ctrl K'} to search every Area.</p>
  {/if}
  <RecentPages bind:this={recent} />
</main>

<style>
  main {
    max-width: calc(var(--width-start) + 2 * var(--space-6));
    margin: 0 auto;
    padding: var(--space-start-top) var(--space-6) var(--space-16);
  }

  .hint {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  @media (max-width: 800px) {
    main {
      padding: var(--space-12) var(--space-4);
    }
  }
</style>
