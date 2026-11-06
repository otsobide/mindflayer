import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, type VueWrapper } from '@vue/test-utils'
import type { Draft } from '@/api/generated/Draft'
import type { Judged } from '@/api/generated/Judged'
import type { Query, Task } from '@/api/transport'
import { commanding, fixture, render, served, servers, texts, unmount, type Commanded } from './panel'

afterEach(unmount)

/** The section of the server `name` on the page. */
function section(page: VueWrapper, name: string) {
  const found = page.findAll('[data-testid="server"]').filter(each => each.get('[data-testid="name"]').text() === name)
  expect(found, name).toHaveLength(1)
  return found[0]!
}

/** Each agent's row of a server's section: its name, how it is given it, what it runs and what it asks. */
function rows(page: VueWrapper, name: string) {
  return section(page, name).findAll('[data-testid="agent"]').map(row => ({
    agent: row.get('[data-testid="agent-name"]').text(),
    given: row.get('[data-testid="given"]').text(),
    runs: row.find('[data-testid="runs"]').exists() ? row.get('[data-testid="runs"]').text() : null,
    why: texts(row, '[data-testid="why"]'),
    starts: row.get('[data-testid="starts"]').text()
  }))
}

/** What judging `draft` is answered with, in tests/fixtures/servers.json. */
function judged(draft: Partial<Draft>): Judged {
  const found = servers().find(each => each.query.type === 'validate' && JSON.stringify((each.query as { draft: Draft }).draft) === JSON.stringify({ ...(each.query as { draft: Draft }).draft, ...draft }))
  if (!found) {
    throw new Error(`no fixture judges ${JSON.stringify(draft)}`)
  }
  return found.answer as Judged
}

/** The change of tests/fixtures/served.json whose task is of `type`. */
function change(type: Task['type']): Commanded {
  return served().find(each => each.task.type === type)!
}

/** Type `value` into the field `testid`, the `nth` of them. */
async function fill(page: VueWrapper, testid: string, value: string, nth = 0) {
  await page.findAll(`input[data-testid="${testid}"]`)[nth]!.setValue(value)
}

/** Wait for what was typed to be judged, a quarter of a second after the last key. */
async function settle() {
  await new Promise(resolve => setTimeout(resolve, 300))
  await flushPromises()
}

/** The lines the dialog shows, as the preview gave them. */
function shown(): (string | null)[] {
  return [...document.body.querySelectorAll('[data-testid="command"]')].map(each => each.textContent)
}

/** How the last server the form judged was drafted, and the add task it then sent. */
function requested(queries: Query[], sent: { task: Task }[]) {
  const judging = queries.filter((query): query is Extract<Query, { type: 'validate' }> => query.type === 'validate').at(-1)!
  return {
    judged: (judging.draft as Extract<Draft, { type: 'add' }>).server,
    task: sent.at(-1)!.task as Extract<Task, { type: 'add' }>
  }
}

/** Every string `value` holds, at any depth, with the key it is under. */
function strings(value: unknown, under = ''): [string, string][] {
  if (typeof value === 'string') {
    return [[under, value]]
  }
  if (Array.isArray(value)) {
    return value.flatMap(each => strings(each, under))
  }
  if (typeof value === 'object' && value !== null) {
    return Object.entries(value).flatMap(([key, each]) => strings(each, key))
  }
  return []
}

