<script lang="ts">
  /*
   * Who else is on the Page. A person is a round mark with their initials in
   * the colour of their cursor. An Agent is square, carries the Agent mark and
   * its name, so it never reads as a person; it lights up while its change is
   * marked in the text.
   */
  import type { Awareness } from 'y-protocols/awareness'
  import type { Present } from '../lib/person'
  import Icon from './Icon.svelte'

  const {
    awareness,
    working,
  }: {
    awareness: Awareness
    /** The names of the Agents whose change is marked right now. */
    working: ReadonlySet<string>
  } = $props()

  let people = $state.raw<[number, Present][]>([])

  $effect(() => {
    let shown = true
    // Presence changes arrive from the editor and the network at any moment,
    // also while Svelte is updating the screen, so they are taken in afterwards.
    const read = () =>
      queueMicrotask(() => {
        if (!shown) return
        people = [...awareness.getStates()].flatMap(([client, state]) =>
          client !== awareness.clientID && state.user ? [[client, state.user as Present]] : [],
        )
      })
    read()
    awareness.on('change', read)
    return () => {
      shown = false
      awareness.off('change', read)
    }
  })

  const initials = (name: string) =>
    name
      .split(/\s+/)
      .filter(Boolean)
      .map(part => part[0])
      .join('')
      .slice(0, 2)
      .toUpperCase()
</script>

<ul class="people" aria-label="On this Page now">
  {#each people as [client, present] (client)}
    {#if 'agent' in present}
      {@const label = present.member ? `${present.name} · Agent of ${present.member}` : `${present.name} · Agent`}
      <li class="agent" class:working={working.has(present.name)} title={label}>
        <Icon name="agent" />
        <span class="name">{present.name}</span>
        <span class="visually-hidden">{present.member ? `, Agent of ${present.member}` : ', Agent'}</span>
      </li>
    {:else}
      <li class="person" style:--person={present.color} title={present.name}>
        <span aria-hidden="true">{initials(present.name)}</span>
        <span class="visually-hidden">{present.name}</span>
      </li>
    {/if}
  {/each}
</ul>

<style>
  .people {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .person {
    display: grid;
    place-items: center;
    width: var(--size-avatar);
    height: var(--size-avatar);
    border-radius: var(--radius-round);
    background: var(--person);
    color: var(--color-on-color);
    font-size: var(--size-avatar-text);
    font-weight: var(--weight-strong);
    letter-spacing: var(--tracking-small);
  }

  /* People overlap a little, each set apart by a ring in the page's colour. */
  .person + .person {
    margin-left: calc(-1 * var(--space-3));
    box-shadow: 0 0 0 var(--line-thick) var(--color-bg);
  }

  .agent {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: var(--size-avatar);
    padding: 0 var(--space-2) 0 var(--space-1);
    border: var(--line) solid var(--color-line-strong);
    border-radius: var(--radius-s);
    color: var(--color-muted);
    font-size: var(--text-s);
    transition:
      color var(--dur-slow) var(--ease-out),
      border-color var(--dur-slow) var(--ease-out),
      background var(--dur-slow) var(--ease-out);
  }

  .agent.working {
    border-color: var(--color-agent);
    background: var(--color-agent-soft);
    color: var(--color-agent);
  }

  .name {
    white-space: nowrap;
  }

  @media (max-width: 640px) {
    .name {
      display: none;
    }
  }
</style>
