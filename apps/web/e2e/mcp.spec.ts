import { randomBytes } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { run, succeed } from './environment.ts'
import { SERVED } from './fixture.ts'
import { expect, test, tree } from './panel.ts'

/*
 * An MCP server added from the panel's form and assigned from the matrix,
 * held to the disk and to the CLI: a workspace of the test's own
 * (`serveable`), whose project names every agent, served by a `mindweaver
 * ui` whose environment sets the server's variable. Each agent's config
 * file is read for the entry, and for the variable's value, which no file
 * may hold; the twin, a copy made beforehand, is given the same lines on
 * the command line, and the project holds the same files in both.
 */

/** The variable the server's secret is read from. */
const VARIABLE = 'ACME_TOKEN'

/**
 * Its value, where the agents would run: drawn as the run starts, never a
 * token written in the repository, and set for this test's server alone.
 */
const VALUE = `acme-${randomBytes(16).toString('hex')}`

test.use({ building: 'serveable', variables: { [VARIABLE]: VALUE } })

/** Each agent's config file, as the project holds it, and what refers to the variable there. */
const FILES = [
  ['.mcp.json', `\${${VARIABLE}}`],
  ['.codex/config.toml', VARIABLE],
  ['opencode.json', `{env:${VARIABLE}}`]
] as const

test('a server added from the form and ticked into a project is in each agent\'s file, and its secret\'s value in none', async ({ page, own }) => {
  const project = join(own.at.workspace, SERVED)
  await own.open('/mcp')

  // In the page's form, by its name, and `exact` labels inside it, as the
  // add page's form is reached.
  const form = page.getByRole('form', { name: 'The new server' })
  await form.getByLabel('Name', { exact: true }).fill('acme')
  await form.getByLabel('Description', { exact: true }).fill('Acme tools')
  await form.getByLabel('Command', { exact: true }).fill('npx')
  await form.getByTestId('add-arg').click()
  await form.getByLabel('Argument 1', { exact: true }).fill('-y')
  await form.getByTestId('add-arg').click()
  await form.getByLabel('Argument 2', { exact: true }).fill('@acme/server')
  await form.getByTestId('add-secret-env').click()
  await form.getByLabel('Secret 1, the name of its variable', { exact: true }).fill(VARIABLE)
  await expect(page.getByTestId('verdict')).toContainText('mindweaver add mcp would write it.')
  await page.getByTestId('add').click()
  const dialog = page.getByRole('dialog')
  const add = `$ mindweaver -W ${own.at.workspace} add --command npx --arg=-y --arg @acme/server --secret-env ${VARIABLE} mcp acme 'Acme tools'`
  await expect(dialog.getByTestId('command')).toHaveText([add])
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()
  const acme = page.getByTestId('server').filter({ has: page.getByRole('heading', { name: 'acme' }) })
  // The secret goes to its environment, so each runs the program alone.
  await expect(acme.getByTestId('runs')).toHaveText(['runs npx -y @acme/server', 'runs npx -y @acme/server', 'runs npx -y @acme/server'])

  await page.getByTestId('to-assign').click()
  const box = page.getByRole('checkbox', { name: new RegExp(`^mcp acme from workspace in ${SERVED}(;|$)`) })
  await box.click()
  const install = `$ mindweaver -W ${own.at.workspace} install -p ${project} --from workspace mcp:acme`
  await expect(page.getByTestId('line')).toHaveText([install])
  await expect(page.getByTestId('asks')).toHaveCount(0)
  await page.getByTestId('apply').click()
  await expect(dialog.getByTestId('command')).toHaveText([install])
  await dialog.getByTestId('run').click()

  // What the run said opens by itself: Codex is written its entry in a
  // project nothing marks trusted, which it reads only once trusted, said
  // by its code, as the line says it.
  const said = page.getByTestId('said')
  await expect(said.getByTestId('line').filter({ hasText: 'only in a project it trusts' })).toContainText('codex_mcp_untrusted')
  await page.keyboard.press('Escape')
  await expect(said).toBeHidden()
  await expect(box).toHaveAttribute('aria-checked', 'true')

  for (const [file, reference] of FILES) {
    const text = readFileSync(join(project, file), 'utf8')
    expect(text, file).toContain('acme')
    expect(text, file).toContain('@acme/server')
    expect(text, file).toContain(reference)
    expect(text, `${file} holds the secret's value`).not.toContain(VALUE)
  }
  const library = readFileSync(join(own.at.workspace, '.mindweaver', 'library', 'mcp', 'acme.toml'), 'utf8')
  expect(library, 'the library holds the secret\'s value').not.toContain(VALUE)

  const twin = own.twin
  succeed(twin, 'mindweaver', twin.workspace, ['add', '--command', 'npx', '--arg=-y', '--arg', '@acme/server', '--secret-env', VARIABLE, 'mcp', 'acme', 'Acme tools'])
  // Codex is given it in a project nothing marks trusted, which the line
  // warns of, so only that it ran is held to here.
  const installed = run(twin, 'mindweaver', twin.workspace, ['install', '-p', SERVED, '--from', 'workspace', 'mcp:acme'])
  expect(installed.stdout, installed.stderr).toContain('installed mcp acme into alpha, from workspace')
  expect(tree(project)).toEqual(tree(join(twin.workspace, SERVED)))
})
