import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises, type DOMWrapper } from '@vue/test-utils'
import type { Cell } from '@/api/generated/Cell'
import type { Offered } from '@/api/generated/Offered'
import type { Snapshot } from '@/api/generated/Snapshot'
import type { Query } from '@/api/transport'
import { answering, applying, commanding, fixture, plans, render, texts, unmount, type Answered } from './panel'

afterEach(unmount)

/** What a project is called in a column's heading: its name, or its entry. */
function titles(snapshot: Snapshot): string[] {
  return snapshot.projects.map(project => project.name ?? project.entry!)
}

/** The offers, grouped by origin as the matrix groups them: each origin where its first offer is. */
function grouped(snapshot: Snapshot): { origin: string, offers: Offered[] }[] {
  const groups: { origin: string, offers: Offered[] }[] = []
  for (const offered of snapshot.offers) {
    const group = groups.find(each => each.origin === offered.origin)
    if (group) {
      group.offers.push(offered)
    } else {
      groups.push({ origin: offered.origin, offers: [offered] })
    }
  }
  return groups
}

type Page = Awaited<ReturnType<typeof render>>['page']

/** The box of `name` from `origin` in the project titled `project`, by what it says it is. */
function box(page: Page, kind: string, name: string, origin: string, project: string): DOMWrapper<HTMLElement> {
  const label = `${kind} ${name} from ${origin} in ${project}`
  const found = page.findAll<HTMLElement>('[data-testid="box"]')
    .filter(each => each.attributes('aria-label')!.split('; ')[0] === label)
  expect(found, label).toHaveLength(1)
  return found[0]!
}

/** Each plan the page asked for, by its boxes, in order. */
function planned(queries: Query[]): Cell[][] {
  return queries.flatMap(query => (query.type === 'plan' ? [query.selection] : []))
}

/** The selections the fixture plans were asked with, in order. */
function selections(): Cell[][] {
  return plans().map(answered => (answered.query as Extract<Query, { type: 'plan' }>).selection)
}

