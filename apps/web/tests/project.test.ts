import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { Project } from '@/api/generated/Project'
import type { Snapshot } from '@/api/generated/Snapshot'
import { projectLink } from '@/routes'
import { commanded, commanding, fixture, render, texts, unmount } from './panel'

afterEach(unmount)

function project(snapshot: Snapshot, entry: string): Project {
  return snapshot.projects.find(each => each.entry === entry)!
}

describe('a project\'s page', () => {
  it('groups what the project holds by kind, in the snapshot\'s order', async () => {
    const snapshot = fixture('workspace')
    const web = project(snapshot, 'web')

    const { page } = await render('/project?entry=web', snapshot)

    expect(page.findAll('section[data-testid^="kind-"]').map(section => section.attributes('data-testid')))
      .toEqual(snapshot.kinds.map(kind => `kind-${kind}`))
    for (const kind of snapshot.kinds) {
      const section = page.get(`[data-testid="kind-${kind}"]`)
      const held = web.artifacts.filter(each => each.kind === kind)
      expect(texts(section, '[data-testid="artifact"] th > div:first-child')).toEqual(held.map(each => each.name))
      expect(section.get('[data-testid="kind-count"]').text()).toBe(String(web.tally.kinds.find(counted => counted.kind === kind)!.count))
    }
  })

  it('says of each artifact how it stands, which agents read it, where it came from and where it is held', async () => {
    const snapshot = fixture('workspace')
    const web = project(snapshot, 'web')

    const { page } = await render('/project?entry=web', snapshot)

    const rows = page.get('[data-testid="kind-skill"]').findAll('[data-testid="artifact"]')
    const skills = web.artifacts.filter(each => each.kind === 'skill')
    skills.forEach((held, index) => {
      const row = rows[index]!
      expect(row.get('[data-testid="state"]').text()).toBe(held.state ?? 'no state')
      expect(texts(row, '[data-testid="agent"]')).toEqual(held.agents)
      expect(row.get('[data-testid="origin"]').text()).toBe(held.origin ?? 'none')
      expect(texts(row, '[data-testid="targets"] li')).toEqual(held.targets)
    })
    expect(texts(page, '[data-testid="kind-skill"] [data-testid="state"]')).toEqual(['foreign', 'outdated', 'edited'])
  })

  it('keeps a slot for each of the project\'s agents, and says which does not read it in words', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/project?entry=docs', snapshot)

    const review = page.get('[data-testid="kind-skill"]').findAll('[data-testid="artifact"]')[1]!
    expect(review.get('th').text()).toContain('review')
    expect(texts(review, '[data-testid="agent"]')).toEqual([])
    expect(texts(review, '[data-testid="agent-unreached"]')).toEqual(['codex, not reached'])
  })

  it('warns of what some of its agents never read', async () => {
    const snapshot = fixture('workspace')
    const [unreached] = project(snapshot, 'docs').unreached

    const { page } = await render('/project?entry=docs', snapshot)

    const warning = page.get('[data-testid="unreached"]')
    expect(warning.text()).toContain('Not reached: codex')
    expect(warning.text()).toContain(`Every ${unreached!.kind} it installs goes to ${unreached!.targets[0]}`)
  })

  it('says which file does not read and why, and what validate says', async () => {
    const snapshot = fixture('workspace')
    const notes = project(snapshot, 'docs').artifacts.find(each => each.name === 'notes')!

    const { page } = await render('/project?entry=docs', snapshot)

    const unread = page.get('[data-testid="unread"]')
    expect(unread.text()).toBe(`does not read: ${notes.unread!.message} ${notes.unread!.path}`)
  })

  it('lists an issue validate finds, saying in words whether it is only a warning', async () => {
    const snapshot = fixture('workspace')
    const api = project(snapshot, 'api')
    api.artifacts[0]!.issues = [
      { code: 'description_too_long', length: 1100, warning: true, message: 'the description is 1100 characters' },
      { code: 'name_empty', warning: false, message: 'the name is empty' }
    ]

    const { page } = await render('/project?entry=api', snapshot)

    expect(texts(page, '[data-testid="issue"]')).toEqual([
      'warning: the description is 1100 characters',
      'invalid: the name is empty'
    ])
  })

  it('lists a failure of the project\'s beside what it holds', async () => {
    const snapshot = fixture('workspace')
    const [failure] = project(snapshot, 'web').failures

    const { page } = await render('/project?entry=web', snapshot)

    expect(page.get('[data-testid="failure"]').text()).toContain(failure!.code)
    expect(page.get('[data-testid="failure"]').text()).toContain(failure!.message)
  })

  it('names the commands that would give it a kind it holds none of', async () => {
    const snapshot = fixture('workspace')
    const workspace = snapshot.workspace.path

    const { page } = await render('/project?entry=docs', snapshot)

    expect(texts(page.get('[data-testid="kind-rule"]'), 'code')).toEqual([
      `$ flayer -W ${workspace} install -p docs rule:<NAME>`,
      `$ flayer -W ${workspace} add -p docs rule <NAME> <DESCRIPTION>`
    ])
  })

  it('names the workspace and the entry for a project linked from beside it, which a walk up from it never meets', async () => {
    const snapshot = fixture('workspace')
    const workspace = snapshot.workspace.path
    const docs = project(snapshot, 'docs')
    docs.entry = '../out/docs'
    docs.path = '/home/you/out/docs'

    const { page } = await render('/project?entry=../out/docs', snapshot)

    expect(texts(page.get('[data-testid="kind-rule"]'), 'code')).toEqual([
      `$ flayer -W ${workspace} install -p ../out/docs rule:<NAME>`,
      `$ flayer -W ${workspace} add -p ../out/docs rule <NAME> <DESCRIPTION>`
    ])
  })

  it('says why a stale entry opens nothing, and how to follow it or let it go', async () => {
    const snapshot = fixture('workspace')
    const workspace = snapshot.workspace.path

    const { page } = await render('/project?entry=gone', snapshot)

    const stale = page.get('[data-testid="stale"]')
    expect(stale.text()).toContain(project(snapshot, 'gone').stale!.message)
    expect(texts(stale, 'code')).toEqual([
      `$ flayer -W ${workspace} relink gone <PATH>`,
      `$ flayer -W ${workspace} unlink gone`
    ])
    expect(page.find('[data-testid="kind-skill"]').exists()).toBe(false)
  })
})

