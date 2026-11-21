<script lang="ts">
  import type { Editor as TipTap } from '@tiptap/core'
  import { onDestroy, untrack } from 'svelte'
  import * as Y from 'yjs'
  import ConnectionStatus from '../components/ConnectionStatus.svelte'
  import Editor from '../components/Editor.svelte'
  import MarkdownView from '../components/MarkdownView.svelte'
  import People from '../components/People.svelte'
  import Toolbar from '../components/Toolbar.svelte'
  import { Connection, type Status } from '../lib/connection'
  import { currentPerson } from '../lib/person'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { session } from '../lib/session.svelte'

  // The screen is created anew for every Page, so the ID never changes underneath it.
  const props: { id: string } = $props()
  const id = untrack(() => props.id)

  // Pages have no title of their own yet; until they do, the ID stands in for it.
  const title = $derived(id)

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

  async function signOut() {
    await session.signOut()
    router.navigate('/sign-in')
  }

  $effect(() => {
    document.title = `${title} · Turtle`
  })

  $effect(() => {
    // An address with an outdated or missing title is corrected in place (ADR-0007).
    const address = pageAddress(id, title)
    if (location.pathname !== address) router.replace(address)
  })
</script>

<header>
  <strong class="brand">Turtle</strong>
  <span class="title">{title}</span>
  <span class="people"><People awareness={connection.awareness} /></span>
  <ConnectionStatus {status} />
  <button type="button" onclick={signOut}>Sign out</button>
</header>

<Toolbar {editor}>
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
      <Editor {doc} {connection} {person} onready={started => (editor = started)} />
    {/if}
  </div>
  {#if markdownShown}
    <div class="markdown"><MarkdownView pageId={id} {doc} /></div>
  {/if}
</main>

<style>
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: var(--line-width) solid var(--color-line);
  }

  .brand {
    color: var(--color-accent);
  }

  .title {
    color: var(--color-muted);
  }

  .people {
    margin-left: auto;
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
