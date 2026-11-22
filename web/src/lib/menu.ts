/*
 * Menus are popovers (ADR-0007): the browser closes them on Escape and on a
 * click elsewhere, and gives the focus back. What is left to do here is to
 * put a menu beside its trigger and to move through it with the arrow keys.
 */

const items = (menu: HTMLElement) => [...menu.querySelectorAll<HTMLElement>('[role="menuitem"]')]

/** Opens a menu at its trigger, above or below it, kept inside the window, with the first item focused. */
export function openMenu(menu: HTMLElement, trigger: HTMLElement, side: 'below' | 'above' = 'below') {
  menu.showPopover()
  const gap = parseFloat(getComputedStyle(menu).getPropertyValue('--space-1')) || 0
  const at = trigger.getBoundingClientRect()
  const size = menu.getBoundingClientRect()
  const left = Math.min(at.left, innerWidth - size.width - gap)
  const top = side === 'above' ? at.top - size.height - gap : at.bottom + gap
  menu.style.left = `${Math.max(gap, left)}px`
  menu.style.top = `${Math.max(gap, Math.min(top, innerHeight - size.height - gap))}px`
  items(menu)[0]?.focus()
}

/** Moves through an open menu: the arrow keys go to the next and previous item, around the ends. */
export function menuKeys(event: KeyboardEvent) {
  const step = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0
  if (step === 0) return
  event.preventDefault()
  const all = items(event.currentTarget as HTMLElement)
  const at = all.indexOf(document.activeElement as HTMLElement)
  all[(at + step + all.length) % all.length]?.focus()
}
