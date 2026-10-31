<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter, type RouteLocationRaw } from 'vue-router'
import { storeToRefs } from 'pinia'
import { defineShortcuts, useToast } from '@nuxt/ui/composables'
import type { CommandPaletteGroup, CommandPaletteItem, InputProps } from '@nuxt/ui'
import type { Offered } from '@/api/generated/Offered'
import type { Planned } from '@/api/generated/Planned'
import type { Project } from '@/api/generated/Project'
import type { Said } from '@/api/generated/Said'
import type { Snapshot } from '@/api/generated/Snapshot'
import { useRunner } from '@/api/commands'
import { batchOf, usePlanOnce } from '@/api/plan'
import { useSnapshot } from '@/api/snapshot'
import { TransportError } from '@/api/transport'
import { UPDATE_ALL } from '@/api/updates'
import { boxOf, found, named } from '@/boxes'
import { artifactLink, projectLink } from '@/routes'
import { useShell } from '@/stores/shell'
import { VIEWS } from './AppSidebar.vue'

/*
 * The command palette: every page, project, artifact and source the panel
 * shows, and a few of its changes, searched as one list over whichever
 * page is open. Each entry is drawn from what the panel already holds, the
 * snapshot, read only while the palette is open; nothing in it is asked
 * for on its own. A choice goes where a link on a page would, or sends the
 * very task a page sends for the same change, through the one runner every
 * page asks, so its lines are previewed by the server and shown, and
 * confirmed where the CLI says they take something away, before they run.
 */

const shell = useShell()
const { paletteOpen: open } = storeToRefs(shell)
const router = useRouter()
const runner = useRunner()
const { busy } = runner
const { data: snapshot } = useSnapshot(open)
const planOnce = usePlanOnce()
const toast = useToast()

/** What is typed, which the palette searches by. */
const term = ref('')

// Ctrl+K, or Cmd+K on a Mac, opens it and closes it, the sheet of
// shortcuts going if it was open. Never while somebody types in a page:
// Nuxt UI passes over a shortcut while a field or anything editable has the
// focus, CodeMirror's editor among them, unless it is told otherwise; so
// the keys stay the field's. It is told otherwise only while the palette is
// open, whose modal holds the focus, so the field with it is the palette's
// own search, which would otherwise hand the keys to the browser (Chromium
// puts the focus in its own search bar) and leave the palette open. Nor
// while a task is shown or runs, whose dialog holds the focus and is
// answered first. Both spellings, since Nuxt UI reads `meta` as Ctrl only
// where the browser says it is no Mac.
function toggle() {
  shell.shortcutsOpen = false
  open.value = !open.value
}
defineShortcuts(computed(() => {
  if (busy.value) {
    return {}
  }
  const shortcut = open.value ? { handler: toggle, usingInput: true } : toggle
  return { meta_k: shortcut, ctrl_k: shortcut }
}))

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

/** Say that nothing ran, and why, by its code, as the runner says a refusal. */
function nothingRan(message: string, code: string | undefined) {
  toast.add({
    title: 'Nothing ran',
    description: code ? `${message} (${code})` : message,
    color: 'error',
    icon: 'i-lucide-ban'
  })
}

/**
 * Install `offered` into `project` as ticking its one box on the matrix
 * does: core plans that box (`Query::Plan`), which says the offer to
 * install from, what it replaces, and what holds it back, and the plan's
 * tasks go to the runner as the matrix's Apply sends them. Nothing of the
 * rules of an install is told here; a box held back runs nothing, and the
 * plan's reason, with its code, says why.
 */
async function install(offered: Offered, project: Project) {
  let plan: Planned
  try {
    plan = await planOnce([{ ...boxOf(project, offered), wanted: true }])
  } catch (error) {
    nothingRan(error instanceof Error ? error.message : String(error), error instanceof TransportError ? error.code : undefined)
    return
  }
  const batch = batchOf(plan)
  if (batch) {
    await runner.ask(batch)
    return
  }
  const held: Said[] = [...plan.locked.map(locked => locked.why), ...plan.conflicts.map(conflict => conflict.why)]
  for (const why of held) {
    nothingRan(why.message, why.code)
  }
  // Nothing to run and nothing held back: the box asks for what is so,
  // since the snapshot the palette listed it from, read when it opened.
  // Said in the plan's terms, which give no reason of their own.
  if (held.length === 0) {
    nothingRan(`The plan of ${offered.kind}:${offered.name} from ${offered.origin} into ${title(project)} runs nothing.`, undefined)
  }
}

/** What a project is called: its name, or its entry when it opens no project. */
function title(project: Project): string {
  return project.name ?? named(project)
}

/**
 * Each offer as an entry choosing where it goes: the projects whose box of
 * it is neither ticked, which it holds from there already, nor closed,
 * which nothing of its kind can go into, as the snapshot draws them on the
 * matrix. Looked up, never worked out; an offer no box of which can be
 * ticked is not listed.
 */
