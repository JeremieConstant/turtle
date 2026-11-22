<script lang="ts">
  /*
   * The Tags of a Page, drawn as small grey pills so they never read as part
   * of the path or the content. With the Write Role a Tag can be added, with
   * the Tags already in use suggested, and taken off again. A change shows
   * once the Page's connection reports it, like anyone else's.
   */
  import { call } from '../lib/ops'
  import { suggestedTags } from '../lib/tags'
  import { reportFailure } from '../lib/toasts'
  import Icon from './Icon.svelte'

  const {
    pageId,
    tags,
    writable,
    listId,
  }: {
    pageId: string
    tags: string[]
    writable: boolean
    /** The ID of the list of suggestions, unique on the screen. */
    listId: string
  } = $props()

  let adding = $state(false)
  let draft = $state('')
  let input = $state<HTMLInputElement>()
  let active = $state(-1)
  /** The Tags in use on Pages the person can read, asked for each time a Tag is added. */
  let inUse = $state<string[]>([])
  const suggestions = $derived(adding ? suggestedTags(inUse, tags, draft) : [])

  async function start() {
    adding = true
    draft = ''
    active = -1
    await Promise.resolve()
    input?.focus()
    call('list_tags', {})
      .then(listed => (inUse = listed.tags))
      // Without suggestions a Tag can still be typed.
      .catch(() => (inUse = []))
  }

  function stop() {
    draft = ''
    adding = false
  }

  async function commit(tag = draft) {
    stop()
    if (!tag.trim()) return
    try {
      await call('add_tag', { page_id: pageId, tag })
    } catch (caught) {
      reportFailure(caught, 'The Tag could not be added. Try again.')
    }
  }

  async function remove(tag: string) {
    try {
      await call('remove_tag', { page_id: pageId, tag })
    } catch (caught) {
      reportFailure(caught, 'The Tag could not be removed. Try again.')
    }
  }

  function onkeydown(event: KeyboardEvent) {
    const count = suggestions.length
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      if (!count) return
      const step = event.key === 'ArrowDown' ? 1 : -1
      active = active < 0 ? (step > 0 ? 0 : count - 1) : (active + step + count) % count
    } else if (event.key === 'Enter') {
      event.preventDefault()
      commit(suggestions[active] ?? draft)
    } else if (event.key === 'Escape') {
      stop()
    }
  }
</script>

<ul class="tags" aria-label="Tags">
  {#each tags as tag (tag)}
    <li class="tag">
      <span>{tag}</span>
      {#if writable}
        <button type="button" class="remove" aria-label="Remove the Tag {tag}" onclick={() => remove(tag)}>
          <Icon name="close" size={10} />
        </button>
      {/if}
    </li>
  {/each}
  {#if writable}
    <li>
      {#if adding}
        <span class="adding">
          <input
            class="new"
            bind:this={input}
            bind:value={draft}
            placeholder="Tag"
            aria-label="New Tag"
            role="combobox"
            aria-expanded={suggestions.length > 0}
            aria-autocomplete="list"
            aria-controls={listId}
            size={Math.max(4, draft.length + 1)}
            oninput={() => (active = -1)}
            onblur={() => commit()}
            {onkeydown}
          />
          {#if suggestions.length}
            <ul class="suggestions" id={listId} role="listbox" aria-label="Tags in use">
              {#each suggestions as tag, index (tag)}
                <li role="option" aria-selected={index === active}>
                  <!-- Chosen on mousedown, before the field loses focus and adds what was typed. -->
                  <button
                    type="button"
                    class:active={index === active}
                    tabindex="-1"
                    onmousedown={event => {
                      event.preventDefault()
                      commit(tag)
                    }}>{tag}</button
                  >
                </li>
              {/each}
            </ul>
          {/if}
        </span>
      {:else}
        <button type="button" class="add" onclick={start} title="Add a Tag">
          <Icon name="plus" size={12} /><span>Tag</span>
        </button>
      {/if}
    </li>
  {/if}
</ul>

<style>
  .tags {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-xs);
  }

  .tag {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: var(--height-tag);
    padding: 0 var(--space-2);
    border-radius: var(--radius-round);
    background: var(--color-sunken);
    color: var(--color-text-2);
    font-weight: var(--weight-medium);
    white-space: nowrap;
    transition: background var(--dur-fast) var(--ease-out);
  }

  .tag:hover {
    background: var(--color-selected);
    color: var(--color-text);
  }

  /* Taking a Tag off is offered on hover and to the keyboard, so the row stays calm. */
  .remove {
    display: grid;
    place-items: center;
    width: var(--size-tag-remove);
    height: var(--size-tag-remove);
    padding: 0;
    border: 0;
    border-radius: var(--radius-round);
    background: none;
    color: var(--color-muted);
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease-out);
  }

  .tag:hover .remove,
  .remove:focus-visible {
    opacity: 1;
  }

  .remove:hover {
    background: var(--color-line-strong);
    color: var(--color-text);
  }

  .add {
    display: flex;
    align-items: center;
    gap: var(--space-half);
    height: var(--height-tag);
    padding: 0 var(--space-2) 0 var(--space-1);
    border: var(--line) dashed var(--color-line-strong);
    border-radius: var(--radius-round);
    background: none;
    color: var(--color-muted);
    font-size: var(--text-xs);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }

  .add:hover {
    border-color: var(--color-muted);
    color: var(--color-text);
  }

  .adding {
    position: relative;
    display: block;
  }

  .new {
    height: var(--height-tag);
    padding: 0 var(--space-2);
    border: var(--line) solid var(--color-focus);
    border-radius: var(--radius-round);
    outline: 0;
    box-shadow: 0 0 0 var(--ring) var(--color-focus-soft);
    background: var(--color-surface);
    font-size: var(--text-xs);
  }

  .suggestions {
    position: absolute;
    top: calc(100% + var(--space-1));
    left: 0;
    z-index: var(--layer-menu);
    min-width: var(--width-menu-s);
    margin: 0;
    padding: var(--space-1);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
    background: var(--color-surface);
    box-shadow: var(--shadow-float);
    list-style: none;
  }

  .suggestions button {
    display: block;
    width: 100%;
    height: var(--height-row);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    color: var(--color-text);
    font-size: var(--text-s);
    text-align: left;
  }

  .suggestions button:hover,
  .suggestions button.active {
    background: var(--color-hover);
  }
</style>
