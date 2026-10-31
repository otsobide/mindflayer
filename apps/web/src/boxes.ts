import type { Cell } from '@/api/generated/Cell'
import type { HeldBox } from '@/api/generated/HeldBox'
import type { Kind } from '@/api/generated/Kind'
import type { Offered } from '@/api/generated/Offered'
import type { Project } from '@/api/generated/Project'
import type { Said } from '@/api/generated/Said'
import type { Snapshot } from '@/api/generated/Snapshot'

/*
 * A box is one offer in one project, as the matrix draws it and the plan
 * takes it (`Cell`). What each box shows is the snapshot's (`boxes`); the
 * matrix and the command palette both find it there, never working out
 * which offer a project holds, which is core's (`install::holding`).
 */

/** A box, but whether it is wanted: what tells it from every other. */
export type Box = Omit<Cell, 'wanted'>

/** What names a project to a command and a box: its entry. */
export function named(project: Project): string {
  return project.entry ?? project.path
}

/** The box of `offered` in `project`: the project by its entry, the offer as a command line names it, `kind:name`, and by its origin. */
export function boxOf(project: Project, offered: Offered): Box {
  return { project: named(project), reference: `${offered.kind}:${offered.name}`, origin: offered.origin }
}

/**
 * What tells one box from another, as a key: its project, its artifact and
 * the offer's origin, apart by a character none of them holds.
 */
export function boxKey(box: Box): string {
  return `${box.project}\u0000${box.reference}\u0000${box.origin}`
}

/** The snapshot's boxes, looked up by the box they are of. */
export interface Found {
  /** The box as the snapshot lists it, when it is not empty. */
  held(box: Box): HeldBox | undefined
  /** Why nothing of `kind` can go into the project `entry` names, when nothing can. */
  closed(entry: string, kind: Kind): Said | undefined
}

/**
 * The snapshot's boxes, found by the box they are of: each one that is not
 * empty, and each kind of each project none can be asked of. Looked up,
 * never worked out.
 */
export function found(snapshot: Snapshot | undefined): Found {
  const held = new Map<string, HeldBox>()
  const closed = new Map<string, Said>()
  for (const boxes of snapshot?.boxes ?? []) {
    for (const box of boxes.held) {
      held.set(boxKey({ project: boxes.entry, reference: `${box.kind}:${box.name}`, origin: box.origin }), box)
    }
    for (const each of boxes.closed) {
      closed.set(`${boxes.entry}\u0000${each.kind}`, each.why)
    }
  }
  return {
    held: box => held.get(boxKey(box)),
    closed: (entry, kind) => closed.get(`${entry}\u0000${kind}`)
  }
}
