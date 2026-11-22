<script lang="ts">
  /*
   * One Page: where it sits, its title, when and by whom it was last edited,
   * and its content. On "Deploying to production" two Members and the Docs
   * Agent are working at the same time.
   *
   * Leaving a Page you changed says so in a toast.
   */
  import type { Editor as TipTap } from '@tiptap/core'
  import { untrack } from 'svelte'
  import { toast } from 'svelte-sonner'
  import Presence from '../components/Presence.svelte'
  import Tags from '../components/Tags.svelte'
  import Editor from '../editor/Editor.svelte'
  import { EDITING, startLiveEditing } from '../lib/mock/live'
  import { get_page, rename_page, replace_page } from '../lib/mock/ops.svelte'
  import type { Author } from '../lib/mock/types'
  import { router } from '../lib/router.svelte'
  import { areaAddress, pageAddress } from '../lib/routes'
  import { exactTime, relativeTime } from '../lib/time'
  import { ui } from '../lib/ui.svelte'

  let { page_id }: { page_id: string } = $props()

  const page = $derived(get_page({ page_id }))
  const writable = $derived(page?.role !== 'read')
  const live = $derived(page_id === 'p-deploy')

  // The content is handed to the editor once per Page; after that the editor owns it.
  const initialHtml = $derived(untrack(() => get_page({ page_id })?.html ?? ''))

  let now = $state(Date.now())
  let agentActive = $state(false)
  let title = $state('')
  let titleEl = $state<HTMLTextAreaElement>()
  let saveTimer: ReturnType<typeof setTimeout> | undefined
  /** Content typed in the last moment that the store has not taken yet. */
  let pending: { page_id: string; html: string; by: Author | null } | null = null
  /** Whether the signed-in Member changed this Page since opening it. */
  let changed = false
  let editor = $state.raw<TipTap>()

  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 15_000)
    return () => clearInterval(t)
  })

  $effect(() => {
    void page_id
    untrack(() => (agentActive = false))
  })

  // The title follows renames from elsewhere (the tree) unless it is being edited.
  $effect(() => {
    const next = page?.title ?? ''
    untrack(() => document.activeElement !== titleEl && (title = next))
  })

  function save(p: NonNullable<typeof pending>) {
    if (get_page({ page_id: p.page_id })) replace_page({ page_id: p.page_id, html: p.html, by: p.by ?? undefined })
    now = Date.now()
  }

  // On the way out: store what is still waiting, and say that it is stored.
  // The store is written a moment later, outside Svelte's teardown.
  $effect(() => {
    void page_id
    return () => {
      clearTimeout(saveTimer)
      const p = pending
      const done = changed && untrack(() => title)
      pending = null
      changed = false
      queueMicrotask(() => {
        if (p) save(p)
        if (done) toast('Changes saved', { description: done })
      })
    }
  })

  // "Edit" in the Entry menu puts the cursor into the content.
  $effect(() => {
    if (editor && ui.focusEditor === page_id) {
      ui.focusEditor = null
      editor.commands.focus('start')
    }
  })

  // Keep the title part of the address current (ADR-0007).
  $effect(() => {
    if (page) {
      const address = pageAddress(page.id, page.title)
      if (router.pathname !== address) router.replace(address + location.hash)
    }
  })

  $effect(() => {
    if (location.hash === '#rename' && titleEl) {
      titleEl.focus()
      titleEl.select()
      history.replaceState(null, '', location.pathname)
    }
  })

  function saveTitle() {
    const next = title.trim() || 'Untitled'
    if (page && next !== page.title) {
      rename_page({ page_id: page.id, title: next })
      changed = true
    }
    title = next
  }

  /* The real editor syncs every keystroke over the CRDT. The mock stores the
     content a moment after typing stops, under the name of whoever typed. */
  function onchange(editor: TipTap, by: Author | null) {
    if (!by) changed = true
    clearTimeout(saveTimer)
    // Read now: by the time the Page is left, the editor may already be gone.
    const p = (pending = { page_id, html: editor.getHTML(), by })
    saveTimer = setTimeout(() => {
      pending = null
      save(p)
    }, 400)
  }

  let stopLive: (() => void) | null = null
  function onready(ready: TipTap) {
    editor = ready
    stopLive?.()
    stopLive = null
    if (!live) return
    stopLive = startLiveEditing(ready, {
      onAgent: () => {
        agentActive = true
        setTimeout(() => (agentActive = false), 7000)
      },
    })
  }
  $effect(() => () => stopLive?.())

  function autosize(node: HTMLTextAreaElement) {
    const fit = () => {
      node.style.height = 'auto'
      node.style.height = node.scrollHeight + 'px'
    }
    $effect(() => {
      void title
      fit()
    })
    window.addEventListener('resize', fit)
    return { destroy: () => window.removeEventListener('resize', fit) }
  }
</script>

