import { existsSync } from 'node:fs'
import { join } from 'node:path'
import type { Page } from '@playwright/test'
import { run, succeed, type Places } from './environment.ts'
import { LINT_SINCE, UPDATED, UPDATING, draft } from './fixture.ts'
import { drawn, expect, test, tree } from './panel.ts'

/*
 * Updating and removing from the panel, held to the disk and to the CLI: a
 * workspace of the test's own (`updatable`), whose library's `lint` both
 * its projects received, `beta`'s copy edited since. Its twin, a copy made
 * beforehand, is given the same lines on the command line, and both
 * projects hold the same files in both.
 */
test.use({ building: 'updatable' })

const [alpha, beta] = UPDATING

/** The row of the projects table naming `project`. */
function row(page: Page, project: string) {
  return page.getByTestId('row').filter({ has: page.getByTestId('name').getByText(project, { exact: true }) })
}

/** The library's `lint` changed in `at`, as its editor saves it: `flayer edit` copying a draft in. */
function changeTheLibrary(at: Places) {
  const since = draft(at, 'lint-since.md', LINT_SINCE)
  succeed(at, 'flayer', at.workspace, ['edit', `skill:${UPDATED}`, '--editor', `cp ${since}`])
}

/** Go to a page from the sidebar, as a person does, and wait for it to be drawn. */
async function visit(page: Page, name: string) {
  await page.getByTestId('sidebar').getByRole('link', { name, exact: true }).click()
  await drawn(page)
}

test('a library skill changed shows outdated, and updating every project brings it up to date while the edited copy is left with why', async ({ page, own }) => {
  await own.open('/projects')
  for (const project of UPDATING) {
    await expect(row(page, project).getByTestId('outdated')).toHaveText('none')
  }

  // Changed while the page is open: the server tells it, and the table
  // draws it, with no reload.
  changeTheLibrary(own.at)
  await expect(row(page, alpha).getByTestId('outdated-badge')).toHaveText('1 outdated')
  // Edited is said over outdated: `beta`'s copy asks for a forced update.
  await expect(row(page, beta).getByTestId('outdated')).toHaveText('none')

  await visit(page, 'Updates')
  const outdated = page.getByTestId('state-outdated').getByTestId('row')
  await expect(outdated).toHaveCount(1)
  await expect(outdated.getByTestId('project')).toHaveText(alpha)
  await expect(outdated.getByTestId('origin')).toHaveText('workspace')
  const edited = page.getByTestId('state-edited').getByTestId('row')
  await expect(edited.getByTestId('project')).toHaveText(beta)
  const left = `skill \`${UPDATED}\` in ${beta} was changed since it was installed`
  await expect(edited.getByTestId('left')).toContainText('edited')
  await expect(edited.getByTestId('left')).toContainText(left)

  await page.getByTestId('update-all').click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText(['$ flayer update --all --yes'])
  await expect(dialog.getByTestId('caution')).toHaveCount(0)
  await dialog.getByTestId('run').click()

  // What the run said opens by itself, the edit it left said by its code.
  const said = page.getByTestId('said')
  await expect(said.getByTestId('line').filter({ hasText: 'was changed since it was installed' })).toContainText('edited')
  await page.keyboard.press('Escape')
  await expect(said).toBeHidden()

  await expect(page.getByTestId('state-outdated').getByTestId('row')).toHaveCount(0)
  await expect(page.getByTestId('state-edited').getByTestId('row').getByTestId('left')).toContainText(left)
  await visit(page, 'Projects')
  for (const project of UPDATING) {
    await expect(row(page, project).getByTestId('outdated')).toHaveText('none')
  }

  const twin = own.twin
  changeTheLibrary(twin)
  const typed = run(twin, 'flayer', twin.workspace, ['update', '--all', '--yes'])
  expect(typed.status, typed.stderr).toBe(1)
  expect(typed.stderr).toContain(`${UPDATED} in ${beta} was changed since it was installed`)
  for (const project of UPDATING) {
    expect(tree(join(own.at.workspace, project)), project).toEqual(tree(join(twin.workspace, project)))
  }
  expect(tree(join(own.at.workspace, alpha))[`.claude/skills/${UPDATED}/SKILL.md`]).toBe(LINT_SINCE)
})

test('removing a library skill shows what removing it everywhere does first, then takes it out of every project but the edited copy, which a forced uninstall then takes out', async ({ page, own }) => {
  await own.open(`/artifact?${new URLSearchParams({ kind: 'skill', name: UPDATED })}`)

  await page.getByTestId('remove-dry').click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText([`$ flayer remove --everywhere skill:${UPDATED}`])
  await expect(dialog.getByTestId('caution')).toHaveCount(0)
  await dialog.getByTestId('run').click()
  const dry = page.getByTestId('remove-said')
  await expect(dry).toContainText(`would remove ${UPDATED} from ${alpha}`)
  await expect(dry).toContainText(`would leave ${UPDATED} in ${beta}, where it is edited`)
  expect(existsSync(join(own.at.workspace, alpha, '.claude/skills', UPDATED)), 'a dry run removes nothing').toBe(true)

  await page.getByTestId('remove-yes').click()
  await expect(dialog.getByTestId('command')).toHaveText([`$ flayer remove --everywhere --yes skill:${UPDATED}`])
  await expect(dialog.getByTestId('caution')).toHaveCount(2)
  await expect(dialog.getByTestId('cancel')).toBeFocused()
  await dialog.getByTestId('run').click()
  await expect(page).toHaveURL(/\/library\?kind=skill$/)

  const twin = own.twin
  succeed(twin, 'flayer', twin.workspace, ['remove', '--everywhere', '--yes', `skill:${UPDATED}`])
  for (const project of UPDATING) {
    expect(tree(join(own.at.workspace, project)), project).toEqual(tree(join(twin.workspace, project)))
  }
  expect(existsSync(join(own.at.workspace, alpha, '.claude/skills', UPDATED))).toBe(false)
  expect(existsSync(join(own.at.workspace, beta, '.claude/skills', UPDATED))).toBe(true)

  // What the removal left, edited with nothing offering it any more, is
  // listed with the forced uninstall that alone takes it out.
  await visit(page, 'Updates')
  const left = page.getByTestId('state-edited').getByTestId('row')
  await expect(left.getByTestId('project')).toHaveText(beta)
  await expect(left.getByTestId('left')).toContainText('only a forced uninstall takes it out')
  await left.getByTestId('change').click()
  await expect(dialog.getByTestId('command')).toHaveText([`$ flayer uninstall -p ${join(own.at.workspace, beta)} --force skill:${UPDATED}`])
  await expect(dialog.getByTestId('caution')).toHaveCount(2)
  await expect(dialog.getByTestId('cancel')).toBeFocused()
  await dialog.getByTestId('run').click()
  await expect(page.getByTestId('state-edited').getByTestId('row')).toHaveCount(0)

  succeed(twin, 'flayer', twin.workspace, ['uninstall', '-p', beta, '--force', `skill:${UPDATED}`])
  expect(tree(join(own.at.workspace, beta))).toEqual(tree(join(twin.workspace, beta)))
  expect(existsSync(join(own.at.workspace, beta, '.claude/skills', UPDATED))).toBe(false)
})
