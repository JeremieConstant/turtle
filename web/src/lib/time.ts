const relative = new Intl.RelativeTimeFormat('en', { numeric: 'auto' })

/**
 * How long ago a moment was, as a person says it: "just now", "4 minutes
 * ago", "yesterday", or the date for anything older than a week. Both are
 * seconds since the Unix epoch, as Turtle reports times.
 */
export function relativeTime(at: number, now: number): string {
  const seconds = at - now
  if (seconds > -45) return 'just now'
  const minutes = Math.round(seconds / 60)
  if (minutes > -60) return relative.format(minutes, 'minute')
  const hours = Math.round(minutes / 60)
  if (hours > -24) return relative.format(hours, 'hour')
  const days = Math.round(hours / 24)
  if (days > -7) return relative.format(days, 'day')
  return new Date(at * 1000).toLocaleDateString('en-GB', { day: 'numeric', month: 'short' })
}

/** A moment to the minute, for where the relative time is not enough. */
export function exactTime(at: number): string {
  return new Date(at * 1000).toLocaleString('en-GB', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}
