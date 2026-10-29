<script setup lang="ts">
import type { Snapshot } from '@/api/generated/Snapshot'
import { useSnapshot } from '@/api/snapshot'

defineSlots<{
  default(props: { snapshot: Snapshot }): unknown
}>()

const { data, error, refetch } = useSnapshot()

// The last answer stays drawn while a new one is asked for, and when asking
// again fails: what was read is still worth reading, and Vue Query will ask
// again. Only a page with no answer at all says it is waiting, or why none
// came.
</script>

<template>
  <slot
    v-if="data"
    :snapshot="data"
  />
  <div
    v-else-if="error"
    class="p-4 sm:p-6"
  >
    <UAlert
      color="error"
      variant="subtle"
      icon="i-lucide-circle-alert"
      title="The workspace could not be read"
      :description="error.message"
      :actions="[{ label: 'Ask again', icon: 'i-lucide-rotate-cw', color: 'error', variant: 'outline', size: 'sm', onClick: () => refetch() }]"
      data-testid="unread"
    />
  </div>
  <div
    v-else
    role="status"
    class="flex items-center gap-2 p-4 text-muted sm:p-6"
    data-testid="reading"
  >
    <UIcon
      name="i-lucide-loader-circle"
      class="size-4 animate-spin"
      aria-hidden="true"
    />
    Reading the workspace
  </div>
</template>