describe('the matrix', () => {
  it('draws every offer, grouped by where it comes from, against every project, ticked as the snapshot says', async () => {
    const snapshot = fixture('assigning')

    const { page } = await render('/assign', snapshot)

    const groups = grouped(snapshot)
    expect(texts(page, '[data-testid="matrix-group-label"]')).toEqual(groups.map(group => group.origin))
    expect(texts(page, '[data-testid="matrix-row-label"]')).toEqual(groups.flatMap(group => group.offers.map(offered => offered.name)))
    expect(texts(page, '[data-testid="matrix-column-label"]')).toEqual(titles(snapshot))
    for (const project of snapshot.projects) {
      const boxes = snapshot.boxes.find(each => each.entry === project.entry)!
      for (const offered of snapshot.offers) {
        const shown = box(page, offered.kind, offered.name, offered.origin, project.name ?? project.entry!)
        const held = boxes.held.find(each => each.kind === offered.kind && each.name === offered.name && each.origin === offered.origin)
        const closed = boxes.closed.find(each => each.kind === offered.kind)
        expect(shown.attributes('aria-checked'), shown.attributes('aria-label')).toBe(String(held?.ticked ?? false))
        expect(shown.attributes('data-state')).toBe(held?.state)
        expect(shown.attributes('aria-disabled')).toBe(closed ? 'true' : undefined)
        if (closed) {
          expect(shown.attributes('aria-label')).toContain(`${closed.why.code}: ${closed.why.message}`)
        }
      }
    }
  })

  it('says each state a box stands in, as core says it, by an icon and in its words', async () => {
    const { page } = await render('/assign', fixture('assigning'))

    const web = box(page, 'skill', 'review', 'workspace', 'web')
    expect(web.attributes('aria-label')).toBe('skill review from workspace in web; edited')
    expect(web.find('svg').exists()).toBe(true)
    const triage = box(page, 'skill', 'triage', 'https://github.com/acme/skills', 'web')
    expect(triage.attributes('aria-checked')).toBe('true')
    expect(triage.attributes('data-state')).toBe('foreign')
    const deploy = box(page, 'skill', 'deploy', 'workspace', 'api')
    expect(deploy.attributes('aria-checked')).toBe('false')
    expect(deploy.attributes('data-state')).toBe('current')
    expect(box(page, 'skill', 'deploy', 'load:../team', 'api').attributes('aria-checked')).toBe('true')
    expect(texts(page, '[data-testid="legend"] li')).toEqual(expect.arrayContaining(['current', 'outdated', 'edited', 'foreign']))
  })

  it('turned round by project, has a row a project and a column an offer, the origins along the top', async () => {
    const snapshot = fixture('assigning')

    const { page } = await render('/assign?by=project', snapshot)

    expect(page.get('[data-testid="by-project"]').attributes('aria-pressed')).toBe('true')
    expect(texts(page, '[data-testid="matrix-row-label"]')).toEqual(titles(snapshot))
    expect(page.findAll('[data-testid="matrix-group"]')).toHaveLength(0)
    const groups = grouped(snapshot)
    expect(texts(page, '[data-testid="matrix-column-label"]')).toEqual(groups.flatMap(group => group.offers.map(offered => offered.name)))
    expect(texts(page, '[data-testid="matrix-band"]')).toEqual(groups.map(group => group.origin))
    expect(box(page, 'skill', 'notes', 'load:../team', 'docs').attributes('data-state')).toBe('outdated')
  })

  it('is filtered by kind, origin and text over the offers, and by agent over the projects', async () => {
    const snapshot = fixture('assigning')

    const { page, router } = await render('/assign?kind=rule', snapshot)
    expect(texts(page, '[data-testid="matrix-row-label"]')).toEqual(['git/commits'])

    await router.push('/assign?origin=load:../team')
    await flushPromises()
    expect(texts(page, '[data-testid="matrix-row-label"]')).toEqual(['deploy', 'notes'])
    expect(texts(page, '[data-testid="matrix-group-label"]')).toEqual(['load:../team'])

    await router.push('/assign?q=issues')
    await flushPromises()
    expect(texts(page, '[data-testid="matrix-row-label"]')).toEqual(['triage'])

    await router.push('/assign?agent=codex')
    await flushPromises()
    const codex = snapshot.projects.filter(project => project.agents.includes('codex'))
    expect(texts(page, '[data-testid="matrix-column-label"]')).toEqual(codex.map(project => project.name!))

    await router.push('/assign?q=nothing-is-called-this')
    await flushPromises()
    expect(page.find('[data-testid="matrix"]').exists()).toBe(false)
    await page.get('[data-testid="no-match"] button').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.query).toEqual({})
  })

  it('moves from box to box with the arrow keys, over the groups\' headings, with one box in the tab order', async () => {
    const { page } = await render('/assign', fixture('assigning'))
    const tabbable = () => page.findAll<HTMLElement>('[data-testid="box"]').filter(each => each.attributes('tabindex') === '0')
    expect(tabbable()).toHaveLength(1)

    const first = tabbable()[0]!
    expect(first.attributes('aria-label')!.split('; ')[0]).toBe('skill deploy from workspace in api')
    first.element.focus()
    await first.trigger('keydown', { key: 'ArrowRight' })
    await flushPromises()
    expect((document.activeElement as HTMLElement).getAttribute('aria-label')!.split('; ')[0]).toBe('skill deploy from workspace in web')

    // From the last offer from the library down over the team's heading.
    const commits = box(page, 'rule', 'git/commits', 'workspace', 'web')
    commits.element.focus()
    await commits.trigger('keydown', { key: 'ArrowDown' })
    await flushPromises()
    expect((document.activeElement as HTMLElement).getAttribute('aria-label')!.split('; ')[0]).toBe('skill deploy from load:../team in web')
    expect(tabbable().map(each => each.attributes('aria-label')!.split('; ')[0])).toEqual(['skill deploy from load:../team in web'])

    await box(page, 'skill', 'deploy', 'load:../team', 'web').trigger('keydown', { key: 'End' })
    await flushPromises()
    expect((document.activeElement as HTMLElement).getAttribute('aria-label')!.split('; ')[0]).toBe('skill deploy from load:../team in gone')
  })
})

