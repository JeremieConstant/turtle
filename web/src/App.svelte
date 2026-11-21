<script lang="ts">
  import { router } from './lib/router.svelte'
  import type { Route } from './lib/routes'
  import { session } from './lib/session.svelte'
  import AreaScreen from './screens/AreaScreen.svelte'
  import AreasScreen from './screens/AreasScreen.svelte'
  import NotFoundScreen from './screens/NotFoundScreen.svelte'
  import PageScreen from './screens/PageScreen.svelte'
  import SetupScreen from './screens/SetupScreen.svelte'
  import SignInScreen from './screens/SignInScreen.svelte'

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
