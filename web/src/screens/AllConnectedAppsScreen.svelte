<script lang="ts">
  /*
   * Every Member's connected apps and Tokens, for a Workspace Admin: whose
   * each is, what it may do and when it was last used, and a way to
   * disconnect any of them, say when a laptop is lost or someone leaves.
   */
  import { router } from '../lib/router.svelte'
  import { access, byMember } from '../lib/connectedApps'
  import { ask } from '../lib/confirm.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { MembersToken } from '../lib/operations.gen'
  import { exactTime, relativeTime } from '../lib/time'

  let entries = $state.raw<MembersToken[] | null>(null)
  let loadError = $state<string | null>(null)
  let busy = $state(false)
  let error = $state<string | null>(null)

  call('list_members_tokens', {})
    .then(list => (entries = list.tokens))
    .catch(caught => {
      loadError =
        caught instanceof OperationError && caught.kind === 'not_permitted'
          ? 'Only a Workspace Admin can see the connected apps of every Member.'
          : 'The connected apps could not be loaded. Reload the page to try again.'
    })

  $effect(() => {
    document.title = 'All connected apps · Turtle'
  })

  const now = () => Math.floor(Date.now() / 1000)
  const members = $derived(entries ? byMember(entries) : [])

  async function disconnect({ member, token }: MembersToken) {
    const sure = await ask({
      title: `Disconnect ${token.name} of ${member.name}?`,
      message: `It stops working at once. ${member.name} can connect it again.`,
      action: 'Disconnect',
      destructive: true,
    })
    if (!sure) return
    busy = true
    error = null
    try {
      await call('revoke_token', { id: token.id, member_id: member.id })
      entries = entries?.filter(known => known.token.id !== token.id) ?? null
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }
</script>

<main class="screen">
  <h1>All connected apps</h1>
  <p class="lede">
    Everything that can act as a Member: apps they connected and Tokens they created. Disconnecting one stops it at once. To end
    everything of someone who leaves, deactivate them under
    <a href="/settings/members" onclick={event => router.follow(event, '/settings/members')}>Members</a>.
  </p>

  {#if loadError}
    <p class="muted section">{loadError}</p>
  {:else if entries !== null}
    {#if error}
      <p class="error section" role="alert">{error}</p>
    {/if}

    {#if members.length === 0}
      <p class="muted section">No Member has connected anything yet.</p>
    {/if}

    {#each members as { member, apps } (member.id)}
      <section class="section">
        <h2 class="section-title">{member.name}</h2>
        <ul class="list">
          {#each apps as app (app.id)}
            <li>
              <div class="item">
                <span class="item-name">{app.name}</span>
                <small class="hint">{access(app, now(), 'All their Areas')}</small>
                <small class="hint">
                  {#if app.last_used_at}
                    Last used <time title={exactTime(app.last_used_at)}>{relativeTime(app.last_used_at, now())}</time>
                  {:else}
                    Never used
                  {/if}
                  · {app.connected ? 'Connected' : 'Token created'}
                  <time title={exactTime(app.created_at)}>{relativeTime(app.created_at, now())}</time>
                </small>
              </div>
              <button
                type="button"
                class="btn"
                disabled={busy}
                aria-label={`Disconnect ${app.name} of ${member.name}`}
                onclick={() => disconnect({ member, token: app })}>Disconnect</button
              >
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  {/if}
</main>
