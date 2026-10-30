import { LIBRARY } from './fixture.ts'
import { expect, listing, test } from './panel.ts'

/*
 * The page of each of the library's own against what `flayer list --json`
 * prints of it: what it is and where it is kept, what validate says, and
 * each project holding a copy, with how that copy stands there, as the
 * project's own listing says.
 */
for (const [kind, name] of LIBRARY) {
  test(`the page of the ${kind} ${name} shows what flayer list --json lists of it`, async ({ page, panel }) => {
    const { library, projects } = listing()
    const held = library!.artifacts.find(each => each.kind === kind && each.name === name)
    if (!held) {
      throw new Error(`flayer list --json lists no ${kind} ${name} in the library`)
    }
    await panel.open(`/artifact?${new URLSearchParams({ kind, name })}`)

    await expect(page.getByTestId('name')).toHaveText(held.name)
    await expect(page.getByTestId('kind')).toHaveText(held.kind)
    await expect(page.getByTestId('summary')).toHaveText(held.summary ?? '')
    await expect(page.getByTestId('target')).toHaveText(held.targets)
    expect(held.issues).toEqual([])
    await expect(page.getByTestId('valid')).toBeVisible()

    // The holders in the listing's order, each project's copy as that
    // project's own rows say it stands.
    const holders = library!.holders.filter(each => each.kind === kind && each.name === name)
    const copies = holders.map(({ entry }) => {
      const project = projects.find(each => each.entry === entry)!
      return { project, copy: project.artifacts.find(each => each.kind === kind && each.name === name)! }
    })
    const rows = page.getByTestId('holder')
    await expect(rows.getByTestId('project')).toHaveText(copies.map(({ project }) => project.name ?? project.entry ?? project.path))
    await expect(rows.getByTestId('state')).toHaveText(copies.map(({ copy }) => copy.state ?? 'no state'))
    for (const [index, { copy }] of copies.entries()) {
      await expect(rows.nth(index).getByTestId('agent')).toHaveText(copy.agents)
      await expect(rows.nth(index).locator('td').last().getByRole('listitem')).toHaveText(copy.targets)
    }
    if (copies.length === 0) {
      await expect(page.getByTestId('no-holding')).toBeVisible()
    }
  })
}
