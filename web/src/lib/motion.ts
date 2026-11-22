/*
 * Svelte transitions take their duration in milliseconds from JavaScript, so
 * they read it from the motion tokens in tokens.css. Reduced motion sets the
 * tokens to 0, which makes these transitions instant as well.
 */
export function dur(token: '--dur-instant' | '--dur-fast' | '--dur-base' | '--dur-slow'): number {
  const value = getComputedStyle(document.documentElement).getPropertyValue(token).trim()
  return parseFloat(value) || 0
}