describe('the page of MCP servers', () => {
  it('lists each of the library\'s servers with what each agent would run, and what it asks first, as core says it', async () => {
    const snapshot = fixture('serving')
    const { page } = await render('/mcp', snapshot)

    expect(texts(page, '[data-testid="server"] [data-testid="name"]')).toEqual(snapshot.library.servers.map(server => server.name))
    const github = rows(page, 'github')
    expect(github.map(row => [row.agent, row.given, row.runs])).toEqual([
      ['claude', 'given whole', 'runs npx -y @modelcontextprotocol/server-github'],
      ['codex', 'given whole', 'runs npx -y @modelcontextprotocol/server-github'],
      ['opencode', 'given whole', 'runs npx -y @modelcontextprotocol/server-github']
    ])
    expect(github.map(row => row.starts)).toEqual([
      'asks to approve it before it starts it in a session, though a run with -p starts it unasked',
      'starts it only in a project its user\'s config marks trusted',
      'starts it without asking'
    ])
    expect(rows(page, 'linear')[0]!.runs).toBe('reaches https://mcp.linear.app/mcp')
  })

  it('says which agent is given none of a server and why, by its code, never by colour alone', async () => {
    const snapshot = fixture('serving')
    const { page } = await render('/mcp', snapshot)

    const events = rows(page, 'events')
    const said = snapshot.library.servers.find(server => server.name === 'events')!.agents
    expect(events.map(row => [row.agent, row.given, row.runs])).toEqual([
      ['claude', 'given whole', 'reaches https://events.example.com/sse'],
      ['codex', 'refused', null],
      ['opencode', 'refused', null]
    ])
    expect(events[1]!.why).toEqual(said[1]!.refused.map(each => `${each.code}: ${each.message}`))
    expect(events[1]!.why[0]).toMatch(/^sse_unsupported: /)
  })

  it('says what doctor finds in each project holding a server, by its code, with what fixes it', async () => {
    const snapshot = fixture('serving')
    const { page } = await render('/mcp', snapshot)

    const holders = section(page, 'github').findAll('[data-testid="holder"]')
    expect(holders.map(holder => holder.get('[data-testid="project"]').text())).toEqual(['api'])
    const found = snapshot.doctor.find(each => each.entry === 'api')!.findings
    expect(found.map(finding => finding.code)).toEqual(['doctor_codex_untrusted', 'doctor_variable_unset'])
    expect(texts(holders[0]!, '[data-testid="finding"]')).toEqual(found.map(finding => `${finding.code}: ${finding.message} Fix: ${finding.fix}`))
    expect(section(page, 'linear').find('[data-testid="no-holders"]').exists()).toBe(true)
  })

  it('gives no element an id another holds, whatever its servers are named', async () => {
    const snapshot = fixture('serving')
    const github = snapshot.library.servers.find(server => server.name === 'github')!
    snapshot.library.servers.push(...['form', 'form-heading', 'github-heading'].map(name => ({ ...structuredClone(github), name })))
    const { page, router } = await render('/mcp', snapshot)

    const ids = [...page.get('[data-testid="mcp"]').element.querySelectorAll('[id]')].map(each => each.id)
    expect(ids.filter((id, at) => ids.indexOf(id) !== at)).toEqual([])
    await router.push('/mcp?edit=form')
    await flushPromises()
    expect(document.activeElement?.closest('[data-testid="form"]')).not.toBeNull()
  })

  it('names the commands that add one when the library holds none', async () => {
    const snapshot = fixture('serving')
    snapshot.library.servers = []
    snapshot.library.artifacts = snapshot.library.artifacts.filter(held => held.kind !== 'mcp')
    const { page } = await render('/mcp', snapshot)

    expect(page.get('[data-testid="empty"]').text()).toContain(`mindweaver -W ${snapshot.workspace.path} add mcp <NAME> <DESCRIPTION> --command <CMD> --secret-env <VAR>`)
    expect(page.find('[data-testid="server-form"]').exists()).toBe(true)
  })
})

