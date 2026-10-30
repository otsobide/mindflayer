import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { Contents } from '@/api/generated/Contents'
import type { Files } from '@/api/generated/Files'
import { artifactLink } from '@/routes'
import { answering, answers, commanded, commanding, fixture, render, texts, unmount, type Answered, type Commanded } from './panel'

afterEach(unmount)

/** The answer `answered` holds for the query of `type` about `reference`, and `file` for contents. */
function answer<Answer>(answered: Answered[], type: 'files' | 'contents', reference: string, file?: string): Answer {
  const found = answered.find(each =>
    each.query.type === type
    && 'reference' in each.query && each.query.reference === reference
    && (file === undefined || ('file' in each.query && each.query.file === file))
  )
  return found!.answer as Answer
}

/** Where one of the library's own is shown, as a link to it takes the router. */
function at(kind: 'skill' | 'rule', name: string): string {
  return `/artifact?${new URLSearchParams({ kind, name })}`
}

describe('one of the library\'s own', () => {
  it('says what it is, its kind, what it is for and where the library keeps it', async () => {
    const snapshot = fixture('workspace')
    const review = snapshot.library.artifacts.find(held => held.name === 'review')!

    const { page } = await render(at('skill', 'review'), snapshot)

    expect(page.get('[data-testid="artifact"] h1').text()).toBe('review')
    expect(page.get('[data-testid="name"]').text()).toBe('review')
    expect(page.get('[data-testid="kind"]').text()).toBe('skill')
    expect(page.get('[data-testid="summary"]').text()).toBe(review.summary)
    expect(texts(page, '[data-testid="target"]')).toEqual(review.targets)
  })

  it('draws its front matter as a table, every key in the order written', async () => {
    const answered = answers()
    const contents = answer<Contents>(answered, 'contents', 'skill:review', 'SKILL.md')

    const { page } = await render(at('skill', 'review'), fixture('workspace'))

    expect(page.findAll('[data-testid="declaration"]').map(row => [row.get('th').text(), row.get('td').text()]))
      .toEqual(contents.manifest!.front_matter!.map(({ key, value }) => [key, value]))
    expect(texts(page, '[data-testid="declaration"] th')).toEqual(['name', 'description', 'license', 'metadata'])
  })

  it('renders its body as markdown, below the front matter and without it', async () => {
    const { page } = await render(at('skill', 'review'), fixture('workspace'))

    const body = page.get('[data-testid="body"] [data-testid="markdown"]')
    expect(body.get('h3').text()).toBe('review')
    expect(body.get('h4').text()).toBe('Steps')
    expect(texts(body, 'ol > li')).toHaveLength(3)
    expect(body.get('table th').text()).toBe('check')
    expect(body.get('pre code').text()).toBe('scripts/diff.sh main')
    expect(body.text()).not.toContain('description: Read a change')
    // A file of its own is named, never linked; another site is.
    expect(body.findAll('a').map(link => link.attributes('href'))).toEqual(['https://example.com/review'])
    expect(texts(body, '[data-testid="unlinked"], [data-unlinked]')).toEqual(['the checklist'])
  })

  it('lists its files, a folder before what it holds, each with its size', async () => {
    const answered = answers()
    const files = answer<Files>(answered, 'files', 'skill:review')

    const { page } = await render(at('skill', 'review'), fixture('workspace'))

    const tree = page.get('[data-testid="files"]')
    expect(tree.findAll('[data-testid="entry"]')).toHaveLength(files.entries.length)
    expect(texts(tree, '[data-testid="folder"]')).toEqual(['references/, the folder references', 'scripts/, the folder scripts'])
    expect(texts(tree, '[data-testid="size"]'))
      .toEqual(files.entries.filter(entry => !entry.folder).map(entry => `${entry.size} bytes`))
    // No file of it is ever a link to the file itself.
    expect(tree.findAll('a')).toEqual([])
  })

  it('opens a file from its listing as the text it is, by its route in the address', async () => {
    const answered = answers()
    const checklist = answer<Contents>(answered, 'contents', 'skill:review', 'references/checklist.md')

    const { page, router } = await render(at('skill', 'review'), fixture('workspace'))
    const button = page.findAll('[data-testid="file"]').find(each => each.text().startsWith('checklist.md'))!
    await button.trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.query.file).toBe('references/checklist.md')
    expect(button.attributes('aria-pressed')).toBe('true')
    expect(page.get('[data-testid="opened"] h3').text()).toBe('references/checklist.md')
    expect(page.get('[data-testid="text"]').text()).toBe(checklist.text!.trimEnd())
    // Drawn as text, never as the markdown it is.
    expect(page.find('[data-testid="opened"] h1, [data-testid="opened"] li').exists()).toBe(false)

    await button.trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.query.file).toBeUndefined()
    expect(page.find('[data-testid="opened"]').exists()).toBe(false)
  })

  it('says why a file\'s text is not shown, with its size', async () => {
    const answered = answers()
    const checklist = answer<Contents>(answered, 'contents', 'skill:review', 'references/checklist.md')
    Object.assign(checklist, { text: null, withheld: 'binary', size: 2048 })

    const { page } = await render(`${at('skill', 'review')}&file=references/checklist.md`, answering(fixture('workspace'), answered).transport)

    expect(page.get('[data-testid="opened-withheld"]').text()).toBe('It is not text: 2048 bytes, not shown.')
    expect(page.find('[data-testid="text"]').exists()).toBe(false)
  })

  it('lists a file it does not follow with why, and offers nothing of it to open', async () => {
    const answered = answers()
    const files = answer<Files>(answered, 'files', 'skill:review')
    files.entries.push({
      path: 'secrets',
      folder: false,
      size: null,
      refused: { code: 'contents_outside', message: '/home/you/Projects/.mindflayer/library/skills/review/secrets leads to /home/you/.ssh, outside /home/you/Projects/.mindflayer/library/skills, so it is not read' }
    })

    const { page } = await render(at('skill', 'review'), answering(fixture('workspace'), answered).transport)

    const refused = page.get('[data-testid="refused"]')
    expect(refused.text()).toContain('not followed')
    expect(refused.text()).toContain('contents_outside')
    expect(refused.find('button').exists()).toBe(false)
  })

  it('says what validate says of it, or that it passes', async () => {
    const snapshot = fixture('workspace')
    const lint = snapshot.library.artifacts.find(held => held.name === 'lint')!
    lint.issues = [{ code: 'description_too_long', length: 1100, warning: true, message: 'the description is 1100 characters' }]

    const passing = await render(at('skill', 'review'), snapshot)
    expect(passing.page.get('[data-testid="valid"]').text()).toContain('passes with no warning')
    unmount()

    const { page } = await render(at('skill', 'lint'), snapshot)
    expect(texts(page, '[data-testid="issue"]')).toEqual(['warning: the description is 1100 characters'])
  })

  it('lists the projects holding a copy of it, how each stands and which agents read it', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render(at('skill', 'review'), snapshot)

    const rows = page.findAll('[data-testid="holder"]')
    expect(rows.map(row => [row.get('[data-testid="project"]').text(), row.get('[data-testid="state"]').text()]))
      .toEqual([['api', 'current'], ['web', 'edited'], ['docs', 'current']])
    expect(rows[0]!.get('[data-testid="project"]').attributes('href')).toBe('/project?entry=api')
    const api = snapshot.projects.find(project => project.entry === 'api')!
    expect(texts(rows[0]!, '[data-testid="agent"]')).toEqual(api.artifacts.find(held => held.name === 'review')!.agents)
  })

  it('lists each holder core names, though its copy names no origin', async () => {
    const snapshot = fixture('workspace')
    // As an install recorded before origins were kept names none.
    for (const project of snapshot.projects) {
      for (const copy of project.artifacts.filter(held => held.kind === 'skill' && held.name === 'review')) {
        copy.origin = null
      }
    }

    const { page } = await render(at('skill', 'review'), snapshot)

    expect(page.findAll('[data-testid="holder"] [data-testid="project"]').map(project => project.text())).toEqual(['api', 'web', 'docs'])
    expect(page.find('[data-testid="no-holding"]').exists()).toBe(false)
  })

  it('names the command that installs it when no project holds a copy', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render(at('rule', 'style/comments'), snapshot)

    expect(texts(page.get('[data-testid="no-holding"]'), 'code')).toEqual([
      `$ flayer -W ${snapshot.workspace.path} install -p <PROJECT> rule:style/comments --from ${snapshot.library.origin}`
    ])
  })

  it('reads a rule as its one file, by its own name', async () => {
    const { page } = await render(at('rule', 'git/commits'), fixture('workspace'))

    expect(texts(page, '[data-testid="file"] [data-testid="size"]')).toHaveLength(1)
    expect(page.get('[data-testid="file"]').text()).toContain('commits.md')
    expect(page.get('[data-testid="body"]').text()).toContain('Write commit messages that say why')
  })

  it('says why its file could not be read', async () => {
    const answered = answers().filter(each => !(each.query.type === 'files' && 'reference' in each.query && each.query.reference === 'skill:lint'))

    const { page } = await render(at('skill', 'lint'), answering(fixture('workspace'), answered).transport)

    expect(page.get('[data-testid="unlisted"]').text()).toContain('not_found')
    expect(page.get('[data-testid="unread-manifest"]').text()).toContain('could not be read')
  })

  it('says when the library keeps nothing by the name the address gives', async () => {
    const { page } = await render(at('skill', 'nowhere'), fixture('workspace'))

    expect(page.get('[data-testid="no-artifact"]').text()).toContain('keeps no skill of its own called nowhere')
    expect(page.get('[data-testid="no-artifact"] a').attributes('href')).toBe('/library')
  })

  it('is reached from the library, which the sidebar lights', async () => {
    const { page, router } = await render('/library?kind=rule', fixture('workspace'))

    await page.get('[data-testid="name"]').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.fullPath).toBe(router.resolve(artifactLink('rule', 'git/commits')).fullPath)
    expect(page.get('[data-testid="artifact"] h1').text()).toBe('git/commits')
    expect(page.get('[data-testid="sidebar"] nav a[aria-current="page"]').text()).toBe('Library')
  })
})

