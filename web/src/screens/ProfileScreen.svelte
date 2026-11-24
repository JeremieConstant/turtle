<script lang="ts">
  /*
   * The Profile of a Member or Guest: their mark, name and email, and whether
   * they are a Guest or a Workspace Admin. Someone the person may not see
   * looks like someone who does not exist.
   */
  import { untrack } from 'svelte'
  import Avatar from '../components/Avatar.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Profile } from '../lib/operations.gen'
  import { standing } from '../lib/profile'
  import { router } from '../lib/router.svelte'
  import { profileAddress } from '../lib/routes'
  import NotFoundScreen from './NotFoundScreen.svelte'

  // The screen is created anew for every Profile, so the ID never changes underneath it.
  const props: { id: string } = $props()
  const id = untrack(() => props.id)

  let profile = $state.raw<Profile | null>(null)
  let missing = $state(false)
  let loadFailed = $state(false)

  call('get_profile', { id }).then(
    found => (profile = found),
    caught => {
      if (caught instanceof OperationError && caught.kind === 'not_found') missing = true
      else loadFailed = true
    },
  )

  const role = $derived(profile && standing(profile))

  $effect(() => {
    if (!profile) return
    document.title = `${profile.name} · Turtle`
    // An address with an outdated or missing name is corrected in place (ADR-0007).
    const address = profileAddress(profile.id, profile.name)
    if (location.pathname !== address) router.replace(address)
  })
</script>

{#if missing}
  <NotFoundScreen />
{:else}
  <main>
    {#if loadFailed}
      <p class="sub">This Profile could not be loaded. Reload the page to try again.</p>
    {:else if profile}
      <div class="head">
        <Avatar name={profile.name} size={72} />
        <div>
          <h1>{profile.name}</h1>
          {#if role || !profile.active}
            <p class="badges">
              {#if role}<span class="badge">{role}</span>{/if}
              {#if !profile.active}<span class="badge">Deactivated</span>{/if}
            </p>
          {/if}
        </div>
      </div>
      <dl>
        <dt>Email</dt>
        <dd><a href="mailto:{profile.email}">{profile.email}</a></dd>
      </dl>
      {#if !profile.active}
        <p class="sub">{profile.name} has been deactivated and can no longer sign in.</p>
      {/if}
    {/if}
  </main>
{/if}

<style>
  main {
    max-width: var(--width-editor);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin-bottom: var(--space-6);
  }

  h1 {
    margin: 0;
    line-height: var(--leading-tight);
    overflow-wrap: anywhere;
  }

  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: var(--space-2) 0 0;
  }

  .badge {
    padding: var(--space-half) var(--space-2);
    border: var(--space-hair) solid var(--color-line);
    border-radius: var(--radius-round);
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--space-2) var(--space-4);
    margin: 0;
  }

  dt {
    color: var(--color-muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .sub {
    color: var(--color-muted);
  }
</style>
