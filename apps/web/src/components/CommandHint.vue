<script lang="ts">
/** One line to run, and what it does. */
export interface Command {
  /** The line, as somebody would type it at a shell. */
  line: string
  /** What it does, said beside it as a shell comment. */
  does?: string
}
</script>

<script setup lang="ts">
defineProps<{
  commands: Command[]
}>()
</script>

<template>
  <!--
    The lines to run where the panel has nothing to show yet: everything it
    shows is made by a command, and these are the ones that would make it.
  -->
  <div
    class="grid grid-cols-[auto_auto] justify-start gap-x-4 gap-y-1 rounded-md border border-default bg-muted px-3 py-2 text-left font-mono text-xs"
    data-testid="commands"
  >
    <template
      v-for="command in commands"
      :key="command.line"
    >
      <code
        class="text-toned"
        :class="{ 'col-span-2': !command.does }"
      ><span
        class="select-none text-dimmed"
        aria-hidden="true"
      >$ </span>{{ command.line }}</code>
      <span
        v-if="command.does"
        class="select-none text-dimmed"
      ># {{ command.does }}</span>
    </template>
  </div>
</template>
