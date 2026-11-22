<script lang="ts">
  import { call, OperationError } from '../lib/ops'
  import type { MemberEntry, SingleUseLink } from '../lib/operations.gen'
  import { session } from '../lib/session.svelte'

  let members = $state.raw<MemberEntry[] | null>(null)
  let loadError = $state<string | null>(null)
  let email = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)
  /** The last link created, with what it is for, to pass on. */
  let shown = $state.raw<{ purpose: string; link: SingleUseLink } | null>(null)
  let copied = $state(false)

  call('list_members', {})
    .then(list => (members = list.members))
    .catch(caught => {
      loadError =
        caught instanceof OperationError && caught.kind === 'not_permitted'
          ? 'Only a Workspace Admin can manage Members.'
          : 'The Members could not be loaded. Reload the page to try again.'
    })

  $effect(() => {
    document.title = 'Members · Turtle'
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

  function show(purpose: string, link: SingleUseLink) {
    shown = { purpose, link }
    copied = false
  }

  async function invite(event: SubmitEvent) {
    event.preventDefault()
    await attempt(async () => {
      const link = await call('invite_member', { email })
      show(`Invitation for ${email.trim()}`, link)
      email = ''
    })
  }

  /** Puts a changed Member into the list; a change to oneself also changes who is signed in. */
  async function changed(entry: MemberEntry) {
    members = members?.map(known => (known.id === entry.id ? entry : known)) ?? null
    if (entry.id === session.member?.id) await session.load()
  }

  function setAdmin(member: MemberEntry, workspace_admin: boolean) {
    return attempt(async () => changed(await call('set_workspace_admin', { id: member.id, workspace_admin })))
  }

  function deactivate(member: MemberEntry) {
    if (!confirm(`Deactivate ${member.name}? They are signed out at once and can no longer sign in.`)) return
    return attempt(async () => changed(await call('deactivate_member', { id: member.id })))
  }

  function resetPassword(member: MemberEntry) {
    return attempt(async () => show(`Password reset for ${member.name}`, await call('create_password_reset', { id: member.id })))
  }

  const address = $derived(shown ? location.origin + shown.link.address : '')
  const expires = $derived(shown ? new Date(shown.link.expires_at * 1000).toLocaleString() : '')

  async function copy() {
    await navigator.clipboard.writeText(address)
    copied = true
  }
</script>

<main>
  <h1>Members</h1>
  {#if loadError}
    <p class="muted">{loadError}</p>
  {:else if members !== null}
    <form onsubmit={invite}>
      <h2>Invite a person</h2>
      <p class="muted">You get a link to pass on. It works once and expires after seven days.</p>
      <div class="row">
        <input type="email" aria-label="Email address" placeholder="Email address" required autocomplete="off" bind:value={email} />
        <button type="submit" disabled={busy}>Invite</button>
      </div>
    </form>

    {#if shown}
      <section class="link" aria-live="polite">
        <strong>{shown.purpose}</strong>
        <div class="row">
          <input readonly aria-label="Link to pass on" value={address} onfocus={event => event.currentTarget.select()} />
          <button type="button" onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
        </div>
        <small class="muted">Works once, until {expires}.</small>
      </section>
    {/if}

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    <ul>
      {#each members as member (member.id)}
        <li class:inactive={!member.active}>
          <div>
            <span>{member.name}</span>
            <small class="muted">{[member.email, member.workspace_admin && 'Workspace Admin', !member.active && 'Deactivated'].filter(Boolean).join(' · ')}</small>
          </div>
          {#if member.active}
            <div class="actions">
              {#if member.workspace_admin}
                <button type="button" disabled={busy} onclick={() => setAdmin(member, false)}>Remove Workspace Admin</button>
              {:else}
                <button type="button" disabled={busy} onclick={() => setAdmin(member, true)}>Make Workspace Admin</button>
              {/if}
              <button type="button" disabled={busy} onclick={() => resetPassword(member)}>Password reset link</button>
              <button type="button" disabled={busy} onclick={() => deactivate(member)}>Deactivate</button>
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

  h1,
  h2 {
    line-height: var(--leading-tight);
  }

  h2 {
    margin-bottom: 0;
    font-size: var(--text-base);
  }

  .row {
    display: flex;
    gap: var(--space-2);
  }

  input {
    flex: 1;
    min-width: 0;
  }

  .link {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-4);
    padding: var(--space-3);
    border: var(--line) solid var(--color-focus);
    border-radius: var(--radius-m);
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

  .inactive span {
    color: var(--color-muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
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
