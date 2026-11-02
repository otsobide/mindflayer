<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { FolderIs } from '@/api/generated/FolderIs'
import type { Folders } from '@/api/generated/Folders'
import { useFolders, type Opened } from '@/api/folders'
import { TransportError } from '@/api/transport'

/*
 * The explorer: a folder at a time, what is directly inside it, for
 * somebody to choose one rather than type its path. Everything it shows is
 * core's listing (`browse::Folders`), asked of the server folder by folder;
 * it puts no path together, and choosing runs nothing. The form it was
 * opened from is handed the path, which goes to the server as a typed one
 * would, held to the path policy and previewed before it runs.
 */

const props = defineProps<{
  /**
   * Where it opens: the folder the field it fills holds, sent as typed for
   * the server to hold to the path policy, or none, or white space alone,
   * for the workspace's root.
   */
  start?: string
  /** What it is opened for, its title. */
  title: string
  /** What choosing a folder does, the word on each button that does it. */
  verb: string
}>()

const open = defineModel<boolean>('open', { required: true })

const emit = defineEmits<{
  /** A folder was chosen, by its absolute path as the listing spelled it. */
  chosen: [path: string]
}>()

/** What it asks for: the folder open, whether hidden ones show, and which opening it is. */
const opened = ref<Opened>({ path: null, hidden: false, visit: 0 })

/**
 * The last folder listed, kept while another is asked for and when that
 * one is refused, so a typo leaves the explorer where it was.
 */
const shown = ref<Folders>()

// Each opening starts where the field says and reads every folder afresh,
// keeping whether hidden folders show from the last.
watch(open, (now) => {
  if (now) {
    const start = props.start ?? ''
    opened.value = { path: start.trim() === '' ? null : start, hidden: opened.value.hidden, visit: opened.value.visit + 1 }
    shown.value = undefined
  }
}, { immediate: true })

const { data, error } = useFolders(opened, open)

/** The path field, as typed: the folder listed once it is, to change and go to. */
const typed = ref('')

watch(data, (now) => {
  if (now) {
    shown.value = now
    typed.value = now.path
  }
})

/** Open `path`, by the path a listing spelled or as typed; none is the workspace's root. */
function go(path: string | null) {
  opened.value = { ...opened.value, path }
}

/** Whether hidden folders show: the folder shown asked for again either way. */
const hidden = computed({
  get: () => opened.value.hidden,
  set: (on: boolean) => {
    opened.value = { ...opened.value, path: shown.value?.path ?? opened.value.path, hidden: on }
  }
})

function choose(path: string) {
  emit('chosen', path)
  open.value = false
}

/** Why the folder asked for was not listed, with its code. */
const refusal = computed(() => {
  const failed = error.value
  if (!failed) {
    return undefined
  }
  const code = failed instanceof TransportError ? failed.code : undefined
  return code ? `${failed.message} (${code})` : failed.message
})

// What each word core says of a folder is drawn as: the panel's look, for
// each word it knows, and the word itself with a plain icon for one it
// does not, as a state's is. The word is always shown, so the icon only
// repeats it.
const LOOKS: Partial<Record<FolderIs, { label: string, icon: string }>> = {
  project: { label: 'mind project', icon: 'i-lucide-folder-git-2' },
  workspace: { label: 'flayer workspace', icon: 'i-lucide-box' },
  repository: { label: 'git repository', icon: 'i-lucide-git-branch' },
  folder: { label: 'folder', icon: 'i-lucide-folder' }
}

const PLAIN = 'i-lucide-folder'

function look(is: FolderIs): { label: string, icon: string } {
  return LOOKS[is] ?? { label: is, icon: PLAIN }
}
</script>

