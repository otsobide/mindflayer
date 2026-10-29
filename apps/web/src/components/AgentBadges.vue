<script setup lang="ts">
import type { Agent } from '@/api/generated/Agent'

defineProps<{
  /** The agents its project's settings name, in report order: a slot each. */
  agents: Agent[]
  /**
   * The agents among them that read a copy of it, as core says; left out
   * for a project's own list, which has nothing to be read.
   */
  reached?: Agent[]
}>()
</script>

<template>
  <span
    v-if="agents.length === 0"
    class="font-mono text-xs text-dimmed"
  >none</span>
  <ul
    v-else
    class="flex flex-wrap gap-1"
    data-testid="agents"
  >
    <!--
      An agent that does not read it keeps its slot, outlined, crossed and
      with an icon of its own, and says so in words for a screen reader: the
      slots line up from row to row, and colour is never the only sign.
    -->
    <li
      v-for="agent in agents"
      :key="agent"
    >
      <UBadge
        v-if="reached === undefined || reached.includes(agent)"
        :icon="reached === undefined ? undefined : 'i-lucide-check'"
        :label="agent"
        color="neutral"
        variant="soft"
        size="sm"
        class="font-mono"
        data-testid="agent"
      />
      <UBadge
        v-else
        icon="i-lucide-eye-off"
        color="neutral"
        variant="outline"
        size="sm"
        class="font-mono text-dimmed"
        :title="`${agent} does not read it`"
        data-testid="agent-unreached"
      >
        <span class="line-through">{{ agent }}</span>
        <span class="sr-only">, not reached</span>
      </UBadge>
    </li>
  </ul>
</template>
