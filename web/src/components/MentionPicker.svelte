<script lang="ts">
  /*
   * The picker that typing `@` where a word starts opens: the Members and
   * Guests the writer may see, found by name or email. Choosing one puts a
   * Mention where the `@` was; closing it leaves the `@` as typed.
   */
  import type { Editor } from '@tiptap/core'
  import { choices } from '../lib/mention'
  import type { Profile } from '../lib/operations.gen'
  import { call } from '../lib/ops'
  import ProfilePicture from './ProfilePicture.svelte'

  const { editor }: { editor: Editor } = $props()

  let dialog = $state<HTMLDialogElement>()
  let query = $state('')
  let people = $state.raw<Profile[] | null>(null)
  let failed = $state(false)
  /** Where the `@` that opened the picker went in. */
  let at = 0

  const offered = $derived(people ? choices(people, query) : [])

  $effect(() => {
    const storage = editor.storage.mention
    storage.ask = position => {
      at = position
      query = ''
      dialog?.showModal()
      void load()
    }
    return () => {
      storage.ask = null
    }
  })

  /** Asked afresh each time, so someone who just joined can be mentioned. */
  async function load() {
    failed = false
    try {
      people = (await call('list_profiles', {})).profiles
    } catch {
      failed = true
    }
  }

  function mention(person: Profile) {
    dialog?.close()
    const typed = editor.state.doc.textBetween(at, at + 1) === '@'
    editor
      .chain()
      .focus()
      .insertContentAt(typed ? { from: at, to: at + 1 } : at, [
        { type: 'mention', attrs: { personId: person.id } },
        { type: 'text', text: ' ' },
      ])
      .run()
  }

  /** Enter takes the best match. */
  function mentionFirst(event: SubmitEvent) {
    event.preventDefault()
    if (offered[0]) mention(offered[0])
  }
</script>

<dialog bind:this={dialog} closedby="any" onclose={() => editor.commands.focus()}>
  <form onsubmit={mentionFirst}>
    <label for="mention-query">Mention someone</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="mention-query" type="search" bind:value={query} autofocus autocomplete="off" placeholder="Search by name or email" />
    {#if failed}
      <p class="note" role="alert">The people could not be loaded. Check your connection and try again.</p>
    {:else if people && offered.length === 0}
      <p class="note">Nobody matches “{query}”.</p>
    {:else if offered.length > 0}
      <ul class="people" aria-label="Matching people">
        {#each offered as person (person.id)}
          <li>
            <button type="button" class="person" onclick={() => mention(person)}>
              <ProfilePicture name={person.name} size={24} picture={person.picture} />
              <span class="name">{person.name}</span>
              <span class="email">{person.email}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="actions">
      <button type="button" class="btn" onclick={() => dialog?.close()}>Cancel</button>
    </div>
  </form>
</dialog>

<style>
  form {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-4);
  }

  label {
    font-size: var(--text-s);
    font-weight: var(--weight-medium);
  }

  .note {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .people {
    display: grid;
    gap: var(--space-half);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .person {
    display: grid;
    grid-template-columns: auto 1fr;
    column-gap: var(--space-2);
    align-items: center;
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-s);
    background: transparent;
    text-align: left;
    transition: background var(--dur-fast) var(--ease-out);
  }

  .person :global(.profile-picture) {
    grid-row: span 2;
  }

  .person:hover,
  .person:focus-visible {
    background: var(--color-hover);
  }

  .name {
    font-weight: var(--weight-medium);
  }

  .email {
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
</style>
