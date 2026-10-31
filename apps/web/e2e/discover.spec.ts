import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { succeed } from './environment.ts'
import { expect, test } from './panel.ts'

/*
 * The page of what could be linked, on a workspace of a test's own
 * (`own`): what a scan of it finds, linked from the panel, held to what
 * the same command leaves in a copy made beforehand.
 */

/** The marker of the workspace at `root`, as bytes. */
function marker(root: string): Buffer {
  return readFileSync(join(root, '.mindflayer', 'flayer.toml'))
}

test('a suggestion the scan finds is linked from the panel and appears in the projects table', async ({ page, own }) => {
  const notes = join(own.at.workspace, 'notes')
  await own.open('/discover')

  // Nothing is scanned until asked; the workspace's own folder is where it starts.
  await expect(page.getByTestId('not-asked')).toBeVisible()
  await expect(page.getByTestId('scan-roots')).toHaveValue(own.at.workspace)
  await page.getByTestId('scan').click()

  await expect(page.getByTestId('suggestion')).toHaveCount(1)
  await expect(page.getByTestId('suggestion').getByTestId('why')).toHaveText('mind project with an id')
  await expect(page.getByTestId('linked-route')).toHaveText(['drafts'])
  await page.getByRole('checkbox', { name: 'notes' }).click()
  await page.getByTestId('link-marked').click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText([`$ flayer -W ${own.at.workspace} link ${notes}`])
  await expect(dialog.getByTestId('caution')).toHaveCount(0)
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()

  // Scanned again once it ran, since no event asks for a scan.
  await expect(page.getByTestId('suggestion')).toHaveCount(0)
  await expect(page.getByTestId('linked-route')).toHaveText(['drafts', 'notes'])

  await page.getByRole('link', { name: 'Projects' }).click()
  await expect(page.getByTestId('row').getByTestId('name')).toHaveText(['drafts', 'notes'])

  succeed(own.twin, 'flayer', own.twin.workspace, ['link', join(own.twin.workspace, 'notes')])
  const [panel, cli] = [marker(own.at.workspace), marker(own.twin.workspace)]
  expect(panel.equals(cli), `the panel left:\n${panel.toString('utf8')}\nthe CLI left:\n${cli.toString('utf8')}`).toBe(true)
})
