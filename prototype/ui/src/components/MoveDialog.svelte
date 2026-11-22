<script lang="ts">
  /*
   * Moves an Entry with its Subtree, also into another Area. Without a query
   * the list shows every Area the Member can write to with its Folders; typing
   * finds any Page or Folder by title. One click moves (move_entry), and the
   * toast that follows can undo it.
   */
  import { moveTo, type Destination } from '../lib/actions'
  import { get_tree, list_areas } from '../lib/mock/ops.svelte'
  import type { Entry, TreeEntry } from '../lib/mock/types'
  import Icon from './Icon.svelte'

  type Option = Destination & { key: string; title: string; depth: number; kind: 'area' | 'folder' | 'page'; where?: string; here: boolean }

  let dialog: HTMLDialogElement
  let entry = $state<Entry | null>(null)
  let query = $state('')
  let active = $state(0)

  export function open(target: Entry) {
    entry = target
    query = ''
    active = 0
    dialog.showModal()
  }

  const options = $derived.by((): Option[] => {
    if (!entry) return []
    const moving = entry
    const q = query.trim().toLowerCase()
    const out: Option[] = []
    for (const area of list_areas()) {
      if (area.role === 'read') continue
      const here = (parent: string | null) => moving.area_id === area.id && moving.parent_id === parent
      if (!q || area.name.toLowerCase().includes(q))
        out.push({ key: area.id, area_id: area.id, parent_id: null, title: area.name, depth: 0, kind: 'area', here: here(null) })
      const walk = (list: TreeEntry[], depth: number, trail: string[]) => {
        for (const e of list) {
          if (e.id === moving.id) continue // never into its own Subtree
          const match = q ? e.title.toLowerCase().includes(q) : e.kind === 'folder'
          if (match)
            out.push({
              key: e.id,
              area_id: area.id,
              parent_id: e.id,
              title: e.title,
              depth: q ? 0 : depth,
              kind: e.kind,
              where: q ? [area.name, ...trail].join(' / ') : undefined,
              here: here(e.id),
            })
          walk(e.children, depth + 1, [...trail, e.title])
        }
      }
      walk(get_tree({ area_id: area.id }), 1, [])
    }
    return out.slice(0, 80)
  })

  $effect(() => {
    void query
    active = 0
  })

  function pick(option: Option) {
    if (!entry || option.here) return
    moveTo(entry, option)
    dialog.close()
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      const step = event.key === 'ArrowDown' ? 1 : -1
      active = (active + step + options.length) % Math.max(1, options.length)
      dialog.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' })
    } else if (event.key === 'Enter' && options[active]) {
      event.preventDefault()
      pick(options[active])
    }
  }
</script>

<dialog bind:this={dialog} closedby="any" aria-label="Move {entry?.title ?? ''}">
  <div class="head">
    <p class="what">Move <strong>{entry?.title}</strong></p>
    <label class="find">
      <Icon name="search" size={14} />
      <!-- svelte-ignore a11y_autofocus -->
      <input
        bind:value={query}
        placeholder="Area, Folder or Page"
        aria-label="Find a place"
        aria-controls="move-options"
        autofocus
        {onkeydown}
      />
    </label>
  </div>
  <ul class="options" id="move-options" role="listbox" aria-label="Places">
    {#each options as option, i (option.key)}
      <li role="option" aria-selected={i === active} aria-disabled={option.here}>
        <button
          class="option"
          class:area={option.kind === 'area'}
          class:active={i === active}
          disabled={option.here}
          style:--depth={option.depth}
          data-index={i}
          tabindex="-1"
          onclick={() => pick(option)}
          onpointermove={() => (active = i)}
        >
          <span class="glyph">{#if option.kind === 'folder'}<Icon name="folder" size={15} />{/if}</span>
          <span class="title">{option.title}</span>
          {#if option.where}<span class="where">{option.where}</span>{/if}
          {#if option.here}<span class="where">Here now</span>{/if}
        </button>
      </li>
    {:else}
      <li class="none">Nothing found</li>
    {/each}
  </ul>
</dialog>

<style>
  dialog {
    width: min(440px, calc(100vw - 2 * var(--space-4)));
    background: var(--color-surface);
  }

  dialog[open] {
    animation: pop var(--dur-base) var(--ease-out);
  }

  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(4px) scale(0.98);
    }
  }

  .head {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-3);
    border-bottom: var(--line) solid var(--color-line);
  }

  .what {
    margin: 0;
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--text-s);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .what strong {
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .find {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 34px;
    padding: 0 var(--space-3);
    border: var(--line) solid var(--color-line-strong);
    border-radius: var(--radius-m);
    color: var(--color-muted);
  }

  .find:focus-within {
    border-color: var(--color-focus);
    box-shadow: 0 0 0 3px var(--color-focus-soft);
  }

  .find input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--color-text);
    font-size: var(--text-base);
  }

  .options {
    max-height: min(360px, 60vh);
    margin: 0;
    padding: var(--space-2);
    overflow-y: auto;
    list-style: none;
    scrollbar-width: thin;
  }

  .option {
    display: flex;
    align-items: center;
    width: 100%;
    height: 32px;
    padding: 0 var(--space-2) 0 calc(var(--space-2) + var(--depth) * var(--indent));
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    color: var(--color-text-2);
    font-size: var(--text-m);
    text-align: left;
  }

  .option.area {
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .option.active:not(:disabled) {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .option:disabled {
    cursor: default;
    color: var(--color-faint);
  }

  .glyph {
    flex: none;
    display: grid;
    place-items: center;
    width: 16px;
    margin-right: var(--space-2);
    color: var(--color-muted);
  }

  .option.area .glyph {
    display: none;
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .where {
    flex: none;
    max-width: 45%;
    margin-left: auto;
    padding-left: var(--space-3);
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--text-s);
    font-weight: var(--weight-normal);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .none {
    padding: var(--space-4);
    color: var(--color-muted);
    font-size: var(--text-s);
    text-align: center;
  }
</style>
