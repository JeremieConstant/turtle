<script lang="ts">
  /*
   * A box to write something down at once, and the Member's latest Quick
   * Notes beneath it. What is saved lands at the top of their Personal Area
   * unless they choose another Area they may write in.
   */
  import type { Editor as TipTap } from '@tiptap/core'
  import { toast } from 'svelte-sonner'
  import { Awareness } from 'y-protocols/awareness'
  import type * as Y from 'yjs'
  import { menuKeys, openMenu } from '../lib/menu'
  import { navigation } from '../lib/navigation.svelte'
  import { call } from '../lib/ops'
  import type { Area, QuickNote } from '../lib/operations.gen'
  import { currentPerson } from '../lib/person'
  import { docFrom, stateOf, unsavedQuickNote } from '../lib/quickNote'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { session } from '../lib/session.svelte'
  import { relativeTime } from '../lib/time'
  import { reportFailure } from '../lib/toasts'
  import Editor from './Editor.svelte'
  import Icon from './Icon.svelte'
  import Toolbar from './Toolbar.svelte'

  let { personal, memberId }: { personal: Area; memberId: string } = $props()

  /** How many Quick Notes are listed; the rest are found in the Areas. */
  const LISTED = 5
  /** How long the confirmation stays before the Area returns, in milliseconds. */
  const CONFIRMED_FOR = 4000

  /** The body's document, and the presence its editor expects, though nobody else writes here. */
  function bodyFrom(state: string) {
    const doc = docFrom(state)
    return { doc, connection: { awareness: new Awareness(doc) } }
  }

  const unsaved = $derived(unsavedQuickNote(memberId))
  const person = $derived(currentPerson(session.member?.name ?? ''))
  let title = $state('')
  let body = $state.raw<{ doc: Y.Doc; connection: { awareness: Awareness } }>(bodyFrom(''))
  let editor = $state.raw<TipTap | null>(null)
  let saving = $state(false)
  let saved = $state.raw<QuickNote | null>(null)
  let quickNotes = $state.raw<QuickNote[]>([])
  let now = $state(Math.floor(Date.now() / 1000))
  let titleField: HTMLInputElement

  /** Where the next Quick Note lands: the Personal Area first, then every Area the Member may write in. */
  let chosenId = $state<string | null>(null)
  const areas = $derived([
    personal,
    ...(navigation.areas ?? []).filter(area => !area.personal && area.role !== 'read'),
  ])
  const area = $derived(areas.find(area => area.id === chosenId) ?? personal)
  let areaButton = $state<HTMLButtonElement>()
  let areaMenu = $state<HTMLDivElement>()

  const mac = /Mac|iPhone|iPad/.test(navigator.platform)
  const shortcut = mac ? '⌘↵' : 'Ctrl+↵'

  $effect(() => {
    const draft = unsaved.read()
    title = draft.title
    body = bodyFrom(draft.body)
    void load()
  })

  $effect(() => {
    const shown = body
    return () => shown.connection.awareness.destroy()
  })

  $effect(() => {
    const ticking = setInterval(() => (now = Math.floor(Date.now() / 1000)), 30_000)
    return () => clearInterval(ticking)
  })

  $effect(() => {
    if (!saved) return
    const shown = setTimeout(() => (saved = null), CONFIRMED_FOR)
    return () => clearTimeout(shown)
  })

  async function load() {
    try {
      quickNotes = (await call('list_quick_notes', { limit: LISTED })).quick_notes
    } catch (caught) {
      reportFailure(caught, 'Your Quick Notes could not be loaded. Reload the page to try again.')
    }
  }

  function keep() {
    unsaved.keep({ title, body: !editor || editor.isEmpty ? '' : stateOf(body.doc) })
    saved = null
  }

  function started(running: TipTap) {
    editor = running
    running.on('update', keep)
  }

  /* Images are Attachments of a Page, and the Page exists only once the Quick Note is saved. */
  async function attach() {
    toast('Add images once the Quick Note is saved, on its Page.')
    return null
  }

  function choose(chosen: Area) {
    areaMenu?.hidePopover()
    chosenId = chosen.id
  }

  async function save() {
    if (saving || !title.trim()) return
    saving = true
    const into = area
    try {
      saved = await call('create_quick_note', {
        title,
        editor_state: editor?.isEmpty === false ? stateOf(body.doc) : null,
        area_id: into.id,
      })
      title = ''
      body = bodyFrom('')
      unsaved.keep({ title: '', body: '' })
      // The Area's tree in the sidebar gets the new Page if it is open there.
      if (navigation.tree(into.id)) navigation.loadTree(into.id).catch(() => {})
      await load()
    } catch (caught) {
      reportFailure(caught, 'The Quick Note could not be saved. It is still in the box; try again.')
    } finally {
      saving = false
      titleField.focus()
    }
  }

  /* Caught on the way down, before the editor takes the same keys for a line break. */
  function onkeydowncapture(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault()
      event.stopPropagation()
      void save()
    }
  }

  function ontitlekey(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.metaKey && !event.ctrlKey) {
      event.preventDefault()
      // TipTap focuses a frame later; what is typed in between must not stay in the title.
      editor?.view.focus()
      editor?.commands.focus('start')
    }
  }
</script>

