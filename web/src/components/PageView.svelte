<script lang="ts">
  import type { Editor as TipTap } from '@tiptap/core'
  import { onDestroy, untrack } from 'svelte'
  import * as Y from 'yjs'
  import { Connection, type Status } from '../lib/connection'
  import type { Page } from '../lib/operations.gen'
  import { currentPerson } from '../lib/person'
  import { router } from '../lib/router.svelte'
  import { areaAddress } from '../lib/routes'
  import { session } from '../lib/session.svelte'
  import AppHeader from './AppHeader.svelte'
  import ConnectionStatus from './ConnectionStatus.svelte'
  import Editor from './Editor.svelte'
  import MarkdownView from './MarkdownView.svelte'
  import People from './People.svelte'
  import Toolbar from './Toolbar.svelte'

  const {
    page,
    ondisconnect,
  }: {
    /** A Page that has been found and may be read. It is opened once and stays open while this lives. */
    page: Page
    /** Called whenever the connection drops, so the screen can find out whether access is gone. */
    ondisconnect: () => void
  } = $props()
  const id = untrack(() => page.id)
  const editable = $derived(page.role !== 'read')

  const areaLink = $derived(areaAddress(page.area.id, page.area.name))
  let status = $state<Status>('connecting')
  let loaded = $state(false)
  let editor = $state.raw<TipTap | null>(null)
  let markdownShown = $state(false)

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
  onDestroy(() => {
    connection.close()
    doc.destroy()
  })
</script>

<AppHeader>
  <a class="area" href={areaLink} onclick={event => router.follow(event, areaLink)}>{page.area.name}</a>
  <span class="title">{page.title}</span>
  <People awareness={connection.awareness} />
  <ConnectionStatus {status} />
</AppHeader>

<!-- Someone who can only read gets no formatting buttons. -->
<Toolbar editor={editable ? editor : null}>
  {#if editor}
    <!-- People never see Markdown unless they ask for it: this is that switch. -->
    <button type="button" onclick={() => (markdownShown = !markdownShown)}>
      {markdownShown ? 'Hide Markdown' : 'Show Markdown'}
    </button>
  {/if}
</Toolbar>

<main>
  <div class="editor">
    <!-- The editor starts only once the stored content is here, so it never
         writes into an empty Page that is about to be filled. -->
    {#if loaded}
      <Editor {doc} {connection} {person} {editable} onready={started => (editor = started)} />
    {/if}
  </div>
  {#if markdownShown}
    <div class="markdown"><MarkdownView pageId={id} {doc} /></div>
  {/if}
</main>

<style>
  .area,
  .title {
    color: var(--color-muted);
  }


  main {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-6);
    max-width: var(--width-content);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
  }

  .editor {
    flex: 1 1 var(--width-editor);
    min-width: 0;
  }

  .markdown {
    flex: 1 1 var(--width-side);
    min-width: 0;
  }
</style>
