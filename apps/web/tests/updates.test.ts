import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { Updates } from '@/api/generated/Updates'
import type { LiveEvent, Transport } from '@/api/transport'
import { answering, commanding, fixture, render, texts, unmount, updates, updatings, type Commanded } from './panel'

afterEach(() => {
  unmount()
  vi.restoreAllMocks()
})

/** The id of each element brought into view while `run` runs, in turn. */
async function scrolledTo(run: () => Promise<unknown>): Promise<string[]> {
  const scrolled: string[] = []
  vi.spyOn(Element.prototype, 'scrollIntoView').mockImplementation(function (this: Element) {
    scrolled.push(this.id)
  })
  await run()
  return scrolled
}

/*
 * The page of updates, drawn from what the CLI's suite wrote: the workspace
 * built to update (tests/fixtures/updating.json), what asking what an
 * update would do there is answered with (updates.json), and each change
 * the page offers, as the backend previewed and ran it (updated.json).
 */

/** What updating every project would do, as the fixture's server answers it. */
function survey(): Updates {
  return updates()[0]!.answer as Updates
}

/** The fixture's answers, with what an update would do changed by `change`. */
function changed(change: (updates: Updates) => void) {
  const answered = updates()
  change(answered[0]!.answer as Updates)
  return answered
}

const [ALL, PROJECT, ONE, FORCED, UNINSTALL, FORCED_UNINSTALL, ORPHANS] = updatings() as [Commanded, Commanded, Commanded, Commanded, Commanded, Commanded, Commanded]

