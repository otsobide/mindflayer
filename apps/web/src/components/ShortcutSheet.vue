<script setup lang="ts">
import { computed } from 'vue'
import { storeToRefs } from 'pinia'
import { defineShortcuts, useKbd } from '@nuxt/ui/composables'
import { useRunner } from '@/api/commands'
import { useShell } from '@/stores/shell'

/*
 * Every key the panel answers, on `?`: what each component does with the
 * keyboard, said in one place. The keys are the components' own (the
 * palette's, the dialog's, the matrix's, the editor's), so this lists them
 * and changes none.
 */

const shell = useShell()
const { shortcutsOpen: open } = storeToRefs(shell)
const { busy } = useRunner()
const { macOS } = useKbd()

// `?` opens it and closes it, the palette going if it was open; never while
// a field or the editor has the focus, where `?` is a character typed, nor
// while a task is shown or runs, as the palette's keys are not.
function toggle() {
  shell.paletteOpen = false
  open.value = !open.value
}
defineShortcuts(computed(() => (busy.value ? {} : { '?': toggle })))

/**
 * One shortcut: each way to press it, a chord of keys as Nuxt UI's `UKbd`
 * spells them (`meta` is Cmd on a Mac and Ctrl elsewhere, `ctrl` Ctrl on
 * either), and what it does.
 */
interface Shortcut {
  keys: string[][]
  does: string
}

interface Section {
  title: string
  shortcuts: Shortcut[]
}

const SECTIONS: Section[] = [
  {
    title: 'Anywhere but a field or the editor',
    shortcuts: [
      { keys: [['meta', 'K']], does: 'Open or close the command palette' },
      { keys: [['?']], does: 'Show or hide these shortcuts' }
    ]
  },
  {
    title: 'In the command palette',
    shortcuts: [
      { keys: [['arrowup'], ['arrowdown']], does: 'Move among what it lists' },
      { keys: [['enter']], does: 'Go there, or choose the change: an install lists the projects it can go into' },
      { keys: [['backspace']], does: 'Back from those projects, while nothing is typed' },
      { keys: [['escape'], ['meta', 'K']], does: 'Close it, the focus back where it was' }
    ]
  },
  {
    title: 'Asked about a change',
    shortcuts: [
      { keys: [['tab']], does: 'Move between Cancel and Run' },
      { keys: [['enter']], does: 'Press the button with the focus: Run, or Cancel when a line takes something away' },
      { keys: [['escape']], does: 'Cancel, with nothing run' }
    ]
  },
  {
    title: 'On the matrix, in Assign',
    shortcuts: [
      { keys: [['arrowup'], ['arrowdown'], ['arrowleft'], ['arrowright']], does: 'Move one box' },
      { keys: [['Page Up'], ['Page Down']], does: 'Move ten rows' },
      { keys: [['Home'], ['End']], does: 'The first or the last box of the row' },
      { keys: [['ctrl', 'Home'], ['ctrl', 'End']], does: 'The first or the last box of the matrix' },
      { keys: [['Space'], ['enter']], does: 'Mark the box the other way round, or take its mark away' }
    ]
  },
  {
    title: 'In the editor',
    shortcuts: [
      { keys: [['meta', 'S']], does: 'Save, its line shown first' },
      { keys: [['tab']], does: 'Leave the editor' }
    ]
  }
]

/** Each key as a screen reader is to say it, where its glyph would be read as a symbol. */
const SAID: Record<string, string> = {
  'ctrl': 'Control',
  'arrowup': 'Up arrow',
  'arrowdown': 'Down arrow',
  'arrowleft': 'Left arrow',
  'arrowright': 'Right arrow',
  'enter': 'Enter',
  'escape': 'Escape',
  'backspace': 'Backspace',
  'tab': 'Tab',
  '?': 'Question mark'
}

/** The ways to press `shortcut`, in words. */
function said(shortcut: Shortcut): string {
  const word = (key: string) => (key === 'meta' ? (macOS.value ? 'Command' : 'Control') : SAID[key] ?? key)
  return shortcut.keys.map(chord => chord.map(word).join(' ')).join(' or ')
}
</script>

<template>
  <UModal
    v-model:open="open"
    title="Keyboard shortcuts"
    description="What the keys do in the panel. None of them is taken while you type in a field or in the editor, whose keys they stay."
    :ui="{ content: 'sm:max-w-2xl' }"
  >
    <template #body>
      <div
        class="flex flex-col gap-6"
        data-testid="shortcuts"
      >
        <section
          v-for="(section, index) in SECTIONS"
          :key="section.title"
          :aria-labelledby="`shortcuts-${index}`"
          data-testid="shortcut-section"
        >
          <h3
            :id="`shortcuts-${index}`"
            class="mb-2 text-xs font-medium text-muted"
          >
            {{ section.title }}
          </h3>
          <dl class="grid grid-cols-[minmax(0,12rem)_1fr] gap-x-4 gap-y-2 text-sm">
            <template
              v-for="shortcut in section.shortcuts"
              :key="shortcut.does"
            >
              <dt
                class="flex flex-wrap items-center gap-1"
                data-testid="keys"
              >
                <span class="sr-only">{{ said(shortcut) }}</span>
                <template
                  v-for="(chord, way) in shortcut.keys"
                  :key="way"
                >
                  <span
                    v-if="way > 0"
                    class="text-xs text-dimmed"
                    aria-hidden="true"
                  >or</span>
                  <span
                    class="inline-flex items-center gap-0.5"
                    aria-hidden="true"
                  >
                    <UKbd
                      v-for="key in chord"
                      :key="key"
                      :value="key"
                    />
                  </span>
                </template>
              </dt>
              <dd
                class="text-default"
                data-testid="does"
              >
                {{ shortcut.does }}
              </dd>
            </template>
          </dl>
        </section>
      </div>
    </template>
  </UModal>
</template>