describe('removing one of the library\'s own', () => {
  /** Removing `review` everywhere, as the CLI's suite previewed and ran it: the dry run, then the removal. */
  const [DRY, YES] = commanded().filter(each => each.task.type === 'remove' && 'name' in each.task && each.task.name === 'review') as [Commanded, Commanded]

  it('shows what removing it everywhere does first, as the dry run said it, and asks before the removal', async () => {
    const { sent, transport } = commanding(fixture('workspace'), commanded(), answers())
    const { page, router } = await render(at('skill', 'review'), transport)

    expect(page.find('[data-testid="remove-yes"]').exists()).toBe(false)
    await page.get('[data-testid="remove-dry"]').trigger('click')
    await flushPromises()
    expect(sent).toEqual([{ task: DRY.task, mode: 'preview' }])
    expect([...document.body.querySelectorAll('[data-testid="command"]')].map(each => each.textContent)).toEqual(DRY.preview.lines.map(line => `$ ${line}`))
    expect(document.body.querySelectorAll('[data-testid="caution"]')).toHaveLength(0)
    document.body.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await vi.waitFor(() => expect(page.find('[data-testid="remove-said"]').exists()).toBe(true))
    expect(page.get('[data-testid="remove-said"]').text()).toBe(DRY.ran.outcome.runs[0]!.stdout.trim())

    await page.get('[data-testid="remove-yes"]').trigger('click')
    await flushPromises()
    expect(sent.at(-1)).toEqual({ task: YES.task, mode: 'preview' })
    expect([...document.body.querySelectorAll('[data-testid="caution"]')].map(each => each.textContent?.replace(/\s+/g, ' ').trim()))
      .toEqual(YES.preview.confirm.map(each => `${each.caution}: ${each.message}`))
    document.body.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await flushPromises()

    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: DRY.task, mode: 'run' }, { task: YES.task, mode: 'run' }])
    await vi.waitFor(() => expect(router.currentRoute.value.fullPath).toBe('/library?kind=skill'))
  })

  it('removes nothing when the removal is cancelled', async () => {
    const { sent, transport } = commanding(fixture('workspace'), commanded(), answers())
    const { page } = await render(at('skill', 'review'), transport)
    await page.get('[data-testid="remove-dry"]').trigger('click')
    await flushPromises()
    document.body.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await vi.waitFor(() => expect(page.find('[data-testid="remove-yes"]').exists()).toBe(true))

    await page.get('[data-testid="remove-yes"]').trigger('click')
    await flushPromises()
    expect(document.activeElement?.getAttribute('data-testid')).toBe('cancel')
    document.body.querySelector<HTMLElement>('[data-testid="cancel"]')!.click()
    await flushPromises()

    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: DRY.task, mode: 'run' }])
    expect(page.find('[data-testid="remove-said"]').exists()).toBe(true)
  })
})

