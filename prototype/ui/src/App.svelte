<script lang="ts">
  import { Toaster } from 'svelte-sonner'
  import Icon from './components/Icon.svelte'
  import Sidebar from './components/Sidebar.svelte'
  import { get_page } from './lib/mock/ops.svelte'
  import { router } from './lib/router.svelte'
  import { ui } from './lib/ui.svelte'
  import AreaScreen from './screens/AreaScreen.svelte'
  import PageScreen from './screens/PageScreen.svelte'
  import SearchScreen from './screens/SearchScreen.svelte'
  import StartScreen from './screens/StartScreen.svelte'

  const route = $derived(router.route)
  const currentPageId = $derived(route.screen === 'page' ? route.page_id : null)

  // The navigation opens the Area of whatever is on screen.
  $effect(() => {
    if (route.screen === 'area') ui.openArea = route.area_id
    if (route.screen === 'page') {
      const area = get_page({ page_id: route.page_id })?.area.id
      if (area) ui.openArea = area
    }
  })

  $effect(() => {
    const titles = { start: 'Turtle', search: 'Search · Turtle', 'not-found': 'Not found · Turtle' } as const
    if (route.screen in titles) document.title = titles[route.screen as keyof typeof titles]
    if (route.screen === 'page') document.title = `${get_page({ page_id: route.page_id })?.title ?? 'Page'} · Turtle`
  })

  let main: HTMLElement
  $effect(() => {
    void router.pathname
    main?.scrollTo({ top: 0 })
  })
</script>

<div class="app" class:drawer={ui.drawerOpen}>
  <div class="side">
    <Sidebar {currentPageId} showSearch={route.screen !== 'start'} />
  </div>
  <button class="scrim" aria-label="Close navigation" onclick={() => (ui.drawerOpen = false)}></button>
  <button class="open-nav icon-btn" aria-label="Open navigation" aria-expanded={ui.drawerOpen} onclick={() => (ui.drawerOpen = true)}>
    <Icon name="menu" size={18} />
  </button>
  <main bind:this={main}>
    {#if route.screen === 'start'}
      <StartScreen />
    {:else if route.screen === 'area'}
      <AreaScreen area_id={route.area_id} />
    {:else if route.screen === 'page'}
      <PageScreen page_id={route.page_id} />
    {:else if route.screen === 'search'}
      <SearchScreen query={route.query} />
    {:else}
      <div class="nf"><h1>Nothing at this address</h1></div>
    {/if}
  </main>
</div>

<!-- Short confirmations that stack in the corner and leave on their own. -->
<Toaster position="bottom-right" visibleToasts={4} gap={8} offset={20} duration={4000} />

<style>
  .app {
    display: grid;
    grid-template-columns: var(--width-side) 1fr;
    height: 100dvh;
  }

  .side {
    min-height: 0;
  }

  main {
    min-width: 0;
    overflow-y: auto;
    scrollbar-gutter: stable;
  }

  .scrim,
  .open-nav {
    display: none;
  }

  .nf {
    padding: var(--space-16) var(--space-6);
    text-align: center;
  }

  /* On narrow screens the navigation becomes a drawer over the content. */
  @media (max-width: 800px) {
    .app {
      grid-template-columns: 1fr;
    }

    .side {
      position: fixed;
      inset: 0 auto 0 0;
      z-index: var(--layer-drawer);
      width: min(var(--width-side), 86vw);
      transform: translateX(-100%);
      transition: transform var(--dur-slow) var(--ease-out);
    }

    .drawer .side {
      transform: none;
      box-shadow: var(--shadow-float);
    }

    .scrim {
      position: fixed;
      inset: 0;
      z-index: calc(var(--layer-drawer) - 1);
      display: block;
      border: 0;
      background: rgb(0 0 0 / 0.2);
      opacity: 0;
      pointer-events: none;
      transition: opacity var(--dur-slow) var(--ease-out);
    }

    .drawer .scrim {
      opacity: 1;
      pointer-events: auto;
    }

    .open-nav {
      position: fixed;
      top: 10px;
      left: var(--space-3);
      z-index: 3;
      display: grid;
      width: 32px;
      height: 32px;
      color: var(--color-text);
    }
  }
</style>
