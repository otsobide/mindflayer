import { computed, inject, provide, type ComputedRef, type InjectionKey } from 'vue'
import { useMutation, useMutationState } from '@tanstack/vue-query'
import { useSettle } from './live'
import { useTransport, type Preview, type Ran, type Task } from './transport'

/** A task shown before it runs: what was asked for, and the lines it is. */
export interface Asking {
  task: Task
  preview: Preview
}

/**
 * How the panel changes anything (D-18): a task, previewed by the server,
 * shown, and run only once somebody says so.
 *
 * Every action of every page goes through `ask`, so none runs a line it did
 * not show first, and none runs one the backend says takes something away
 * without the question being one: which lines those are is the preview's
 * `confirm`, the CLI's list, and nothing here decides it. The lines shown
 * are the preview's, and the panel never puts one together itself.
 */
export interface Runner {
  /**
   * Preview `task` and show it, its lines and what they take away. Answers
   * what its run was answered with, once the views are drawn from what it
   * changed, or nothing when it was let go, refused, or something else is
   * already under way.
   */
  ask(task: Task): Promise<Ran | undefined>
  /** What is shown now, waiting for a yes or a no. */
  asking: ComputedRef<Asking | undefined>
  /** Whether what is shown takes something away, which makes it a confirmation. */
  confirming: ComputedRef<boolean>
  /** Whether a run of it is under way, its views not yet drawn again. */
  running: ComputedRef<boolean>
  /** Whether anything is: a preview asked for, one shown, or a run. */
  busy: ComputedRef<boolean>
  /** Yes: run what is shown. */
  run(): Promise<void>
  /** No: let it go, nothing run. */
  cancel(): void
  /**
   * What the last run that answered was answered with, for the warnings
   * drawer: not the run mutation's own answer, which Vue Query clears as the
   * next run starts and leaves cleared when that one fails, so a later
   * refusal would leave the drawer an earlier run's toast opens saying
   * nothing has run.
   */
  said: ComputedRef<Ran | undefined>
}

export const runnerKey: InjectionKey<Runner> = Symbol('runner')

/** The panel's runner, for a page to ask with. */
export function useRunner(): Runner {
  const runner = inject(runnerKey)
  if (!runner) {
    throw new Error('the panel was started without its runner')
  }
  return runner
}

/**
 * What a front end makes of how a task ended, which the runner leaves to
 * whatever draws: the panel's toasts and drawers (`CommandRunner.vue`).
 */
export interface Say {
  /** It ran, and its views are drawn from what it changed. */
  ran(ran: Ran): void
  /**
   * It did not run, or its answer never came: refused when previewed or when
   * run (`ran` false, or true and refused with a 4xx, nothing ran either
   * way), or the server failed or stopped while it ran, which may have
   * changed something.
   */
  refused(error: unknown, ran: boolean): void
}

/**
 * Make the panel's one runner and hand it to everything under the caller:
 * one, so two pages cannot show two tasks at once, and made where Vue Query
 * and the transport are at hand, telling `say` how each task ended.
 *
 * The preview and the run are Vue Query mutations, so what the server
 * answered stays in Vue Query; what is shown is the preview's answer, held
 * until it is answered. A run is settled on the stream's word that it
 * finished (`useSettle`), so what says it is over comes once the views show
 * what it did.
 */
export function provideRunner(say: Say): Runner {
  const transport = useTransport()
  const settle = useSettle()

  const previewing = useMutation({
    mutationKey: ['preview'],
    mutationFn: (task: Task) => transport.send(task, 'preview')
  })
  const running = useMutation({
    mutationKey: ['run'],
    mutationFn: (task: Task) => settle(() => transport.send(task, 'run')),
    // Each run's answer is kept for as long as the tab is open, not the five
    // minutes after the next run starts that Vue Query keeps one by default:
    // the drawer may be opened on the last that answered at any time, and a
    // tab runs few enough for every answer to cost nothing kept.
    gcTime: Infinity
  })
  const answered = useMutationState({
    filters: { mutationKey: ['run'], status: 'success' },
    select: mutation => mutation.state.data as Ran
  })

  /** Who is waiting on what is shown, to be told how it ended. */
  let answer: ((ran: Ran | undefined) => void) | undefined

  const asking = computed<Asking | undefined>(() => {
    const preview = previewing.data.value
    const task = previewing.variables.value
    return preview && task ? { task, preview } : undefined
  })

  function end(ran: Ran | undefined) {
    previewing.reset()
    const waiting = answer
    answer = undefined
    waiting?.(ran)
  }

  const busy = computed(() => previewing.isPending.value || asking.value !== undefined || running.isPending.value)

  const runner: Runner = {
    async ask(task) {
      if (busy.value) {
        return undefined
      }
      try {
        await previewing.mutateAsync(task)
      } catch (error) {
        previewing.reset()
        say.refused(error, false)
        return undefined
      }
      return new Promise<Ran | undefined>((resolve) => {
        answer = resolve
      })
    },
    asking,
    confirming: computed(() => (asking.value?.preview.confirm.length ?? 0) > 0),
    running: computed(() => running.isPending.value),
    busy,
    async run() {
      const asked = asking.value
      if (!asked || running.isPending.value) {
        return
      }
      try {
        const ran = await running.mutateAsync(asked.task)
        say.ran(ran)
        end(ran)
      } catch (error) {
        say.refused(error, true)
        end(undefined)
      }
    },
    cancel() {
      if (!running.isPending.value) {
        end(undefined)
      }
    },
    said: computed(() => answered.value.at(-1))
  }
  provide(runnerKey, runner)
  return runner
}
