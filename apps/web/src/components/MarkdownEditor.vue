<script setup lang="ts">
import { onBeforeUnmount, onMounted, useTemplateRef, watch } from 'vue'
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands'
import { markdown } from '@codemirror/lang-markdown'
import { yamlFrontmatter } from '@codemirror/lang-yaml'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { EditorState } from '@codemirror/state'
import { EditorView, drawSelection, highlightActiveLine, keymap, lineNumbers } from '@codemirror/view'
import { tags } from '@lezer/highlight'

const props = defineProps<{
  /** What the editor is called, for whoever cannot see it. */
  label: string
}>()

/** The text, as the editor holds it, a whole file. */
const text = defineModel<string>({ required: true })

const emit = defineEmits<{
  /** Asked for from the keyboard, Ctrl-S or Cmd-S, as an editor saves. */
  save: []
}>()

/*
 * CodeMirror 6, drawn inside a shadow root of its own. Its styles are
 * style-mod's, which writes a `<style>` element into a document, refused by
 * the server's CSP (`default-src 'self'`), and adopts a constructed sheet
 * instead into a shadow root (`adoptedStyleSheets`), which the CSP has no
 * say over: a sheet built through the CSSOM is not an inline style. A nonce
 * was the other way, and would have had the server write one into every
 * page it serves. Whatever else CodeMirror styles it sets through the
 * CSSOM too (`element.style`), never a `style` attribute.
 *
 * The page's look reaches it through its custom properties, which a shadow
 * root inherits as any element does: the palette, the colour mode's
 * switch included, and the monospace font.
 */
const host = useTemplateRef<HTMLDivElement>('host')
let view: EditorView | undefined

/** How the text looks: the panel's colours, by its variables, for each mode alike. */
const look = EditorView.theme({
  '&': {
    color: 'var(--ui-text)',
    backgroundColor: 'var(--ui-bg)',
    border: '1px solid var(--ui-border)',
    borderRadius: '0.375rem',
    fontSize: '0.75rem',
    height: '100%'
  },
  '&.cm-focused': {
    outline: '2px solid var(--ui-primary)',
    outlineOffset: '1px'
  },
  '.cm-scroller': {
    fontFamily: 'var(--font-mono)',
    lineHeight: '1.6'
  },
  '.cm-content': {
    caretColor: 'var(--ui-text-highlighted)',
    padding: '0.5rem 0'
  },
  '.cm-cursor, .cm-dropCursor': {
    borderLeftColor: 'var(--ui-text-highlighted)'
  },
  '.cm-gutters': {
    backgroundColor: 'var(--ui-bg-muted)',
    color: 'var(--ui-text-muted)',
    borderRight: '1px solid var(--ui-border)',
    borderTopLeftRadius: '0.375rem',
    borderBottomLeftRadius: '0.375rem'
  },
  '.cm-activeLine': {
    backgroundColor: 'color-mix(in oklab, var(--ui-bg-elevated) 60%, transparent)'
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'var(--ui-bg-elevated)',
    color: 'var(--ui-text-highlighted)'
  },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': {
    backgroundColor: 'color-mix(in oklab, var(--ui-primary) 25%, transparent)'
  }
})

/** Markdown and its front matter, coloured by what each part is, never only by colour: a heading is bold too. */
const marked = HighlightStyle.define([
  { tag: tags.heading, fontWeight: '600', color: 'var(--ui-text-highlighted)' },
  { tag: tags.strong, fontWeight: '600' },
  { tag: tags.emphasis, fontStyle: 'italic' },
  { tag: [tags.link, tags.url], color: 'var(--ui-primary)', textDecoration: 'underline' },
  { tag: tags.monospace, color: 'var(--ui-info)' },
  { tag: [tags.processingInstruction, tags.meta, tags.contentSeparator], color: 'var(--ui-text-muted)' },
  { tag: [tags.propertyName, tags.definition(tags.propertyName)], color: 'var(--ui-primary)' },
  { tag: [tags.string, tags.content], color: 'var(--ui-text)' },
  { tag: [tags.comment, tags.quote], color: 'var(--ui-text-muted)', fontStyle: 'italic' },
  { tag: [tags.bool, tags.number, tags.null, tags.atom], color: 'var(--ui-warning)' }
])

onMounted(() => {
  const shadow = host.value!.attachShadow({ mode: 'open' })
  view = new EditorView({
    root: shadow,
    parent: shadow,
    state: EditorState.create({
      doc: text.value,
      extensions: [
        // A file written with CRLF is given back with CRLF, rather than
        // turned to LF by a save that changed one line.
        ...(text.value.includes('\r\n') ? [EditorState.lineSeparator.of('\r\n')] : []),
        lineNumbers(),
        history(),
        drawSelection(),
        highlightActiveLine(),
        EditorView.lineWrapping,
        // Tab is left to move the focus on, as it does everywhere else on
        // the page: the editor traps no key a keyboard needs to leave it.
        keymap.of([
          {
            key: 'Mod-s',
            preventDefault: true,
            run: () => {
              emit('save')
              return true
            }
          },
          ...historyKeymap,
          ...defaultKeymap
        ]),
        yamlFrontmatter({ content: markdown() }),
        syntaxHighlighting(marked),
        look,
        EditorView.contentAttributes.of({ 'aria-label': props.label }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            text.value = update.state.doc.toString()
          }
        })
      ]
    })
  })
})

// The text changed from outside, read again from the disk: the editor
// shows it, its undo history kept, so reading again can be undone.
watch(text, (value) => {
  if (view && value !== view.state.doc.toString()) {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } })
  }
})

onBeforeUnmount(() => {
  view?.destroy()
  view = undefined
})

/** Put the focus in the text, as a click there would. */
function focus() {
  view?.focus()
}

defineExpose({ focus })
</script>

<template>
  <div
    ref="host"
    class="min-h-80"
    data-testid="editor"
  />
</template>