describe('what somebody else wrote in one', () => {
  it('shows a SKILL.md holding a script tag, and an untrusted name, as the text they are', async () => {
    const untrusted = '<img src=x onerror="alert(document.cookie)">'
    const snapshot = fixture('workspace')
    const review = snapshot.library.artifacts.find(held => held.name === 'review')!
    review.name = untrusted
    review.summary = '<script>alert(1)</script>'
    const answered = answers().filter(each => 'reference' in each.query && each.query.reference === 'skill:review')
    for (const each of answered) {
      Object.assign(each.query, { reference: `skill:${untrusted}` })
    }
    const contents = answer<Contents>(answered, 'contents', `skill:${untrusted}`, 'SKILL.md')
    const text = `---\nname: ${untrusted}\ndescription: Read it\n---\n\n# ${untrusted}\n\n<script>fetch('/api/commands', {method: 'POST'})</script>\n`
    Object.assign(contents, {
      text,
      manifest: {
        front_matter: [{ key: 'name', value: untrusted }, { key: 'description', value: 'Read it' }],
        body: `\n# ${untrusted}\n\n<script>fetch('/api/commands', {method: 'POST'})</script>\n`
      }
    })

    const { page } = await render(at('skill', untrusted), answering(snapshot, answered).transport)
    await page.findAll('[data-testid="file"]').find(each => each.text().startsWith('SKILL.md'))!.trigger('click')
    await flushPromises()

    expect(document.querySelector('script')).toBeNull()
    expect(document.querySelector('img')).toBeNull()
    expect(document.querySelector('[onerror]')).toBeNull()
    expect(page.get('[data-testid="artifact"] h1').text()).toBe(untrusted)
    expect(page.get('[data-testid="name"]').text()).toBe(untrusted)
    expect(page.get('[data-testid="summary"]').text()).toBe('<script>alert(1)</script>')
    expect(page.get('[data-testid="declaration"] td').text()).toBe(untrusted)
    const body = page.get('[data-testid="body"]')
    expect(body.get('h3').text()).toBe(untrusted)
    expect(body.text()).toContain('<script>fetch(\'/api/commands\', {method: \'POST\'})</script>')
    expect(page.get('[data-testid="text"]').text()).toBe(text.trimEnd())
  })
})
