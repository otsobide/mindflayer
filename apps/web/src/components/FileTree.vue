<script setup lang="ts">
import type { Files } from '@/api/generated/Files'
import type { Listed } from '@/api/generated/Listed'

defineProps<{
  /** What an artifact holds, as core lists it. */
  files: Files
  /** The route of the file on screen, if one is. */
  selected?: string
}>()

const emit = defineEmits<{
  /** A file was chosen, by the route its listing gives it. */
  select: [path: string]
}>()

// How far in an entry is drawn, by how deep its route is: a class each,
// written out so the stylesheet has them, rather than a style the CSP
// would refuse. Deeper than the last is drawn at the last.
const INDENT = ['ps-2', 'ps-6', 'ps-10', 'ps-14', 'ps-18', 'ps-22']

function depth(entry: Listed): number {
  return entry.path.split('/').length - 1
}

function indent(entry: Listed): string {
  return INDENT[Math.min(depth(entry), INDENT.length - 1)]!
}

/** The last part of its route: what it is called in the folder it is in. */
function named(entry: Listed): string {
  return entry.path.split('/').at(-1) ?? entry.path
}
</script>

<template>
  <!--
    A list of every entry in the order core lists them, a folder before what
    it holds, each drawn in by how deep it is and named in full for a screen
    reader. A file is a button that asks for its text, never a link to it:
    no file of an artifact is served as itself.
  -->
  <ul
    class="flex flex-col rounded-lg border border-default py-1 font-mono text-xs"
    aria-label="Its files"
    data-testid="files"
  >
    <li
      v-for="entry in files.entries"
      :key="entry.path"
      data-testid="entry"
    >
      <div
        v-if="entry.refused"
        class="flex items-start gap-1.5 py-1 pe-2"
        :class="indent(entry)"
        data-testid="refused"
      >
        <UIcon
          name="i-lucide-file-lock"
          class="mt-px size-3.5 shrink-0 text-warning"
          aria-hidden="true"
        />
        <span>
          <span class="text-highlighted">{{ named(entry) }}</span>
          <span class="sr-only">, at {{ entry.path }},</span>
          <span class="text-warning"> not followed:</span>
          <span class="font-sans text-muted"> {{ entry.refused.message }}</span>
          <span class="text-dimmed"> ({{ entry.refused.code }})</span>
        </span>
      </div>
      <div
        v-else-if="entry.folder"
        class="flex items-center gap-1.5 py-1 pe-2 text-muted"
        :class="indent(entry)"
        data-testid="folder"
      >
        <UIcon
          name="i-lucide-folder"
          class="size-3.5 shrink-0"
          aria-hidden="true"
        />
        <span>{{ named(entry) }}/</span>
        <span class="sr-only">, the folder {{ entry.path }}</span>
      </div>
      <button
        v-else
        type="button"
        class="flex w-full items-center gap-1.5 py-1 pe-2 text-left hover:bg-elevated/50 focus-visible:outline-2 focus-visible:outline-primary"
        :class="[indent(entry), selected === entry.path && 'bg-elevated text-highlighted']"
        :aria-pressed="selected === entry.path"
        data-testid="file"
        @click="emit('select', entry.path)"
      >
        <UIcon
          name="i-lucide-file-text"
          class="size-3.5 shrink-0 text-dimmed"
          aria-hidden="true"
        />
        <span class="truncate">{{ named(entry) }}</span>
        <span class="sr-only">, at {{ entry.path }}</span>
        <span
          class="ms-auto shrink-0 tabular-nums text-dimmed"
          data-testid="size"
        >{{ entry.size }} bytes</span>
      </button>
    </li>
    <li
      v-if="files.truncated"
      class="px-2 py-1 font-sans text-muted"
      data-testid="truncated"
    >
      It holds more than a listing shows: the rest is left out.
    </li>
  </ul>
</template>
