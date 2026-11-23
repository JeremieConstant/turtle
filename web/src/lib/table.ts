/*
 * Tables as Markdown can express them (ADR-0006): rows of cells whose first
 * row is the header, each cell holding a single paragraph. Merged cells,
 * header columns, column widths and alignment are not offered, and what is
 * pasted with them arrives without them.
 */
import type { Editor, Extensions } from '@tiptap/core'
import { Table, TableCell, TableHeader, TableRow } from '@tiptap/extension-table'
import type { EditorState } from '@tiptap/pm/state'
import { isInTable, selectedRect } from '@tiptap/pm/tables'

/** Whatever a pasted cell says about spans, widths or alignment, it is one plain cell. */
const plainCell = {
  content: 'paragraph',
  addAttributes() {
    return {
      colspan: { default: 1, parseHTML: () => 1 },
      rowspan: { default: 1, parseHTML: () => 1 },
      colwidth: { default: null, parseHTML: () => null },
      align: { default: null, parseHTML: () => null },
    }
  },
}

export const TABLE_EXTENSIONS: Extensions = [
  Table.configure({ resizable: false }),
  TableRow,
  TableHeader.extend(plainCell),
  TableCell.extend(plainCell),
]

/** How many rows and columns the table dialog lets someone ask for. */
export const TABLE_SIZE = { rows: { min: 1, max: 100, initial: 3 }, columns: { min: 1, max: 20, initial: 3 } }

/** A number typed into the table dialog, kept within what it allows. */
export function clampSize(value: number, { min, max }: { min: number; max: number }): number {
  return Number.isFinite(value) ? Math.min(max, Math.max(min, Math.round(value))) : min
}

/** Inserts a table of the given size, its first row being the header. */
export function insertTable(editor: Editor, rows: number, columns: number) {
  editor.chain().focus().insertTable({ rows, cols: columns, withHeaderRow: true }).run()
}

export type TableEdit = {
  id: string
  label: string
  /** Whether it takes something away. */
  danger?: boolean
  run: (editor: Editor) => void
}

export const TABLE_EDITS: TableEdit[] = [
  { id: 'row-above', label: 'Add row above', run: e => e.chain().focus().addRowBefore().run() },
  { id: 'row-below', label: 'Add row below', run: e => e.chain().focus().addRowAfter().run() },
  { id: 'column-before', label: 'Add column before', run: e => e.chain().focus().addColumnBefore().run() },
  { id: 'column-after', label: 'Add column after', run: e => e.chain().focus().addColumnAfter().run() },
  { id: 'delete-row', label: 'Delete row', danger: true, run: e => e.chain().focus().deleteRow().run() },
  { id: 'delete-column', label: 'Delete column', danger: true, run: e => e.chain().focus().deleteColumn().run() },
  { id: 'delete-table', label: 'Delete table', danger: true, run: e => e.chain().focus().deleteTable().run() },
]

/**
 * The edits that apply where the cursor is. The first row stays the header:
 * nothing is added above it and it is not deleted on its own; the whole
 * table is. A table keeps at least one column.
 */
export function allowedTableEdits(state: EditorState): Set<string> {
  if (!isInTable(state)) return new Set()
  const { top, left, right, map } = selectedRect(state)
  const allowed = new Set(['row-below', 'column-before', 'column-after', 'delete-table'])
  if (top > 0) {
    allowed.add('row-above')
    allowed.add('delete-row')
  }
  if (right - left < map.width) allowed.add('delete-column')
  return allowed
}
