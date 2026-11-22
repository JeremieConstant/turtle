<script lang="ts">
  /*
   * One open Page: where it sits, who is on it, its title, when and by whom it
   * was last edited, and its content, edited live with the others. There is no
   * "saved" message: every change goes over the connection as it is made, so
   * the connection's state is the only signal about saving.
   */
  import type { Editor as TipTap } from '@tiptap/core'
  import { onDestroy, untrack } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import * as Y from 'yjs'
  import { markAgentChange, unmarkAgentChange } from '../lib/agent-changes'
  import { Connection, type Status } from '../lib/connection'
  import type { Page } from '../lib/operations.gen'
  import { currentPerson, type Present } from '../lib/person'
  import { router } from '../lib/router.svelte'
  import { areaAddress, pageAddress } from '../lib/routes'
  import { session } from '../lib/session.svelte'
  import { exactTime, relativeTime } from '../lib/time'
  import { authorName } from '../lib/versions'
  import ConnectionStatus from './ConnectionStatus.svelte'
  import Editor from './Editor.svelte'
  import MarkdownView from './MarkdownView.svelte'
  import People from './People.svelte'
  import Toolbar from './Toolbar.svelte'
  import Versions from './Versions.svelte'

  const {
    page,
    ondisconnect,
    onagent,
  }: {
    /** A Page that has been found and may be read. It is opened once and stays open while this lives. */
    page: Page
    /** Called whenever the connection drops, so the screen can find out whether access is gone. */
    ondisconnect: () => void
    /** Called when an Agent has just changed the Page, which records a Version. */
    onagent: () => void
  } = $props()
  const id = untrack(() => page.id)
  const editable = $derived(page.role !== 'read')
  const areaLink = $derived(areaAddress(page.area.id, page.area.name))

  let status = $state<Status>('connecting')
  let loaded = $state(false)
  let editor = $state.raw<TipTap | null>(null)
  let markdownShown = $state(false)
  let versionsShown = $state(false)

  // Relative times ("4 minutes ago") move on while the Page is open.
  let now = $state(Math.floor(Date.now() / 1000))
  $effect(() => {
    const ticking = setInterval(() => (now = Math.floor(Date.now() / 1000)), 30_000)
    return () => clearInterval(ticking)
  })

  // The app shows Pages only to a signed-in Member.
  const person = currentPerson(session.member?.name ?? 'Unknown')
  const doc = new Y.Doc()
  const connection = new Connection(
    `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws/${encodeURIComponent(id)}`,
    doc,
    next => {
      status = next
      if (next === 'disconnected') ondisconnect()
    },
    () => (loaded = true),
  )

  /*
   * An Agent's change. Turtle sends it as an ordinary update and announces the
   * Agent right after. An announcement that follows no fresh update is an
   * Agent that was already there, for someone who just arrived: nothing to mark.
   */
  const AGENT_FOLLOWS_UPDATE_MS = 1000
  /** As long as the mark holds and fades in the editor's styles. */
  const AGENT_MARKED_MS = 7_400
  let lastArrival = 0
  let marks = 0
  /** The Agents whose change is marked now. */
  const working = new SvelteSet<string>()
  /** How many of each Agent's changes are marked now, so each stays lit until its last one fades. */
  const marked = new Map<string, number>()
  const timers = new Set<ReturnType<typeof setTimeout>>()

  doc.on('update', (_update: Uint8Array, origin: unknown) => {
    if (origin === connection) lastArrival = Date.now()
  })

  connection.awareness.on('update', ({ added, updated }: { added: number[]; updated: number[] }, origin: unknown) => {
    if (origin !== connection || Date.now() - lastArrival > AGENT_FOLLOWS_UPDATE_MS) return
    for (const client of [...added, ...updated]) {
      const user = connection.awareness.getStates().get(client)?.user as Present | undefined
      if (user && 'agent' in user) agentChanged(user.name)
    }
  })

  function agentChanged(name: string) {
    onagent()
    working.add(name)
    marked.set(name, (marked.get(name) ?? 0) + 1)
    const mark = `agent-${++marks}`
    // The editor may still be starting; the change then shows without a mark.
    const marking = editor && markAgentChange(editor.state, name, mark)
    if (editor && marking) editor.view.dispatch(marking)
    const markedIn = editor
    const timer = setTimeout(() => {
      timers.delete(timer)
      if (markedIn && !markedIn.isDestroyed) markedIn.view.dispatch(unmarkAgentChange(markedIn.state, mark))
      const left = (marked.get(name) ?? 1) - 1
      if (left > 0) marked.set(name, left)
      else {
        marked.delete(name)
        working.delete(name)
      }
    }, AGENT_MARKED_MS)
    timers.add(timer)
  }

  onDestroy(() => {
    timers.forEach(clearTimeout)
    connection.close()
    doc.destroy()
  })

  const edited = $derived(page.last_edited)
  /** Separates the names of the authors as a sentence does: "A, B and C". */
  const joiner = (index: number, count: number) => (index === 0 ? '' : index === count - 1 ? ' and ' : ', ')