describe('the address of a project\'s page', () => {
  it('opens the project it names on its own, as after a reload', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/project?entry=api', snapshot)

    expect(page.get('[data-testid="project"] h1').text()).toBe('api')
    expect(page.get('[data-testid="facts"]').text()).toContain(project(snapshot, 'api').path)
  })

  it('reaches an entry that leads out of the workspace, or is the workspace itself', async () => {
    const snapshot = fixture('workspace')
    const entries = ['..', '../elsewhere/web', '.']
    entries.forEach((entry, index) => {
      snapshot.projects[index]!.entry = entry
    })

    const { page, router } = await render('/', snapshot)

    for (const [index, entry] of entries.entries()) {
      const link = router.resolve(projectLink(entry))
      // A dot segment the browser would resolve away is in the query, never the path.
      expect(link.path).toBe('/project')
      await router.push(link.fullPath)
      await flushPromises()
      expect(router.currentRoute.value.query.entry).toBe(entry)
      expect(page.get('[data-testid="facts"]').text()).toContain(snapshot.projects[index]!.path)
    }
  })

  it('says when no project is at the entry it names', async () => {
    const { page } = await render('/project?entry=nowhere', fixture('workspace'))

    expect(page.get('[data-testid="no-project"]').text()).toContain('links no project as nowhere')
    expect(page.get('[data-testid="no-project"] a').attributes('href')).toBe('/projects')
  })

  it('lights the projects in the sidebar', async () => {
    const { page } = await render('/project?entry=api', fixture('workspace'))

    expect(page.get('[data-testid="sidebar"] nav a[aria-current="page"]').text()).toBe('Projects')
  })
})

