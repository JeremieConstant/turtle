/*
 * Svelte transitions take their durations in milliseconds from JavaScript, so
 * they read them from the motion tokens in tokens.css. Reduced motion sets
 * the tokens to 0, which makes every transition here instant as well.
 */
import { cubicOut } from 'svelte/easing'
import type { TransitionConfig } from 'svelte/transition'

export function dur(token: '--dur-instant' | '--dur-fast' | '--dur-base' | '--dur-slow' | '--dur-agent'): number {
  const value = getComputedStyle(document.documentElement).getPropertyValue(token).trim()
  return parseFloat(value) || 0
}

/** A row of the tree folding away: its height closes while it fades. */
export function fold(node: HTMLElement, { delay = 0 }: { delay?: number } = {}): TransitionConfig {
  const height = node.offsetHeight
  return {
    delay,
    duration: dur('--dur-base'),
    easing: cubicOut,
    css: (t) => `height:${t * height}px;opacity:${t};overflow:hidden`,
  }
}

/** Something arriving from just below where it will rest. */
export function rise(node: HTMLElement, { delay = 0, y = 6 }: { delay?: number; y?: number } = {}): TransitionConfig {
  return {
    delay,
    duration: dur('--dur-slow'),
    easing: cubicOut,
    css: (t, u) => `opacity:${t};transform:translateY(${u * y}px)`,
  }
}

export const flipOptions = () => ({ duration: dur('--dur-slow'), easing: cubicOut })
