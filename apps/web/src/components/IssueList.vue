<script setup lang="ts">
import type { Held } from '@/api/generated/Held'

defineProps<{
  /** What is wrong with an artifact, as the snapshot holds it. */
  held: Pick<Held, 'issues' | 'unread'>
}>()
</script>

<template>
  <!--
    Why its file does not read, then everything `validate` says of it, each
    saying in words whether it is only a warning: the word is core's
    (`warning`), never told here from the code.
  -->
  <ul
    v-if="held.unread || held.issues.length"
    class="flex flex-col gap-1"
  >
    <li
      v-if="held.unread"
      class="flex items-start gap-1.5"
      data-testid="unread"
    >
      <UIcon
        name="i-lucide-file-x"
        class="mt-px size-3.5 shrink-0 text-error"
        aria-hidden="true"
      />
      <span>
        <span class="font-mono text-error">does not read:</span>
        {{ held.unread.message }}
        <code class="block font-mono text-dimmed">{{ held.unread.path }}</code>
      </span>
    </li>
    <li
      v-for="issue in held.issues"
      :key="`${issue.code}:${issue.message}`"
      class="flex items-start gap-1.5"
      data-testid="issue"
    >
      <UIcon
        :name="issue.warning ? 'i-lucide-triangle-alert' : 'i-lucide-circle-x'"
        class="mt-px size-3.5 shrink-0"
        :class="issue.warning ? 'text-warning' : 'text-error'"
        aria-hidden="true"
      />
      <span>
        <span
          class="font-mono"
          :class="issue.warning ? 'text-warning' : 'text-error'"
        >{{ issue.warning ? 'warning:' : 'invalid:' }}</span>
        {{ issue.message }}
      </span>
    </li>
  </ul>
  <span
    v-else
    class="font-mono text-dimmed"
  >none</span>
</template>
