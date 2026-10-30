import { PROJECTS } from './fixture.ts'
import { expect, listing, test } from './panel.ts'

/*
 * Each project's page against what `flayer list --json` prints of it: the
 * facts at the top, and every artifact it holds, kind by kind, in the
 * listing's order, as the page lays them out without sorting its own.
 */
for (const entry of PROJECTS) {
  test(`the page of ${entry} shows what flayer list --json lists of it`, async ({ page, panel }) => {
    const project = listing().projects.find(each => each.entry === entry)
    if (!project) {
      throw new Error(`flayer list --json lists no project at ${entry}`)
    }
    await panel.open(`/project?${new URLSearchParams({ entry })}`)

    await expect(page.getByTestId('health')).toHaveText(project.health)
    const facts = page.getByTestId('facts')
    await expect(facts).toContainText(project.path)
    await expect(facts).toContainText(project.id ?? 'none')
    await expect(facts).toContainText(`.mind/${project.marker}`)
    await expect(page.getByTestId('share')).toHaveText(project.settings!.share)

    for (const { kind } of project.tally.kinds) {
      const held = project.artifacts.filter(each => each.kind === kind)
      const section = page.getByTestId(`kind-${kind}`)
      await expect(section.getByTestId('kind-count')).toHaveText(String(held.length))

      const rows = section.getByTestId('artifact')
      await expect(rows.locator('th > div:first-child')).toHaveText(held.map(each => each.name))
      await expect(rows.getByTestId('state')).toHaveText(held.map(each => each.state ?? 'no state'))
      await expect(rows.getByTestId('origin')).toHaveText(held.map(each => each.origin ?? 'none'))
      for (const [index, each] of held.entries()) {
        const row = rows.nth(index)
        if (each.summary !== null) {
          await expect(row.locator('th')).toContainText(each.summary)
        }
        await expect(row.getByTestId('agent')).toHaveText(each.agents)
        await expect(row.getByTestId('targets').getByRole('listitem')).toHaveText(each.targets)
      }
    }
  })
}