describe('the page of updates', () => {
  it('lists each installation under its state, the states in core\'s order, each counted as core counts it', async () => {
    const { transport } = answering(fixture('updating'), updates())

    const { page } = await render('/updates', transport)

    const answer = survey()
    const states = answer.states.map(counted => counted.state)
    expect(page.findAll('section[data-testid^="state-"]').map(section => section.attributes('data-testid'))).toEqual(states.map(state => `state-${state}`))
    expect(texts(page, '[data-testid="counts"] [data-testid="count"]')).toEqual(answer.states.map(counted => String(counted.count)))
    for (const state of states) {
      const section = page.get(`[data-testid="state-${state}"]`)
      const listed = answer.projects.flatMap(project => project.installations
        .filter(pending => pending.state === state)
        .map(pending => [pending.name, project.name, pending.origin ?? 'not kept']))
      const rows = section.findAll('[data-testid="row"]').map(row => [
        row.get('[data-testid="name"]').text(),
        row.get('[data-testid="project"]').text(),
        row.get('[data-testid="origin"]').text()
      ])
      expect(rows, state).toEqual(listed)
    }
  })

  it('says what an update does with each, and why it leaves what it leaves, in core\'s words and by its code', async () => {
    const { transport } = answering(fixture('updating'), updates())

    const { page } = await render('/updates', transport)

    const rows = page.findAll('[data-testid="row"]')
    const left = survey().projects
      .flatMap(project => project.installations)
      .flatMap(pending => (pending.left ? [`${pending.left.code}: ${pending.left.message}`] : []))
    expect(texts(page, '[data-testid="left"]').sort()).toEqual(left.sort())
    const edited = page.get('[data-testid="state-edited"] [data-testid="row"]')
    expect(edited.get('[data-testid="does"]').text()).toContain('leave')
    expect(texts(edited, '[data-testid="copy"]')).toEqual(['.claude/skills/lint'])
    expect(edited.get('[data-testid="left"]').text()).toContain('edited')
    const missing = page.get('[data-testid="state-missing"] [data-testid="row"]')
    expect(missing.get('[data-testid="does"]').text()).toContain('put_back')
    expect(texts(missing, '[data-testid="copy"]')).toEqual(['.agents/skills/review'])
    expect(rows.length).toBe(survey().projects.flatMap(project => project.installations).length)
  })

  it('says each entry that opens no project, whose installations cannot be told', async () => {
    const { transport } = answering(fixture('updating'), updates())

    const { page } = await render('/updates', transport)

    const [unread] = survey().unread
    expect(page.get('[data-testid="unread"]').text()).toContain(unread!.code)
    expect(page.get('[data-testid="unread"]').text()).toContain(unread!.message)
  })

  it('offers each project an update of its own only where an update writes something', async () => {
    const { transport } = answering(fixture('updating'), updates())

    const { page } = await render('/updates', transport)

    const projects = page.findAll('[data-testid="by-project"] [data-testid="project"]')
    expect(projects.map(project => project.get('[data-testid="project-name"]').text())).toEqual(survey().projects.map(project => project.name))
    expect(projects.map(project => project.find('[data-testid="update-project"]').exists())).toEqual(survey().projects.map(project => project.writes))
  })

  it('offers no update of every project while it would write nothing', async () => {
    const { transport } = answering(fixture('updating'), changed((updates) => {
      updates.writes = false
    }))

    const { page } = await render('/updates', transport)

    expect(page.get('[data-testid="update-all"]').attributes('disabled')).toBeDefined()
  })

  it('says when everything installed is current, and the command that says so too', async () => {
    const snapshot = fixture('updating')
    const { transport } = answering(snapshot, changed((updates) => {
      updates.projects = []
      updates.writes = false
      updates.unread = []
      updates.states = updates.states.map(counted => ({ ...counted, count: 0 }))
    }))

    const { page } = await render('/updates', transport)

    expect(page.get('[data-testid="nothing"]').text()).toContain('is current')
    expect(page.get('[data-testid="nothing"] [data-testid="commands"]').text()).toContain(`flayer -W ${snapshot.workspace.path} update --all`)
    expect(page.find('[data-testid="by-project"]').exists()).toBe(false)
  })

  it('says why, with its code, when what an update would do could not be read', async () => {
    const { transport } = answering(fixture('updating'), [])

    const { page } = await render('/updates', transport)

    expect(page.get('[data-testid="updates-unread"]').text()).toContain('not_found')
  })

  it('brings the list a link names by its hash into view once it is drawn, and not again when a change is told', async () => {
    const answered = updates()
    const { transport } = answering(fixture('updating'), answered)
    let tell: (event: LiveEvent) => void = () => {}
    const streamed = { ...transport, subscribe: (listener: (event: LiveEvent) => void) => (tell = listener, () => {}) } as Transport

    const scrolled = await scrolledTo(() => render('/updates#state-edited', streamed))

    expect(scrolled).toEqual(['state-edited'])
    ;(answered[0]!.answer as Updates).writes = false
    const told = await scrolledTo(async () => {
      tell({ type: 'changed', scopes: [{ type: 'workspace' }] })
      await flushPromises()
    })
    expect(document.querySelector('[data-testid="update-all"]')?.hasAttribute('disabled')).toBe(true)
    expect(told).toEqual([])
  })

  it('is reached at what is outdated from the projects table\'s badge', async () => {
    const { transport } = answering(fixture('updating'), updates())
    const { page, router } = await render('/projects', transport)

    const scrolled = await scrolledTo(async () => {
      await page.get('[data-testid="outdated"] a').trigger('click')
      await flushPromises()
    })

    expect(router.currentRoute.value.fullPath).toBe('/updates#state-outdated')
    expect(scrolled).toEqual(['state-outdated'])
  })

  it('lights the updates in the sidebar', async () => {
    const { transport } = answering(fixture('updating'), updates())

    const { page } = await render('/updates', transport)

    expect(page.get('[data-testid="sidebar"] nav a[aria-current="page"]').text()).toBe('Updates')
  })
})

/** What the dialog shows: each line, and each caution with its words. */
function shown() {
  return {
    lines: [...document.body.querySelectorAll('[data-testid="command"]')].map(each => each.textContent),
    cautions: [...document.body.querySelectorAll('[data-testid="caution"]')].map(each => each.textContent?.replace(/\s+/g, ' ').trim())
  }
}

