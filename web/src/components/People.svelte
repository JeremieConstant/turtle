<script lang="ts">
  import type { Awareness } from 'y-protocols/awareness'
  import type { Person } from '../lib/person'

  const { awareness }: { awareness: Awareness } = $props()

  let people = $state.raw<Person[]>([])

  $effect(() => {
    let shown = true
    // Presence changes arrive from the editor and the network at any moment,
    // also while Svelte is updating the screen, so they are taken in afterwards.
    const read = () =>
      queueMicrotask(() => {
        if (!shown) return
        people = [...awareness.getStates().values()].flatMap(state => (state.user ? [state.user as Person] : []))
      })
    read()
    awareness.on('change', read)
    return () => {
      shown = false
      awareness.off('change', read)
    }
  })
</script>

<span class="people">
  {#each people as person}
    <span class="person" style:--person={person.color}>{person.name}</span>
  {/each}
</span>

<style>
  .people {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .person {
    padding: var(--space-hair) var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--person);
    color: var(--color-on-person);
    font-size: var(--text-s);
  }
</style>
