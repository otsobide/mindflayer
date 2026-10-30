import type { Held } from '../src/api/generated/Held'
import { expect, listing, test } from './panel.ts'

/*
 * The overview's cards against `flayer list --json`, counted here from the
 * rows the listing prints rather than read from a count beside them: the
 * panel's come from the snapshot's summary, so the two meet only if core
 * counts what it lists.
 */
test('the overview counts what flayer list --json lists', async ({ page, panel }) => {
  const { library, projects } = listing()
  await panel.open('/')

  // Each directory once, however many entries spell it, as the summary
  // counts them.
  const once = (stale: boolean) => new Set(projects.filter(project => (project.stale !== null) === stale).map(project => project.path)).size
  const held: Held[] = projects.filter(project => project.stale === null).flatMap(project => project.artifacts)
  const card = (name: string) => page.getByTestId(`card-${name}`)

  await expect(card('projects').getByTestId('count')).toHaveText(String(once(false)))
  await expect(card('stale').getByTestId('count')).toHaveText(String(once(true)))
  for (const state of ['outdated', 'edited'] as const) {
    await expect(card(state).getByTestId('count')).toHaveText(String(held.filter(each => each.state === state).length))
  }

  // A card for every kind the listing reads, in its order, whatever the
  // fixture holds of it.
  const kinds = projects[0]!.tally.kinds.map(counted => counted.kind)
  expect(kinds.length).toBeGreaterThan(0)
  const cards = page.locator('[data-testid^="card-kind-"]')
  await expect(cards).toHaveCount(kinds.length)
  expect(await cards.evaluateAll(each => each.map(card => card.getAttribute('data-testid')))).toEqual(kinds.map(kind => `card-kind-${kind}`))
  for (const kind of kinds) {
    const inProjects = held.filter(each => each.kind === kind).length
    const inLibrary = library!.artifacts.filter(each => each.kind === kind).length
    await expect(card(`kind-${kind}`).getByTestId('count')).toHaveText(String(inProjects))
    await expect(card(`kind-${kind}`)).toContainText(`${inLibrary} in the library`)
  }
})
