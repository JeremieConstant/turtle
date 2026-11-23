<script lang="ts">
  import { toast } from 'svelte-sonner'
  import { ask } from '../lib/confirm.svelte'
  import type { Connecting } from '../lib/operations.gen'
  import { call, OperationError } from '../lib/ops'
  import { router } from '../lib/router.svelte'
  import { session } from '../lib/session.svelte'

  $effect(() => {
    document.title = 'Connect AI · Turtle'
  })

  let connecting = $state<Connecting | null>(null)
  let failed = $state(false)
  call('describe_connecting', {})
    .then(found => (connecting = found))
    .catch(() => (failed = true))

  let switching = $state(false)
  let switchError = $state<string | null>(null)

  /** Turns connecting AI on or off for the whole Workspace, as a Workspace Admin. */
  async function setConnectingAi(on: boolean) {
    if (!on) {
      const sure = await ask({
        title: 'Turn off connecting AI?',
        message:
          'No AI client can connect to Turtle, and connected apps stop working for everyone until it is on again. Tokens made by hand keep working.',
        action: 'Turn off',
        destructive: true,
      })
      if (!sure) return
    }
    switching = true
    switchError = null
    try {
      await call('set_connecting_ai', { on })
      connecting = await call('describe_connecting', {})
    } catch (caught) {
      switchError = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      switching = false
    }
  }

  const CLIENTS = [
    { id: 'claude', label: 'Claude' },
    { id: 'claude-code', label: 'Claude Code' },
    { id: 'other', label: 'Other' },
  ] as const
  type Client = (typeof CLIENTS)[number]['id']

  let chosen = $state<Client>('claude')
  let tabs = $state<HTMLElement[]>([])

  /** Arrow keys move between the tabs, as in any tab list. */
  function tabKeys(event: KeyboardEvent, index: number) {
    const step = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0
    if (step === 0) return
    event.preventDefault()
    const next = (index + step + CLIENTS.length) % CLIENTS.length
    chosen = CLIENTS[next].id
    tabs[next]?.focus()
  }

  let addressField = $state<HTMLInputElement | null>(null)

  /**
   * The clipboard is missing outside secure origins, such as Turtle on an
   * intranet over http, and may be refused. Copying the selected field still
   * works there; failing that, the address is left selected to copy by hand.
   */
  async function copy(address: string) {
    try {
      await navigator.clipboard.writeText(address)
      toast('Address copied')
      return
    } catch {
      // Tried again below, through the field.
    }
    addressField?.select()
    if (document.execCommand('copy')) toast('Address copied')
    else toast('The address could not be copied. Copy it from the field by hand.')
  }
</script>