describe('unlinking a project from its page', () => {
  it('asks first, in the CLI\'s words, runs nothing until confirmed, then goes back to the projects', async () => {
    const unlink = commanded()[1]!
    const { sent, transport } = commanding()
    const { page, router } = await render('/project?entry=gone', transport)

    await page.get('[data-testid="unlink"]').trigger('click')
    await flushPromises()

    expect(sent).toEqual([{ task: unlink.task, mode: 'preview' }])
    expect([...document.body.querySelectorAll('[data-testid="caution"]')].map(each => each.textContent?.replace(/\s+/g, ' ').trim()))
      .toEqual(unlink.preview.confirm.map(each => `${each.caution}: ${each.message}`))
    document.body.querySelector<HTMLElement>('[data-testid="cancel"]')!.click()
    await flushPromises()
    expect(sent.filter(each => each.mode === 'run')).toEqual([])
    expect(router.currentRoute.value.path).toBe('/project')

    await page.get('[data-testid="unlink"]').trigger('click')
    await flushPromises()
    document.body.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await flushPromises()

    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: unlink.task, mode: 'run' }])
    // The projects' page is loaded as it is first shown.
    await vi.waitFor(() => expect(router.currentRoute.value.path).toBe('/projects'))
  })
})

describe('changing a project\'s agents and share mode from its page', () => {
  /** The toggled task the CLI's suite previewed and ran: `web` turned to name Codex too. */
  const TOGGLED = commanded().find(each => each.task.type === 'toggled')!

  /** The box of `testid` in the settings form, which Reka draws as a button. */
  function box(testid: string): HTMLElement {
    return document.body.querySelector<HTMLElement>(`[data-testid="${testid}"] [role="checkbox"], [role="checkbox"][data-testid="${testid}"]`)!
  }

  it('starts from what the settings say, and offers nothing to apply until a toggle is turned over', async () => {
    const { page } = await render('/project?entry=web', fixture('workspace'))

    const form = page.get('[data-testid="settings-form"]')
    expect(['claude', 'codex', 'opencode'].map(agent => box(`agent-${agent}`).getAttribute('aria-checked'))).toEqual(['true', 'false', 'true'])
    expect(form.get('[data-testid="share-mode"] [role="radio"][aria-checked="true"]').attributes('value')).toBe('shared')
    expect(box('edit-tracked').getAttribute('aria-checked')).toBe('false')
    expect(form.get('[data-testid="apply-settings"]').attributes('disabled')).toBeDefined()

    box('agent-codex').click()
    await flushPromises()
    expect(form.get('[data-testid="apply-settings"]').attributes('disabled')).toBeUndefined()

    box('agent-codex').click()
    await flushPromises()
    expect(form.get('[data-testid="apply-settings"]').attributes('disabled')).toBeDefined()
  })

  it('sends the toggles as they stand beside what the settings said, and shows the lines the task module builds', async () => {
    const { sent, transport } = commanding()
    const { page } = await render('/project?entry=web', transport)

    box('agent-codex').click()
    await flushPromises()
    await page.get('[data-testid="settings-form"]').trigger('submit')
    await flushPromises()

    expect(sent).toEqual([{ task: TOGGLED.task, mode: 'preview' }])
    expect([...document.body.querySelectorAll('[data-testid="command"]')].map(each => each.textContent)).toEqual(TOGGLED.preview.lines.map(line => `$ ${line}`))
    document.body.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await flushPromises()
    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: TOGGLED.task, mode: 'run' }])
  })

  it('offers no toggles for a project an older marker marks, nor for an entry that opens none', async () => {
    const snapshot = fixture('workspace')
    project(snapshot, 'web').migrates = true

    const { page, router } = await render('/project?entry=web', snapshot)
    expect(page.find('[data-testid="settings-form"]').exists()).toBe(false)

    await router.push('/project?entry=gone')
    await flushPromises()
    expect(page.find('[data-testid="settings-form"]').exists()).toBe(false)
  })
})
