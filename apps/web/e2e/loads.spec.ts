import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import type { Page } from '@playwright/test'
import { succeed, type Places } from './environment.ts'
import { LOADED, OWN } from './fixture.ts'
import { expect, test } from './panel.ts'

/*
 * Loading another workspace from the panel, held to the disk and to the
 * CLI: a workspace of the test's own (`loadable`), in a home of its own
 * whose default workspace knows `team` beside it, loaded from the list of
 * the workspaces this machine knows, its skill then offered on the matrix,
 * and unloaded again, after which it is offered no more. A copy of the
 * workspace made beforehand is given the same lines on the command line,
 * and both markers say the same.
 */
test.use({ building: 'loadable' })

/** `text` read as a pattern of itself. */
function literally(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/** The box of the skill `name`, offered from `origin`, in the project `alpha`, by what it says it is. */
function box(page: Page, name: string, origin: string) {
  return page.getByRole('checkbox', { name: new RegExp(`^skill ${literally(name)} from ${literally(origin)} in alpha(;|$)`) })
}

/** The marker of the workspace `at` names, as it is on disk. */
function marker(at: Places): string {
  return readFileSync(join(at.workspace, '.mindflayer', 'flayer.toml'), 'utf8')
}

test('a workspace this machine knows, loaded from the panel, offers its skill on the matrix, and unloaded, offers it no more, as its lines do in the CLI', async ({ page, own }) => {
  const team = join(dirname(own.at.workspace), 'team')
  const origin = `load:${LOADED.entry}`
  const dialog = page.getByRole('dialog')
  const twin = own.twin

  await own.open('/loads')
  await expect(page.getByTestId('no-loaded')).toBeVisible()
  const known = page.getByTestId('known-workspace').filter({ has: page.getByTestId('path').getByText(team, { exact: true }) })
  await known.getByRole('button', { name: 'Load team' }).click()
  const load = `$ flayer load ${team}`
  await expect(dialog.getByTestId('command')).toHaveText([load])
  await expect(dialog.getByTestId('caution')).toHaveCount(0)
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()

  const loaded = page.getByTestId('load')
  await expect(loaded).toHaveCount(1)
  await expect(loaded.getByTestId('entry')).toHaveText(LOADED.entry)
  await expect(loaded.getByTestId('offer')).toHaveText([`skill:${LOADED.name} ${LOADED.description}`])
  await expect(known.getByText(`loaded as ${LOADED.entry}`)).toBeVisible()
  await expect(known.getByRole('button', { name: 'Load team' })).toHaveCount(0)
  succeed(twin, 'flayer', twin.workspace, ['load', team])
  expect(marker(own.at)).toBe(marker(twin))

  await loaded.getByTestId('assign').click()
  await expect(page).toHaveURL(/\/assign\?origin=/)
  await expect(box(page, LOADED.name, origin)).toBeVisible()

  await page.getByRole('link', { name: 'Loads' }).click()
  await page.getByRole('button', { name: 'Unload team' }).click()
  const unload = `$ flayer unload ${LOADED.entry}`
  await expect(dialog.getByTestId('command')).toHaveText([unload])
  await expect(dialog.getByTestId('caution')).not.toHaveCount(0)
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()

  await expect(page.getByTestId('no-loaded')).toBeVisible()
  await expect(known.getByRole('button', { name: 'Load team' })).toBeVisible()
  succeed(twin, 'flayer', twin.workspace, ['unload', LOADED.entry])
  expect(marker(own.at)).toBe(marker(twin))

  await page.getByRole('link', { name: 'Assign' }).click()
  await expect(box(page, OWN, 'workspace')).toBeVisible()
  await expect(box(page, LOADED.name, origin)).toHaveCount(0)
})