<main class="screen">
  <h1>Connect AI</h1>
  <p class="lede">
    Let an AI client such as Claude read and write in Turtle as you. It never has more access than you, and what it changes shows as
    yours through the app.
  </p>

  <!-- Notices about connecting, such as why it may not work from here, go first, above the steps. -->
  {#if failed}
    <p class="error" role="alert">How to connect could not be loaded. Reload the page to try again.</p>
  {:else if connecting?.turned_off}
    <section class="notice">
      <strong>A Workspace Admin has turned off connecting AI</strong>
      <p>No AI client can connect to Turtle for now, and apps already connected do not work until it is turned on again.</p>
      {#if session.member?.workspace_admin}
        <p>Turn it on again below when you are ready.</p>
      {:else}
        <p>Ask a Workspace Admin if you need it.</p>
      {/if}
    </section>
  {:else if connecting && !connecting.mcp_address}
    <section class="notice">
      <strong>Connecting an AI client is not available yet</strong>
      <p>This Turtle does not know the address people open it at, so an AI client could not sign in to it.</p>
      {#if session.member?.workspace_admin}
        <p>
          Set <code>TURTLE_PUBLIC_URL</code> to that address, such as <code>https://turtle.example.org</code>, and restart Turtle.
        </p>
      {:else}
        <p>Ask a Workspace Admin to set Turtle’s public address.</p>
      {/if}
    </section>
  {/if}

  {#if connecting?.mcp_address && !connecting.turned_off}
    {@const address = connecting.mcp_address}
    <section class="section">
      <div class="field">
        <span id="mcp-address">Turtle’s MCP address</span>
        <div class="controls">
          <input
            class="input grow"
            readonly
            aria-labelledby="mcp-address"
            value={address}
            bind:this={addressField}
            onfocus={event => event.currentTarget.select()}
          />
          <button type="button" class="btn" onclick={() => copy(address)}>Copy</button>
        </div>
      </div>
    </section>

    <section class="section">
      <div class="tabs" role="tablist" aria-label="AI client">
        {#each CLIENTS as client, index (client.id)}
          <button
            type="button"
            role="tab"
            id="tab-{client.id}"
            aria-controls="steps-{client.id}"
            aria-selected={chosen === client.id}
            tabindex={chosen === client.id ? 0 : -1}
            bind:this={tabs[index]}
            onclick={() => (chosen = client.id)}
            onkeydown={event => tabKeys(event, index)}
          >
            {client.label}
          </button>
        {/each}
      </div>

      <div class="steps" role="tabpanel" id="steps-{chosen}" aria-labelledby="tab-{chosen}">
        {#if chosen === 'claude'}
          <p class="muted">For Claude on the web and the Claude Desktop app.</p>
          <ol>
            <li>In Claude, open <strong>Settings</strong>, then <strong>Connectors</strong>.</li>
            <li>Choose <strong>Add custom connector</strong>.</li>
            <li>Name it <strong>Turtle</strong> and paste the address above. Choose <strong>Add</strong>.</li>
            <li>Choose <strong>Connect</strong> next to Turtle. Turtle opens in your browser.</li>
            <li>Sign in to Turtle if it asks, then choose <strong>Allow</strong>.</li>
          </ol>
        {:else if chosen === 'claude-code'}
          <ol>
            <li>In a terminal, run:<pre><code>claude mcp add --transport http turtle {address}</code></pre></li>
            <li>Start Claude Code and type <code>/mcp</code>. Choose <strong>turtle</strong>, then <strong>Authenticate</strong>.</li>
            <li>Turtle opens in your browser. Sign in if it asks, then choose <strong>Allow</strong>.</li>
          </ol>
          <p class="muted">You sign in once; Claude Code stays connected after that.</p>
        {:else}
          <ol>
            <li>Any AI client that speaks MCP and signs in with OAuth can connect. Add an MCP server in it and paste the address above.</li>
            <li>When it opens Turtle in your browser, sign in if it asks, then choose <strong>Allow</strong>.</li>
          </ol>
          <p class="muted">
            A client that cannot sign in that way can use a Token instead. Create one under
            <a href="/settings/connected-apps#for-developers" onclick={event => router.follow(event, '/settings/connected-apps#for-developers')}
              >Connected apps</a
            >, and give the client the
            address above with the Token as <code>Authorization: Bearer …</code>
          </p>
        {/if}
      </div>
    </section>
  {/if}

  {#if connecting && session.member?.workspace_admin}
    <section class="section stack">
      <h2 class="section-title">For Workspace Admins</h2>
      {#if connecting.turned_off}
        <p class="muted">Connecting AI is off for the whole Workspace. Turned on again, connected apps work as before.</p>
        <div class="controls">
          <button type="button" class="btn primary" disabled={switching} onclick={() => setConnectingAi(true)}>Turn on connecting AI</button>
        </div>
      {:else}
        <p class="muted">
          You can turn connecting AI off for the whole Workspace, for example while a policy is being decided. Connected apps stop working
          until it is on again; Tokens made by hand keep working.
        </p>
        <div class="controls">
          <button type="button" class="btn" disabled={switching} onclick={() => setConnectingAi(false)}>Turn off connecting AI</button>
        </div>
      {/if}
      {#if switchError}
        <p class="error" role="alert">{switchError}</p>
      {/if}
    </section>
  {/if}
</main>

<style>
  .tabs {
    display: flex;
    gap: var(--space-1);
    border-bottom: var(--line) solid var(--color-line);
  }

  [role='tab'] {
    padding: var(--space-2) var(--space-3);
    border: none;
    border-bottom: var(--line-thick) solid transparent;
    margin-bottom: calc(-1 * var(--line));
    background: none;
    color: var(--color-muted);
    font: inherit;
    font-size: var(--text-m);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  [role='tab']:hover {
    color: var(--color-text);
  }

  [role='tab'][aria-selected='true'] {
    border-bottom-color: var(--color-text);
    color: var(--color-text);
  }

  .steps {
    padding-top: var(--space-4);
  }

  ol {
    display: grid;
    gap: var(--space-3);
    margin: 0;
    padding-left: var(--space-6);
  }

  .steps > .muted {
    margin: var(--space-4) 0 0;
  }

  .steps > .muted:first-child {
    margin: 0 0 var(--space-4);
  }

  pre {
    margin: var(--space-2) 0 0;
    padding: var(--space-3);
    border-radius: var(--radius-m);
    background: var(--color-sunken);
    overflow-x: auto;
  }
</style>
