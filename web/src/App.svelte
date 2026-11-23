<script lang="ts">
  import { tick } from 'svelte'
  import { Toaster } from 'svelte-sonner'
  import ConfirmDialog from './components/ConfirmDialog.svelte'
  import Icon from './components/Icon.svelte'
  import Sidebar from './components/Sidebar.svelte'
  import { appearance } from './lib/appearance.svelte'
  import { router } from './lib/router.svelte'
  import { inShell, type Route } from './lib/routes'
  import { asksForSearch } from './lib/search'
  import { session } from './lib/session.svelte'
  import { ui } from './lib/ui.svelte'
  import AgentsScreen from './screens/AgentsScreen.svelte'
  import ConnectedAppsScreen from './screens/ConnectedAppsScreen.svelte'
  import AreaScreen from './screens/AreaScreen.svelte'
  import AuthorizeScreen from './screens/AuthorizeScreen.svelte'
  import ConnectAiScreen from './screens/ConnectAiScreen.svelte'
  import DeletedAreasScreen from './screens/DeletedAreasScreen.svelte'
  import InvitationScreen from './screens/InvitationScreen.svelte'
  import MembersScreen from './screens/MembersScreen.svelte'
  import NotFoundScreen from './screens/NotFoundScreen.svelte'
  import PageScreen from './screens/PageScreen.svelte'
  import PasswordResetScreen from './screens/PasswordResetScreen.svelte'
  import SearchScreen from './screens/SearchScreen.svelte'
  import SetupScreen from './screens/SetupScreen.svelte'
  import SignInScreen from './screens/SignInScreen.svelte'
  import StartScreen from './screens/StartScreen.svelte'
  import TrashScreen from './screens/TrashScreen.svelte'

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
    if (target === '/sign-in' && route.screen !== 'setup') returnTo = location.pathname + location.search
    router.replace(target)
  })

  let content = $state<HTMLElement | null>(null)
  let side = $state<HTMLElement | null>(null)
  let opener = $state<HTMLElement | null>(null)

  /** What is on the screen. A corrected address (ADR-0007) changes the path but not this. */
  const shown = $derived('id' in route ? `${route.screen} ${route.id}` : route.screen)

  // Another screen starts at its top, and the drawer has done its job.
  $effect(() => {
    void shown
    content?.scrollTo({ top: 0 })
    ui.drawerOpen = false
  })

  async function openDrawer() {
    ui.drawerOpen = true
    await tick()
    side?.querySelector<HTMLElement>('a, button, input')?.focus()
  }

  function closeDrawer() {
    if (!ui.drawerOpen) return
    ui.drawerOpen = false
    opener?.focus()
  }

  /** Moves the focus to the search of the screen, or else to the one in the sidebar. */
  async function focusSearch() {
    const field =
      content?.querySelector<HTMLInputElement>('input[type=search]') ??
      side?.querySelector<HTMLInputElement>('input[type=search]')
    if (!field) return
    // On a narrow window the sidebar's search is in the drawer, which has to be out to take the focus.
    if (side?.contains(field) && !field.checkVisibility({ visibilityProperty: true })) {
      ui.drawerOpen = true
      await tick()
    }
    field.focus()
  }

  function onkeydown(event: KeyboardEvent) {
    // A checkbox or a radio button is an input that nothing is typed into.
    const typing =
      event.target instanceof Element &&
      event.target.closest(
        'input:not([type=checkbox], [type=radio]), textarea, select, [contenteditable]:not([contenteditable="false"])',
      ) !== null
    if (asksForSearch(event, typing) && inShell(route)) {
      // Otherwise the slash would land in the field it brings the focus to.
      event.preventDefault()
      void focusSearch()
      return
    }
    // Escape closes a menu or dialog that is open in the drawer before it closes the drawer.
    if (event.key === 'Escape' && !document.querySelector(':popover-open, dialog[open]')) closeDrawer()
  }
</script>

<svelte:window {onkeydown} />