{#if page}
  <article class="page">
    <header class="bar">
      <div class="where">
        <nav class="path" aria-label="Where this Page sits">
          <a href={areaAddress(page.area.id, page.area.name)} onclick={(e) => router.follow(e, areaAddress(page.area.id, page.area.name))}>
            {page.area.name}
          </a>
          {#each page.path as step (step.id)}
            <span class="sep" aria-hidden="true">/</span>
            {#if step.kind === 'page'}
              <a href={pageAddress(step.id, step.title)} onclick={(e) => router.follow(e, pageAddress(step.id, step.title))}>{step.title}</a>
            {:else}
              <span>{step.title}</span>
            {/if}
          {/each}
        </nav>
        {#if page.tags.length || writable}
          <div class="bar-tags"><Tags id="bar-tags" page_id={page.id} tags={page.tags} {writable} /></div>
        {/if}
      </div>
      {#if live}
        <Presence editing={EDITING} {agentActive} />
      {/if}
    </header>

    <div class="body">
      {#key page.id}
        <div class="head">
          {#if writable}
            <textarea
              class="title"
              rows="1"
              bind:this={titleEl}
              bind:value={title}
              use:autosize
              onblur={saveTitle}
              onkeydown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault()
                  ;(e.currentTarget as HTMLTextAreaElement).blur()
                }
              }}
              aria-label="Title"
            ></textarea>
          {:else}
            <h1 class="title">{page.title}</h1>
          {/if}
          <p class="meta">
            {#key page.edited_at}
              <span class="edited" class:agent={page.edited_by.kind === 'agent'}>
                Edited <time datetime={new Date(page.edited_at).toISOString()} title={exactTime(page.edited_at)}>{relativeTime(page.edited_at, now)}</time>
                by
                {#if page.edited_by.kind === 'agent'}
                  <span class="agent-name">{page.edited_by.name}</span>
                {:else}
                  {page.edited_by.name}
                {/if}
              </span>
            {/key}
            {#if !writable}
              <span class="role">You can read this Page</span>
            {/if}
          </p>
          {#if page.tags.length || writable}
            <div class="meta-tags">
              <Tags id="meta-tags" page_id={page.id} tags={page.tags} {writable} />
            </div>
          {/if}
        </div>
        <Editor html={initialHtml} editable={writable} {onready} {onchange} />
      {/key}
    </div>
  </article>
{:else}
  <div class="missing">
    <h1>This Page does not exist</h1>
    <p>It may have been moved to the Trash.</p>
  </div>
{/if}

<style>
  .page {
    min-height: 100%;
  }

  .bar {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    height: var(--height-bar);
    padding: 0 var(--space-6);
    background: rgb(255 255 255 / 0.86);
    backdrop-filter: blur(12px) saturate(180%);
  }

  .path {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    color: var(--color-muted);
    font-size: var(--text-s);
    white-space: nowrap;
    overflow: hidden;
  }

  .path a {
    text-decoration: none;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: color var(--dur-fast) var(--ease-out);
  }

  .path a:hover {
    color: var(--color-text);
  }

  .sep {
    color: var(--color-faint);
  }

  .where {
    display: flex;
    align-items: center;
    min-width: 0;
  }

  .bar-tags {
    flex: none;
    margin-left: var(--space-5);
  }

  .bar-tags :global(.tags) {
    flex-wrap: nowrap;
  }


  .meta-tags {
    display: none;
    margin-top: var(--space-3);
  }

  /* Where the bar has no room beside the path, the Tags move under the title. */
  @media (max-width: 1180px) {
    .bar-tags {
      display: none;
    }

    .meta-tags {
      display: block;
    }
  }

  .body {
    max-width: calc(var(--width-text) + 2 * var(--space-6));
    margin: 0 auto;
    padding: var(--space-10) var(--space-6) 0;
  }

  .head {
    margin-bottom: var(--space-8);
  }

  .title {
    display: block;
    width: 100%;
    margin: 0;
    padding: 0;
    border: 0;
    outline: 0;
    resize: none;
    overflow: hidden;
    background: none;
    font-size: var(--text-h1);
    font-weight: var(--weight-strong);
    line-height: var(--leading-tight);
    letter-spacing: var(--tracking-display);
  }

  textarea.title:focus-visible {
    outline: none;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-4);
    margin: var(--space-3) 0 0;
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .edited {
    animation: meta-in var(--dur-slow) var(--ease-out);
  }

  @keyframes meta-in {
    from {
      opacity: 0.2;
    }
  }

  .agent-name {
    color: var(--color-agent);
    font-weight: var(--weight-medium);
  }

  .role {
    color: var(--color-text-2);
  }

  .role::before {
    content: '';
    display: inline-block;
    width: 4px;
    height: 4px;
    margin: 0 var(--space-3) 2px 0;
    border-radius: var(--radius-round);
    background: var(--color-faint);
    vertical-align: middle;
  }

  .missing {
    padding: var(--space-16) var(--space-6);
    text-align: center;
    color: var(--color-muted);
  }

  .missing h1 {
    color: var(--color-text);
    font-size: var(--text-h2);
  }

  @media (max-width: 800px) {
    .bar {
      padding: 0 var(--space-4) 0 calc(var(--space-4) + 36px);
    }

    .body {
      padding: var(--space-6) var(--space-4) 0;
    }

    .title {
      font-size: 1.625rem;
    }
  }
</style>
