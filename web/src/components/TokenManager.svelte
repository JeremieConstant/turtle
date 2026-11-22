<script lang="ts">
  import { untrack } from 'svelte'
  import { ask } from '../lib/confirm.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Area, CreatedToken, TokenSummary } from '../lib/operations.gen'

  /**
   * The caller's own Tokens, or with `agentId` those of a standalone Agent,
   * which only a Workspace Admin manages.
   */
  const { agentId }: { agentId?: string } = $props()
  const agent_id = untrack(() => agentId) ?? null

  let tokens = $state.raw<TokenSummary[] | null>(null)
  let areas = $state.raw<Area[]>([])
  let loadFailed = $state(false)

  let name = $state('')
  let readOnly = $state(false)
  let limitAreas = $state(false)
  let chosenAreas = $state<string[]>([])
  /** The last day the Token works, as YYYY-MM-DD; empty for no expiry. */
  let lastDay = $state('')

  let busy = $state(false)
  let error = $state<string | null>(null)
  /** The Token just created; its value is never shown again. */
  let created = $state.raw<CreatedToken | null>(null)
  let copied = $state(false)

  Promise.all([call('list_tokens', { agent_id }), call('list_areas', {})])
    .then(([tokenList, areaList]) => {
      tokens = tokenList.tokens
      areas = areaList.areas
    })
    .catch(() => (loadFailed = true))

  const today = new Date().toISOString().slice(0, 10)

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

  function create(event: SubmitEvent) {
    event.preventDefault()
    return attempt(async () => {
      created = await call('create_token', {
        agent_id,
        name,
        read_only: readOnly,
        area_ids: limitAreas ? chosenAreas : null,
        // The Token works until the end of its last day, in local time.
        expires_at: lastDay ? Math.floor(new Date(`${lastDay}T23:59:59`).getTime() / 1000) : null,
      })
      copied = false
      tokens = [created.token, ...(tokens ?? [])]
      name = ''
      readOnly = false
      limitAreas = false
      chosenAreas = []
      lastDay = ''
    })
  }

  async function revoke(token: TokenSummary) {
    const sure = await ask({
      title: `Revoke ${token.name}?`,
      message: 'Agents using it lose access at once.',
      action: 'Revoke',
      destructive: true,
    })
    if (!sure) return
    return attempt(async () => {
      await call('revoke_token', { id: token.id, agent_id })
      tokens = tokens?.filter(known => known.id !== token.id) ?? null
      if (created?.token.id === token.id) created = null
    })
  }

  async function copy(value: string) {
    await navigator.clipboard.writeText(value)
    copied = true
  }

  function when(seconds: number): string {
    return new Date(seconds * 1000).toLocaleString()
  }

  /** What a Token may do, in a few words. */
  function limits(token: TokenSummary): string {
    const everywhere = agent_id ? 'All Areas it has a Role in' : 'All your Areas'
    const reach = token.areas ? `Only ${token.areas.map(area => area.name).join(', ') || 'Areas you can no longer read'}` : everywhere
    const parts = [token.read_only ? 'Read-only' : 'Read and write', reach]
    if (token.expires_at) {
      parts.push(token.expires_at * 1000 < Date.now() ? `Expired ${when(token.expires_at)}` : `Until ${when(token.expires_at)}`)
    }
    return parts.join(' · ')
  }
</script>

{#if loadFailed}
  <p class="muted">The Tokens could not be loaded. Reload the page to try again.</p>
{:else if tokens !== null}
  <form class="stack" onsubmit={create}>
    <h2 class="section-title">New Token</h2>
    <label class="field">
      <span>Name</span>
      <input
        class="input"
        required
        autocomplete="off"
        placeholder={agent_id ? 'Where it is used, e.g. CI pipeline' : 'What the Agent is, e.g. Claude on my laptop'}
        bind:value={name}
      />
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={readOnly} />
      Read-only: the Agent can read but change nothing
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={limitAreas} disabled={areas.length === 0} />
      Only chosen Areas
    </label>
    {#if limitAreas}
      <fieldset class="choices">
        <legend>Areas the Agent can reach</legend>
        {#each areas as area (area.id)}
          <label class="check">
            <input type="checkbox" value={area.id} bind:group={chosenAreas} />
            {area.name}
          </label>
        {/each}
      </fieldset>
    {/if}
    <label class="field">
      <span>Last day it works</span>
      <input class="input" type="date" min={today} bind:value={lastDay} />
      <small class="hint">Leave empty for a Token that does not expire.</small>
    </label>
    <button type="submit" class="btn primary" disabled={busy || (limitAreas && chosenAreas.length === 0)}>Create Token</button>
  </form>

  {#if created}
    <section class="section notice" aria-live="polite">
      <strong>{created.token.name}</strong>
      <p>Copy this Token now. Turtle shows it only once.</p>
      <div class="controls">
        <input class="input grow" readonly aria-label="The new Token" value={created.value} onfocus={event => event.currentTarget.select()} />
        <button type="button" class="btn" onclick={() => copy(created!.value)}>{copied ? 'Copied' : 'Copy'}</button>
      </div>
      <small class="hint">The Agent sends it as <code>Authorization: Bearer …</code></small>
    </section>
  {/if}

  {#if error}
    <p class="error section" role="alert">{error}</p>
  {/if}

  {#if tokens.length === 0}
    <p class="muted section">{agent_id ? 'This Agent has' : 'You have'} no Tokens yet.</p>
  {:else}
    <ul class="list section">
      {#each tokens as token (token.id)}
        <li>
          <div class="item">
            <span class="item-name">{token.name}</span>
            <small class="hint">{limits(token)}</small>
            <small class="hint">
              {token.last_used_at ? `Last used ${when(token.last_used_at)}` : 'Never used'} · Created {when(token.created_at)}
            </small>
          </div>
          <button type="button" class="btn" disabled={busy} onclick={() => revoke(token)}>Revoke</button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}
