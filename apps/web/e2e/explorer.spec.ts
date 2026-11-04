import { basename, dirname, join } from 'node:path'
import { given } from './environment.ts'
import { expect, test } from './panel.ts'

/*
 * The explorer, on the suite's fixture: a folder walked to and chosen
 * fills the field it was opened from, and goes to the command as a typed
 * one does. Nothing is changed: the link is previewed and cancelled, and a
 * scan only reads.
 */

test('a project walked to in the explorer is the folder the Link form previews', async ({ page, panel }) => {
  const at = given()
  const api = join(at.workspace, 'api')
  await panel.open('/projects')

  await page.getByTestId('link-browse').click()
  const explorer = page.getByRole('dialog', { name: 'Choose a folder to link' })
  // At the workspace's root, since the field holds nothing; then up a
  // folder by the trail, and down again by the workspace's name.
  await expect(explorer.getByTestId('crumb-here')).toHaveText(basename(at.workspace))
  await explorer.getByTestId('crumb').last().click()
  await expect(explorer.getByTestId('crumb-here')).toHaveText(basename(dirname(at.workspace)))
  await explorer.getByRole('button', { name: `Open ${basename(at.workspace)}`, exact: true }).click()
  await expect(explorer.getByTestId('folder-here').getByTestId('folder-is')).toHaveText('weaver workspace')
  const row = explorer.getByTestId('folder').filter({ has: page.getByRole('button', { name: 'Open api', exact: true }) })
  await expect(row.getByTestId('folder-is')).toHaveText('mind project')
  await expect(row.getByTestId('folder-linked')).toHaveText('linked')

  await row.getByRole('button', { name: 'Choose api', exact: true }).click()
  await expect(explorer).toBeHidden()
  await expect(page.getByTestId('link-path')).toHaveValue(api)

  await page.getByTestId('link').click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText([`$ mindweaver -W ${at.workspace} link ${api}`])
  await dialog.getByTestId('cancel').click()
  await expect(dialog).toBeHidden()
})

test('a folder typed in the explorer and added to Discover is scanned with the folders to look under', async ({ page, panel }) => {
  const at = given()
  const web = join(at.workspace, 'web')
  await panel.open('/discover')
  await page.getByTestId('scan-roots').fill('')

  await page.getByTestId('scan-browse').click()
  const explorer = page.getByRole('dialog', { name: 'Choose a folder to look under' })
  await expect(explorer.getByTestId('crumb-here')).toHaveText(basename(at.workspace))
  // Typed, and gone to on Enter, as from the keyboard.
  await explorer.getByTestId('folder-path').fill(web)
  await explorer.getByTestId('folder-path').press('Enter')
  await expect(explorer.getByTestId('crumb-here')).toHaveText('web')
  await expect(explorer.getByTestId('folder-here').getByTestId('folder-is')).toHaveText('mind project')
  await explorer.getByTestId('folder-choose-here').click()
  await expect(explorer).toBeHidden()
  await expect(page.getByTestId('scan-roots')).toHaveValue(web)

  await page.getByTestId('scan').click()
  await expect(page.getByTestId('scanned')).toContainText(web)
  await expect(page.getByTestId('linked-route')).toHaveText(['web'])
  await expect(page.getByTestId('suggestion')).toHaveCount(0)
})
