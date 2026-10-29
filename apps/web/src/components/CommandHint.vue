<script lang="ts">
import type { Workspace } from '@/api/generated/Workspace'

/** One line to run, and what it does. */
export interface Command {
  /** The line, as somebody would type it at a shell. */
  line: string
  /** What it does, said beside it as a shell comment. */
  does?: string
}

/**
 * `flayer -W <workspace> <words>`: a line that acts on the workspace on
 * screen wherever it is typed.
 *
 * Without `-W` it would act on the one the shell it is typed in resolves
 * to: activated there, the first above, or the default in home. That is
 * not this one for a panel launched with `-W` or on an activated workspace,
 * nor from a project linked from beside the workspace, which a walk up from
 * the project never meets; `mind` there consults another one or none. So a
 * project's line is a `flayer` one naming it with `-p`, as the README's own
 * do, rather than a `mind` one typed from its folder.
 */
export function flayer(workspace: Workspace, words: string): string {
  return `flayer -W ${workspace.path} ${words}`
}

/**
 * The line that makes `workspace` when it is not made yet, which a line of
 * [`flayer`] needs first: a workspace `-W` names is never made for it, and
 * `flayer init` makes it where the first command that writes would.
 */
export function making(workspace: Workspace): Command[] {
  return workspace.made ? [] : [{ line: `flayer init ${workspace.path}`, does: 'make it, for -W to name' }]
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
