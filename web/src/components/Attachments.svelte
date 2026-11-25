<script lang="ts">
  /*
   * The Attachments of a Page, under its content: each file with its name
   * and size, downloaded from its link. With the Write Role, files can be
   * attached from here and removed again; images pasted or dropped into
   * the content show up here too.
   */
  import { fileSize } from '../lib/attachments'
  import type { Attachment } from '../lib/operations.gen'

  const {
    attachments,
    writable,
    onattach,
    onremove,
  }: {
    attachments: Attachment[]
    writable: boolean
    /** Attaches the chosen files to the Page. */
    onattach: (files: File[]) => void
    /** Removes the Attachment, once the person has confirmed it. */
    onremove: (attachment: Attachment) => void
  } = $props()

  let picker = $state<HTMLInputElement>()

  function chosen() {
    const files = [...(picker?.files ?? [])]
    // Cleared, so choosing the same file again attaches it again.
    if (picker) picker.value = ''
    if (files.length) onattach(files)
  }
</script>

{#if attachments.length || writable}
  <section class="section stack" aria-labelledby="attachments-title">
    <div class="controls spread">
      <h2 class="section-title" id="attachments-title">Attachments</h2>
      {#if writable}
        <button type="button" class="btn" onclick={() => picker?.click()}>Attach file</button>
        <input class="visually-hidden" type="file" multiple tabindex="-1" bind:this={picker} onchange={chosen} />
      {/if}
    </div>
    {#if attachments.length}
      <ul class="list">
        {#each attachments as attachment (attachment.id)}
          <li>
            <a class="item-name" href={attachment.url} download={attachment.name}>{attachment.name}</a>
            <span class="controls">
              <span class="hint">{fileSize(attachment.size)}</span>
              {#if writable}
                <button type="button" class="btn" aria-label="Remove {attachment.name}" onclick={() => onremove(attachment)}>
                  Remove
                </button>
              {/if}
            </span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="hint">No Attachments yet. Attach a file here, or paste or drop an image into the Page.</p>
    {/if}
  </section>
{/if}
