<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useMutation } from '@tanstack/vue-query'
import { useToast } from '@nuxt/ui/composables'
import type { DropdownMenuItem } from '@nuxt/ui'
import type { Reason } from '@/api/generated/Reason'
import { useRunner } from '@/api/commands'
import { useSettleSwitch } from '@/api/live'
import { useSnapshot } from '@/api/snapshot'
import { TransportError, useTransport, type Switchable } from '@/api/transport'
import { useWorkspaces } from '@/api/workspaces'

/*
 * The workspace the panel acts on, and why that one, in the header of every
 * page; and the workspaces it may act on instead, as core lists them, one
 * of which it switches to.
 *
 * It names a workspace by the path the list gives it, and offers only those
 * core says may be switched to (`switchable`): the server holds a switch to
 * the same list, read again as it arrives, so nothing here decides where
 * the panel may go. Once switched, every line it shows names the new one
 * with `-W`, and every view reads it again.
 */

/**
 * How each rule that chose a workspace is said, as the TUI's header says
 * it: a `Record`, so a rule core adds does not compile until it is said.
 * Named with `-W` is what a switch makes any workspace but the launch's,
 * since every line the panel runs from then on names it so.
 */
const CHOSEN: Record<Reason, string> = {
  explicit: 'named with -W',
  activated: 'activated',
  above: 'found above',
  home: 'default workspace'
}

const transport = useTransport()
const router = useRouter()
const toast = useToast()
const settle = useSettleSwitch()
const { busy } = useRunner()
const { data: snapshot } = useSnapshot()

const open = ref(false)
const { data: workspaces, error, isPending } = useWorkspaces(open)

const switching = useMutation({
  mutationKey: ['switch'],
  mutationFn: (to: Switchable) => settle(() => transport.switch({ path: to.path }))
})

/** The workspace in use, as the snapshot says it, and why that one. */
const current = computed(() => snapshot.value?.workspace)

/** What a row is called: its marker's name, or its path when it has none to read. */
function called(row: Switchable): string {
  return row.name ?? row.path
}

/**
 * What the menu says of a row below its name, every state in words: where
 * it is, on a line of its own, then the rest.
 */
function described(row: Switchable): string {
  const facts: string[] = []
  if (row.in_use) {
    facts.push('in use')
  }
  if (row.default) {
    facts.push('the default')
  }
  if (row.launched) {
    facts.push(`launched here, ${CHOSEN[row.reason]}`)
  }
  if (row.stale !== null) {
    facts.push(`does not open: ${row.stale}`)
  } else {
    const projects = `${row.projects} project${row.projects === 1 ? '' : 's'}`
    facts.push(row.stale_projects > 0 ? `${projects}, ${row.stale_projects} stale` : projects)
  }
  return `${row.path}\n${facts.join(' · ')}`
}

/**
 * Switch to `row`, then draw the overview of it once every view has its
 * answers, which the stream's word that it was taken sets off in every
 * tab, this one too, and say so; or say why the server would not, nothing
 * switched.
 */
async function switchTo(row: Switchable) {
  let to: Switchable
  try {
    to = await switching.mutateAsync(row)
  } catch (refusal) {
    const message = refusal instanceof Error ? refusal.message : String(refusal)
    const code = refusal instanceof TransportError ? refusal.code : undefined
    toast.add({
      title: 'Not switched',
      description: code ? `${message} (${code})` : message,
      color: 'error',
      icon: 'i-lucide-ban'
    })
    return
  }
  await router.push('/')
  toast.add({
    title: `Switched to ${called(to)}`,
    description: `Every line the panel runs now names it: -W ${to.path}`,
    color: 'success',
    icon: 'i-lucide-circle-check'
  })
}

/**
 * Each workspace as an item of the menu, checked for the one in use, and
 * open to a switch only where core offers one: a state said by the check,
 * by the words beside it and by whether it can be chosen, never by colour.
 */
const items = computed<DropdownMenuItem[]>(() => {
  const heading: DropdownMenuItem = { type: 'label', label: 'Switch workspace' }
  if (error.value) {
    return [heading, { type: 'label', label: `They could not be read: ${error.value.message}` }]
  }
  if (isPending.value || !workspaces.value) {
    return [heading, { type: 'label', label: 'Reading the workspaces this machine knows' }]
  }
  return [
    heading,
    ...workspaces.value.map(row => ({
      type: 'checkbox' as const,
      label: called(row),
      description: described(row),
      checked: row.in_use,
      disabled: !row.switchable,
      onSelect: () => {
        if (row.switchable) {
          void switchTo(row)
        }
      }
    }))
  ]
})
</script>

<template>
  <!--
    Drawn where it is rather than at the end of the page's body, so the
    menu is inside the page's landmark, as everything on a page is: a menu
    is no dialog, which axe lets stand outside one.
  -->
  <UDropdownMenu
    v-model:open="open"
    :items="items"
    :portal="false"
    :content="{ align: 'end' }"
    :ui="{ content: 'w-[min(32rem,calc(100vw-2rem))]', itemDescription: 'whitespace-pre-line wrap-anywhere font-mono text-xs' }"
  >
    <!--
      Named by what it shows, the words a reader is not shown read first,
      rather than by an aria-label, which a search for a field by its label
      ("Name") would find in "named with -W" too. The rule's words stay in
      the name on a narrow screen, where they are not shown.
    -->
    <UButton
      color="neutral"
      variant="ghost"
      size="sm"
      icon="i-lucide-box"
      trailing-icon="i-lucide-chevrons-up-down"
      :disabled="busy || switching.isPending.value"
      :loading="switching.isPending.value"
      class="min-w-0"
      data-testid="workspace-switcher"
    >
      <span class="sr-only">Switch workspace; in use: </span>
      <span
        class="truncate font-medium"
        data-testid="workspace-name"
      >{{ current?.name ?? 'Workspace' }}</span>
      <template v-if="current">
        <span class="sr-only">, </span>
        <span
          class="sr-only truncate font-mono text-xs text-muted sm:not-sr-only"
          data-testid="workspace-reason"
        >{{ CHOSEN[current.reason] }}</span>
      </template>
    </UButton>
  </UDropdownMenu>
</template>