<section class="quick" aria-labelledby="quick-label">
  <h2 id="quick-label" class="visually-hidden">Quick Note</h2>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="box" {onkeydowncapture}>
    <input
      bind:this={titleField}
      bind:value={title}
      class="title-field"
      placeholder="Title"
      aria-label="Quick Note title"
      maxlength="200"
      oninput={keep}
      onkeydown={ontitlekey}
    />
    {#if editor}
      <Toolbar {editor} inline />
    {/if}
    <div class="body">
      {#key body}
        <Editor
          doc={body.doc}
          connection={body.connection}
          {person}
          autofocus={false}
          compact
          label="Quick Note"
          {attach}
          onready={started}
        />
      {/key}
    </div>
    <div class="foot">
      <p class="where" aria-live="polite">
        {#if saved}
          {@const address = pageAddress(saved.id, saved.title)}
          <span class="saved">
            Saved to <a href={address} onclick={event => router.follow(event, address)}>{saved.area.name}</a>
          </span>
        {:else}
          Will be saved in
          <button
            type="button"
            class="area"
            bind:this={areaButton}
            aria-haspopup="menu"
            aria-label="Area: {area.name}"
            title="Choose the Area"
            onclick={() => areaMenu && areaButton && openMenu(areaMenu, areaButton)}
          >
            <span>{area.name}</span>
            <Icon name="caret" />
          </button>
        {/if}
      </p>
      <button class="btn primary" disabled={saving || !title.trim()} onclick={save}>
        Save <kbd>{shortcut}</kbd>
      </button>
    </div>
  </div>
  <div class="menu" popover role="menu" tabindex="-1" aria-label="Area" bind:this={areaMenu} onkeydown={menuKeys}>
    {#each areas as choice (choice.id)}
      <button
        type="button"
        class="menu-item"
        role="menuitemradio"
        aria-checked={choice.id === area.id}
        onclick={() => choose(choice)}
      >
        {choice.name}
      </button>
    {/each}
  </div>

  {#if quickNotes.length}
    <ul class="notes" aria-label="Recent Quick Notes">
      {#each quickNotes as quickNote (quickNote.id)}
        {@const address = pageAddress(quickNote.id, quickNote.title)}
        <li class:fresh={quickNote.id === saved?.id}>
          <a class="title" href={address} onclick={event => router.follow(event, address)}>
            {quickNote.title}
          </a>
          <span class="side">
            {#if !quickNote.area.personal}
              <span class="moved">in {quickNote.area.name}</span>
            {/if}
            <time datetime={new Date(quickNote.created_at * 1000).toISOString()}>{relativeTime(quickNote.created_at, now)}</time>
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .quick {
    margin-top: var(--space-12);
  }

  .box {
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-l);
    background: var(--color-side);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      background var(--dur-fast) var(--ease-out),
      box-shadow var(--dur-fast) var(--ease-out);
  }

  .box:focus-within {
    border-color: var(--color-focus);
    background: var(--color-surface);
    box-shadow: 0 0 0 var(--space-1) var(--color-focus-soft);
  }

  .title-field {
    display: block;
    width: 100%;
    padding: var(--space-4) var(--space-4) var(--space-3);
    border: 0;
    outline: 0;
    background: none;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-h3);
    font-weight: var(--weight-strong);
    line-height: var(--leading-tight);
  }

  .title-field::placeholder {
    color: var(--color-muted);
  }

  .body {
    max-height: var(--height-quick-note-max);
    padding: var(--space-3) var(--space-4) var(--space-2);
    overflow-y: auto;
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: 0 var(--space-3) var(--space-3) var(--space-4);
  }

  .where {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .area {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
    height: var(--size-tool);
    padding: 0 var(--space-1) 0 var(--space-2);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-s);
    font-weight: var(--weight-medium);
    transition:
      background var(--dur-fast) var(--ease-out),
      border-color var(--dur-fast) var(--ease-out);
  }

  .area span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .area:hover {
    border-color: var(--color-line-strong);
    background: var(--color-hover);
  }

  .area :global(svg) {
    flex: none;
    color: var(--color-muted);
  }

  .menu {
    position: fixed;
    min-width: var(--width-text-style-menu);
  }

  .menu-item[aria-checked='true'] {
    font-weight: var(--weight-medium);
  }

  .saved {
    color: var(--color-text);
    animation: saved-in var(--dur-base) var(--ease-out);
  }

  .saved a {
    color: inherit;
  }

  @keyframes saved-in {
    from {
      opacity: 0;
      transform: translateY(var(--space-hair));
    }
  }

  kbd {
    padding: 0 var(--space-1);
    border: var(--line) solid color-mix(in srgb, var(--color-bg) 25%, transparent);
    border-radius: var(--radius-s);
    color: color-mix(in srgb, var(--color-bg) 75%, transparent);
    font-family: inherit;
    font-size: var(--text-xs);
  }

  .notes {
    margin: var(--space-6) 0 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin: 0 calc(-1 * var(--space-3));
    padding: 0 var(--space-3);
    border-radius: var(--radius-m);
    transition: background var(--dur-fast) var(--ease-out);
  }

  li + li {
    border-top: var(--line) solid var(--color-line);
  }

  li:hover {
    background: var(--color-side);
  }

  li.fresh {
    animation: fresh var(--dur-agent) var(--ease-out);
  }

  @keyframes fresh {
    from {
      background: var(--color-focus-soft);
    }
  }

  .title {
    flex: 1;
    min-width: 0;
    padding: var(--space-3) 0;
    overflow: hidden;
    color: var(--color-text);
    font-size: var(--text-m);
    font-weight: var(--weight-medium);
    text-decoration: none;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .side {
    display: flex;
    gap: var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-s);
    white-space: nowrap;
  }

  .moved {
    color: var(--color-text-2);
  }

  @media (max-width: 800px) {
    kbd {
      display: none;
    }
  }
</style>
