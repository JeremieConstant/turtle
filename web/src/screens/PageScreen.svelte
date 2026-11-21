<script lang="ts">
  import { untrack } from 'svelte'
  import AppHeader from '../components/AppHeader.svelte'
  import PageView from '../components/PageView.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Page } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import NotFoundScreen from './NotFoundScreen.svelte'

  // The screen is created anew for every Page, so the ID never changes underneath it.
  const props: { id: string } = $props()
  const id = untrack(() => props.id)

  let page = $state.raw<Page | null>(null)
  /** Set when there is no such Page or the caller may not read it; the two look the same. */
  let missing = $state(false)
  let loadFailed = $state(false)

  call('get_page', { id })
    .then(found => (page = found))
    .catch(caught => {
      if (caught instanceof OperationError && caught.kind === 'not_found') missing = true
      else loadFailed = true
    })

  $effect(() => {
    if (page) document.title = `${page.title} · Turtle`
  })

  $effect(() => {
    if (!page) return
    // An address with an outdated or missing title is corrected in place (ADR-0007).
    const address = pageAddress(page.id, page.title)
    if (location.pathname !== address) router.replace(address)
  })
</script>

{#if missing}
  <NotFoundScreen />
{:else if loadFailed}
  <AppHeader />
  <main class="message">This Page could not be loaded. Reload the page to try again.</main>
{:else if page}
  <PageView {page} />
{/if}

<style>
  .message {
    max-width: var(--width-form);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
    color: var(--color-muted);
  }
</style>
