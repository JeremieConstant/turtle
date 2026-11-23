<script lang="ts">
  import { untrack } from 'svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Area, CreatedToken, TokenSummary } from '../lib/operations.gen'

  /**
   * Creates a Token by hand: the caller's own, or with `agentId` one of a
   * standalone Agent, which only a Workspace Admin manages. Its value is shown
   * once, here.
   */
  const {
    agentId,
    placeholder,
    oncreated,
  }: {
    agentId?: string
    /** An example name, to say what a Token is for. */
    placeholder: string
    oncreated: (token: TokenSummary) => void
  } = $props()
  const agent_id = untrack(() => agentId) ?? null

  let areas = $state.raw<Area[]>([])
  call('list_areas', {})
    .then(list => (areas = list.areas))
    .catch(() => (areas = []))

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

  const today = new Date().toISOString().slice(0, 10)

  async function create(event: SubmitEvent) {
    event.preventDefault()
    busy = true
    error = null
    try {
      created = await call('create_token', {
        agent_id,
        name,
        read_only: readOnly,
        area_ids: limitAreas ? chosenAreas : null,
        // The Token works until the end of its last day, in local time.
        expires_at: lastDay ? Math.floor(new Date(`${lastDay}T23:59:59`).getTime() / 1000) : null,
      })
      copied = false
      oncreated(created.token)
      name = ''
      readOnly = false
      limitAreas = false
      chosenAreas = []
      lastDay = ''
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }

  async function copy(value: string) {
    await navigator.clipboard.writeText(value)
    copied = true
  }
</script>

<form class="stack" onsubmit={create}>
  <h3 class="section-title">New Token</h3>
  <label class="field">
    <span>Name</span>
    <input class="input" required autocomplete="off" {placeholder} bind:value={name} />
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={readOnly} />
    Read-only: it can read but change nothing
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={limitAreas} disabled={areas.length === 0} />
    Only chosen Areas
  </label>
  {#if limitAreas}
    <fieldset class="choices">
      <legend>Areas it can reach</legend>
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

{#if error}
  <p class="error section" role="alert">{error}</p>
{/if}

{#if created}
  <section class="section notice" aria-live="polite">
    <strong>{created.token.name}</strong>
    <p>Copy this Token now. Turtle shows it only once.</p>
    <div class="controls">
      <input class="input grow" readonly aria-label="The new Token" value={created.value} onfocus={event => event.currentTarget.select()} />
      <button type="button" class="btn" onclick={() => copy(created!.value)}>{copied ? 'Copied' : 'Copy'}</button>
    </div>
    <small class="hint">Send it as <code>Authorization: Bearer …</code></small>
  </section>
{/if}