describe('the form adding one', () => {
  it('judges what is typed as mindweaver add mcp would, and says what core refuses in its words', async () => {
    const { page } = await render('/mcp', commanding(fixture('serving'), [], servers()).transport)

    await fill(page, 'name', 'acme')
    await fill(page, 'description', 'Acme\'s tools')
    await fill(page, 'server-command', 'npx')
    await page.get('[data-testid="add-arg"]').trigger('click')
    await page.get('[data-testid="add-arg"]').trigger('click')
    await fill(page, 'arg', '-y', 0)
    await fill(page, 'arg', '@acme/server', 1)
    await page.get('[data-testid="add-secret-env"]').trigger('click')
    await fill(page, 'secret-env-variable', 'ACME-TOKEN')
    await settle()

    const refused = servers()[1]!.answer as Judged
    expect(page.get('[data-testid="refused"]').text()).toContain('mindweaver add mcp would refuse it')
    expect(page.get('[data-testid="refused"]').text()).toContain(refused.refused!.code)
    expect(page.get('[data-testid="refused-message"]').text()).toBe(refused.refused!.message)

    await fill(page, 'secret-env-variable', 'ACME_TOKEN')
    await settle()
    expect(page.find('[data-testid="refused"]').exists()).toBe(false)
    expect(page.get('[data-testid="verdict"]').text()).toContain('mindweaver add mcp would write it.')
  })

  it('adds one as the line the server previews, each secret the name of the variable it is read from', async () => {
    const add = change('add')
    const { sent, transport } = commanding(fixture('serving'), [add], servers())
    const { page } = await render('/mcp', transport)

    await fill(page, 'name', 'acme')
    await fill(page, 'description', 'Acme\'s tools')
    await fill(page, 'server-command', 'npx')
    await page.get('[data-testid="add-arg"]').trigger('click')
    await page.get('[data-testid="add-arg"]').trigger('click')
    await fill(page, 'arg', '-y', 0)
    await fill(page, 'arg', '@acme/server', 1)
    await page.get('[data-testid="add-secret-env"]').trigger('click')
    await fill(page, 'secret-env-variable', 'ACME_TOKEN')
    await page.get('[data-testid="server-form"]').trigger('submit')
    await flushPromises()

    expect(sent).toEqual([{ task: add.task, mode: 'preview' }])
    expect(shown()).toEqual(add.preview.lines.map(line => `$ ${line}`))
    document.body.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await flushPromises()
    expect(sent.map(each => each.mode)).toEqual(['preview', 'run'])
  })

  it('never sends a secret\'s value: each field in a secret\'s place takes its variable\'s name, sent there alone, and a header as typed', async () => {
    const { queries, sent, transport } = commanding(fixture('serving'), [], servers())
    const { page } = await render('/mcp', transport)

    await fill(page, 'name', 'acme')
    await fill(page, 'description', 'Acme\'s tools')
    await page.get('[data-testid="transport"] [role="radio"][value="address"]').trigger('click')
    await flushPromises()
    await fill(page, 'url', 'https://{tenant}.example.com/mcp')
    await page.get('[data-testid="add-header"]').trigger('click')
    await fill(page, 'header-key', 'Accept')
    await fill(page, 'header-value', 'application/json')
    await page.get('[data-testid="add-secret-header"]').trigger('click')
    await fill(page, 'secret-header-key', 'X-Api-Key')
    await fill(page, 'secret-header-variable', 'ACME_API_KEY')
    await fill(page, 'bearer-variable', 'ACME_TOKEN')
    await page.get('[data-testid="add-url-var"]').trigger('click')
    await fill(page, 'url-var-name', 'tenant')
    await fill(page, 'url-var-variable', 'ACME_TENANT')
    await settle()
    await page.get('[data-testid="server-form"]').trigger('submit')
    await flushPromises()

    // No field the form draws takes a value for a secret: each in a secret's
    // place asks for the name of its variable, and says nothing is stored.
    expect(page.findAll('input[type="password"]')).toHaveLength(0)
    expect(page.get('input[data-testid="secret-header-variable"]').attributes('aria-label')).toBe('Secret header 1, the name of its variable')
    const bearer = page.get('input[data-testid="bearer-variable"]').element
    expect(document.querySelector(`label[for="${bearer.id}"]`)!.textContent).toContain('Bearer token, the name of its variable')
    expect(page.get('[data-testid="secret-headers"] [data-testid="never-stored"]').text()).toContain('No value is stored')
    expect(page.get('[data-testid="headers"]').text()).toContain('no secret goes here')
    // What went out, judged and then previewed, holds each secret as the
    // name typed for its variable, in a secret's place, and the header as
    // typed, in its own: no string the form was not given, whatever key a
    // field of it went under.
    const { judged: draft, task } = requested(queries, sent)
    for (const server of [draft, task.server]) {
      expect(server).toEqual({
        type: 'address',
        url: 'https://{tenant}.example.com/mcp',
        events: false,
        headers: [['Accept', 'application/json']],
        secret_headers: [['X-Api-Key', 'ACME_API_KEY']],
        bearer: 'ACME_TOKEN',
        url_vars: [['tenant', 'ACME_TENANT']]
      })
    }
    const typed = ['acme', 'Acme\'s tools', 'https://{tenant}.example.com/mcp', 'Accept', 'application/json', 'X-Api-Key', 'ACME_API_KEY', 'ACME_TOKEN', 'tenant', 'ACME_TENANT']
    for (const [key, said] of strings(task)) {
      expect([...typed, 'add', 'workspace', 'mcp', 'address'], `${key}: ${said}`).toContain(said)
    }
  })

  it('sends a program\'s settings as typed, and each secret in its environment as its variable\'s name alone', async () => {
    const { queries, sent, transport } = commanding(fixture('serving'), [], servers())
    const { page } = await render('/mcp', transport)

    await fill(page, 'name', 'acme')
    await fill(page, 'description', 'Acme\'s tools')
    await fill(page, 'server-command', 'npx')
    await page.get('[data-testid="add-env"]').trigger('click')
    await fill(page, 'env-key', 'LOG_LEVEL')
    await fill(page, 'env-value', 'debug')
    await page.get('[data-testid="add-secret-env"]').trigger('click')
    await fill(page, 'secret-env-variable', 'ACME_TOKEN')
    await page.get('[data-testid="add-arg-env"]').trigger('click')
    await fill(page, 'arg-env-variable', 'ACME_ORG')
    await settle()
    await page.get('[data-testid="server-form"]').trigger('submit')
    await flushPromises()

    expect(page.get('[data-testid="env"]').text()).toContain('no secret goes here')
    expect(page.get('[data-testid="secret-env"] [data-testid="never-stored"]').text()).toContain('No value is stored')
    const { judged: draft, task } = requested(queries, sent)
    for (const server of [draft, task.server]) {
      expect(server).toEqual({
        type: 'program',
        command: 'npx',
        args: [],
        env: [['LOG_LEVEL', 'debug']],
        secret_env: ['ACME_TOKEN'],
        arg_env: ['ACME_ORG']
      })
    }
    const typed = ['acme', 'Acme\'s tools', 'npx', 'LOG_LEVEL', 'debug', 'ACME_TOKEN', 'ACME_ORG']
    for (const [key, said] of strings(task)) {
      expect([...typed, 'add', 'workspace', 'mcp', 'program'], `${key}: ${said}`).toContain(said)
    }
  })
})

