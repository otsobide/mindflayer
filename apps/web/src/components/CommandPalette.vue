<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter, type RouteLocationRaw } from 'vue-router'
import { storeToRefs } from 'pinia'
import { defineShortcuts } from '@nuxt/ui/composables'
import type { CommandPaletteGroup, CommandPaletteItem, InputProps } from '@nuxt/ui'
import type { Snapshot } from '@/api/generated/Snapshot'
import { useRunner } from '@/api/commands'
import { useSnapshot } from '@/api/snapshot'
import { artifactLink, projectLink } from '@/routes'
import { useShell } from '@/stores/shell'
import { VIEWS } from './AppSidebar.vue'

/*
 * The command palette: every page, project, artifact and source the panel
 * shows, searched as one list, over whichever page is open. Each entry is
 * drawn from what the panel already holds, the snapshot, read only while
 * the palette is open; nothing in it is asked for on its own, and a choice
 * goes where a link on a page would.
 */

const shell = useShell()
const { paletteOpen: open } = storeToRefs(shell)
const router = useRouter()
const { busy } = useRunner()
const { data: snapshot } = useSnapshot(open)

/** What is typed, which the palette searches by. */
const term = ref('')

// Ctrl+K, or Cmd+K on a Mac, opens it and closes it. Never while somebody
// types: Nuxt UI passes over a shortcut while a field or anything editable
// has the focus, CodeMirror's editor among them, unless it is told
// otherwise, which this one is not; so the keys stay the field's. Nor
// while a task is shown or runs, whose dialog holds the focus and is
// answered first. Both spellings, since Nuxt UI reads `meta` as Ctrl only
// where the browser says it is no Mac.
function toggle() {
  open.value = !open.value
}
defineShortcuts(computed(() => (busy.value ? {} : { meta_k: toggle, ctrl_k: toggle })))

// The search field goes with the palette, and a browser tells nobody when
// the element with the focus is taken away: vueuse, which Nuxt UI asks what
// has the focus, would go on holding the field, and every shortcut would be
// passed over as typed into it, until something else took the focus. So
// whatever has it in the palette is let go of as it closes, before Reka
// gives the focus back where it was.
watch(open, (now, was) => {
  if (was && !now && document.activeElement instanceof HTMLElement) {
    document.activeElement.blur()
  }
})

/**
 * What a choice does, held until the palette has closed: by then Reka has
 * given the focus back where it was before the palette opened, so whatever
 * the choice opens next takes it from there and gives it back there too.
 */
let chosen: (() => void) | undefined

/** An entry's `onSelect`: close the palette, then do `does`. */
function choose(does: () => void): () => void {
  return () => {
    chosen = does
    open.value = false
  }
}

/** Once the palette has gone: forget what was typed, and do what was chosen. */
function closed() {
  term.value = ''
  const does = chosen
  chosen = undefined
  // A task later, as Reka hands the focus back a task after the palette goes.
  if (does) {
    setTimeout(does, 0)
  }
}

function go(to: RouteLocationRaw): () => void {
  return choose(() => void router.push(to))
}

/**
 * An entry drawn as plain text: the palette would otherwise put what matched
 * on the page as HTML, marked up, and the words are somebody's, a summary or
 * a name, which nothing in the panel puts there as HTML.
 */
function plain(item: CommandPaletteItem): CommandPaletteItem {
  return { ...item, labelHtml: '', suffixHtml: '', descriptionHtml: '' }
}

/**
 * Every entry, by group: the pages in the sidebar's order, then what the
 * snapshot lists, each in its order. Searched, each group comes where its
 * best match ranks, so the first entry highlighted is the nearest.
 */
function grouped(snapshot: Snapshot | undefined): CommandPaletteGroup[] {
  const groups: CommandPaletteGroup[] = [
    {
      id: 'pages',
      label: 'Pages',
      items: VIEWS.map(view => plain({ label: view.label, icon: view.icon, onSelect: go(view.to) }))
    }
  ]
  if (!snapshot) {
    return groups
  }
  groups.push(
    {
      id: 'projects',
      label: 'Projects',
      items: snapshot.projects.flatMap(project => (project.entry === null
        ? []
        : [plain({
            label: project.name ?? project.entry,
            suffix: project.health,
            description: project.path,
            icon: 'i-lucide-folder-git-2',
            onSelect: go(projectLink(project.entry))
          })]))
    },
    {
      id: 'library',
      label: 'Library',
      items: snapshot.library.artifacts.map(held => plain({
        label: held.name,
        suffix: held.kind,
        description: held.summary ?? undefined,
        icon: 'i-lucide-library-big',
        onSelect: go(artifactLink(held.kind, held.name))
      }))
    },
    {
      id: 'shelf',
      label: 'Shelf',
      items: snapshot.shelf.map(shelved => plain({
        label: shelved.name,
        suffix: shelved.kind,
        description: shelved.summary ? `${shelved.origin}: ${shelved.summary}` : shelved.origin,
        icon: 'i-lucide-package',
        onSelect: go({ path: '/shelf', query: { kind: shelved.kind, q: shelved.name } })
      }))
    },
    {
      id: 'sources',
      label: 'Sources',
      items: snapshot.sources.map(source => plain({
        label: source.origin,
        suffix: source.kind,
        icon: 'i-lucide-git-branch',
        onSelect: go('/sources')
      }))
    }
  )
  return groups
}

const groups = computed(() => grouped(snapshot.value))

/**
 * The search field, named: a placeholder is no name to assistive
 * technology. Typed apart from its use, as Nuxt UI's props for the field
 * name none of the attributes it passes on to the element.
 */
const SEARCH: InputProps & { 'aria-label': string } = { 'aria-label': 'Search the panel' }

// Every word typed has to be found somewhere in an entry, its label, the
// word after it or its description, so a second word narrows what the
// first found; each a slip of a finger away at most: Nuxt UI's threshold
// of 0.1 finds nothing for `revew`, and Fuse's own 0.6 finds near anything
// for a word.
const FUSE = { fuseOptions: { useTokenSearch: true, tokenMatch: 'all' as const, threshold: 0.3 } }
</script>

<template>
  <UModal
    v-model:open="open"
    title="Command palette"
    description="Go to a page, a project, one of the library's own, the shelf or a source."
    :ui="{ content: 'sm:max-w-2xl h-full sm:h-[28rem]' }"
    @after:leave="closed"
  >
    <template #content>
      <UCommandPalette
        v-model:search-term="term"
        :groups="groups"
        :fuse="FUSE"
        placeholder="Go to a page, a project or an artifact"
        :input="SEARCH"
        close
        class="h-full"
        data-testid="palette"
        @update:open="open = $event"
      />
    </template>
  </UModal>
</template>
