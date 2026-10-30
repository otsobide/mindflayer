import type { Held } from '@/api/generated/Held'
import type { Project } from '@/api/generated/Project'
import type { Snapshot } from '@/api/generated/Snapshot'

/** A project holding one of the library's own, and its copy there. */
export interface Holding {
  project: Project
  copy: Held
}

/**
 * Every project holding `held`, one of the library's own, with its copy
 * there, in the registry's order: each project core names as holding it
 * (`library.holders`), found by its entry, and its artifact of that kind
 * and name. Which copies are the library's is core's to tell, never a
 * comparison of a copy's origin here: an install recorded before origins
 * were kept names none, and core still takes it for the library's.
 */
export function holding(snapshot: Snapshot, held: Pick<Held, 'kind' | 'name'>): Holding[] {
  const holders = snapshot.library.holders.filter(holder => holder.kind === held.kind && holder.name === held.name)
  return snapshot.projects.flatMap(project =>
    holders.some(holder => holder.entry === project.entry)
      ? project.artifacts.filter(copy => copy.kind === held.kind && copy.name === held.name).map(copy => ({ project, copy }))
      : []
  )
}
