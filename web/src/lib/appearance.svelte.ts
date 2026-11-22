/*
 * Light, dark, or as the system: the person's choice of appearance. It is
 * kept in this browser, not in the Workspace, and shows as data-appearance
 * on <html>, where tokens.css sets color-scheme from it. index.html applies
 * the kept choice before the first paint with the same key and attribute.
 */

export type Appearance = 'system' | 'light' | 'dark'

/** The choices, in the order they are offered. */
export const APPEARANCES: { value: Appearance; label: string }[] = [
  { value: 'system', label: 'As the system' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
]

const KEY = 'turtle-appearance'

function kept(): Appearance {
  try {
    const value = localStorage.getItem(KEY)
    if (value === 'light' || value === 'dark') return value
  } catch {
    // Without storage nothing was kept.
  }
  return 'system'
}

export const appearance = $state({ choice: kept() })

/** Makes a choice of appearance, at once and for the next visit. */
export function chooseAppearance(choice: Appearance) {
  appearance.choice = choice
  try {
    if (choice === 'system') localStorage.removeItem(KEY)
    else localStorage.setItem(KEY, choice)
  } catch {
    // It still holds while the page is open.
  }
  const root = document.documentElement
  if (choice === 'system') delete root.dataset.appearance
  else root.dataset.appearance = choice
}
