import { existsSync } from 'node:fs'
import { join } from 'node:path'
import type { Page } from '@playwright/test'
import type { Envelope } from '../src/api/generated/Envelope'
import type { Listing } from '../src/api/generated/Listing'
import { succeed, type Places } from './environment.ts'
import { ASSIGNED, OFFERED } from './fixture.ts'
import { expect, test, tree } from './panel.ts'

/*
 * The command palette, driven from the keyboard alone, held to the disk
 * and to the CLI: a workspace of the test's own (`assignable`), the team's
 * skill installed into one project by choosing it in the palette and
 * confirming the line it shows, every copy the agents read where it
 * should be, and a copy of the workspace made beforehand given the same
 * line on the command line, after which both hold the same files and the
 * CLI says the same of both.
 */
test.use({ building: 'assignable' })

/** Where each agent reads a skill a project holds, which an install copies it into: Claude, then Codex. */
const FOLDERS = ['.claude/skills', '.agents/skills']

/** Whether `project` holds a copy of the offered skill where each agent reads one. */
function held(at: Places, project: string): boolean[] {
  return FOLDERS.map(folder => existsSync(join(at.workspace, project, folder, OFFERED.name, 'SKILL.md')))
}

/** What `flayer list --json` says of the workspace `at` names, its folder written as `<workspace>`, to compare with a copy's. */
function said(at: Places): string {
  const ran = succeed(at, 'flayer', at.workspace, ['list', '--json'])
  const envelope = JSON.parse(ran.stdout) as Envelope<Listing>
  return JSON.stringify(envelope.data).replaceAll(at.workspace, '<workspace>')
}

/** Whether the focus is inside the dialog named `name`. */
async function within(page: Page, name: string): Promise<boolean> {
  return page.evaluate(named => [...document.querySelectorAll('[role="dialog"]')]
    .some(dialog => dialog.getAttribute('aria-labelledby') !== null
      && document.getElementById(dialog.getAttribute('aria-labelledby')!)?.textContent === named
      && dialog.contains(document.activeElement)), name)
}

test('a skill installed from the command palette with the keyboard alone is copied where each agent reads it, as its line does in the CLI', async ({ page, own }) => {
  const [alpha, beta] = ASSIGNED
  await own.open('/')
  const palette = page.getByRole('dialog', { name: 'Command palette' })
  const search = palette.getByRole('textbox', { name: 'Search the panel' })

  // Opened over what had the focus, which it traps, named for assistive
  // technology, and gives back on Escape.
  const opener = page.getByTestId('open-palette')
  await opener.focus()
  await page.keyboard.press('ControlOrMeta+K')
  await expect(palette).toBeVisible()
  await expect(search).toBeFocused()
  await expect(palette.getByRole('listbox')).toHaveAccessibleName(/.+/)
  for (const key of ['Tab', 'Tab', 'Tab', 'Shift+Tab', 'Shift+Tab', 'Shift+Tab', 'Shift+Tab']) {
    await page.keyboard.press(key)
    expect(await within(page, 'Command palette'), `the focus, after ${key}`).toBe(true)
  }
  await page.keyboard.press('Escape')
  await expect(palette).toBeHidden()
  await expect(opener).toBeFocused()

  // The offer, then the project, each typed and chosen with Enter.
  await page.keyboard.press('ControlOrMeta+K')
  await expect(search).toBeFocused()
  await page.keyboard.type(`install ${OFFERED.name}`)
  await expect(palette.getByRole('option').first()).toContainText(`Install ${OFFERED.name}`)
  await page.keyboard.press('Enter')
  await expect(palette.getByRole('option')).toHaveText([new RegExp(`^${alpha}`), new RegExp(`^${beta}`)])
  await page.keyboard.type(alpha)
  await expect(palette.getByRole('option')).toHaveText([new RegExp(`^${alpha}`)])
  await page.keyboard.press('Enter')
  await expect(palette).toBeHidden()

  // The line core's plan of that one box runs, shown first, Run focused
  // since it takes nothing away, and run by Enter.
  const line = `$ flayer -W ${own.at.workspace} install -p ${join(own.at.workspace, alpha)} --from ${OFFERED.origin} skill:${OFFERED.name}`
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText([line])
  await expect(dialog.getByTestId('caution')).toHaveCount(0)
  await expect(dialog.getByTestId('run')).toBeFocused()
  await page.keyboard.press('Enter')
  await expect(dialog).toBeHidden()

  await expect.poll(() => held(own.at, alpha)).toEqual([true, true])
  expect(held(own.at, beta)).toEqual([false, false])

  const twin = own.twin
  succeed(twin, 'flayer', twin.workspace, ['install', '-p', alpha, '--from', OFFERED.origin, `skill:${OFFERED.name}`])
  for (const project of ASSIGNED) {
    expect(tree(join(own.at.workspace, project)), project).toEqual(tree(join(twin.workspace, project)))
  }
  expect(said(own.at)).toEqual(said(twin))
})
