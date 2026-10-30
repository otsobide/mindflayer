import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { succeed } from './environment.ts'
import { expect, test } from './panel.ts'

/*
 * What the panel changes, held to what the CLI does: each test changes a
 * workspace of its own (`own`), shows the task's line before it runs, and
 * leaves the workspace's marker byte for byte as the same command leaves
 * it in a copy made beforehand.
 */

/** The marker of the workspace at `root`, as bytes. */
function marker(root: string): Buffer {
  return readFileSync(join(root, '.mindflayer', 'flayer.toml'))
}

/** Whether two markers hold the same bytes, with both as text for a failure to show. */
function same(panel: Buffer, cli: Buffer): void {
  expect(panel.equals(cli), `the panel left:\n${panel.toString('utf8')}\nthe CLI left:\n${cli.toString('utf8')}`).toBe(true)
}

test('a project linked from the panel leaves flayer.toml as flayer link leaves it', async ({ page, own }) => {
  const notes = join(own.at.workspace, 'notes')
  await own.open('/projects')

  await page.getByTestId('link-path').fill(notes)
  await page.getByTestId('link').click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText([`$ flayer link ${notes}`])
  await expect(dialog.getByTestId('caution')).toHaveCount(0)
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()
  await expect(page.getByTestId('row').getByTestId('name')).toHaveText(['drafts', 'notes'])

  succeed(own.twin, 'flayer', own.twin.workspace, ['link', join(own.twin.workspace, 'notes')])
  same(marker(own.at.workspace), marker(own.twin.workspace))

  await page.getByTestId('projects').getByTestId('open-transcript').click()
  await expect(page.getByTestId('transcript').getByTestId('line')).toHaveText([`$ flayer link ${notes}`])
})

test('a project is unlinked from the panel only once confirmed, leaving flayer.toml as flayer unlink leaves it', async ({ page, own }) => {
  const before = marker(own.at.workspace)
  await own.open(`/project?${new URLSearchParams({ entry: 'drafts' })}`)

  await page.getByTestId('unlink').click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText(['$ flayer unlink drafts'])
  await expect(dialog.getByTestId('caution')).toHaveText([/^unlinks/])
  await expect(dialog.getByTestId('run')).toHaveText('Confirm and run')
  await expect(dialog.getByTestId('cancel')).toBeFocused()
  await dialog.getByTestId('cancel').click()
  await expect(dialog).toBeHidden()
  same(marker(own.at.workspace), before)

  await page.getByTestId('unlink').click()
  await dialog.getByTestId('run').click()
  await expect(page).toHaveURL(`${own.served.origin}/projects`)
  await expect(page.getByTestId('no-projects')).toBeVisible()

  succeed(own.twin, 'flayer', own.twin.workspace, ['unlink', 'drafts'])
  same(marker(own.at.workspace), marker(own.twin.workspace))
})
