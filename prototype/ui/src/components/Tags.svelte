<script lang="ts">
  /*
   * The Tags of a Page. Each one is a search for every Page that carries it;
   * with the Write Role a Tag can be added (add_tag) or taken off (remove_tag).
   */
  import { add_tag, list_tags, remove_tag } from '../lib/mock/ops.svelte'
  import { router } from '../lib/router.svelte'
  import { searchAddress } from '../lib/routes'
  import Icon from './Icon.svelte'

  let { page_id, tags, writable, id }: { page_id: string; tags: string[]; writable: boolean; id: string } = $props()

  let adding = $state(false)
  let draft = $state('')
  let input = $state<HTMLInputElement>()
  const suggestions = $derived(adding ? list_tags().filter((t) => !tags.includes(t)) : [])

  async function start() {
    adding = true
    draft = ''
    await Promise.resolve()
    input?.focus()
  }

  function commit() {
    if (draft.trim()) add_tag({ page_id, tag: draft })
    draft = ''
    adding = false
  }
</script>

<ul class="tags" aria-label="Tags">
  {#each tags as tag (tag)}
    <li class="tag">
      <a href={searchAddress('#' + tag)} onclick={(e) => router.follow(e, searchAddress('#' + tag))}>
        <span class="hash" aria-hidden="true">#</span>{tag}
      </a>
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
        <input
          class="new"
          bind:this={input}
          bind:value={draft}
          list="{id}-suggestions"
          placeholder="Tag"
          aria-label="New Tag"
          size={Math.max(4, draft.length + 1)}
          onblur={commit}
          onkeydown={(e) => {
            if (e.key === 'Enter') commit()
            if (e.key === 'Escape') ((draft = ''), (adding = false))
          }}
        />
        <datalist id="{id}-suggestions">
          {#each suggestions as s (s)}<option value={s}></option>{/each}
        </datalist>
      {:else}
        <button class="add" onclick={start} aria-label="Add a Tag" title="Add a Tag">
          <Icon name="plus" size={12} />{#if !tags.length}<span>Tag</span>{/if}
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
    gap: var(--space-1) var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-s);
  }

  .tag {
    position: relative;
    display: flex;
    align-items: center;
  }

  .tag a {
    color: var(--color-text-2);
    text-decoration: none;
    white-space: nowrap;
    transition: color var(--dur-fast) var(--ease-out);
  }

  .tag a:hover {
    color: var(--color-text);
    text-decoration: underline;
    text-decoration-color: var(--color-line-strong);
    text-underline-offset: 3px;
  }

  .hash {
    color: var(--color-faint);
  }

  /* Taking a Tag off is offered only on hover, so the row reads as words. */
  .remove {
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    margin-left: 2px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-s);
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
    background: var(--color-line);
    color: var(--color-text);
  }

  .add {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: 20px;
    padding: 0 var(--space-1);
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    color: var(--color-muted);
    font-size: var(--text-s);
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }

  .add:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .new {
    height: 20px;
    padding: 0 var(--space-1);
    border: var(--line) solid var(--color-focus);
    border-radius: var(--radius-s);
    outline: 0;
    box-shadow: 0 0 0 3px var(--color-focus-soft);
    background: var(--color-surface);
    font-size: var(--text-s);
  }
</style>
