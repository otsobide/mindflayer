import { ref } from 'vue'
import { defineStore } from 'pinia'
import type { Cell } from '@/api/generated/Cell'

/** Whether two boxes are one: the same project, artifact and origin. */
function same(a: Omit<Cell, 'wanted'>, b: Omit<Cell, 'wanted'>): boolean {
  return a.project === b.project && a.reference === b.reference && a.origin === b.origin
}

/**
 * The boxes of the matrix somebody ticked or unticked and has not applied
 * yet, each as the plan takes it (`Cell`), in the order they were marked,
 * which is the order the plan's changes go in.
 *
 * What somebody asked for, which only the browser knows: the boxes as they
 * stand are the snapshot's, and what the marks amount to the plan's. Kept
 * here rather than on the page, so they outlast a look at a project's page
 * and back. A box marked back to what it showed is no mark, as a box on the
 * install screen marked twice asks for nothing.
 */
export const useMarks = defineStore('marks', () => {
  const marks = ref<Cell[]>([])

  /** The mark on the box `cell` names, if it has one. */
  function find(cell: Omit<Cell, 'wanted'>): Cell | undefined {
    return marks.value.find(mark => same(mark, cell))
  }

  /**
   * Mark the box `cell` names to be `cell.wanted`, which `ticked` says it is
   * not as the snapshot draws it; or, when it is, take its mark away.
   */
  function mark(cell: Cell, ticked: boolean) {
    const kept = marks.value.filter(each => !same(each, cell))
    marks.value = cell.wanted === ticked ? kept : [...kept, { ...cell }]
  }

  /** Forget every mark: once what they asked for has run, or by hand. */
  function clear() {
    marks.value = []
  }

  return { marks, find, mark, clear }
})
