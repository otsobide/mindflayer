import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { Project } from '@/api/generated/Project'
import type { Snapshot } from '@/api/generated/Snapshot'
import { projectLink } from '@/routes'
import { fixture, render, texts, unmount } from './panel'

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

    const { page } = await render('/project?entry=docs', snapshot)

    const rules = page.get('[data-testid="kind-rule"]')
    expect(rules.get('[data-testid="kind-empty"]').text()).toContain(project(snapshot, 'docs').path)
    expect(texts(rules, 'code')).toContain('$ mind install rule:<NAME>')
    expect(texts(rules, 'code')).toContain('$ mind add rule <NAME> <DESCRIPTION>')
  })

  it('says why a stale entry opens nothing, and how to follow it or let it go', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/project?entry=gone', snapshot)

    const stale = page.get('[data-testid="stale"]')
    expect(stale.text()).toContain(project(snapshot, 'gone').stale!.message)
    expect(texts(stale, 'code')).toEqual(['$ flayer relink gone <PATH>', '$ flayer unlink gone'])
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