describe('the form changing one', () => {
  it('starts from how core reads the server back, and sends it again as edit told its options', async () => {
    const edit = change('edit_server')
    const { sent, transport } = commanding(fixture('serving'), [edit], servers())
    const { page } = await render('/mcp?edit=github', transport)

    expect((page.get('input[data-testid="server-command"]').element as HTMLInputElement).value).toBe('npx')
    expect(page.findAll('input[data-testid="arg"]').map(input => (input.element as HTMLInputElement).value)).toEqual(['-y', '@modelcontextprotocol/server-github'])
    expect((page.get('input[data-testid="secret-env-variable"]').element as HTMLInputElement).value).toBe('GITHUB_PERSONAL_ACCESS_TOKEN')
    expect(page.find('input[data-testid="name"]').exists()).toBe(false)

    await fill(page, 'server-command', 'docker')
    await fill(page, 'arg', 'run', 0)
    await fill(page, 'arg', '-i', 1)
    await page.get('[data-testid="add-arg"]').trigger('click')
    await fill(page, 'arg', 'ghcr.io/github/github-mcp-server', 2)
    await settle()
    expect(page.get('[data-testid="verdict"]').text()).toContain('mindweaver edit would write it.')
    expect(judged({ type: 'edit_server' }).refused).toBeNull()

    await page.get('[data-testid="server-form"]').trigger('submit')
    await flushPromises()
    expect(sent).toEqual([{ task: edit.task, mode: 'preview' }])
    expect(shown()).toEqual(edit.preview.lines.map(line => `$ ${line}`))
    document.body.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await vi.waitFor(() => expect(sent.map(each => each.mode)).toEqual(['preview', 'run']))
  })

  it('is opened from a server\'s section, and one no form can say is changed in its file', async () => {
    const snapshot = fixture('serving')
    snapshot.library.servers.find(server => server.name === 'linear')!.given = null
    const { page, router } = await render('/mcp', snapshot)

    expect(section(page, 'github').get('[data-testid="change"]').attributes('href')).toBe('/mcp?edit=github#mcp-form-heading')
    expect(section(page, 'linear').get('[data-testid="edit-file"]').attributes('href')).toBe('/edit?kind=mcp&name=linear')
    await router.push('/mcp?edit=linear')
    await flushPromises()
    expect(document.activeElement?.id).toBe('mcp-form-heading')
    expect(page.find('[data-testid="unformed"]').exists()).toBe(true)
    expect(page.find('[data-testid="server-form"]').exists()).toBe(false)
  })
})
