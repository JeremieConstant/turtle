<script lang="ts">
  import { untrack } from 'svelte'
  import Logo from '../components/Logo.svelte'
  import { connectionRequest } from '../lib/connect'
  import type { ConnectionRequest } from '../lib/operations.gen'
  import { call, OperationError } from '../lib/ops'
  import { session } from '../lib/session.svelte'

  // The screen is created anew for every request, so the query never changes underneath it.
  /** The app's OAuth request, as the authorization endpoint passed it on. */
  const props: { query: string } = $props()
  const request = connectionRequest(untrack(() => props.query))

  let asking = $state<ConnectionRequest | null>(null)
  let error = $state<string | null>(request ? null : 'This connection request is not valid. Start connecting again from the app.')
  let busy = $state(false)

  $effect(() => {
    document.title = 'Connect an app · Turtle'
  })

  if (request) {
    call('describe_connection_request', { client_id: request.client_id, redirect_uri: request.redirect_uri })
      .then(found => (asking = found))
      .catch(caught => (error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'))
  }

  /** Sends the browser back to the app with the answer, which ends this page. */
  async function answer(allow: boolean) {
    if (!request) return
    busy = true
    try {
      const { redirect_to } = await call('answer_connection_request', { ...request, allow })
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
          It can read and change everything you can, and what it changes shows as yours through {asking.app_name}.
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
      <div class="answer">
        <button type="button" class="btn" disabled={busy} onclick={() => answer(false)}>Deny</button>
        <button type="button" class="btn primary" disabled={busy} onclick={() => answer(true)}>Allow</button>
      </div>
    {/if}
  </div>
</main>

<style>
  .domain {
    word-break: break-all;
  }

  .answer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
