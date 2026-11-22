// ADR-0007: every dialog, confirmations included, is a <dialog> dressed in the
// tokens. A question goes through ask() in lib/confirm, which opens
// ConfirmDialog; these tests keep the browser's own popups out of the app.

import { readdirSync, readFileSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'vitest'

const SOURCE = fileURLToPath(new URL('../src', import.meta.url))

/** A call of the browser's own confirm, alert or prompt, with or without window. in front. */
const POPUP = /(?<![\w.]|[\w$]\.)(?:window\.)?(?:confirm|alert|prompt)\s*\(/g

test('the rule finds the browser popups and nothing named like them', () => {
  const code = 'if (!confirm(a)) x(); window.alert(b); prompt (c); ask(d); toast.confirm(e); reconfirm(f)'
  expect(code.match(POPUP)).toEqual(['confirm(', 'window.alert(', 'prompt ('])
})

test('nothing in the app asks with the browser popups', () => {
  const popups = readdirSync(SOURCE, { recursive: true, encoding: 'utf8' })
    .filter(file => /\.(svelte|ts)$/.test(file) && !file.endsWith('.test.ts'))
    .flatMap(file =>
      [...readFileSync(join(SOURCE, file), 'utf8').matchAll(POPUP)].map(match => `${relative(SOURCE, join(SOURCE, file))}: ${match[0]}`),
    )
  expect(popups).toEqual([])
})