{#if failed}
  <main class="message">Turtle cannot be reached. Reload the page to try again.</main>
{:else if !session.loaded || target !== null}
  <!-- Nothing is shown until it is clear who is here and where they may go. -->
{:else if !inShell(route)}
  {#if route.screen === 'setup'}
    <SetupScreen />
  {:else if route.screen === 'sign-in'}
    <SignInScreen {returnTo} />
  {:else if route.screen === 'invitation'}
    {#key route.secret}
      <InvitationScreen secret={route.secret} />
    {/key}
  {:else if route.screen === 'password-reset'}
    {#key route.secret}
      <PasswordResetScreen secret={route.secret} />
    {/key}
  {:else if route.screen === 'authorize'}
    {#key route.query}
      <AuthorizeScreen query={route.query} />
    {/key}
  {/if}
{:else}
  <div class="app" class:drawer={ui.drawerOpen}>
    <a class="skip" href="#content">Skip to the content</a>
    <div class="bar">
      <button
        type="button"
        class="icon-btn"
        aria-label="Open the sidebar"
        aria-expanded={ui.drawerOpen}
        bind:this={opener}
        onclick={openDrawer}
      >
        <Icon name="menu" size={18} />
      </button>
    </div>
    <div class="side" bind:this={side}><Sidebar /></div>
    <button type="button" class="scrim" aria-label="Close the sidebar" tabindex="-1" onclick={closeDrawer}></button>
    <div class="content" id="content" tabindex="-1" bind:this={content}>
      {#if route.screen === 'home'}
        <StartScreen />
      {:else if route.screen === 'area'}
        {#key route.id}
          <AreaScreen id={route.id} />
        {/key}
      {:else if route.screen === 'page'}
        {#key route.id}
          <PageScreen id={route.id} />
        {/key}
      {:else if route.screen === 'trash'}
        {#key route.areaId}
          <TrashScreen areaId={route.areaId} />
        {/key}
      {:else if route.screen === 'search'}
        <SearchScreen query={route.query} />
      {:else if route.screen === 'connect-ai'}
        <ConnectAiScreen />
      {:else if route.screen === 'members'}
        <MembersScreen />
      {:else if route.screen === 'connected-apps'}
        <ConnectedAppsScreen />
      {:else if route.screen === 'agents'}
        <AgentsScreen />
      {:else if route.screen === 'deleted-areas'}
        <DeletedAreasScreen />
      {:else}
        <NotFoundScreen />
      {/if}
    </div>
  </div>
{/if}

<ConfirmDialog />

<!-- Short messages that stack in the corner and leave on their own. -->
<Toaster theme={appearance.choice} position="bottom-right" visibleToasts={4} gap={8} offset={20} duration={4000} />

<style>
  .app {
    display: grid;
    grid-template-columns: var(--width-side) 1fr;
    height: var(--height-screen);
  }

  .side {
    min-height: 0;
  }

  /* Positioned, so that anything placed absolutely inside a screen scrolls with it
     instead of stretching the window and giving it a second scrollbar. */
  .content {
    position: relative;
    min-width: 0;
    overflow-y: auto;
    scrollbar-gutter: stable;
    outline: none;
  }

  .bar,
  .scrim {
    display: none;
  }

  /* Out of sight until the keyboard reaches it. */
  .skip {
    position: fixed;
    top: var(--space-2);
    left: var(--space-2);
    z-index: var(--layer-menu);
    padding: var(--space-1) var(--space-3);
    border-radius: var(--radius-m);
    background: var(--color-text);
    color: var(--color-bg);
    font-size: var(--text-m);
    transform: translateY(calc(-1 * var(--space-16)));
  }

  .skip:focus {
    transform: none;
  }

  /* On a narrow window the sidebar becomes a drawer over the screen. */
  @media (max-width: 800px) {
    .app {
      grid-template-columns: 1fr;
      grid-template-rows: var(--height-bar) 1fr;
    }

    .bar {
      display: flex;
      align-items: center;
      padding: 0 var(--space-3);
      border-bottom: var(--line) solid var(--color-line);
    }

    .bar .icon-btn {
      width: var(--space-8);
      height: var(--space-8);
      color: var(--color-text);
    }

    /* Hidden, not only moved away, so the keyboard does not land in it while it is closed. */
    .side {
      position: fixed;
      inset: 0 auto 0 0;
      z-index: var(--layer-drawer);
      width: min(var(--width-side), var(--width-drawer-max));
      visibility: hidden;
      transform: translateX(-100%);
      transition:
        transform var(--dur-slow) var(--ease-out),
        visibility var(--dur-slow);
    }

    .drawer .side {
      visibility: visible;
      transform: none;
      box-shadow: var(--shadow-float);
      /* Visible at once, so the focus can move into it as it opens. */
      transition: transform var(--dur-slow) var(--ease-out);
    }

    .scrim {
      position: fixed;
      inset: 0;
      z-index: calc(var(--layer-drawer) - 1);
      display: block;
      padding: 0;
      border: 0;
      border-radius: 0;
      background: var(--color-scrim);
      opacity: 0;
      pointer-events: none;
      transition: opacity var(--dur-slow) var(--ease-out);
    }

    .drawer .scrim {
      opacity: 1;
      pointer-events: auto;
    }
  }

  .message {
    max-width: var(--width-form);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
    color: var(--color-muted);
  }
</style>