describe('marking boxes', () => {
  it('sends the boxes marked to be planned, in the order they were marked, and nothing for a box marked back', async () => {
    const [simple] = selections()
    const { queries, transport } = answering(fixture('assigning'), plans())

    const { page } = await render('/assign', transport)
    expect(page.find('[data-testid="plan"]').exists()).toBe(false)

    await box(page, 'skill', 'format', 'https://github.com/acme/skills', 'api').trigger('click')
    await box(page, 'skill', 'format', 'https://github.com/acme/skills', 'docs').trigger('click')
    await flushPromises()

    expect(planned(queries).at(-1)).toEqual(simple)
    const format = box(page, 'skill', 'format', 'https://github.com/acme/skills', 'api')
    expect(format.attributes('aria-checked')).toBe('true')
    expect(format.attributes('data-changed')).toBe('true')
    expect(format.attributes('aria-label')).toContain('marked to be ticked')
    expect(page.get('[data-testid="marked"]').text()).toBe('2 marked')

    // Marked back to what it showed: no mark.
    await format.trigger('click')
    await flushPromises()
    expect(planned(queries).at(-1)).toEqual([simple![1]])
    await box(page, 'skill', 'format', 'https://github.com/acme/skills', 'docs').trigger('click')
    await flushPromises()
    expect(page.find('[data-testid="plan"]').exists()).toBe(false)
  })

  it('unticks a box ticked, asking it away, and marks nothing in a project none of its kind can go into', async () => {
    const [, replaced] = selections()
    const { queries, transport } = answering(fixture('assigning'), plans())
    const { page } = await render('/assign', transport)

    await box(page, 'skill', 'format', 'https://github.com/acme/skills', 'bare').trigger('click')
    await box(page, 'skill', 'format', 'https://github.com/acme/skills', 'gone').trigger('click')
    await flushPromises()
    expect(planned(queries)).toEqual([])

    await box(page, 'skill', 'deploy', 'workspace', 'api').trigger('click')
    await flushPromises()
    expect(planned(queries).at(-1)).toEqual(replaced)
    expect(texts(page, '[data-testid="line"]')).toEqual([])
  })

  it('draws the locks and the conflicts of the plan it is answered with, on the boxes and below', async () => {
    const [, , heldBack] = selections()
    const answer = plans()[2]!.answer as { conflicts: { cells: Cell[], why: { code: string, message: string } }[], locked: { cell: Cell, why: { code: string, message: string } }[] }
    const { queries, transport } = answering(fixture('assigning'), plans())
    const { page } = await render('/assign', transport)

    for (const cell of heldBack!) {
      const [kind, name] = cell.reference.split(':') as [string, string]
      const project = cell.project
      await box(page, kind, name, cell.origin, project).trigger('click')
    }
    await flushPromises()

    expect(planned(queries).at(-1)).toEqual(heldBack)
    expect(texts(page, '[data-testid="conflict"]')).toEqual(answer.conflicts.map(conflict =>
      `docs skill:deploy, ${conflict.why.code}: ${conflict.why.message}`))
    expect(texts(page, '[data-testid="lock"]')).toEqual(answer.locked.map(locked =>
      `${locked.cell.project} ${locked.cell.reference} from ${locked.cell.origin}, ${locked.why.code}: ${locked.why.message}`))
    for (const conflict of answer.conflicts) {
      for (const cell of conflict.cells) {
        const [kind, name] = cell.reference.split(':') as [string, string]
        const shown = box(page, kind, name, cell.origin, cell.project)
        expect(shown.attributes('data-held')).toBe('conflict')
        expect(shown.attributes('aria-label')).toContain(`held back, ${conflict.why.code}`)
      }
    }
    for (const locked of answer.locked) {
      const [kind, name] = locked.cell.reference.split(':') as [string, string]
      const shown = box(page, kind, name, locked.cell.origin, locked.cell.project)
      expect(shown.attributes('data-held')).toBe('locked')
      expect(shown.attributes('aria-label')).toContain(`held back, ${locked.why.code}: ${locked.why.message}`)
    }
    expect(box(page, 'skill', 'format', 'https://github.com/acme/skills', 'api').attributes('data-held')).toBeUndefined()
  })

  it('says why a plan was refused, with its code, and applies nothing', async () => {
    const { transport } = answering(fixture('assigning'), [])
    const { page } = await render('/assign', transport)

    await box(page, 'skill', 'notes', 'load:../team', 'api').trigger('click')
    await flushPromises()

    expect(page.get('[data-testid="plan-refused"]').text()).toContain('not_found')
    expect(page.findAll('[data-testid="line"]')).toHaveLength(0)
    expect(page.get('[data-testid="apply"]').attributes('disabled')).toBeDefined()
  })
})

describe('applying a plan', () => {
  it('shows the lines the plan\'s tasks are, sends them back as one batch, and clears the marks once it ran', async () => {
    const [simple] = selections()
    const [applied] = applying()
    const answered: Answered[] = plans()
    const { sent, transport } = commanding(fixture('assigning'), applying(), answered)
    const { page } = await render('/assign', transport)

    for (const cell of simple!) {
      await box(page, 'skill', 'format', cell.origin, cell.project).trigger('click')
    }
    await flushPromises()
    expect(texts(page, '[data-testid="line"]')).toEqual(applied!.preview.lines.map(line => `$ ${line}`))

    await page.get('[data-testid="apply"]').trigger('click')
    await flushPromises()
    const dialog = document.querySelector('[role="dialog"]')!
    expect([...dialog.querySelectorAll('[data-testid="command"]')].map(line => line.textContent!.trim()))
      .toEqual(applied!.preview.lines.map(line => `$ ${line}`))
    ;(dialog.querySelector('[data-testid="run"]') as HTMLElement).click()
    await flushPromises()

    expect(sent.filter(each => each.mode === 'run').map(each => each.task)).toEqual([applied!.task])
    expect(applied!.task).toEqual({ type: 'apply_install', changes: (answered[0]!.answer as { tasks: unknown[] }).tasks })
    expect(page.find('[data-testid="plan"]').exists()).toBe(false)
    expect(box(page, 'skill', 'format', simple![0]!.origin, 'api').attributes('data-changed')).toBeUndefined()
  })
})