function installs(snapshot: Snapshot): CommandPaletteItem[] {
  const boxes = found(snapshot)
  return snapshot.offers.flatMap((offered) => {
    const into = snapshot.projects.filter(project => !boxes.closed(named(project), offered.kind)
      && !boxes.held(boxOf(project, offered))?.ticked)
    if (into.length === 0) {
      return []
    }
    return [plain({
      label: `Install ${offered.name}`,
      suffix: offered.kind,
      description: offered.summary ? `from ${offered.origin}: ${offered.summary}` : `from ${offered.origin}`,
      icon: 'i-lucide-package-plus',
      placeholder: `Install ${offered.name}, from ${offered.origin}, into a project`,
      children: into.map(project => plain({
        label: title(project),
        suffix: project.agents.length > 0 ? project.agents.join(', ') : 'no agent',
        description: project.path,
        icon: 'i-lucide-folder-git-2',
        onSelect: choose(() => void install(offered, project))
      }))
    })]
  })
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
 * Every entry, by group: the pages in the sidebar's order and the actions,
 * then each offer to install and what else the snapshot lists, each in its
 * order. Searched, each group comes where its best match ranks, so the
 * first entry highlighted is the nearest.
 */
function grouped(snapshot: Snapshot | undefined): CommandPaletteGroup[] {
  const groups: CommandPaletteGroup[] = [
    {
      id: 'pages',
      label: 'Pages',
      items: VIEWS.map(view => plain({ label: view.label, icon: view.icon, onSelect: go(view.to) }))
    },
    {
      id: 'actions',
      label: 'Actions',
      items: [
        // The page of updates' **Update all**, offered whatever an update
        // would do: whether one writes anything is core's survey, a query
        // of its own that reads every origin again, which the palette does
        // not ask for each time it opens. The lines say it before they run.
        plain({
          label: 'Update all',
          description: 'Bring what every project was given up to date with where it came from',
          icon: 'i-lucide-refresh-cw',
          onSelect: choose(() => void runner.ask(UPDATE_ALL))
        }),
        plain({
          label: 'Keyboard shortcuts',
          description: 'What the keys do in the panel',
          icon: 'i-lucide-keyboard',
          kbds: ['?'],
          onSelect: choose(() => (shell.shortcutsOpen = true))
        })
      ]
    }
  ]
  if (!snapshot) {
    return groups
  }
  groups.push(
    {
      id: 'install',
      label: 'Install',
      items: installs(snapshot)
    },
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

const listed = computed(() => grouped(snapshot.value))

/**
 * Whatever is typed, offered as a folder to link, last: the link form's
 * task with the path as typed, which the server holds to the path policy
 * and refuses with why when it is no folder it takes. Offered for any
 * words, since telling a path from a name is that policy's, and kept out of
 * the search, which no folder not linked yet is in.
 */
const linking = computed<CommandPaletteGroup>(() => {
  const path = term.value
  return {
    id: 'link',
    label: 'Link',
    ignoreFilter: true,
    items: path === ''
      ? []
      : [plain({
          label: `Link ${path}`,
          description: 'The folder at this absolute path, as a project of the workspace',
          icon: 'i-lucide-link',
          onSelect: choose(() => void runner.ask({ type: 'link', path }))
        })]
  }
})

const groups = computed(() => [...listed.value, linking.value])

/**
 * The search field, named: a placeholder is no name to assistive
 * technology. Typed apart from its use, as Nuxt UI's props for the field
 * name none of the attributes it passes on to the element.
 */
const SEARCH: InputProps & { 'aria-label': string } = { 'aria-label': 'Search the panel' }

// Every word typed has to be found somewhere in an entry, its label, the
// word after it or its description, so a second word narrows what the
// first found; each a slip of a finger away at most: Nuxt UI's threshold
// of 0.1 finds nothing for `revew`, 0.3 finds `came` for `acme`, and
// Fuse's own 0.6 near anything for a word.
const FUSE = { fuseOptions: { useTokenSearch: true, tokenMatch: 'all' as const, threshold: 0.2 } }
</script>

<template>
  <UModal
    v-model:open="open"
    title="Command palette"
    description="Go to a page, a project, one of the library's own, the shelf or a source, or choose a change, whose lines are shown before they run."
    :ui="{ content: 'sm:max-w-2xl h-full sm:h-[28rem]' }"
    @after:leave="closed"
  >
    <template #content>
      <UCommandPalette
        v-model:search-term="term"
        :groups="groups"
        :fuse="FUSE"
        placeholder="Go anywhere, install, update all, or type a folder's path to link it"
        :input="SEARCH"
        close
        class="h-full"
        data-testid="palette"
        @update:open="open = $event"
      />
    </template>
  </UModal>
</template>
