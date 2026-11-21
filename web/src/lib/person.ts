/** How a person appears to the others on a Page. */
export type Person = { name: string; color: string }

const PERSON_COLORS = 7

/** Until there are accounts, everyone is a visitor with a name and colour that last for the browser session. */
export function currentPerson(): Person {
  try {
    const stored = sessionStorage.getItem('turtle-user')
    if (stored) return JSON.parse(stored)
  } catch {
    // Storage can be unavailable; a fresh identity is fine then.
  }
  const pick = Math.floor(Math.random() * PERSON_COLORS) + 1
  const token = `--color-person-${pick}`
  // The others receive the colour itself, so it is read out of the design tokens here.
  const color = getComputedStyle(document.documentElement).getPropertyValue(token).trim()
  const person = { name: `Visitor ${Math.floor(Math.random() * 900) + 100}`, color: color || `var(${token})` }
  // Without the stylesheet the colour is only a reference; it is not kept, so the next Page reads the real one.
  if (!color) return person
  try {
    sessionStorage.setItem('turtle-user', JSON.stringify(person))
  } catch {
    // See above.
  }
  return person
}
