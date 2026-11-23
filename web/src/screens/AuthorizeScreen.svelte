<script lang="ts">
  import { untrack } from 'svelte'
  import Logo from '../components/Logo.svelte'
  import { connectionRequest } from '../lib/connect'
  import type { Area, ConnectionRequest } from '../lib/operations.gen'
  import { call, OperationError } from '../lib/ops'
  import { session } from '../lib/session.svelte'

  // The screen is created anew for every request, so the query never changes underneath it.
  /** The app's OAuth request, as the authorization endpoint passed it on. */
  const props: { query: string } = $props()
  const request = connectionRequest(untrack(() => props.query))

  let asking = $state<ConnectionRequest | null>(null)
  let areas = $state.raw<Area[]>([])
  let error = $state<string | null>(request ? null : 'This connection request is not valid. Start connecting again from the app.')
  let busy = $state(false)

  // Without opening "Limit access", the app gets what it asked for.
  let onlyReading = $state(false)
  let limitAreas = $state(false)
  let chosenAreas = $state<string[]>([])

  /** Whether the app will be able to change things, as the Member has limited it. */
  const changes = $derived(asking?.asks_to_change === true && !onlyReading)
  /** The limits chosen, in a few words, so they show while "Limit access" is closed. */
  const limits = $derived(
    [
      asking?.asks_to_change && onlyReading ? 'only reading' : null,
      limitAreas ? `${chosenAreas.length} ${chosenAreas.length === 1 ? 'Area' : 'Areas'}` : null,
    ].filter(Boolean),
  )

  $effect(() => {
    document.title = 'Connect an app · Turtle'
  })

  if (request) {
    Promise.all([
      call('describe_connection_request', { client_id: request.client_id, redirect_uri: request.redirect_uri, scope: request.scope }),
      call('list_areas', {}),
    ])
      .then(([found, areaList]) => {
        asking = found
        areas = areaList.areas
      })
      .catch(caught => (error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'))
  }

  /** Sends the browser back to the app with the answer, which ends this page. */
  async function answer(allow: boolean) {
    if (!request) return
    busy = true
    try {
      const { redirect_to } = await call('answer_connection_request', {
        ...request,
        only_reading: onlyReading,
        area_ids: limitAreas ? chosenAreas : null,
        allow,
      })
      location.assign(redirect_to)
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
      busy = false
    }
  }
</script>

<main class="standalone">
  <div class="standalone-card">
    <div>
      <strong class="wordmark"><Logo size="var(--size-mark)" />Turtle</strong>
      {#if asking}
        <h1>{asking.app_name} wants to act as {session.member?.name} in Turtle</h1>
        <p class="muted">
          {#if changes}
            It can read and change everything you can{limitAreas ? ' in the Areas you chose' : ''}, and what it changes shows
            as yours through {asking.app_name}.
          {:else}
            It can read everything you can{limitAreas ? ' in the Areas you chose' : ''}, and change nothing.
          {/if}
        </p>
      {:else}
        <h1>Connect an app</h1>
      {/if}
    </div>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    {#if asking}
      <div class="notice">
        <p>Your answer goes to <strong class="domain">{asking.redirect_domain}</strong>. Allow only if that is the app you are connecting.</p>
      </div>
      <details class="limit">
        <summary>
          Limit access
          {#if limits.length > 0}<small class="hint">· {limits.join(', ')}</small>{/if}
        </summary>
        <div class="stack">
          {#if asking.asks_to_change}
            <label class="check">
              <input type="checkbox" bind:checked={onlyReading} />
              Only reading: it can read but change nothing
            </label>
          {:else}
            <p class="hint">It asks only to read, so it can change nothing.</p>
          {/if}
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
        </div>
      </details>
      <div class="answer">
        <button type="button" class="btn" disabled={busy} onclick={() => answer(false)}>Deny</button>
        <button
          type="button"
          class="btn primary"
          disabled={busy || (limitAreas && chosenAreas.length === 0)}
          onclick={() => answer(true)}>Allow</button
        >
      </div>
    {/if}
  </div>
</main>

<style>
  .domain {
    word-break: break-all;
  }

  .limit > summary {
    cursor: pointer;
    font-size: var(--text-m);
    font-weight: var(--weight-medium);
  }

  .limit[open] > summary {
    margin-bottom: var(--space-3);
  }

  .answer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
