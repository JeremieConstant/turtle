<script lang="ts">
  import TokenManager from '../components/TokenManager.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { AgentEntry } from '../lib/operations.gen'

  let agents = $state.raw<AgentEntry[] | null>(null)
  let loadError = $state<string | null>(null)
  let name = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)
  /** The Agent whose Tokens are open, if any. */
  let openTokens = $state<string | null>(null)
  /** The Agent being renamed, with the name typed so far. */
  let renaming = $state<{ id: string; name: string } | null>(null)

  call('list_agents', {})
    .then(list => (agents = list.agents))
    .catch(caught => {
      loadError =
        caught instanceof OperationError && caught.kind === 'not_permitted'
          ? 'Only a Workspace Admin can manage Agents.'
          : 'The Agents could not be loaded. Reload the page to try again.'
    })

  $effect(() => {
    document.title = 'Agents · Turtle'
  })

  /** Runs an operation, showing what went wrong if it fails. */
  async function attempt(run: () => Promise<void>) {
    busy = true
    error = null
    try {
      await run()
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }

  /** Puts a new or changed Agent into the list, kept in order of name. */
  function changed(entry: AgentEntry) {
    const others = (agents ?? []).filter(known => known.id !== entry.id)
    agents = [...others, entry].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))
  }

  function create(event: SubmitEvent) {
    event.preventDefault()
    return attempt(async () => {
      const agent = await call('create_agent', { name })
      changed(agent)
      name = ''
      openTokens = agent.id
    })
  }

  function rename(event: SubmitEvent) {
    event.preventDefault()
    const target = renaming
    if (!target) return
    return attempt(async () => {
      changed(await call('rename_agent', target))
      renaming = null
    })
  }

  function deactivate(agent: AgentEntry) {
    if (!confirm(`Deactivate ${agent.name}? Its Tokens stop working at once.`)) return
    return attempt(async () => {
      changed(await call('deactivate_agent', { id: agent.id }))
      if (openTokens === agent.id) openTokens = null
    })
  }
</script>

<main>
  <h1>Agents</h1>
  <p class="muted">
    A standalone Agent acts with its own name and Tokens, not as any person. Admins of an Area give it a Role there like a Member;
    a Role given to all Members does not include it.
  </p>
  {#if loadError}
    <p class="muted">{loadError}</p>
  {:else if agents !== null}
    <form class="row" onsubmit={create}>
      <input aria-label="Name of the new Agent" placeholder="Name, e.g. Release notes writer" required autocomplete="off" bind:value={name} />
      <button type="submit" disabled={busy}>Create Agent</button>
    </form>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    {#if agents.length === 0}
      <p class="muted">There are no Agents yet.</p>
    {/if}
    <ul>
      {#each agents as agent (agent.id)}
        <li class:inactive={!agent.active}>
          {#if renaming?.id === agent.id}
            <form class="row" onsubmit={rename}>
              <input aria-label="New name of {agent.name}" required autocomplete="off" bind:value={renaming.name} />
              <button type="submit" disabled={busy}>Save</button>
              <button type="button" onclick={() => (renaming = null)}>Cancel</button>
            </form>
          {:else}
            <div>
              <span>{agent.name}</span>
              <small class="muted">{agent.active ? 'Agent' : 'Agent · Deactivated'}</small>
            </div>
            {#if agent.active}
              <div class="actions">
                <button type="button" aria-expanded={openTokens === agent.id} onclick={() => (openTokens = openTokens === agent.id ? null : agent.id)}>
                  Tokens
                </button>
                <button type="button" disabled={busy} onclick={() => (renaming = { id: agent.id, name: agent.name })}>Rename</button>
                <button type="button" disabled={busy} onclick={() => deactivate(agent)}>Deactivate</button>
              </div>
            {/if}
          {/if}
          {#if openTokens === agent.id && agent.active}
            <div class="tokens">
              {#key agent.id}
                <TokenManager agentId={agent.id} />
              {/key}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</main>

<style>
  main {
    max-width: var(--width-content);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
  }

  h1 {
    line-height: var(--leading-tight);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .row input {
    flex: 1;
    min-width: 0;
  }

  ul {
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    padding: var(--space-2) 0;
    border-bottom: var(--line) solid var(--color-line);
  }

  li > div:first-child {
    display: flex;
    flex-direction: column;
  }

  li > form {
    flex: 1;
  }

  .inactive span {
    color: var(--color-muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .tokens {
    flex-basis: 100%;
    padding: var(--space-3);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
  }

  .muted {
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  small.muted {
    font-size: var(--text-s);
  }

  .error {
    color: var(--color-danger);
  }
</style>