<template>
  <UModal
    v-model:open="open"
    :title="title"
    description="Folders are only read. Choosing one fills the field; nothing runs until you press its button, and the line is shown first."
    :ui="{ content: 'sm:max-w-2xl' }"
  >
    <template #body>
      <div
        class="flex flex-col gap-3"
        data-testid="folder-picker"
      >
        <form
          class="flex items-end gap-2"
          aria-label="Go to a folder"
          @submit.prevent="go(typed)"
        >
          <UFormField
            label="Folder"
            description="An absolute path, as you would type it to link."
            name="folder"
            class="min-w-0 flex-1"
          >
            <UInput
              v-model="typed"
              icon="i-lucide-folder-search"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              data-testid="folder-path"
            />
          </UFormField>
          <UButton
            type="submit"
            label="Go"
            size="sm"
            color="neutral"
            variant="outline"
            :disabled="typed === ''"
            data-testid="folder-go"
          />
        </form>

        <div class="flex flex-wrap items-center gap-2">
          <UButton
            v-if="shown?.home"
            label="Home"
            icon="i-lucide-house"
            size="xs"
            color="neutral"
            variant="outline"
            data-testid="folder-home"
            @click="go(shown.home!)"
          />
          <UButton
            label="Workspace"
            icon="i-lucide-box"
            size="xs"
            color="neutral"
            variant="outline"
            data-testid="folder-workspace"
            @click="go(null)"
          />
          <USwitch
            v-model="hidden"
            :label="`Hidden folders (${shown?.hidden ?? 0})`"
            size="sm"
            class="ms-auto"
            data-testid="folder-hidden"
          />
        </div>

        <nav
          v-if="shown"
          aria-label="Where this folder is"
        >
          <ol
            class="flex flex-wrap items-center gap-1 font-mono text-xs"
            data-testid="trail"
          >
            <li
              v-for="(crumb, index) in shown.trail"
              :key="crumb.path"
              class="flex items-center gap-1"
            >
              <UIcon
                v-if="index > 0"
                name="i-lucide-chevron-right"
                class="size-3 text-dimmed"
                aria-hidden="true"
              />
              <UButton
                v-if="index < shown.trail.length - 1"
                :label="crumb.name"
                size="xs"
                color="neutral"
                variant="link"
                class="px-0.5 font-mono"
                data-testid="crumb"
                @click="go(crumb.path)"
              />
              <span
                v-else
                aria-current="location"
                class="px-0.5 font-medium text-highlighted"
                data-testid="crumb-here"
              >{{ crumb.name }}</span>
            </li>
          </ol>
        </nav>

        <UAlert
          v-if="refusal"
          color="error"
          variant="subtle"
          icon="i-lucide-circle-alert"
          title="That folder was not listed"
          :description="refusal"
          data-testid="folder-refused"
        />

        <p
          v-if="!shown && !refusal"
          role="status"
          class="text-sm text-muted"
          data-testid="folder-reading"
        >
          Reading the folder…
        </p>

        <template v-if="shown">
          <p
            class="flex flex-wrap items-center gap-2 text-sm"
            data-testid="folder-here"
          >
            <UIcon
              :name="look(shown.is).icon"
              class="size-4 shrink-0 text-muted"
              aria-hidden="true"
            />
            <span class="text-muted">This folder is a</span>
            <span
              class="font-medium text-highlighted"
              data-testid="folder-is"
            >{{ look(shown.is).label }}</span>
            <UBadge
              v-if="shown.linked"
              label="linked"
              icon="i-lucide-check"
              color="neutral"
              variant="soft"
              size="sm"
              data-testid="folder-linked"
            />
          </p>

          <ul
            v-if="shown.folders.length > 0"
            class="flex max-h-80 flex-col divide-y divide-default overflow-y-auto rounded-lg border border-default"
            aria-label="The folders in it"
            data-testid="folders"
          >
            <li
              v-for="folder in shown.folders"
              :key="folder.path"
              class="flex items-center gap-2 px-3 py-1.5"
              data-testid="folder"
            >
              <UIcon
                :name="look(folder.is).icon"
                class="size-4 shrink-0 text-muted"
                aria-hidden="true"
              />
              <UButton
                :label="folder.name"
                :aria-label="`Open ${folder.name}`"
                size="sm"
                color="neutral"
                variant="link"
                class="min-w-0 truncate px-0 font-mono"
                data-testid="folder-open"
                @click="go(folder.path)"
              />
              <span
                class="shrink-0 text-xs text-muted"
                data-testid="folder-is"
              >{{ look(folder.is).label }}</span>
              <UBadge
                v-if="folder.linked"
                label="linked"
                icon="i-lucide-check"
                color="neutral"
                variant="soft"
                size="sm"
                class="shrink-0"
                data-testid="folder-linked"
              />
              <UBadge
                v-if="folder.symlink"
                label="symlink"
                icon="i-lucide-corner-down-right"
                color="neutral"
                variant="outline"
                size="sm"
                class="shrink-0"
                data-testid="folder-symlink"
              />
              <UButton
                :label="verb"
                :aria-label="`${verb} ${folder.name}`"
                size="xs"
                color="neutral"
                variant="outline"
                class="ms-auto shrink-0"
                data-testid="folder-choose"
                @click="choose(folder.path)"
              />
            </li>
          </ul>

          <p
            v-else
            class="text-sm text-muted"
            data-testid="folder-empty"
          >
            No folders in it{{ !hidden && shown.hidden > 0 ? `, but ${shown.hidden} hidden` : '' }}.
          </p>

          <p
            v-if="shown.truncated"
            class="text-sm text-muted"
            data-testid="folder-truncated"
          >
            Only the first {{ shown.folders.length }} are listed: type the path of one further down to go to it.
          </p>

          <ul
            v-if="shown.failures.length > 0"
            class="flex flex-col gap-1.5"
            aria-label="What could not be read"
            data-testid="folder-failures"
          >
            <li
              v-for="(failure, index) in shown.failures"
              :key="index"
              class="flex items-start gap-2 text-sm"
              data-testid="folder-failure"
            >
              <UIcon
                name="i-lucide-circle-alert"
                class="mt-0.5 size-4 shrink-0 text-warning"
                aria-hidden="true"
              />
              <span class="min-w-0 break-words">
                <span class="me-2 font-mono text-xs text-dimmed">{{ failure.code }}<span class="sr-only">:</span></span>
                {{ failure.message }}
              </span>
            </li>
          </ul>
        </template>
      </div>
    </template>

    <template #footer>
      <div class="flex w-full flex-wrap items-center justify-end gap-2">
        <code
          v-if="shown"
          class="me-auto min-w-0 truncate font-mono text-xs text-muted"
          data-testid="folder-at"
        >{{ shown.path }}</code>
        <UButton
          label="Cancel"
          color="neutral"
          variant="outline"
          data-testid="folder-cancel"
          @click="open = false"
        />
        <UButton
          :label="`${verb} this folder`"
          icon="i-lucide-check"
          :disabled="!shown"
          data-testid="folder-choose-here"
          @click="shown && choose(shown.path)"
        />
      </div>
    </template>
  </UModal>
</template>
