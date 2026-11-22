/** As many Tags as a short list beside the field can show. */
const MAX_SUGGESTIONS = 6

/**
 * The text typed so far as Turtle stores a Tag: in lower case, without a
 * leading #, with spaces as hyphens. Mirrors add_tag, so what is typed is
 * compared with Tags as they are stored.
 */
function asStored(text: string): string {
  return text.trim().replace(/^#+/, '').trim().toLowerCase().split(/\s+/).filter(Boolean).join('-')
}

/**
 * The Tags to offer while one is typed: those in use, in the order given,
 * that the Page does not carry yet and that contain what is typed.
 */
export function suggestedTags(inUse: string[], carried: string[], typed: string): string[] {
  const wanted = asStored(typed)
  return inUse.filter(tag => !carried.includes(tag) && tag.includes(wanted)).slice(0, MAX_SUGGESTIONS)
}
