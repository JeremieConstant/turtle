<script lang="ts">
  import { router } from './lib/router.svelte'
  import type { Route } from './lib/routes'
  import { session } from './lib/session.svelte'
  import AreaScreen from './screens/AreaScreen.svelte'
  import AreasScreen from './screens/AreasScreen.svelte'
  import InvitationScreen from './screens/InvitationScreen.svelte'
  import MembersScreen from './screens/MembersScreen.svelte'
  import NotFoundScreen from './screens/NotFoundScreen.svelte'
  import PageScreen from './screens/PageScreen.svelte'
  import PasswordResetScreen from './screens/PasswordResetScreen.svelte'
  import SetupScreen from './screens/SetupScreen.svelte'
  import SignInScreen from './screens/SignInScreen.svelte'
  import TokensScreen from './screens/TokensScreen.svelte'

  const route = $derived(router.route)

  let failed = $state(false)
  session.load().catch(() => (failed = true))

  /** Where a person who had to sign in first was going. */
  let returnTo = $state('/')

  /** The address someone must be sent to instead of the one they asked for, if any. */
  function redirect(route: Route): string | null {
    if (session.setupRequired) return route.screen === 'setup' ? null : '/setup'
    // Once an account exists, setup is never reachable again.
    if (route.screen === 'setup') return session.member ? '/' : '/sign-in'
    // A link someone was given works whether or not anyone is signed in here.
    if (route.screen === 'invitation' || route.screen === 'password-reset') return null
    if (!session.member) return route.screen === 'sign-in' ? null : '/sign-in'
    if (route.screen === 'sign-in') return '/'
    return null
  }

  const target = $derived(session.loaded ? redirect(route) : null)

  $effect(() => {
    if (target === null) return
    if (target === '/sign-in' && route.screen !== 'setup') returnTo = location.pathname
    router.replace(target)
  })
</script>

{#if failed}
  <main class="message">Turtle cannot be reached. Reload the page to try again.</main>
{:else if !session.loaded || target !== null}
  <!-- Nothing is shown until it is clear who is here and where they may go. -->
{:else if route.screen === 'setup'}
  <SetupScreen />
{:else if route.screen === 'sign-in'}
  <SignInScreen {returnTo} />
{:else if route.screen === 'home'}
  <AreasScreen />
{:else if route.screen === 'area'}
  {#key route.id}
    <AreaScreen id={route.id} />
  {/key}
{:else if route.screen === 'page'}
  {#key route.id}
    <PageScreen id={route.id} />
  {/key}
{:else if route.screen === 'members'}
  <MembersScreen />
{:else if route.screen === 'tokens'}
  <TokensScreen />
{:else if route.screen === 'invitation'}
  {#key route.secret}
    <InvitationScreen secret={route.secret} />
  {/key}
{:else if route.screen === 'password-reset'}
  {#key route.secret}
    <PasswordResetScreen secret={route.secret} />
  {/key}
{:else if route.screen === 'not-found'}
  <NotFoundScreen />
{/if}

<style>
  .message {
    max-width: var(--width-form);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
    color: var(--color-muted);
  }
</style>
