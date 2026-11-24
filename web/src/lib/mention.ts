import { Node } from '@tiptap/core'
import { Plugin } from '@tiptap/pm/state'
import { mount, unmount } from 'svelte'
import Avatar from '../components/Avatar.svelte'
import { Lookup } from './lookup'
import type { MentionedPerson, Profile } from './operations.gen'
import { call } from './ops'
import { router } from './router.svelte'
import { profileAddress } from './routes'

/** How a Mention is written as a link address: `turtle:member/` and the person's ID, as in its Markdown (ADR-0013). */
export const SCHEME = 'turtle:member/'

/** The ID of the person a link address mentions, when it is a Mention; as the server reads it. */
export function personIdOf(address: string): string | null {
  const id = address.startsWith(SCHEME) ? address.slice(SCHEME.length) : ''
  return /^[A-Za-z0-9_-]+$/.test(id) ? id : null
}

/** How a Mention is drawn. */
export type Badge =
  /** Someone whose Profile the reader sees: their mark and name, leading to the Profile. */
  | { state: 'profile'; name: string; address: string; active: boolean }
  /** Someone whose Profile the reader may not see: the name alone, leading nowhere. */
  | { state: 'name'; name: string }
  /** An ID that names no Member or Guest. */
  | { state: 'unknown' }
  /** Asking for the name failed; it is asked again when the window comes back into view. */
  | { state: 'failed' }

/** How a Mention of the given person is drawn; `null` for nobody known, `undefined` when asking failed. */
export function badgeOf(person: MentionedPerson | null | undefined): Badge {
  if (person === undefined) return { state: 'failed' }
  if (person === null) return { state: 'unknown' }
  const { profile } = person
  if (!profile) return { state: 'name', name: person.name }
  return { state: 'profile', name: profile.name, address: profileAddress(profile.id, profile.name), active: profile.active }
}

/** The most people the picker offers at once. */
const CHOICES = 8

/** Text compared without case or accents, so "emile" finds "Émile". */
const folded = (text: string) => text.normalize('NFD').replace(/\p{M}/gu, '').toLocaleLowerCase()

/**
 * Whom the `@` picker offers for what was typed: the people whose Profiles the
 * writer sees (as `list_profiles` gives them, so never an Agent), whose name
 * or email holds it. A deactivated person is no longer offered.
 */
export function choices(people: Profile[], query: string): Profile[] {
  const wanted = folded(query.trim())
  return people
    .filter(person => person.active && (folded(person.name).includes(wanted) || folded(person.email).includes(wanted)))
    .slice(0, CHOICES)
}

/** The people that the Mentions on screen name, asked for together. */
function people(): Lookup<MentionedPerson> {
  return new Lookup(async ids => {
    const answer = await call('get_mentioned_people', { person_ids: ids })
    return new Map(answer.people.map(person => [person.id, person]))
  })
}

type MentionStorage = {
  people: Lookup<MentionedPerson>
  /** Set by whatever shows the picker: called with where `@` went in when it is typed where a word starts. */
  ask: ((at: number) => void) | null
}

declare module '@tiptap/core' {
  interface Storage {
    mention: MentionStorage
  }
}

/**
 * A Mention of a Member or Guest by their ID, drawn as a badge with their
 * picture or initials and current name that opens their Profile; to someone
 * who may not see the Profile, the name alone. Never shown as a raw link.
 */
export const Mention = Node.create<Record<string, never>, MentionStorage>({
  name: 'mention',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  // The person's name is its whole text, so it carries no formatting (as in Markdown).
  marks: '',

  addStorage() {
    // One per editor, so opening a Page asks for its people afresh.
    return { people: people(), ask: null }
  },

  onDestroy() {
    this.storage.people.stop()
  },

  addAttributes() {
    return {
      personId: {
        default: null,
        parseHTML: element => personIdOf(element.getAttribute('href') ?? ''),
        renderHTML: attributes => ({ href: `${SCHEME}${attributes.personId}` }),
      },
    }
  },

  parseHTML() {
    // Before the Page link, whose `turtle:` addresses include these.
    return [{ tag: `a[href^="${SCHEME}"]`, priority: 60 }]
  },

  renderHTML({ HTMLAttributes }) {
    return ['a', { ...HTMLAttributes, 'data-mention': '' }]
  },

  addProseMirrorPlugins() {
    const editor = this.editor
    return [
      new Plugin({
        props: {
          // Typed where a word starts, `@` opens the picker. The `@` goes in
          // as typed and stays if nothing is chosen; a choice takes its place.
          handleTextInput: (view, from, _to, text) => {
            const ask = editor.storage.mention.ask
            if (!ask || !text.endsWith('@')) return false
            // Quick typing can arrive as several characters at once.
            const before = text.length > 1 ? text.at(-2) : view.state.doc.textBetween(Math.max(0, from - 1), from, ' ', ' ')
            if (!before || /\s/.test(before)) queueMicrotask(() => ask(from + text.length - 1))
            return false
          },
        },
      }),
    ]
  },

  addNodeView() {
    return ({ node }) => {
      const personId = String(node.attrs.personId)
      const element = document.createElement('a')
      element.classList.add('mention', 'loading')
      element.dataset.mention = ''
      const mark = document.createElement('span')
      mark.classList.add('mention-mark')
      const name = document.createElement('span')
      name.textContent = '@…'
      element.append(mark, name)
      let avatar: ReturnType<typeof mount> | null = null
      let address: string | null = null
      // Followed with a click, while editing too: being one piece, it is selected with the keyboard instead.
      element.addEventListener('click', event => {
        if (address) router.follow(event, address)
        else event.preventDefault()
      })
      const showMark = (person: string | null) => {
        if (avatar) void unmount(avatar)
        avatar = person === null ? null : mount(Avatar, { target: mark, props: { name: person, size: 20 } })
      }
      const stop = this.storage.people.watch(personId, person => {
        const badge = badgeOf(person)
        element.className = `mention ${badge.state}`
        address = badge.state === 'profile' ? badge.address : null
        if (address) element.href = address
        else element.removeAttribute('href')
        element.classList.toggle('deactivated', badge.state === 'profile' && !badge.active)
        showMark(badge.state === 'profile' ? badge.name : null)
        name.textContent = badge.state === 'profile' || badge.state === 'name' ? badge.name : '@?'
        if (badge.state === 'unknown') element.title = 'This person is not known in this Workspace.'
        else if (badge.state === 'failed')
          element.title = 'The name could not be loaded. Check your connection; it is tried again when you come back to this window.'
        else if (badge.state === 'profile' && !badge.active) element.title = 'Deactivated'
        else element.removeAttribute('title')
      })
      return {
        dom: element,
        destroy: () => {
          stop()
          showMark(null)
        },
      }
    }
  },
})
