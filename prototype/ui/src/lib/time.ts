const rtf = new Intl.RelativeTimeFormat('en', { numeric: 'auto' })

/** "just now", "4 minutes ago", "yesterday", or a date for anything older. */
export function relativeTime(at: number, now = Date.now()): string {
  const seconds = Math.round((at - now) / 1000)
  if (seconds > -45) return 'just now'
  const minutes = Math.round(seconds / 60)
  if (minutes > -60) return rtf.format(minutes, 'minute')
  const hours = Math.round(minutes / 60)
  if (hours > -24) return rtf.format(hours, 'hour')
  const days = Math.round(hours / 24)
  if (days > -7) return rtf.format(days, 'day')
  return new Date(at).toLocaleDateString('en-GB', { day: 'numeric', month: 'short' })
}

export const exactTime = (at: number) =>
  new Date(at).toLocaleString('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })
