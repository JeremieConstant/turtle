/** How a person appears to the others on a Page. */
export type Person = { name: string; color: string }

const PERSON_COLORS = 7

/** A signed-in Member as the others see them: their name, and a colour that lasts for the browser session. */
export function currentPerson(name: string): Person {
  return { name, color: sessionColor() }
}

function sessionColor(): string {
  try {
    const stored = sessionStorage.getItem('turtle-color')
    if (stored) return stored
  } catch {
    // Storage can be unavailable; a fresh colour is fine then.
  }
  const pick = Math.floor(Math.random() * PERSON_COLORS) + 1
  const token = `--color-person-${pick}`
  // The others receive the colour itself, so it is read out of the design tokens here.
  const color = getComputedStyle(document.documentElement).getPropertyValue(token).trim()
  // Without the stylesheet the colour is only a reference; it is not kept, so the next Page reads the real one.
  if (!color) return `var(${token})`
  try {
    sessionStorage.setItem('turtle-color', color)
  } catch {
    // See above.
  }
  return color
}
