<script lang="ts">
  /*
   * A box to write something down without choosing where it belongs, and the
   * Member's latest Quick Notes beneath it. What is saved lands at the top of
   * their Personal Area.
   */
  import { navigation } from '../lib/navigation.svelte'
  import { call } from '../lib/ops'
  import type { Area, QuickNote } from '../lib/operations.gen'
  import { unsavedQuickNote } from '../lib/quickNote'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { relativeTime } from '../lib/time'
  import { reportFailure } from '../lib/toasts'

  let { personal, memberId }: { personal: Area; memberId: string } = $props()

  /** How many Quick Notes are listed; the rest are found in the Areas. */
  const LISTED = 5
  /** How long the confirmation stays before the hint returns, in milliseconds. */
  const CONFIRMED_FOR = 4000

  const unsaved = $derived(unsavedQuickNote(memberId))
  let text = $state('')
  let saving = $state(false)
  let saved = $state.raw<QuickNote | null>(null)
  let quickNotes = $state.raw<QuickNote[]>([])
  let now = $state(Math.floor(Date.now() / 1000))
  let box: HTMLTextAreaElement

  const mac = /Mac|iPhone|iPad/.test(navigator.platform)
  const shortcut = mac ? '⌘↵' : 'Ctrl+↵'

  $effect(() => {
    text = unsaved.read()
    void load()
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

  function oninput() {
    unsaved.keep(text)
    saved = null
  }

  async function save() {
    if (saving || !text.trim()) return
    saving = true
    try {
      const sent = text
      saved = await call('create_quick_note', { text: sent })
      // Whatever was typed while the Quick Note was on its way stays in the box.
      text = text.startsWith(sent) ? text.slice(sent.length).trimStart() : text
      unsaved.keep(text)
      // The Personal Area's tree in the sidebar gets the new Page if it is open there.
      if (navigation.tree(personal.id)) navigation.loadTree(personal.id).catch(() => {})
      await load()
    } catch (caught) {
      reportFailure(caught, 'The Quick Note could not be saved. It is still in the box; try again.')
    } finally {
      saving = false
      box.focus()
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault()
      void save()
    }
  }
</script>

<section class="quick" aria-labelledby="quick-label">
  <h2 id="quick-label" class="visually-hidden">Quick Note</h2>
  <div class="box">
    <textarea
      bind:this={box}
      bind:value={text}
      rows="3"
      placeholder="Write a Quick Note"
      aria-label="Quick Note"
      aria-describedby="quick-hint"
      {oninput}
      {onkeydown}
    ></textarea>
    <div class="foot">
      <p id="quick-hint" class="hint" aria-live="polite">
        {#if saved}
          {@const address = pageAddress(saved.id, saved.title)}
          <span class="saved">
            Saved to <a href={address} onclick={event => router.follow(event, address)}>{saved.area.name}</a>
          </span>
        {:else}
          Lands at the top of {personal.name}. The first line is the title.
        {/if}
      </p>
      <button class="btn primary" disabled={saving || !text.trim()} onclick={save}>
        Save <kbd>{shortcut}</kbd>
      </button>
    </div>
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

  textarea {
    display: block;
    width: 100%;
    min-height: var(--height-quick-note);
    max-height: var(--height-quick-note-max);
    field-sizing: content;
    padding: var(--space-4) var(--space-4) var(--space-2);
    border: 0;
    outline: 0;
    resize: none;
    background: none;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-base);
    line-height: var(--leading-body);
  }

  textarea::placeholder {
    color: var(--color-muted);
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: 0 var(--space-3) var(--space-3) var(--space-4);
  }

  .hint {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-s);
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
