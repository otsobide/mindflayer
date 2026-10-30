import { existsSync } from 'node:fs'
import { join } from 'node:path'
import type { Page } from '@playwright/test'
import { succeed, type Places } from './environment.ts'
import { ASSIGNED, OFFERED } from './fixture.ts'
import { expect, test, tree } from './panel.ts'

/*
 * Assigning from the matrix, held to the disk and to the CLI: a workspace of
 * the test's own (`assignable`), an offer ticked into two projects and out
 * of one again, each batch's line shown before it runs, and every copy the
 * agents read where it should be. A copy of the workspace made beforehand
 * is given the same lines on the command line, and both projects hold the
 * same files in both.
 */
test.use({ building: 'assignable' })

/** Where each agent reads a skill a project holds, which an install copies it into: Claude, then Codex. */
const FOLDERS = ['.claude/skills', '.agents/skills']

/** The box of the offered skill in `project`, by what it says it is. */
function box(page: Page, project: string) {
  return page.getByRole('checkbox', { name: new RegExp(`^skill ${OFFERED.name} from ${OFFERED.origin} in ${project}(;|$)`) })
}

/** Whether `project` holds a copy of the offered skill where each agent reads one. */
function held(at: Places, project: string): boolean[] {
  return FOLDERS.map(folder => existsSync(join(at.workspace, project, folder, OFFERED.name, 'SKILL.md')))
}

test('a skill ticked into two projects is copied where each agent reads it, and unticked out of one, as its lines do in the CLI', async ({ page, own }) => {
  const [alpha, beta] = ASSIGNED
  const folder = (project: string) => join(own.at.workspace, project)
  await own.open('/assign')
  for (const project of ASSIGNED) {
    await expect(box(page, project)).toHaveAttribute('aria-checked', 'false')
  }

  await box(page, alpha).click()
  await box(page, beta).click()
  const install = `$ flayer install -p ${folder(alpha)} -p ${folder(beta)} --from ${OFFERED.origin} skill:${OFFERED.name}`
  await expect(page.getByTestId('line')).toHaveText([install])
  await page.getByTestId('apply').click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText([install])
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()

  await expect(page.getByTestId('plan')).toHaveCount(0)
  for (const project of ASSIGNED) {
    await expect(box(page, project)).toHaveAttribute('aria-checked', 'true')
    await expect(box(page, project)).toHaveAttribute('data-state', 'current')
    expect(held(own.at, project), project).toEqual([true, true])
  }

  await box(page, beta).click()
  const uninstall = `$ flayer uninstall -p ${folder(beta)} skill:${OFFERED.name}`
  await expect(page.getByTestId('line')).toHaveText([uninstall])
  await page.getByTestId('apply').click()
  await expect(dialog.getByTestId('command')).toHaveText([uninstall])
  await expect(dialog.getByTestId('caution')).not.toHaveCount(0)
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()

  await expect(box(page, beta)).toHaveAttribute('aria-checked', 'false')
  await expect(box(page, alpha)).toHaveAttribute('aria-checked', 'true')
  expect(held(own.at, alpha)).toEqual([true, true])
  expect(held(own.at, beta)).toEqual([false, false])

  const twin = own.twin
  succeed(twin, 'flayer', twin.workspace, ['install', '-p', alpha, '-p', beta, '--from', OFFERED.origin, `skill:${OFFERED.name}`])
  succeed(twin, 'flayer', twin.workspace, ['uninstall', '-p', beta, `skill:${OFFERED.name}`])
  for (const project of ASSIGNED) {
    expect(tree(folder(project)), project).toEqual(tree(join(twin.workspace, project)))
  }
})
