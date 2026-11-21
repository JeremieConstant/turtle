<script lang="ts">
  import AppHeader from '../components/AppHeader.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Area, CreatedToken, TokenSummary } from '../lib/operations.gen'

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

  Promise.all([call('list_tokens', {}), call('list_areas', {})])
    .then(([tokenList, areaList]) => {
      tokens = tokenList.tokens
      areas = areaList.areas
    })
    .catch(() => (loadFailed = true))

  $effect(() => {
    document.title = 'Tokens · Turtle'
  })

  const mcpAddress = `${location.origin}/mcp`
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

  function revoke(token: TokenSummary) {
    if (!confirm(`Revoke ${token.name}? Agents using it lose access at once.`)) return
    return attempt(async () => {
      await call('revoke_token', { id: token.id })
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
    const reach = token.areas ? `Only ${token.areas.map(area => area.name).join(', ') || 'Areas you can no longer read'}` : 'All your Areas'
    const parts = [token.read_only ? 'Read-only' : 'Read and write', reach]
    if (token.expires_at) {
      parts.push(token.expires_at * 1000 < Date.now() ? `Expired ${when(token.expires_at)}` : `Until ${when(token.expires_at)}`)
    }
    return parts.join(' · ')
  }
</script>

<AppHeader />

<main>
  <h1>Tokens</h1>
  <p class="muted">
    A Token lets an Agent act as you, through MCP at <code>{mcpAddress}</code> or the HTTP API. It never has more access than you,
    and it stops working when you revoke it.
  </p>

  {#if loadFailed}
    <p class="muted">Your Tokens could not be loaded. Reload the page to try again.</p>
  {:else if tokens !== null}
    <form onsubmit={create}>
      <h2>New Token</h2>
      <label class="field">
        <span>Name</span>
        <input required autocomplete="off" placeholder="What the Agent is, e.g. Claude on my laptop" bind:value={name} />
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
        <fieldset>
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
        <input type="date" min={today} bind:value={lastDay} />
        <small class="muted">Leave empty for a Token that does not expire.</small>
      </label>
      <button type="submit" disabled={busy || (limitAreas && chosenAreas.length === 0)}>Create Token</button>
    </form>

    {#if created}
      <section class="created" aria-live="polite">
        <strong>{created.token.name}</strong>
        <p>Copy this Token now. Turtle shows it only once.</p>
        <div class="row">
          <input readonly aria-label="The new Token" value={created.value} onfocus={event => event.currentTarget.select()} />
          <button type="button" onclick={() => copy(created!.value)}>{copied ? 'Copied' : 'Copy'}</button>
        </div>
        <small class="muted">The Agent sends it as <code>Authorization: Bearer …</code></small>
      </section>
    {/if}

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    {#if tokens.length === 0}
      <p class="muted">You have no Tokens yet.</p>
    {:else}
      <ul>
        {#each tokens as token (token.id)}
          <li>
            <div>
              <span>{token.name}</span>
              <small class="muted">{limits(token)}</small>
              <small class="muted">
                {token.last_used_at ? `Last used ${when(token.last_used_at)}` : 'Never used'} · Created {when(token.created_at)}
              </small>
            </div>
            <button type="button" disabled={busy} onclick={() => revoke(token)}>Revoke</button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</main>

<style>
  main {
    max-width: var(--width-form);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
  }

  h1,
  h2 {
    line-height: var(--leading-tight);
  }

  h2 {
    margin-bottom: 0;
    font-size: var(--text-base);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .field span,
  legend {
    font-size: var(--text-m);
    font-weight: var(--weight-strong);
  }

  .check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  fieldset {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border: var(--line-width) solid var(--color-line);
    border-radius: var(--radius-m);
  }

  form button {
    align-self: flex-start;
  }

  .row {
    display: flex;
    gap: var(--space-2);
  }

  .row input {
    flex: 1;
    min-width: 0;
  }

  .created {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-4);
    padding: var(--space-3);
    border: var(--line-width) solid var(--color-accent);
    border-radius: var(--radius-m);
  }

  .created p {
    margin: 0;
  }

  code {
    overflow-wrap: anywhere;
  }

  ul {
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
    border-bottom: var(--line-width) solid var(--color-line);
  }

  li > div {
    display: flex;
    flex-direction: column;
    min-width: 0;
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
