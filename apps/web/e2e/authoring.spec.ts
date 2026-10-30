import { createHash } from 'node:crypto'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import type { Page } from '@playwright/test'
import { succeed } from './environment.ts'
import { AUTHORED, draft } from './fixture.ts'
import { expect, test, tree } from './panel.ts'

/*
 * Authoring in the library from the panel, held to the disk and to the CLI:
 * a workspace of the test's own (`authorable`), whose library holds
 * `review`. Its twin, a copy made beforehand, is given the same lines on
 * the command line, and both libraries hold the same files in both.
 */
test.use({ building: 'authorable' })

/** The SHA-256 of `text`, as core spells one. */
function sha256(text: string): string {
  return createHash('sha256').update(text).digest('hex')
}

/** Where the workspace at `root` keeps its own skills, and the file `review` is read from. */
function library(root: string): string {
  return join(root, '.mindflayer', 'library', 'skills')
}

function manifest(root: string): string {
  return join(library(root), AUTHORED, 'SKILL.md')
}

/** The editor's text, as somebody reaches it: by its name, inside the shadow root it is drawn in. */
function editor(page: Page) {
  return page.getByRole('textbox', { name: `The text of ${AUTHORED}'s SKILL.md` })
}

/** Put the caret at the end of the editor's text and type `text` there. */
async function append(page: Page, text: string) {
  await editor(page).click()
  await page.keyboard.press('ControlOrMeta+End')
  await page.keyboard.type(text)
}

test('a library skill edited in the browser is written on disk as flayer edit --contents writes it, and its text taken away after', async ({ page, own }) => {
  const before = readFileSync(manifest(own.at.workspace), 'utf8')
  await own.open(`/artifact?${new URLSearchParams({ kind: 'skill', name: AUTHORED })}`)
  await page.getByTestId('edit').click()
  await expect(editor(page)).toBeVisible()
  await expect(page.getByTestId('verdict')).toContainText('flayer edit would write it.')

  const added = 'Say why, every time.'
  await append(page, added)
  const text = `${before}${added}`
  await expect(page.getByTestId('unsaved')).toBeVisible()

  await page.getByTestId('save').click()
  const dialog = page.getByRole('dialog')
  const route = `.mindflayer/tmp/edit-${sha256(text).slice(0, 16)}.md`
  await expect(dialog.getByTestId('command')).toHaveText([`$ flayer edit --contents ${route} --if-unchanged ${sha256(before)} skill:${AUTHORED}`])
  await expect(dialog.getByTestId('caution')).toHaveCount(0)
  await dialog.getByTestId('run').click()
  await expect(page.getByTestId('unsaved')).toHaveCount(0)

  expect(readFileSync(manifest(own.at.workspace), 'utf8')).toBe(text)
  expect(readdirSync(join(own.at.workspace, '.mindflayer', 'tmp')), 'the text put there for the line is taken away').toEqual([])
  const twin = own.twin
  const typed = draft(twin, 'review.md', text)
  succeed(twin, 'flayer', twin.workspace, ['edit', '--contents', typed, '--if-unchanged', sha256(before), `skill:${AUTHORED}`])
  expect(tree(library(own.at.workspace))).toEqual(tree(library(twin.workspace)))
})

test('a change made on disk while the editor is open makes the save fail with core\'s message, and writes nothing over it', async ({ page, own }) => {
  const before = readFileSync(manifest(own.at.workspace), 'utf8')
  await own.open(`/edit?${new URLSearchParams({ kind: 'skill', name: AUTHORED })}`)
  await expect(page.getByTestId('verdict')).toContainText('flayer edit would write it.')
  await append(page, 'Typed in the browser.')

  // Changed meanwhile with the real command, as another editor would.
  const elsewhere = `${before}Changed on the command line.\n`
  const changed = draft(own.at, 'elsewhere.md', elsewhere)
  succeed(own.at, 'flayer', own.at.workspace, ['edit', '--contents', changed, `skill:${AUTHORED}`])
  await expect(page.getByTestId('changed')).toBeVisible()
  await expect(editor(page)).toContainText('Typed in the browser.')

  await page.getByTestId('save').click()
  await page.getByRole('dialog').getByTestId('run').click()

  const said = page.getByTestId('said')
  const message = `${manifest(own.at.workspace)}: changed since it was read, its SHA-256 now ${sha256(elsewhere)} and not ${sha256(before)}, so it is not written over; read it again`
  await expect(said.getByTestId('line')).toContainText(['edit_stale'])
  await expect(said.getByTestId('line')).toContainText([message])
  expect(readFileSync(manifest(own.at.workspace), 'utf8')).toBe(elsewhere)
  expect(readdirSync(join(own.at.workspace, '.mindflayer', 'tmp'))).toEqual([])
})

test('a skill added from a template and one renamed are what flayer add and flayer rename write', async ({ page, own }) => {
  await own.open('/library?kind=skill')
  await page.getByTestId('new').click()
  await page.getByLabel('Name').fill('deploy-docs')
  await page.getByLabel('Description').fill('Publish the docs')
  await page.getByRole('radio', { name: /^full/ }).click()
  await expect(page.getByTestId('verdict')).toContainText('flayer add would write it.')
  await page.getByTestId('add').click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText(['$ flayer add --template full skill deploy-docs \'Publish the docs\''])
  await dialog.getByTestId('run').click()
  await expect(page).toHaveURL(/\/artifact\?kind=skill&name=deploy-docs$/)

  await page.getByTestId('rename-to').fill('publish-docs')
  await page.getByTestId('rename-run').click()
  await expect(dialog.getByTestId('command')).toHaveText(['$ flayer rename skill:deploy-docs publish-docs'])
  await dialog.getByTestId('run').click()
  await expect(page).toHaveURL(/\/artifact\?kind=skill&name=publish-docs$/)

  const twin = own.twin
  succeed(twin, 'flayer', twin.workspace, ['add', '--template', 'full', 'skill', 'deploy-docs', 'Publish the docs'])
  succeed(twin, 'flayer', twin.workspace, ['rename', 'skill:deploy-docs', 'publish-docs'])
  expect(tree(library(own.at.workspace))).toEqual(tree(library(twin.workspace)))
})