describe('each change the page of updates offers', () => {
  it.each([
    ['updates every project', ALL, '[data-testid="update-all"]'],
    ['updates one project', PROJECT, '[data-testid="by-project"] [data-testid="project"]:first-child [data-testid="update-project"]'],
    ['updates one installation in one project', ONE, '[data-testid="state-outdated"] [data-testid="row"]:first-child [data-testid="change"]'],
    ['forces an update over an edited copy', FORCED, '[data-testid="state-edited"] [data-testid="row"] [data-testid="change"]'],
    ['uninstalls what nothing offers any more', UNINSTALL, '[data-testid="state-orphaned"] [data-testid="row"] [data-testid="change"]'],
    ['forces an uninstall of an edited copy nothing offers any more', FORCED_UNINSTALL, '[data-testid="state-edited"] [data-testid="row"]:nth-child(2) [data-testid="change"]'],
    ['takes every orphaned installation out', ORPHANS, '[data-testid="uninstall-orphans"]']
  ])('%s: the task the CLI\'s line is, shown before it runs, asked about when it takes something', async (_, expected, button) => {
    const { sent, transport } = commanding(fixture('updating'), updatings(), updates())
    const { page } = await render('/updates', transport)

    await page.get(button).trigger('click')
    await flushPromises()

    expect(sent).toEqual([{ task: expected.task, mode: 'preview' }])
    const { lines, cautions } = shown()
    expect(lines).toEqual(expected.preview.lines.map(line => `$ ${line}`))
    expect(cautions).toEqual(expected.preview.confirm.map(each => `${each.caution}: ${each.message}`))
    const run = document.body.querySelector<HTMLElement>('[data-testid="run"]')!
    expect(run.textContent?.trim()).toBe(expected.preview.confirm.length > 0 ? 'Confirm and run' : 'Run')

    run.click()
    await flushPromises()

    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: expected.task, mode: 'run' }])
  })

  it('forces nothing without a yes: cancelled, it runs nothing', async () => {
    const { sent, transport } = commanding(fixture('updating'), updatings(), updates())
    const { page } = await render('/updates', transport)

    await page.get('[data-testid="state-edited"] [data-testid="change"]').trigger('click')
    await flushPromises()
    expect(shown().cautions).toEqual(FORCED.preview.confirm.map(each => `${each.caution}: ${each.message}`))
    expect(document.activeElement?.getAttribute('data-testid')).toBe('cancel')
    document.body.querySelector<HTMLElement>('[data-testid="cancel"]')!.click()
    await flushPromises()

    expect(sent.filter(each => each.mode === 'run')).toEqual([])
  })

  it('names each change by what it does, the artifact and the project it is in', async () => {
    const { transport } = answering(fixture('updating'), updates())

    const { page } = await render('/updates', transport)

    expect(page.findAll('[data-testid="change"]').map(button => button.attributes('aria-label'))).toEqual([
      'Update: skill lint in api',
      'Update: rule git/commits in api',
      'Force update: skill lint in web',
      'Force uninstall: skill release in web',
      'Update: skill review in api',
      'Uninstall: skill format in web'
    ])
  })

  it('offers each change forced only where core says only a forced one reaches the copy', async () => {
    const { transport } = answering(fixture('updating'), updates())

    const { page } = await render('/updates', transport)

    const forced = survey().projects
      .flatMap(project => project.installations.map(pending => [`${pending.kind} ${pending.name} in ${project.name}`, pending.forced] as const))
    expect(forced.filter(([, force]) => force)).toEqual([['skill lint in web', true], ['skill release in web', true]])
    for (const [named, force] of forced) {
      const label = page.get(`[data-testid="change"][aria-label$=": ${named}"]`).attributes('aria-label')
      expect(label?.startsWith('Force '), named).toBe(force)
    }
  })
})
