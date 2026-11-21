<script lang="ts">
  /*
   * Who else is on the Page. People are round, with their initials in their
   * colour. An Agent is square, carries the Agent mark, and says "Agent" in
   * its name, so it never reads as a person.
   */
  import type { Editing } from '../lib/mock/live'
  import Icon from './Icon.svelte'

  let { editing, agentActive }: { editing: Editing[]; agentActive: boolean } = $props()

  const initials = (name: string) =>
    name
      .split(' ')
      .map((p) => p[0])
      .join('')
      .slice(0, 2)
</script>

<ul class="presence" aria-label="On this Page now">
  {#each editing as e (e.author.id)}
    {#if e.author.kind === 'member'}
      <li class="person" style:--person={e.color} title="{e.author.name} is editing">
        <span aria-hidden="true">{initials(e.author.name)}</span>
        <span class="visually-hidden">{e.author.name}, editing</span>
      </li>
    {:else}
      <li class="agent" class:active={agentActive} title="{e.author.name} · Agent">
        <Icon name="agent" size={14} />
        <span class="name">{e.author.name}</span>
      </li>
    {/if}
  {/each}
</ul>

<style>
  .presence {
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
    width: 26px;
    height: 26px;
    border-radius: var(--radius-round);
    background: var(--person);
    color: var(--color-on-color);
    font-size: 11px;
    font-weight: var(--weight-strong);
    letter-spacing: var(--tracking-small);
  }

  .person + .person {
    margin-left: calc(-1 * var(--space-3));
    box-shadow: 0 0 0 2px var(--color-bg);
  }

  .agent {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: 26px;
    padding: 0 var(--space-2) 0 6px;
    border: var(--line) solid var(--color-line-strong);
    border-radius: var(--radius-s);
    color: var(--color-muted);
    font-size: var(--text-s);
    transition:
      color var(--dur-slow) var(--ease-out),
      border-color var(--dur-slow) var(--ease-out),
      background var(--dur-slow) var(--ease-out);
  }

  .agent.active {
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
