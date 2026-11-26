/** How many colours an Area's mark can have: `--color-area-1` to `--color-area-6` in tokens.css. */
export const AREA_COLORS = 6

/**
 * The colour of an Area's mark, from 1 to `AREA_COLORS`. It comes from the
 * Area's ID, so it stays the same after a rename and is the same for everyone.
 */
export function areaColor(areaId: string): number {
  let hash = 0
  for (const char of areaId) hash = (hash * 31 + char.codePointAt(0)!) >>> 0
  return (hash % AREA_COLORS) + 1
}

/** The letter on an Area's mark: the first character of its name that is a letter or digit, as a capital. */
export function areaInitial(name: string): string {
  const found = name.match(/[\p{L}\p{N}]/u)
  return found ? found[0].toLocaleUpperCase() : '·'
}
