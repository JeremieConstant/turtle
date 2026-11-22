import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import html from '../../index.html?raw'

function fakeBrowser(stored: Record<string, string> = {}) {
  const storage = new Map(Object.entries(stored))
  const root = { dataset: {} as Record<string, string | undefined> }
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => void storage.set(key, value),
    removeItem: (key: string) => void storage.delete(key),
  })
  vi.stubGlobal('document', { documentElement: root })
  return { storage, root }
}

/** A fresh copy of the module, so it reads the browser it starts in. */
const load = () => import('./appearance.svelte')

beforeEach(() => vi.resetModules())
afterEach(() => vi.unstubAllGlobals())

test('without a choice the app follows the system', async () => {
  fakeBrowser()
  const { appearance } = await load()
  expect(appearance.choice).toBe('system')
})

test('a choice made earlier in this browser is the one in force', async () => {
  fakeBrowser({ 'turtle-appearance': 'dark' })
  const { appearance } = await load()
  expect(appearance.choice).toBe('dark')
})

test('choosing dark or light keeps it in the browser and applies it to the page', async () => {
  const { storage, root } = fakeBrowser()
  const { appearance, chooseAppearance } = await load()
  chooseAppearance('dark')
  expect(appearance.choice).toBe('dark')
  expect(storage.get('turtle-appearance')).toBe('dark')
  expect(root.dataset.appearance).toBe('dark')
  chooseAppearance('light')
  expect(storage.get('turtle-appearance')).toBe('light')
  expect(root.dataset.appearance).toBe('light')
})

test('choosing the system forgets the choice and leaves the page to the system', async () => {
  const { storage, root } = fakeBrowser({ 'turtle-appearance': 'dark' })
  root.dataset.appearance = 'dark'
  const { appearance, chooseAppearance } = await load()
  chooseAppearance('system')
  expect(appearance.choice).toBe('system')
  expect(storage.has('turtle-appearance')).toBe(false)
  expect(root.dataset.appearance).toBeUndefined()
})

test('a kept value that is not an appearance counts as no choice', async () => {
  fakeBrowser({ 'turtle-appearance': 'sepia' })
  const { appearance } = await load()
  expect(appearance.choice).toBe('system')
})

test('without storage the choice still applies for as long as the page is open', async () => {
  const { root } = fakeBrowser()
  vi.stubGlobal('localStorage', {
    getItem: () => {
      throw new Error('blocked')
    },
    setItem: () => {
      throw new Error('blocked')
    },
    removeItem: () => {
      throw new Error('blocked')
    },
  })
  const { appearance, chooseAppearance } = await load()
  expect(appearance.choice).toBe('system')
  chooseAppearance('dark')
  expect(appearance.choice).toBe('dark')
  expect(root.dataset.appearance).toBe('dark')
})

/** Runs the script index.html runs before anything is painted. */
function runPrePaintScript() {
  const script = html.match(/<head>[\s\S]*?<script>([\s\S]*?)<\/script>[\s\S]*?<\/head>/)?.[1]
  if (!script) throw new Error('index.html has no script in its head')
  new Function(script)()
}

test('before the first paint the page takes on the choice kept in the browser', async () => {
  const first = fakeBrowser()
  const { chooseAppearance } = await load()
  chooseAppearance('dark')
  const { root } = fakeBrowser(Object.fromEntries(first.storage))
  runPrePaintScript()
  expect(root.dataset.appearance).toBe('dark')
})

test('before the first paint, without a choice, the page is left to the system', () => {
  const { root } = fakeBrowser()
  runPrePaintScript()
  expect(root.dataset.appearance).toBeUndefined()
})

test('before the first paint, without storage, the page is left to the system', () => {
  const { root } = fakeBrowser()
  vi.stubGlobal('localStorage', {
    getItem: () => {
      throw new Error('blocked')
    },
  })
  expect(runPrePaintScript).not.toThrow()
  expect(root.dataset.appearance).toBeUndefined()
})
