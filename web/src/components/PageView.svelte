<script lang="ts">
  import type { Editor as TipTap } from '@tiptap/core'
  import { onDestroy, untrack } from 'svelte'
  import * as Y from 'yjs'
  import ConnectionStatus from './ConnectionStatus.svelte'
  import Editor from './Editor.svelte'
  import MarkdownView from './MarkdownView.svelte'
  import People from './People.svelte'
  import Toolbar from './Toolbar.svelte'
  import { Connection, type Status } from '../lib/connection'
  import { currentPerson } from '../lib/person'
  import type { Page } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { areaAddress } from '../lib/routes'
  import { session } from '../lib/session.svelte'
  import AppHeader from './AppHeader.svelte'

  /** A Page that has been found and may be read. It is opened once and stays open while this lives. */
  const { page }: { page: Page } = $props()
  const id = untrack(() => page.id)
  const editable = $derived(page.role !== 'read')

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
    next => (status = next),
    () => (loaded = true),
  )
  onDestroy(() => {
    connection.close()
    doc.destroy()
  })

</script>

<AppHeader>
  <a class="area" href={areaAddress(page.area.id, page.area.name)} onclick={event => router.follow(event, areaAddress(page.area.id, page.area.name))}>{page.area.name}</a>
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
  .area {
    color: var(--color-muted);
  }

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
