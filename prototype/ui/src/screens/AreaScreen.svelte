<script lang="ts">
  /*
   * An Area on its own: its name, its top-level Entries, and on an empty
   * Area the one thing to do there, which is to create the first Page.
   */
  import Icon from '../components/Icon.svelte'
  import { create_page, get_area, get_tree } from '../lib/mock/ops.svelte'
  import { router } from '../lib/router.svelte'
  import { areaAddress, pageAddress } from '../lib/routes'
  import { collapsed, ui } from '../lib/ui.svelte'

  let { area_id }: { area_id: string } = $props()

  const area = $derived(get_area({ area_id }))
  const tree = $derived(area ? get_tree({ area_id }) : [])
  const writable = $derived(area?.role !== 'read')

  $effect(() => {
    if (area && router.pathname !== areaAddress(area.id, area.name)) router.replace(areaAddress(area.id, area.name))
  })

  function firstPage() {
    if (!area) return
    const id = create_page({ area_id: area.id, title: 'Untitled' })
    router.navigate(pageAddress(id, 'Untitled') + '#rename')
  }

  function openFolder(id: string) {
    ui.openArea = area_id
    collapsed.delete(id)
  }
</script>

{#if area}
  <div class="screen">
    <div class="column">
      <h1>{area.name}</h1>
      <p class="sub">
        {#if area.personal}Your own Area. Quick Notes land here.{:else if area.role === 'read'}You can read this Area.{:else}{area.role === 'admin' ? 'You are an Admin here.' : 'You can write here.'}{/if}
      </p>

      {#if tree.length}
        <ul class="entries">
          {#each tree as entry (entry.id)}
            <li>
              {#if entry.kind === 'page'}
                <a href={pageAddress(entry.id, entry.title)} onclick={(e) => router.follow(e, pageAddress(entry.id, entry.title))}>
                  <span class="glyph"></span>{entry.title}
                </a>
              {:else}
                <button onclick={() => openFolder(entry.id)}>
                  <span class="glyph"><Icon name="folder" size={15} /></span>{entry.title}
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      {:else}
        <div class="empty">
          <p>This Area has no Pages yet.</p>
          {#if writable}
            <button class="btn primary" onclick={firstPage}><Icon name="plus" size={14} /> Create the first Page</button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{:else}
  <div class="screen"><div class="column"><h1>This Area does not exist</h1></div></div>
{/if}

<style>
  .screen {
    padding: var(--space-12) var(--space-6) var(--space-16);
  }

  .column {
    max-width: var(--width-text);
    margin: 0 auto;
  }

  h1 {
    margin: 0;
    font-size: var(--text-h1);
    font-weight: var(--weight-strong);
    letter-spacing: var(--tracking-display);
    line-height: var(--leading-tight);
  }

  .sub {
    margin: var(--space-2) 0 var(--space-8);
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  .entries {
    margin: 0 calc(-1 * var(--space-3));
    padding: 0;
    list-style: none;
  }

  .entries a,
  .entries button {
    display: flex;
    align-items: center;
    width: 100%;
    height: 40px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-m);
    background: none;
    font-size: var(--text-base);
    text-align: left;
    text-decoration: none;
    transition: background var(--dur-fast) var(--ease-out);
  }

  .entries a:hover,
  .entries button:hover {
    background: var(--color-side);
  }

  .glyph {
    display: grid;
    place-items: center;
    width: 16px;
    margin-right: var(--space-3);
    color: var(--color-muted);
  }

  .empty {
    display: grid;
    justify-items: start;
    gap: var(--space-4);
    padding: var(--space-8) 0;
    border-top: var(--line) solid var(--color-line);
  }

  .empty p {
    margin: 0;
    color: var(--color-text-2);
  }

  @media (max-width: 800px) {
    .screen {
      padding: var(--space-16) var(--space-4) var(--space-12);
    }
  }
</style>
