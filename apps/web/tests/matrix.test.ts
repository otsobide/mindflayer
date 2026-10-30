import { afterEach, describe, expect, it } from 'vitest'
import type { Kind } from '@/api/generated/Kind'
import type { Snapshot } from '@/api/generated/Snapshot'
import { fixture, render, unmount } from './panel'

afterEach(unmount)

/** How many projects and offers the matrix is held to drawing a screen of. */
const PROJECTS = 50
const OFFERS = 200

/** Where the offers come from, each a group of the matrix's. */
const ORIGINS = ['workspace', 'load:../team', 'https://github.com/acme/skills', 'https://github.com/acme/rules']

/**
 * The assigning fixture grown to `projects` projects by `offers` offers, as
 * a workspace with a large library, a few loads and a busy shelf would
 * answer: every project made from its first, every offer from its first, a
 * seventh of the boxes holding something, a few of them somebody's.
 */
function grown(projects = PROJECTS, offers = OFFERS): Snapshot {
  const snapshot = fixture('assigning')
  const [project] = snapshot.projects
  const [offered] = snapshot.offers
  snapshot.offers = Array.from({ length: offers }, (_, index) => ({
    ...offered!,
    kind: (index % 5 === 4 ? 'rule' : 'skill') as Kind,
    name: `artifact-${String(index).padStart(3, '0')}`,
    summary: `What artifact ${index} is for`,
    origin: ORIGINS[index % ORIGINS.length]!
  }))
  snapshot.projects = Array.from({ length: projects }, (_, index) => {
    const name = `project-${String(index).padStart(2, '0')}`
    return { ...project!, entry: name, name, path: `/home/you/Assign/${name}` }
  })
  snapshot.boxes = snapshot.projects.map((each, at) => ({
    entry: each.entry!,
    closed: [],
    held: snapshot.offers
      .filter((_, index) => (index + at) % 7 === 0)
      .map((one, index) => ({ kind: one.kind, name: one.name, origin: one.origin, state: index % 9 === 0 ? 'foreign' : 'current', ticked: true }))
  }))
  return snapshot
}

/** How many rows of the grid are in the document: its heading's, its groups' and its lines'. */
function rows(): number {
  return document.querySelectorAll('[data-testid="matrix"] [role="row"]').length
}

describe(`a matrix of ${PROJECTS} projects by ${OFFERS} offers`, () => {
  it('holds at most 100 rows in the document, either way round, and says how many there are', async () => {
    const snapshot = grown()

    await render('/assign', snapshot)
    expect(rows()).toBeGreaterThan(10)
    expect(rows()).toBeLessThanOrEqual(100)
    const grid = document.querySelector('[role="grid"]')!
    expect(grid.getAttribute('aria-rowcount')).toBe(String(OFFERS + ORIGINS.length + 1))
    expect(grid.getAttribute('aria-colcount')).toBe(String(PROJECTS + 1))
    unmount()

    await render('/assign?by=project', snapshot)
    expect(rows()).toBeLessThanOrEqual(100)
    expect(document.querySelectorAll('[role="columnheader"]').length).toBeLessThan(OFFERS)
    expect(document.querySelector('[role="grid"]')!.getAttribute('aria-colcount')).toBe(String(OFFERS + 1))
  })

  // A benchmark held to a bound, run with every other test of the panel so
  // CI's `test (web)` holds it there too, though once the others are done
  // (vite.config.ts): vitest's own `bench` reports times and fails on none.
  // The page's modules are loaded first, as a browser has them once the
  // page's chunk arrives, so what is timed is the first render: the
  // snapshot answered, the page drawn from it.
  it('draws its first screen within 300 ms', async () => {
    await render('/assign', fixture('assigning'))
    unmount()
    const snapshot = grown()

    const times: number[] = []
    for (let run = 0; run < 3; run++) {
      const started = performance.now()
      await render('/assign', structuredClone(snapshot))
      times.push(performance.now() - started)
      expect(rows()).toBeGreaterThan(10)
      unmount()
    }

    const [, median] = [...times].sort((a, b) => a - b)
    expect(median, `each render took ${times.map(time => `${Math.round(time)} ms`).join(', ')}`).toBeLessThan(300)
  })
})
