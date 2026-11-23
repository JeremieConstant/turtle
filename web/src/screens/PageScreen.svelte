<script lang="ts">
  import { untrack } from 'svelte'
  import PageView from '../components/PageView.svelte'
  import { navigation } from '../lib/navigation.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Page } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { session } from '../lib/session.svelte'
  import { above, findEntry } from '../lib/tree'
  import { ui } from '../lib/ui.svelte'
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

  /**
   * When the last check that could not reach Turtle began; the editor retries
   * every second. A check that got an answer does not hold back the next one,
   * so a Role that changes twice in quick succession still reaches the editor.
   */
  let lastCheck = 0
  const CHECK_INTERVAL = 5000

  /**
   * The editor lost its connection. The server also closes it when the
   * session ends or the Page can no longer be read, and then refuses every
   * retry, so find out which: signed out goes to sign-in (App sees it), a Page
   * gone shows as not found, and a changed Role reaches the editor.
   */
  async function checkAccess() {
    if (Date.now() - lastCheck < CHECK_INTERVAL) return
    lastCheck = Date.now()
    try {
      await session.load()
      if (!session.member) return
      page = await call('get_page', { id })
      lastCheck = 0
    } catch (caught) {
      if (caught instanceof OperationError && caught.kind === 'not_found') missing = true
      // Otherwise Turtle cannot be reached; the editor keeps retrying.
    }
  }

  /**
   * Reads the Page again for when and by whom it was last edited. An Agent's
   * change is recorded as a Version at once; people's changes within two
   * minutes, so an open Page looks again now and then.
   */
  function refresh() {
    call('get_page', { id })
      .then(found => (page = found))
      // A Page that cannot be read now is found out when the connection drops.
      .catch(() => {})
  }
  const REFRESH_INTERVAL = 30_000

  // Once the Page is found; reading it again must not start the interval over.
  const opened = $derived(page !== null)
  $effect(() => {
    if (!opened) return
    const refreshing = setInterval(refresh, REFRESH_INTERVAL)
    return () => clearInterval(refreshing)
  })

  /**
   * The title the sidebar's tree last showed for this Page. When it changes,
   * the Page was renamed from an Entry menu, and the screen follows. A tree
   * loaded before the Page is no newer than it, so its title is not taken.
   */
  let titleInTree: string | undefined
  $effect(() => {
    if (!page) return
    const title = findEntry(navigation.tree(page.area.id) ?? [], page.id)?.title
    untrack(() => {
      if (page && title !== undefined && titleInTree !== undefined && title !== titleInTree && title !== page.title) {
        page = { ...page, title }
      }
      titleInTree = title
    })
  })

  /**
   * Where the sidebar's tree last showed this Page: what it is beneath, or
   * nothing once it left the tree. When that changes, the Page was moved,
   * and it is read again for its new Area and path.
   */
  let placeInTree: { areaId: string; place: string | null } | undefined
  $effect(() => {
    if (!page) return
    const areaId = page.area.id
    const tree = navigation.tree(areaId)
    if (!tree) return
    const place = findEntry(tree, page.id) ? above(tree, page.id).join('/') : null
    untrack(() => {
      // In the Area it was just found in, the place is taken as it is.
      if (placeInTree?.areaId === areaId && place !== placeInTree.place) refresh()
      placeInTree = { areaId, place }
    })
  })

  $effect(() => {
    if (page) document.title = `${page.title} · Turtle`
  })

  // The sidebar shows the tree of the Area the Page is in.
  $effect(() => {
    if (page) ui.openArea = page.area.id
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
  <main class="message">This Page could not be loaded. Reload the page to try again.</main>
{:else if page}
  <PageView {page} ondisconnect={checkAccess} onagent={refresh} />
{/if}

<style>
  .message {
    max-width: var(--width-form);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
    color: var(--color-muted);
  }
</style>
