import { basename, dirname, join } from 'node:path'
import type { Page } from '@playwright/test'
import { LOADED, OWN } from './fixture.ts'
import { expect, test } from './panel.ts'

/*
 * Switching workspace from the header, on a workspace of the test's own
 * (`loadable`), in a home of its own whose default workspace knows `team`
 * beside it: the header says which workspace is in use and why, the
 * switcher lists what core lists, and once switched every page draws
 * `team` and every line previewed names it with `-W`, until switched back.
 * Nothing is written: the home is held to stay as it was made, as the
 * fixture holds every home of its own.
 */
test.use({ building: 'loadable' })

/** The switcher's menu, open. */
async function opened(page: Page) {
  await page.getByTestId('workspace-switcher').click()
  return page.getByRole('menu')
}

/**
 * Load the known workspace `name` from the page of loads, and find the
 * dialog showing `line`, the line the server previewed; then let it go.
 */
async function loading(page: Page, name: string, line: string) {
  await page.getByTestId('sidebar').getByRole('link', { name: 'Loads' }).click()
  await page.getByRole('button', { name: `Load ${name}` }).click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText([line])
  await dialog.getByTestId('cancel').click()
  await expect(dialog).toBeHidden()
}

test('a workspace this machine knows, switched to from the header, is what every page draws and every line names, until switched back', async ({ page, own }) => {
  const workspace = own.at.workspace
  const team = join(dirname(workspace), 'team')
  const named = basename(workspace)
  const switcher = page.getByTestId('workspace-switcher')

  await own.open('/')
  await expect(switcher.getByTestId('workspace-name')).toHaveText(named)
  await expect(switcher.getByTestId('workspace-reason')).toHaveText('named with -W')
  await expect(page.getByTestId('workspace-path')).toHaveText(workspace)
  await loading(page, 'team', `$ flayer -W ${workspace} load ${team}`)

  let menu = await opened(page)
  const choices = menu.getByRole('menuitemcheckbox')
  await expect(choices).toHaveCount(3)
  await expect(menu.getByRole('menuitemcheckbox', { name: new RegExp(`^${named}`) })).toHaveAttribute('aria-checked', 'true')
  await menu.getByRole('menuitemcheckbox', { name: /^team/ }).click()

  await expect(page).toHaveURL(`${own.served.origin}/`)
  await expect(switcher.getByTestId('workspace-name')).toHaveText('team')
  await expect(switcher.getByTestId('workspace-reason')).toHaveText('named with -W')
  await expect(page.getByTestId('workspace-path')).toHaveText(team)
  await page.getByTestId('sidebar').getByRole('link', { name: 'Library' }).click()
  await expect(page.getByRole('main').getByText(LOADED.name, { exact: true }).first()).toBeVisible()
  await expect(page.getByRole('main').getByText(OWN, { exact: true })).toHaveCount(0)
  await loading(page, named, `$ flayer -W ${team} load ${workspace}`)

  menu = await opened(page)
  await menu.getByRole('menuitemcheckbox', { name: new RegExp(`^${named}`) }).click()
  await expect(page.getByTestId('workspace-path')).toHaveText(workspace)
  await loading(page, 'team', `$ flayer -W ${workspace} load ${team}`)
})
