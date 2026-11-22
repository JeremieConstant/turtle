/*
 * The one question the app may be asking right now, before an action that
 * cannot be taken back or that people should not take by accident.
 * ConfirmDialog shows it; screens only call ask() and wait for the answer.
 */

export interface Question {
  /** The question itself, naming what it acts on: "Revoke CI pipeline?" */
  title: string
  /** What follows from saying yes. */
  message: string
  /** What the button that says yes says: "Revoke". */
  action: string
  /** Set for an action that removes or ends something, so its button looks like it. */
  destructive?: boolean
}

export const confirmation = $state<{ question: Question | null }>({ question: null })

let settle: ((yes: boolean) => void) | null = null

/** Asks a question and says whether the person confirmed it. A new question cancels one still open. */
export function ask(question: Question): Promise<boolean> {
  answer(false)
  confirmation.question = question
  return new Promise(resolve => (settle = resolve))
}

/** Answers the open question, if there is one. */
export function answer(yes: boolean) {
  const open = settle
  settle = null
  confirmation.question = null
  open?.(yes)
}
