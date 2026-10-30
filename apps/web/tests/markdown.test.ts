import { afterEach, describe, expect, it } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import MarkdownBody from '@/components/MarkdownBody.vue'

let mounted: VueWrapper | undefined

afterEach(() => {
  mounted?.unmount()
  mounted = undefined
  document.body.innerHTML = ''
})

/** `text` drawn on a page of its own. */
function draw(text: string): VueWrapper {
  mounted = mount(MarkdownBody, { props: { text }, attachTo: document.body })
  return mounted
}

/** Every attribute on every element the drawing made, as `tag attribute`. */
function attributes(page: VueWrapper): string[] {
  return [page.element, ...page.element.querySelectorAll('*')].flatMap(element =>
    [...element.attributes].map(attribute => `${element.tagName.toLowerCase()} ${attribute.name}`)
  )
}

describe('markdown somebody else wrote', () => {
  it('shows a script, and any other markup, as the text it is', () => {
    const page = draw('# Deploy <b>now</b>\n\nRun it. <script>alert(document.cookie)</script>\n\n<div onclick="steal()" style="color: red">block</div>\n')

    expect(document.querySelector('script')).toBeNull()
    expect(document.querySelector('b')).toBeNull()
    expect(document.querySelector('div[onclick]')).toBeNull()
    expect(page.text()).toContain('<script>')
    expect(page.text()).toContain('</script>')
    expect(page.text()).toContain('<div onclick="steal()" style="color: red">block</div>')
    expect(page.get('h3').text()).toBe('Deploy <b>now</b>')
  })

  it('gives no element a style or an event, only the classes it chose', () => {
    const page = draw('| left | right | centre |\n| :--- | ---: | :---: |\n| a | b | c |\n\n<p style="x" onmouseover="y">p</p>\n')

    const made = attributes(page)
    expect(made.filter(each => /\s(style|on\w+)$/.test(each))).toEqual([])
    expect(page.findAll('th').map(cell => cell.classes())).toEqual([
      expect.arrayContaining(['text-left']),
      expect.arrayContaining(['text-right']),
      expect.arrayContaining(['text-center'])
    ])
  })

  it('links only to another site, in a tab of its own, and never to a file beside the skill', () => {
    const page = draw([
      '[the guide](https://example.com/guide)',
      '[a reference](references/guide.md)',
      '[above](../../secrets.md)',
      '[here](#steps)',
      '[run](javascript:alert(1))',
      '[inline](data:text/html,<script>alert(1)</script>)',
      '[mail](mailto:team@example.com)'
    ].join('\n\n'))

    const links = page.findAll('a')
    expect(links.map(link => link.attributes('href'))).toEqual(['https://example.com/guide', 'mailto:team@example.com'])
    for (const link of links) {
      expect(link.attributes('target')).toBe('_blank')
      expect(link.attributes('rel')).toBe('noopener noreferrer nofollow')
    }
    expect(page.findAll('[data-unlinked]').map(text => text.text())).toEqual(['a reference', 'above', 'here', 'run', 'inline'])
    expect(page.html()).not.toContain('javascript:')
    expect(page.html()).not.toContain('data:')
  })

  it('loads no image, saying what it showed instead', () => {
    const page = draw('![the logo](logo.png) ![](https://example.com/pixel.gif) ![x](data:image/png;base64,AAAA)')

    expect(document.querySelector('img')).toBeNull()
    expect(page.findAll('[data-image]').map(image => image.text())).toEqual(['[image: the logo]', '[image: no description]', '[image: x]'])
    expect(page.html()).not.toContain('data:image')
    expect(page.html()).not.toContain('example.com')
  })

  it('keeps its headings below the section it is drawn in, a level at a time', () => {
    const page = draw('## Steps\n\n#### Deep\n\n# Top\n\n###### Deepest\n')

    expect(page.findAll('h3, h4, h5, h6').map(heading => `${heading.element.tagName} ${heading.text()}`)).toEqual([
      'H3 Steps',
      'H4 Deep',
      'H3 Top',
      'H4 Deepest'
    ])
  })

  it('draws lists, fences, quotes and emphasis with the elements they are', () => {
    const page = draw('1. one\n2. **two**\n\n- [x] done\n- [ ] not yet\n\n> quoted *text*\n\n```sh\necho <hi>\n```\n\n---\n\n`<code>` and ~~gone~~\n')

    expect(page.findAll('ol > li').map(item => item.text())).toEqual(['one', 'two'])
    expect(page.findAll('ul > li').map(item => item.text())).toEqual(['[x] done', '[ ] not yet'])
    expect(page.get('blockquote em').text()).toBe('text')
    expect(page.get('pre code').text()).toBe('echo <hi>')
    expect(page.find('hr').exists()).toBe(true)
    expect(page.findAll('p > code').map(code => code.text())).toEqual(['<code>'])
    expect(page.get('del').text()).toBe('gone')
  })

  it('draws a task box once, in a loose list as in a tight one', () => {
    const page = draw('1. [ ] loose\n\n2. [x] two\n')

    expect(page.findAll('ol > li').map(item => item.text())).toEqual(['[ ] loose', '[x] two'])
    expect(page.findAll('ol > li code').map(code => code.text())).toEqual(['[ ]', '[x]'])
    expect(page.findAll('ol > li > p').map(paragraph => paragraph.text())).toEqual(['[ ] loose', '[x] two'])
  })
})