</script>

<article class="page">
  <header class="bar">
    <nav class="path" aria-label="Where this Page is">
      <a href={areaLink} onclick={event => router.follow(event, areaLink)}>{page.area.name}</a>
      {#each page.path as entry (entry.id)}
        <span class="separator" aria-hidden="true">/</span>
        {#if entry.kind === 'page'}
          {@const link = pageAddress(entry.id, entry.title)}
          <a href={link} onclick={event => router.follow(event, link)}>{entry.title}</a>
        {:else}
          <span>{entry.title}</span>
        {/if}
      {/each}
    </nav>
    <div class="side">
      <People awareness={connection.awareness} {working} />
      <ConnectionStatus {status} />
      {#if editor}
        <!-- People never see Markdown unless they ask for it: this is that switch. -->
        <button type="button" class="btn" aria-pressed={markdownShown} onclick={() => (markdownShown = !markdownShown)}>
          Markdown
        </button>
      {/if}
      <button type="button" class="btn" aria-pressed={versionsShown} onclick={() => (versionsShown = !versionsShown)}>
        Versions
      </button>
    </div>
  </header>

  <div class="columns">
    <div class="body">
      <div class="head">
        <h1 class="title">{page.title}</h1>
        <p class="meta">
          {#if edited}
            <span class="edited">
              Edited
              <time datetime={new Date(edited.at * 1000).toISOString()} title={exactTime(edited.at)}>
                {relativeTime(edited.at, now)}
              </time>
              by
              {#each edited.by as author, index}{joiner(index, edited.by.length)}<span class:agent={author.kind !== 'member'}
                  >{authorName(author)}</span
                >{:else}someone who is no longer here{/each}
            </span>
          {/if}
          {#if !editable}
            <span class="role">You can read this Page but not change it</span>
          {/if}
        </p>
      </div>

      <!-- Someone who can only read gets no formatting buttons. -->
      {#if editable && editor}
        <Toolbar {editor} />
      {/if}

      <!-- The editor starts only once the stored content is here, so it never
           writes into an empty Page that is about to be filled. -->
      {#if loaded}
        <Editor {doc} {connection} {person} {editable} onready={started => (editor = started)} />
      {/if}
    </div>

    {#if markdownShown}
      <aside class="panel"><MarkdownView pageId={id} {doc} /></aside>
    {/if}
    {#if versionsShown}
      <aside class="panel"><Versions pageId={id} {editable} /></aside>
    {/if}
  </div>
</article>

<style>
  .page {
    min-height: 100%;
  }

  .bar {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    /* Where the path and the people do not fit side by side, the path keeps a row of its own. */
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    min-height: var(--height-bar);
    padding: var(--space-1) var(--space-6);
    background: var(--color-bar);
    backdrop-filter: blur(var(--blur-bar)) saturate(180%);
  }

  .path {
    flex: 1 1 var(--width-menu);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    color: var(--color-muted);
    font-size: var(--text-s);
    white-space: nowrap;
    overflow: hidden;
  }

  .path > * {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .path a {
    text-decoration: none;
    transition: color var(--dur-fast) var(--ease-out);
  }

  .path a:hover {
    color: var(--color-text);
  }

  .path .separator {
    flex: none;
    color: var(--color-faint);
  }

  .side {
    flex: none;
    display: flex;
    margin-left: auto;
    align-items: center;
    gap: var(--space-3);
  }

  .columns {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--space-8);
    padding: var(--space-10) var(--space-6) 0;
  }

  .body {
    flex: 1 1 var(--width-editor);
    min-width: 0;
    max-width: var(--width-text);
  }

  .panel {
    flex: 0 1 var(--width-side);
    min-width: 0;
  }

  .head {
    margin-bottom: var(--space-8);
  }

  .title {
    margin: 0;
    font-size: var(--text-h1);
    font-weight: var(--weight-strong);
    line-height: var(--leading-tight);
    letter-spacing: var(--tracking-display);
    overflow-wrap: anywhere;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-4);
    margin: var(--space-3) 0 0;
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .meta:empty {
    display: none;
  }

  .edited .agent {
    color: var(--color-agent);
    font-weight: var(--weight-medium);
  }

  .role {
    color: var(--color-text-2);
  }

  @media (max-width: 800px) {
    /* Two rows of bar would hide too much of a small screen: it scrolls away with the Page. */
    .bar {
      position: static;
      padding: var(--space-1) var(--space-4);
    }

    .columns {
      padding: var(--space-6) var(--space-4) 0;
    }

    .title {
      font-size: var(--text-h2);
    }
  }
</style>
