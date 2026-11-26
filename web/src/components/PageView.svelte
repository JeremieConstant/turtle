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
  import { attach } from '../lib/attachments'
  import { beginAgentChange, endAgentChange, unmarkAgentChange } from '../lib/agent-changes'
  import { ask } from '../lib/confirm.svelte'
  import { Connection, type ChangingAgent, type Status } from '../lib/connection'
  import { call } from '../lib/ops'
  import type { Attachment, Page } from '../lib/operations.gen'
  import { currentPerson, type Present } from '../lib/person'
  import { router } from '../lib/router.svelte'
  import { areaAddress, pageAddress } from '../lib/routes'
  import { session } from '../lib/session.svelte'
  import { exactTime, relativeTime } from '../lib/time'
  import { reportFailure } from '../lib/toasts'
  import { authorName } from '../lib/versions'
  import Attachments from './Attachments.svelte'
  import ConnectionStatus from './ConnectionStatus.svelte'
  import Editor from './Editor.svelte'
  import MarkdownView from './MarkdownView.svelte'
  import People from './People.svelte'
  import Tags from './Tags.svelte'
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
    /** Called when Turtle announces an Agent, once its change is recorded as a Version. */
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

  /**
   * The Page's Tags as the Page was read, until the connection says what they
   * are now. From then on it alone reports them, as each change is made here
   * or by anyone else, so an answer arriving late never shows older Tags.
   */
  let tags = $state<string[]>(untrack(() => page.tags))

  let attachments = $state<Attachment[]>([])

  function loadAttachments() {
    call('list_attachments', { id })
      .then(listed => (attachments = listed.attachments))
      .catch(caught => reportFailure(caught, 'The Attachments of this Page could not be loaded.'))
  }
  loadAttachments()

  /** Attaches a file, reporting a failure, and lists it with the others. */
  async function attachFile(file: File) {
    try {
      const attached = await attach(id, file)
      attachments = [...attachments, attached]
      return attached
    } catch (caught) {
      reportFailure(caught, `${file.name || 'The file'} could not be attached. Try again.`)
      return null
    }
  }

  /** Removes an Attachment once the person confirms it, and takes it off the list. */
  async function removeAttachment(attachment: Attachment) {
    const sure = await ask({
      title: `Remove ${attachment.name}?`,
      message: attachment.shown_as_image
        ? 'Where the Page shows this image, it will show as missing. The file cannot be brought back.'
        : 'Links to this file will no longer download it. The file cannot be brought back.',
      action: 'Remove',
      destructive: true,
    })
    if (!sure) return
    try {
      await call('remove_attachment', { id: attachment.id })
      attachments = attachments.filter(other => other.id !== attachment.id)
    } catch (caught) {
      reportFailure(caught, `${attachment.name} could not be removed. Try again.`)
    }
  }

  // Relative times ("4 minutes ago") move on while the Page is open.
  let now = $state(Math.floor(Date.now() / 1000))
  $effect(() => {
    const ticking = setInterval(() => (now = Math.floor(Date.now() / 1000)), 30_000)
    return () => clearInterval(ticking)
  })

  // The app shows Pages only to a signed-in Member.
  const person = currentPerson(session.member?.name ?? 'Unknown', session.member?.id)
  const doc = new Y.Doc()
  const connection = new Connection(
    `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws/${encodeURIComponent(id)}`,
    doc,
    next => {
      status = next
      if (next === 'disconnected') ondisconnect()
    },
    () => (loaded = true),
    agentChanged,
    changed => (tags = changed),
  )

  /** As long as the mark holds and fades in the editor's styles. */
  const AGENT_MARKED_MS = 7_400
  let marks = 0
  /** The Agents whose change is marked now. */
  const working = new SvelteSet<string>()
  /** How many of each Agent's changes are marked now, so each stays lit until its last one fades. */
  const marked = new Map<string, number>()
  const timers = new Set<ReturnType<typeof setTimeout>>()

  /** An Agent's change arrives, naming the Agent: exactly that change is marked with its name. */
  function agentChanged(agent: ChangingAgent, apply: () => void) {
    const mark = `agent-${++marks}`
    // The editor may still be starting; the change then shows without a mark.
    const markedIn = editor && !editor.isDestroyed ? editor : null
    markedIn?.view.dispatch(beginAgentChange(markedIn.state, agent.name, mark))
    try {
      apply()
    } finally {
      if (markedIn && !markedIn.isDestroyed) markedIn.view.dispatch(endAgentChange(markedIn.state))
    }
    working.add(agent.name)
    marked.set(agent.name, (marked.get(agent.name) ?? 0) + 1)
    const timer = setTimeout(() => {
      timers.delete(timer)
      if (markedIn && !markedIn.isDestroyed) markedIn.view.dispatch(unmarkAgentChange(markedIn.state, mark))
      const left = (marked.get(agent.name) ?? 1) - 1
      if (left > 0) marked.set(agent.name, left)
      else {
        marked.delete(agent.name)
        working.delete(agent.name)
      }
    }, AGENT_MARKED_MS)
    timers.add(timer)
  }

  // Turtle announces an Agent once its change is recorded as a Version, so
  // that is when the Page reads who last edited it.
  connection.awareness.on('update', ({ added, updated }: { added: number[]; updated: number[] }, origin: unknown) => {
    if (origin !== connection) return
    const states = connection.awareness.getStates()
    const agentAnnounced = [...added, ...updated].some(client => {
      const user = states.get(client)?.user as Present | undefined
      return user !== undefined && 'agent' in user
    })
    if (agentAnnounced) onagent()
  })

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
    {#if tags.length || editable}
      <div class="bar-tags">
        <Tags listId="bar-tags" pageId={id} {tags} writable={editable} />
      </div>
    {/if}
    <div class="side">
      <People awareness={connection.awareness} {working} />
      <ConnectionStatus {status} />
      {#if editor}
        <!-- People never see Markdown unless they ask for it: this is that switch. -->
        <button type="button" class="btn ghost" aria-pressed={markdownShown} onclick={() => (markdownShown = !markdownShown)}>
          Markdown
        </button>
      {/if}
      <button type="button" class="btn ghost" aria-pressed={versionsShown} onclick={() => (versionsShown = !versionsShown)}>
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
        {#if tags.length || editable}
          <div class="meta-tags">
            <Tags listId="meta-tags" pageId={id} {tags} writable={editable} />
          </div>
        {/if}
      </div>

      <!-- Someone who can only read gets no formatting buttons. -->
      {#if editable && editor}
        <Toolbar {editor} />
      {/if}

      <!-- The editor starts only once the stored content is here, so it never
           writes into an empty Page that is about to be filled. -->
      {#if loaded}
        <Editor {doc} {connection} {person} {editable} pageId={id} attach={attachFile} onready={started => (editor = started)} />
      {/if}

      <Attachments {attachments} writable={editable} onattach={files => files.forEach(file => void attachFile(file))}
        onremove={attachment => void removeAttachment(attachment)}
      />
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
    /* Never wider than the bar: on a narrow window the people, the connection
       note and the buttons wrap onto more rows rather than run off the screen. */
    flex: 0 1 auto;
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    min-width: 0;
    margin-left: auto;
    align-items: center;
    gap: var(--space-2) var(--space-3);
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

  /* Tags sit beside the path, apart from the content; where the bar has no
     room for them, they move under the title. */
  .bar-tags {
    flex: none;
  }

  .bar-tags :global(.tags) {
    flex-wrap: nowrap;
  }

  .meta-tags {
    display: none;
    margin-top: var(--space-3);
  }

  @media (max-width: 1180px) {
    .bar-tags {
      display: none;
    }

    .meta-tags {
      display: block;
    }
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
