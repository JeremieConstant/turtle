/**
 * How a person appears to the others on a Page: their name, and which of the
 * colours people are told apart by is theirs, from 1 to PERSON_COLORS. The
 * number is sent, not the colour, so the others show it in their own
 * appearance and everyone still names it the same.
 */
export type Person = { name: string; colorNumber: number }

/**
 * Who is on a Page as the others see them: a person, or an Agent that just
 * changed it, with the Member it acts for unless it is a standalone Agent.
 * Turtle announces Agents itself, without a colour or a cursor.
 */
export type Present = Person | { name: string; agent: true; member?: string }

/** How many --color-person-* tokens there are. */
const PERSON_COLORS = 6
const KEY = 'turtle-color-number'

/** A signed-in Member as the others see them: their name, and a colour that lasts for the browser session. */
export function currentPerson(name: string): Person {
  return { name, colorNumber: sessionColorNumber() }
}

/**
 * The colour to show a person in. What arrives from another browser is not
 * trusted, so anything but a colour number becomes a quiet colour.
 */
export function personColor(person: { colorNumber?: unknown }): string {
  return isColorNumber(person.colorNumber) ? `var(--color-person-${person.colorNumber})` : 'var(--color-muted)'
}

function isColorNumber(value: unknown): value is number {
  return Number.isInteger(value) && (value as number) >= 1 && (value as number) <= PERSON_COLORS
}

function sessionColorNumber(): number {
  try {
    const stored = Number(sessionStorage.getItem(KEY) ?? NaN)
    if (isColorNumber(stored)) return stored
  } catch {
    // Storage can be unavailable; a fresh colour is fine then.
  }
  const pick = Math.floor(Math.random() * PERSON_COLORS) + 1
  try {
    sessionStorage.setItem(KEY, String(pick))
  } catch {
    // See above.
  }
  return pick
}
