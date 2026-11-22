<script lang="ts">
  /*
   * The Tags of a Page. Each one is a search for every Page that carries it;
   * with the Write Role a Tag can be added (add_tag) or taken off (remove_tag).
   * Tags are drawn as small grey pills, so they never read as part of the path.
   */
  import { add_tag, list_tags, remove_tag } from '../lib/mock/ops.svelte'
  import { router } from '../lib/router.svelte'
  import { searchAddress } from '../lib/routes'
  import Icon from './Icon.svelte'

  let { page_id, tags, writable, id }: { page_id: string; tags: string[]; writable: boolean; id: string } = $props()

  let adding = $state(false)
  let draft = $state('')
  let input = $state<HTMLInputElement>()
  let active = $state(-1)
  const suggestions = $derived(
    adding
      ? list_tags()
          .filter((t) => !tags.includes(t) && t.includes(draft.trim().toLowerCase().replace(/^#/, '')))
          .slice(0, 6)
      : [],
  )

  async function start() {
    adding = true
    draft = ''
    await Promise.resolve()
    input?.focus()
  }

  function commit(tag = draft) {
    if (tag.trim()) add_tag({ page_id, tag })
    draft = ''
    adding = false
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      const n = suggestions.length
      if (n) active = (active + (event.key === 'ArrowDown' ? 1 : n - 1) + (active < 0 && event.key === 'ArrowUp' ? 1 : 0)) % n
    } else if (event.key === 'Enter') {
      event.preventDefault()
      commit(active >= 0 && suggestions[active] ? suggestions[active] : draft)
    } else if (event.key === 'Escape') {
      draft = ''
      adding = false
    }
  }
</script>

<ul class="tags" aria-label="Tags">
  {#each tags as tag (tag)}
    <li class="tag">
      <a href={searchAddress('#' + tag)} title="Pages tagged {tag}" onclick={(e) => router.follow(e, searchAddress('#' + tag))}>{tag}</a>
      {#if writable}
        <button class="remove" aria-label="Remove the Tag {tag}" onclick={() => remove_tag({ page_id, tag })}>
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
            aria-autocomplete="list"
            aria-controls="{id}-suggestions"
            size={Math.max(4, draft.length + 1)}
            oninput={() => (active = -1)}
            onblur={() => commit()}
            {onkeydown}
          />
          {#if suggestions.length}
            <ul class="suggestions" id="{id}-suggestions" role="listbox" aria-label="Tags in use">
              {#each suggestions as tag, i (tag)}
                <li role="option" aria-selected={i === active}>
                  <button class:active={i === active} tabindex="-1" onmousedown={(e) => (e.preventDefault(), commit(tag))}>{tag}</button>
                </li>
              {/each}
            </ul>
          {/if}
        </span>
      {:else}
        <button class="add" onclick={start} aria-label="Add a Tag" title="Add a Tag">
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
    gap: var(--space-1) var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-xs);
  }

  .tag {
    display: flex;
    align-items: center;
    height: 22px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-round);
    background: var(--color-sunken);
    transition: background var(--dur-fast) var(--ease-out);
  }

  .tag:hover {
    background: var(--color-selected);
  }

  .tag a {
    color: var(--color-text-2);
    font-weight: var(--weight-medium);
    text-decoration: none;
    white-space: nowrap;
  }

  .tag:hover a {
    color: var(--color-text);
  }

  /* Taking a Tag off is offered only on hover, so the row stays calm. */
  .remove {
    display: grid;
    place-items: center;
    width: 0;
    height: 14px;
    margin-left: 0;
    padding: 0;
    border: 0;
    border-radius: var(--radius-round);
    background: none;
    color: var(--color-muted);
    overflow: hidden;
    opacity: 0;
    transition:
      opacity var(--dur-fast) var(--ease-out),
      width var(--dur-fast) var(--ease-out),
      margin var(--dur-fast) var(--ease-out);
  }

  .tag:hover .remove,
  .remove:focus-visible {
    width: 14px;
    margin-left: var(--space-1);
    margin-right: -4px;
    opacity: 1;
  }

  .remove:hover {
    background: var(--color-line-strong);
    color: var(--color-text);
  }

  .add {
    display: flex;
    align-items: center;
    gap: 3px;
    height: 22px;
    padding: 0 var(--space-2) 0 6px;
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
    height: 22px;
    padding: 0 var(--space-2);
    border: var(--line) solid var(--color-focus);
    border-radius: var(--radius-round);
    outline: 0;
    box-shadow: 0 0 0 3px var(--color-focus-soft);
    background: var(--color-surface);
    font-size: var(--text-xs);
  }

  .suggestions {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: var(--layer-menu);
    min-width: 160px;
    margin: 0;
    padding: var(--space-1);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
    background: var(--color-surface);
    box-shadow: var(--shadow-float);
    list-style: none;
    animation: suggest-in var(--dur-fast) var(--ease-out);
  }

  @keyframes suggest-in {
    from {
      opacity: 0;
      transform: translateY(-3px);
    }
  }

  .suggestions button {
    display: block;
    width: 100%;
    height: 28px;
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
