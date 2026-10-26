# Where the agents read skills and rules

This is the reference for where Claude Code, Codex and OpenCode look for
skills and rules, at project and user scope, and what each does with
duplicates and ignored files. Mindflayer's agent adapters build on it: the
target table (#28), installing into several folders (#34), the managed block
in `AGENTS.md` (#42), Claude's rule files (#43) and the predicate that decides
how Claude reaches the block (#44). [docs/architecture.md](architecture.md)
says what Mindflayer does with these facts and why; this page says what the
agents do.

**Last verified: 2026-10-26, on Claude Code 2.1.309.** Codex and OpenCode were
not run, so every fact about them is docs-only, as of the same date.

Each fact carries one of two marks:

- **verified**: seen on the real binary by `scripts/agent-probe/probe.sh`, in
  the run recorded under [the Claude Code run](#the-claude-code-run).
- **docs-only**: taken from the agent's documentation or, where that is
  silent, from its source code, and not run here. Each one Mindflayer relies
  on names a fallback: what Mindflayer does, or what still holds, if the
  binary turns out to disagree.

## What the decisions rest on

- [D-02](architecture.md#d-02-installs-stay-local-unless-a-project-shares-them),
  installs stay local: **holds for Claude Code.** An excluded skill, an
  excluded symlinked skill, an excluded `AGENTS.md`, excluded rules under
  `.claude/rules/mindflayer` and an excluded `CLAUDE.local.md` all loaded,
  and git confirmed the exclusions in the run that set them up. For Codex and
  OpenCode it
  is docs-only: their source does no gitignore filtering that anyone found,
  and shared mode is the fallback.
- [D-03](architecture.md#d-03-rules-reach-every-agent-through-agentsmd),
  rules reach every agent through `AGENTS.md`: **holds for Claude Code.** It
  read `AGENTS.md` when no `CLAUDE.md` or `CLAUDE.local.md` was in its start
  directory or above, and did not when a
  `CLAUDE.md` sat above the git root or a `CLAUDE.local.md` beside it. A
  `CLAUDE.md` holding `@AGENTS.md` loaded it once. The user memory file and
  `.claude/rules` files did not count as a `CLAUDE.md`. A rule with `paths:`
  loaded only once Claude read a matching file.
- [D-07](architecture.md#d-07-one-copy-per-agent-folder), one copy per agent
  folder: **holds.** Claude did not load a skill that was only in
  `.agents/skills`, so the second copy in `.claude/skills` is needed, and it
  loaded a relative symlink to `.agents/skills` once, the condition D-07 set
  for the symlink opt-in. One thing that opt-in has to know: git never follows
  a symlink, so the exclude entry for one must not end in `/` as a directory's
  does, or it ignores nothing.

No result contradicts D-02, D-03 or D-07.

## How the checks are run

`scripts/agent-probe/probe.sh` builds throwaway git repositories and asks the
agent about each one. It talks to the agent's API with the operator's own
login, so it is run by hand, never by CI or `make test`.

- **One temp dir.** Everything lives in `target/agent-probe-run`, deleted when
  the script exits however it exits, the login copy included. The only file
  taken from the real home is Claude's `.credentials.json`; settings, skills,
  rules, `CLAUDE.md` and plugins stay out, because they are exactly what must
  not take part. A login close to expiry is refused, since a refresh inside
  the run could rotate the refresh token the real session holds.
- **A mount namespace.** The agent starts under bubblewrap, with the real
  home an empty tmpfs, the temp dir mounted at `/mnt/agent-probe`, `/tmp` a
  private tmpfs and the environment cleared. `HOME`, the `XDG_*_HOME`
  variables, `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and `OPENCODE_CONFIG_DIR` all
  point into the temp dir. Pointing the variables away is not enough on its
  own: Codex and OpenCode read `~/.agents/skills` whatever their own variables
  say. Mounting the temp dir somewhere new also makes the parent-folder checks
  possible: Claude reads instruction files from every folder above where it
  starts, and above `target/` sit this checkout's `AGENTS.md` and, in a linked
  worktree, the main checkout's `CLAUDE.md`.
- **Guards.** The part that runs agents refuses unless `HOME` is the temp
  home and the real one is out of sight. Before every check it asserts that
  the temp home holds no user-scope skills, rules, `CLAUDE.md` or `AGENTS.md`
  (one check puts a user memory file there on purpose, and removes it), and
  that the only instruction files above the scenario are the ones it wrote.
- **The real home.** Before and after the run it fingerprints `~/.agents`,
  `~/.codex`, `~/.claude/skills`, `~/.claude/rules`, `~/.claude/CLAUDE.md`
  and `~/.claude.json`, and fails if any changed. Not the whole `~/.claude`:
  a Claude Code session running the script writes there itself.
- **Tokens.** Every file under test carries a token of its own
  (`PROBE-AGENTS-MD-1`, ...), and each run asks the model once which tokens it
  holds and which skills it can invoke. A token in the answer means the file
  reached the model. Skills are counted in the session's init message, not in
  the answer. Claude's `InstructionsLoaded` hook, set in the temp home's user
  settings, records each instruction file that loads and why, which does not
  depend on the model at all.

To re-run it: Linux with `bwrap`, and `jq` and `git` in `/usr/bin` or `/bin`,
the only `PATH` inside the namespace; and a logged-in Claude Code, its native
binary on `PATH` or named by `PROBE_CLAUDE_BIN`. A launcher script, as an npm
install gives, is refused, and so is a version manager's shim, since the
binary must call itself Claude Code: `mise which claude` names the one behind
mise's.

```bash
scripts/agent-probe/probe.sh            # same as: probe.sh claude
```

It prints every command, answer and verdict, keeps them in
`target/agent-probe-results/<time>/`, and exits non-zero when a result
differs from what this page says. Codex and OpenCode get checks from a
`codex.sh` or `opencode.sh` beside it, defining `<agent>_prepare` (copy the
login in) and `<agent>_checks`; the harness already points `CODEX_HOME` and
`OPENCODE_CONFIG_DIR` into the temp dir. Until those exist, `probe.sh codex`
refuses.

## Claude Code

### Skills

| Fact | Mark | Evidence, or source and fallback |
|---|---|---|
| Project skills are `.claude/skills/<name>/SKILL.md` in the start directory. | verified | `claude-excluded`; the debug log lists `project=[<repo>/.claude/skills]`. |
| `.agents/skills` is not read. | verified | `claude-excluded`: `probe-agents-skill` is in neither the init message nor the answer. |
| A skill excluded through `info/exclude` loads. | verified | `claude-excluded`. |
| A relative symlink `.claude/skills/<n>` -> `../../.agents/skills/<n>`, excluded, loads once. | verified | `claude-excluded`: listed once in the init message; the debug log says `Loaded 2 unique skills (... project: 2 ...)`. |
| A skill's description reaches the model at start; its body does not until it is invoked. | verified | The `-BODY` tokens never appeared. |
| User skills are `<config dir>/skills`. | verified, in part | The debug log lists `user=/mnt/agent-probe/home/.claude/skills`. The run set `CLAUDE_CONFIG_DIR` to `$HOME/.claude`, so it cannot tell which of the two moves the folder; the [docs](https://code.claude.com/docs/en/env-vars) say `CLAUDE_CONFIG_DIR`. |
| Managed skills are read from `/etc/claude-code/.claude/skills` on Linux. | verified | The debug log lists `managed=/etc/claude-code/.claude/skills`. Mindflayer never writes there. |
| Project skills are also read from every folder above the start directory up to the repository root, and from a nested `.claude/skills` once Claude touches a file below it. | docs-only | [skills](https://code.claude.com/docs/en/skills). Not relied on: Mindflayer installs at the project root. |
| One name at several scopes resolves managed, then personal, then project. | docs-only | [skills](https://code.claude.com/docs/en/skills). Fallback: Mindflayer installs at project scope only ([D-22](architecture.md#d-22-user-scope-installs-come-later)), and `flayer doctor` (#96) reports a project skill a user-scope one shadows. |
| A skill folder holding `.claude-plugin/plugin.json` loads as a plugin, with its hooks and MCP servers. | docs-only | [skills](https://code.claude.com/docs/en/skills). Fallback: #34 drops that folder whatever Claude would do with it. |
| A linked worktree with no `.claude/skills` of its own loads the main checkout's. | docs-only | [worktrees](https://code.claude.com/docs/en/worktrees). A caveat of D-02; nothing relies on it. |
| Before 2.1.280, a `manifest.json` in `~/.claude/skills` moved the skills it listed to `.trash`. | docs-only | [skills](https://code.claude.com/docs/en/skills), [changelog](https://code.claude.com/docs/en/changelog). Fallback: Mindflayer never writes in `~/.claude/skills` (D-22). |

### Rules and instruction files

| Fact | Mark | Evidence, or source and fallback |
|---|---|---|
| `CLAUDE.md` is read from the start directory and every folder above it, past the git root. | verified | `claude-parent`. |
| `AGENTS.md` is read natively only when no `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` exists in the start directory or above. | verified, in part | `claude-excluded` and `claude-user-memory` (read), `claude-parent` and `claude-local` (not read). No run placed a `.claude/CLAUDE.md`, but the debug log shows the check looking for all three names. Fallback for that one: #44 counts it, and if Claude did not, the `@AGENTS.md` import would be redundant rather than a second read, since an import never loads the file twice ([memory](https://code.claude.com/docs/en/memory)). |
| That check is made by a built-in plugin, `cc-plugin-agents-md@builtin`. | verified | Listed in the init message's plugins; the debug log's lines come from it. The [memory](https://code.claude.com/docs/en/memory) docs' "Project instructions" setting lives under that plugin's ID. |
| For a start directory four levels below `/`, the check covered four directories. | verified | Debug: `CLAUDE.md, .claude/CLAUDE.md, CLAUDE.local.md found 0 of 4 directories`. So `/` itself is probably not checked; #44 walks up to and including `/`, which differs only for a `CLAUDE.md` at `/`. |
| An `AGENTS.md` excluded through `info/exclude` is read. | verified | `claude-excluded`. |
| A `CLAUDE.md` holding `@AGENTS.md` loads `AGENTS.md` once. | verified | `claude-import`: one `include` record in the hook, and the model counted the token once. |
| A `CLAUDE.local.md` alone, excluded, is read and stops `AGENTS.md` being read. | verified | `claude-local`. |
| The user memory file (`$CLAUDE_CONFIG_DIR/CLAUDE.md`) is read and does not count for the `AGENTS.md` check. | verified | `claude-user-memory`: the hook records it as `User`, and `AGENTS.md` was read as well. |
| `.claude/rules` files do not count as a `CLAUDE.md` for that check. | verified | `claude-excluded`: the rules and `AGENTS.md` both loaded. |
| `AGENTS.md` read natively fires no `InstructionsLoaded` hook. | verified | `claude-excluded`: `AGENTS.md` reached the model and the hook recorded only the rule. The debug log's `AGENTS.md loaded: <path>` is the record instead. |
| After Claude reads a file in a subfolder, the `AGENTS.md` check also looks in that subfolder. | verified | `claude-touch`, debug: `found 0 of 1 directories` right after reading `src/lib.rs`. |
| `.claude/rules/**/*.md` is searched recursively. | verified | The rules sat two folders down, in `.claude/rules/mindflayer/probe/`. |
| A rule without `paths:`, excluded, loads at session start. | verified | `claude-excluded`: hook `session_start`. |
| A rule with `paths:`, excluded, does not load at start, and loads when Claude reads a matching file. | verified | `claude-excluded` (absent), then `claude-touch`: hook `path_glob_match`, triggered by `src/lib.rs`, globs `["src/**/*.rs"]`. |
| Front matter is stripped before a rule is injected, and `paths` is the only key read. | docs-only | [memory](https://code.claude.com/docs/en/memory). Fallback: harmless either way; #43 keeps the front matter in the file. |
| Block-level HTML comments are stripped before injection. | docs-only | [memory](https://code.claude.com/docs/en/memory). Not relied on: the block's markers are harmless text if they show. |
| `AGENTS.md` is read natively since 2.1.277 (2.1.281 on Bedrock, Vertex, Foundry, gateways and with telemetry off). | docs-only | [changelog](https://code.claude.com/docs/en/changelog). The run had non-essential traffic off and read it on 2.1.309. Fallback: the import (#44) covers older versions when a `CLAUDE.md` exists; an older Claude in a project without one is documented, not handled. |
| The "Project instructions" setting can switch native `AGENTS.md` reading off or on beside `CLAUDE.md`. | docs-only | [memory](https://code.claude.com/docs/en/memory). Out of #44's scope, which assumes the default. |
| An empty `CLAUDE_CONFIG_DIR`. | not checked | Still an [open question](architecture.md#open-questions). |

### The Claude Code run

- **When:** 2026-10-26, 18:43 to 18:44 UTC.
- **Binary:** Claude Code 2.1.309, the native Linux x86-64 build
  (`~/.local/share/mise/installs/claude/2.1.309/claude`).
- **Model:** `--model haiku`, which resolved to `claude-haiku-5-5`.
- **Host:** Linux 7.2.8, bubblewrap 0.13.1, git 2.56.0.
- **Cost:** six `claude -p` runs, 0.0101 USD in all.
- **Login:** `~/.claude/.credentials.json` copied in, valid for another 255
  minutes, and not refreshed during the run.
- **User settings in the temp home:** only the `InstructionsLoaded` hook,
  appending each record to `results/<run>.loaded.jsonl`.

The environment, cleared first and then set to:

```
HOME=/mnt/agent-probe/home
XDG_CONFIG_HOME=/mnt/agent-probe/home/.config
XDG_DATA_HOME=/mnt/agent-probe/home/.local/share
XDG_STATE_HOME=/mnt/agent-probe/home/.local/state
XDG_CACHE_HOME=/mnt/agent-probe/home/.cache
CLAUDE_CONFIG_DIR=/mnt/agent-probe/home/.claude
CODEX_HOME=/mnt/agent-probe/home/.codex
OPENCODE_CONFIG_DIR=/mnt/agent-probe/home/.config/opencode
PATH=/usr/bin:/bin
LANG=C.UTF-8
PROBE_REAL_HOME=<the operator's home, hidden by the namespace>
PROBE_CLAUDE_BIN=<the binary above>
```

and, for each `claude` only, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`
(telemetry, error reports and the updater off: the run needs the API and
nothing else) and `PROBE_SCENARIO=<run>`, which names the hook's log.

Every run was, from inside the scenario's directory:

```
claude -p <prompt> --model haiku --output-format json --verbose \
  --permission-mode dontAsk --no-session-persistence --max-budget-usd 0.10 \
  --debug-file /mnt/agent-probe/results/<run>.debug <extra>
```

with `--max-turns 2` as the extra for the runs that list, and
`--tools Read --allowedTools Read --max-turns 3` for `claude-touch`, the one
run that has to read a file and the only one given a tool. It had no other;
the others were asked to call none, and each answered in one turn.

The prompt for the runs that list:

```
This is an automated configuration probe. Do not call any tool. Reply with exactly two lines and nothing else.
Line 1: "TOKENS:" followed by every distinct string that starts with "PROBE-" and appears anywhere in your system prompt, memory files, rules or skill descriptions, each followed by how many times it appears there, in parentheses, as in TOKEN-A(1) TOKEN-B(2); or "none".
Line 2: "SKILLS:" followed by the name of every skill you can invoke, separated by spaces; or "none".
```

The prompt for `claude-touch`:

```
This is an automated configuration probe. Call the Read tool exactly once, on src/lib.rs, and call no other tool. Then reply with exactly one line and nothing else: "TOKENS:" followed by every distinct string that starts with "PROBE-" and appears anywhere in your system prompt, memory files, rules, skill descriptions or tool results, separated by spaces; or "none".
```

Every skill file's description reads `Probe skill. If asked for probe tokens,
include <token>.`, its body the same with `-BODY` appended to the token. Every
other file under test holds the one line `If asked for probe tokens, include
<token>.`, except where a run below says otherwise. The 17 skills every init
message listed besides the probe ones are Claude's own (`deep-research`,
`dataviz`, `update-config`, ...).

#### `claude-excluded`: checks 1, 3, 4 and 6

A fresh repository at `/mnt/agent-probe/stage/claude-excluded`:

```
AGENTS.md                                    PROBE-AGENTS-MD-1
.claude/skills/probe-claude-skill/SKILL.md   PROBE-CLAUDE-SKILL-1
.claude/skills/probe-linked-skill            -> ../../.agents/skills/probe-linked-skill
.agents/skills/probe-linked-skill/SKILL.md   PROBE-LINKED-SKILL-1
.agents/skills/probe-agents-skill/SKILL.md   PROBE-AGENTS-SKILL-1
.claude/rules/mindflayer/probe/unscoped.md   PROBE-RULE-UNSCOPED-1
.claude/rules/mindflayer/probe/scoped.md     PROBE-RULE-SCOPED-1, under paths: ["src/**/*.rs"]
src/lib.rs                                   PROBE-FILE-1, in a comment
```

with this block appended to `.git/info/exclude`, as Mindflayer's local mode
writes one:

```
# mindflayer:begin
# project .
/AGENTS.md
/.agents/skills/probe-agents-skill/
/.agents/skills/probe-linked-skill/
/.claude/skills/probe-claude-skill/
/.claude/skills/probe-linked-skill
/.claude/rules/mindflayer/probe/unscoped.md
/.claude/rules/mindflayer/probe/scoped.md
# mindflayer:end
```

Git agreed every one was ignored:

```
$ git status --porcelain --ignored --untracked-files=all
?? src/lib.rs
!! .agents/skills/probe-agents-skill/SKILL.md
!! .agents/skills/probe-linked-skill/SKILL.md
!! .claude/rules/mindflayer/probe/scoped.md
!! .claude/rules/mindflayer/probe/unscoped.md
!! .claude/skills/probe-claude-skill/SKILL.md
!! .claude/skills/probe-linked-skill
!! AGENTS.md
```

The answer:

```
TOKENS: PROBE-RULE-UNSCOPED-1(1) PROBE-AGENTS-MD-1(1) PROBE-CLAUDE-SKILL-1(1) PROBE-LINKED-SKILL-1(1)
SKILLS: probe-claude-skill probe-linked-skill dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

- Init message: `probe-claude-skill` and `probe-linked-skill` once each, no
  `probe-agents-skill`, and Claude's own 17.
- `InstructionsLoaded`: `session_start Project .../.claude/rules/mindflayer/probe/unscoped.md`,
  and nothing else.
- Debug log: `$.fs.ancestors (cc-plugin-agents-md): CLAUDE.md, .claude/CLAUDE.md, CLAUDE.local.md found 0 of 4 directories`,
  then `AGENTS.md, .claude/AGENTS.md found 1 of 4 directories`, then
  `no CLAUDE.md found; AGENTS.md loaded: /mnt/agent-probe/stage/claude-excluded/AGENTS.md`.

#### `claude-touch`: check 6, on touching a matching file

The same repository, asked to read `src/lib.rs` first:

```
TOKENS: PROBE-FILE-1 PROBE-RULE-UNSCOPED-1 PROBE-AGENTS-MD-1 PROBE-RULE-SCOPED-1
```

- `InstructionsLoaded`: `session_start` for `unscoped.md`, then
  `path_glob_match` for `scoped.md`, with `trigger_file_path`
  `.../src/lib.rs` and `globs` `["src/**/*.rs"]`.
- Debug log, after the read: `AGENTS.md, .claude/AGENTS.md found 0 of 1 directories`
  and the same for the `CLAUDE.md` names, for `src/`.

#### `claude-import`: check 4, `CLAUDE.md` importing `AGENTS.md`

A fresh repository with `CLAUDE.md` holding `@AGENTS.md` and then
`PROBE-CLAUDE-MD-2`, and `AGENTS.md` holding `PROBE-AGENTS-MD-2`; nothing
excluded.

```
TOKENS: PROBE-CLAUDE-MD-2(1) PROBE-AGENTS-MD-2(1)
SKILLS: dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

- `InstructionsLoaded`: `session_start Project .../claude-import/CLAUDE.md`,
  then `include Project .../claude-import/AGENTS.md`, once.

#### `claude-parent`: check 4, a `CLAUDE.md` above the git root

`/mnt/agent-probe/stage/claude-parent/CLAUDE.md` holding
`PROBE-PARENT-CLAUDE-MD-3`, the stand-in for `~/Work`, and a fresh repository
below it at `claude-parent/repo` with `AGENTS.md` holding
`PROBE-AGENTS-MD-3`. Claude started in the repository.

```
TOKENS: PROBE-PARENT-CLAUDE-MD-3(1)
SKILLS: dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

- `InstructionsLoaded`: `session_start Project .../claude-parent/CLAUDE.md`
  only.
- Debug log: no `AGENTS.md loaded` line.

#### `claude-local`: `CLAUDE.local.md` alone

A fresh repository with `CLAUDE.local.md` holding `PROBE-CLAUDE-LOCAL-MD-4`
and `AGENTS.md` holding `PROBE-AGENTS-MD-4`, both excluded in a block as
above.

```
TOKENS: PROBE-CLAUDE-LOCAL-MD-4(1)
SKILLS: dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

- `InstructionsLoaded`: `session_start Local .../claude-local/CLAUDE.local.md`
  only.
- Debug log: no `AGENTS.md loaded` line.

#### `claude-user-memory`: the user memory file does not count

A fresh repository with `AGENTS.md` holding `PROBE-AGENTS-MD-5`, and
`$CLAUDE_CONFIG_DIR/CLAUDE.md` holding `PROBE-USER-CLAUDE-MD-5` for this run
only.

```
TOKENS: PROBE-USER-CLAUDE-MD-5(1) PROBE-AGENTS-MD-5(1)
SKILLS: dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

- `InstructionsLoaded`: `session_start User /mnt/agent-probe/home/.claude/CLAUDE.md`.
- Debug log: `found 0 of 4 directories` for the `CLAUDE.md` names, then
  `no CLAUDE.md found; AGENTS.md loaded: .../claude-user-memory/AGENTS.md`.

#### `git-symlink`: excluding a symlinked skill folder

Git only, no agent. A repository with `.claude/skills/linked` a symlink to
`../../.agents/skills/linked`: with `/.claude/skills/linked/` as the only line
of `info/exclude`, `git check-ignore` did not report it ignored; with
`/.claude/skills/linked` it did. Git never follows a symlink, so to it the
link is a file, and a pattern ending in `/` matches only directories.

#### The real home, before and after

| Path | Before and after |
|---|---|
| `~/.agents` | a folder, unchanged |
| `~/.codex` | a folder, unchanged |
| `~/.claude/skills` | a folder, unchanged |
| `~/.claude/rules` | absent both times |
| `~/.claude/CLAUDE.md` | absent both times |
| `~/.claude.json` | a file, unchanged |

Unchanged means the same names, link targets and file contents, by SHA-256.

## Codex

Nothing here was run. The issue targets Codex 0.16x; these facts are from the
documentation and source as of 2026-10-26. Codex's documentation moved from
developers.openai.com/codex to learn.chatgpt.com.

### Skills

| Fact | Mark | Source, and the fallback |
|---|---|---|
| Project skills are `.agents/skills/<name>/SKILL.md` in every folder from the start directory up to the repository root. | docs-only | [build-skills](https://learn.chatgpt.com/docs/build-skills). Fallback: the target table (#28) is one table in core, so moving Codex's target is a one-line change; a `codex.sh` check before #34 ships settles it. |
| User skills are `$HOME/.agents/skills`, taken from the OS home, not from `CODEX_HOME`. | docs-only | [build-skills](https://learn.chatgpt.com/docs/build-skills); source, [host_roots.rs](https://github.com/openai/codex/blob/main/codex-rs/ext/skills/src/host_roots.rs). Fallback: Mindflayer does not install at user scope (D-22); `--agents-dir` points checks at it. |
| `/etc/codex/skills` (admin) and bundled system skills are read too. | docs-only | [build-skills](https://learn.chatgpt.com/docs/build-skills). Not used. |
| `<project>/.codex/skills` (trusted projects only) and `$CODEX_HOME/skills` (deprecated) are still read. | docs-only | Source, [host_roots.rs](https://github.com/openai/codex/blob/main/codex-rs/ext/skills/src/host_roots.rs). Never targeted ([D-07](architecture.md#d-07-one-copy-per-agent-folder)). |
| `.agents/skills` is probed with no trust check. | docs-only | Source, [host_roots.rs](https://github.com/openai/codex/blob/main/codex-rs/ext/skills/src/host_roots.rs). Fallback: were it gated, skills would reach Codex only in trusted projects, and the agents left unreached are reported (D-02). |
| Two skills with one name are both shown, not merged. | docs-only | [build-skills](https://learn.chatgpt.com/docs/build-skills). Fallback: D-07's copies are identical; `flayer doctor` (#96) reports shadowing. |
| Symlinked skill folders are followed. | docs-only | [build-skills](https://learn.chatgpt.com/docs/build-skills). Not relied on: installs are copies. |
| Ignored files are read: discovery checks the disk and does no gitignore filtering. | docs-only | Source search only, [host_roots.rs](https://github.com/openai/codex/blob/main/codex-rs/ext/skills/src/host_roots.rs). Fallback: shared mode (D-02), where installed copies are plain files in the tree. |
| A `name` over 64 characters or an empty `description` rejects the skill. | docs-only | Source, [parser.rs](https://github.com/openai/codex/blob/main/codex-rs/skills/src/parser.rs). Fallback: Mindflayer holds names to 64 characters and requires a description when it writes. |

### Rules

| Fact | Mark | Source, and the fallback |
|---|---|---|
| Project `AGENTS.md` is read from the project root (the git root, by `project_root_markers`) down to the start directory, at most one file per folder (`AGENTS.override.md`, then `AGENTS.md`, then `project_doc_fallback_filenames`), concatenated root first. Without a project root, only the start directory. | docs-only | [agents-md](https://learn.chatgpt.com/docs/agent-configuration/agents-md). Fallback: #42 warns when an `AGENTS.override.md` hides the file and when there is no git root above. |
| The global file is `$CODEX_HOME/AGENTS.override.md`, else `$CODEX_HOME/AGENTS.md`. | docs-only | [agents-md](https://learn.chatgpt.com/docs/agent-configuration/agents-md). Not written by Mindflayer. |
| The project chain shares a 32 KiB budget (`project_doc_max_bytes`) and a file past it is truncated; the global file is not counted. | docs-only | Source, [agents_md.rs](https://github.com/openai/codex/blob/main/codex-rs/core/src/agents_md.rs). Fallback: #42 warns as the chain nears the budget; counting the global file too would only warn earlier. |
| There is no `@`-import. | docs-only | [agents-md](https://learn.chatgpt.com/docs/agent-configuration/agents-md). Why D-03 writes rule text into `AGENTS.md` itself. |
| A project marked `trust_level = "untrusted"` skips project `AGENTS.md`; one with no trust entry still reads it. | docs-only | Source, [agents_md.rs](https://github.com/openai/codex/blob/main/codex-rs/core/src/agents_md.rs), [config_toml.rs](https://github.com/openai/codex/blob/main/codex-rs/config/src/config_toml.rs). Check 5 was not run. Fallback: a check that reads Codex's config (#96) can say a project is distrusted. |
| An ignored `AGENTS.md` is read. | docs-only | Source search only. Fallback: shared mode, or `edit_tracked` for a tracked `AGENTS.md`, which no exclude touches. |
| `CODEX_HOME` moves `~/.codex`. | docs-only | [config-reference](https://learn.chatgpt.com/docs/config-file/config-reference). How Codex reads an empty one is unknown, an [open question](architecture.md#open-questions). |

## OpenCode

Nothing here was run, neither the installed V2 nor V1 through
`npx opencode-ai@1.19.4`. OpenCode has two maintained lines that both install
a binary named `opencode`: V1, 1.19.x (latest 1.19.4, 2026-10-22, docs at
opencode.ai/docs), and V2, 2.1.x (latest tag 2.1.4, 2026-10-23, docs at
opencode.ai/v2/docs) ([releases](https://github.com/anomalyco/opencode/releases)).

### Skills

| Fact | Mark | Source, and the fallback |
|---|---|---|
| V1 reads `.opencode/skills`, `.claude/skills` and `.agents/skills` from the start directory up to the git worktree, plus `~/.config/opencode/skills`, `~/.claude/skills`, `~/.agents/skills` and config `skills.paths`/`skills.urls`. | docs-only | [V1 skills](https://opencode.ai/docs/skills/); source, [skill/index.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/skill/index.ts). Fallback: it reads both folders D-07 fills, so either copy reaches it. |
| V1 finds skills with a recursive `skills/**/SKILL.md` glob that includes hidden folders. | docs-only | Source, [skill/index.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/skill/index.ts). Fallback: Mindflayer stages in `.mind/tmp`, never inside a skills folder. |
| V1 keys a skill by its front matter `name`, and 1.19.4 does not enforce the documented name rules. | docs-only | Source, [skill/index.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/skill/index.ts). Fallback: Mindflayer requires the name to match the folder, so V1 and V2 see one ID. |
| V1 given one name twice logs `duplicate skill name`, and the winner is whichever copy finishes parsing last. | docs-only | Source, [skill/index.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/skill/index.ts). Check 2 was not run. Fallback: D-07 writes both copies identically in one staged step, so either winner is the same skill. |
| `OPENCODE_DISABLE_CLAUDE_CODE` or `OPENCODE_DISABLE_CLAUDE_CODE_SKILLS` turns `.claude/skills` off in V1. | docs-only | [V1 rules](https://opencode.ai/docs/rules/). Fallback: the `.agents/skills` copy still reaches it. |
| V2 reads `~/.config/opencode/skills`, `~/.claude/skills`, `~/.agents/skills`, `.opencode/skills`, `.claude/skills` and `.agents/skills` from the start directory up to the project root, plus `skills` config entries. A skill's ID is its folder name. Later sources win: built-in, `.claude/skills`, `.agents/skills`, `~/.config/opencode/skills`, project `.opencode/skills`, config; within a group the global folder comes first, then the farthest ancestor down. Hidden folders and `node_modules` are skipped, six levels deep at most. | docs-only | [V2 skills](https://opencode.ai/v2/docs/skills/). Fallback: the copies are identical, so which wins does not matter. `~/.agents/skills` beating a project's `.claude/skills` is why doctor reports shadowing (D-22). |
| Ignored files are read: V1's scan has no gitignore option. | docs-only | Source search only, [skill/index.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/skill/index.ts). Fallback: shared mode (D-02). |

### Rules

| Fact | Mark | Source, and the fallback |
|---|---|---|
| V1 reads every `AGENTS.md` from the start directory up to the worktree, and a project `CLAUDE.md` (then `CONTEXT.md`) only when there is none; globally `~/.config/opencode/AGENTS.md`, else `~/.claude/CLAUDE.md`. `@` references are not followed. | docs-only | [V1 rules](https://opencode.ai/docs/rules/); source, [instruction.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/session/instruction.ts). Fallback: none needed; a project with a block has an `AGENTS.md`, so V1 never falls back to its `CLAUDE.md`. |
| V2 reads only `AGENTS.md`: the global `~/.config/opencode/AGENTS.md` and every `AGENTS.md` from the start directory up toward home (up to the project root outside home), nested ones when it reads below them; `instructions` after them. | docs-only | [V2 instructions](https://opencode.ai/v2/docs/instructions/), [migration](https://opencode.ai/v2/docs/migrate-v1/). Fallback: #42 refuses to write `AGENTS.md` into a workspace root or above linked projects, since V2 would read it in every project below. |
| `OPENCODE_DISABLE_PROJECT_CONFIG=1` makes V2 skip project `AGENTS.md`. | docs-only | [V2 instructions](https://opencode.ai/v2/docs/instructions/). Not relied on. |
| An ignored `AGENTS.md` is read. | docs-only | Source search only. Fallback: shared mode. |

### OpenCode's own folders

| Fact | Mark | Source, and the fallback |
|---|---|---|
| `OPENCODE_CONFIG_DIR` adds a folder loaded after `~/.config/opencode` and the project's `.opencode`; it does not replace `~/.config/opencode`. | docs-only | [config](https://opencode.ai/docs/config/). Fallback: still an [open question](architecture.md#open-questions); `AgentHomes` takes both folders when a check first reads OpenCode's. |
| Whether `$XDG_CONFIG_HOME` moves `~/.config/opencode`. | unknown | Not documented, not run. Fallback: `--opencode-config-dir` names the folder outright. |
| How an empty `OPENCODE_CONFIG_DIR` is read. | unknown | Not documented, not run. |

## Facts the roadmap cites

Where every fact #28, #34 and #42 to #44 build on stands. A docs-only one
carries its fallback in the tables above.

| Issue | Fact | Mark |
|---|---|---|
| #28 | Claude reads `.claude/skills` and not `.agents/skills`. | verified |
| #28 | Codex reads `.agents/skills`. | docs-only |
| #28 | OpenCode V1 and V2 read `.agents/skills`, and `.claude/skills` too. | docs-only |
| #28 | `.codex/skills` and `~/.codex/skills` are deprecated or undocumented. | docs-only |
| #34 | An excluded copy in `.claude/skills` loads in Claude. | verified |
| #34 | An excluded copy in `.agents/skills` loads in Codex and OpenCode. | docs-only |
| #34 | A `.claude-plugin/` folder in a skill makes Claude load it as a plugin. | docs-only |
| #34 | OpenCode given identical copies in both folders shows one skill without an error. | docs-only |
| #34, D-07 | A relative symlink in `.claude/skills` loads once in Claude; its exclude entry must not end in `/`. | verified |
| #42 | The root `AGENTS.md` reaches Claude (when #44 says so), Codex and OpenCode. | verified for Claude; docs-only for Codex and OpenCode |
| #42 | An excluded `AGENTS.md` is read by Claude. | verified |
| #42 | Codex's 32 KiB budget covers the project chain only. | docs-only |
| #42 | An `AGENTS.override.md` hides `AGENTS.md` from Codex. | docs-only |
| #42 | Without a git root, Codex reads only the start directory. | docs-only |
| #42 | OpenCode V2 reads `AGENTS.md` up to home. | docs-only |
| #43 | An excluded `.claude/rules/mindflayer/<route>.md` loads in Claude, with and without `paths:`. | verified |
| #43 | A rule with `paths:` loads only once Claude reads a matching file; one without loads at start. | verified |
| #43 | `.claude/rules` is searched recursively, and its files do not count as a `CLAUDE.md`. | verified |
| #43 | Claude strips a rule's front matter before injecting it. | docs-only |
| #44 | Claude reads `AGENTS.md` only with no `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` in the start directory or above. | verified for `CLAUDE.md` and `CLAUDE.local.md`; `.claude/CLAUDE.md` seen only among the names the check looks for |
| #44 | A `CLAUDE.md` above the git root counts. | verified |
| #44 | The user memory file does not count. | verified |
| #44 | `@AGENTS.md` in a `CLAUDE.md` loads it once. | verified |
| #44 | A `CLAUDE.local.md` alone stops `AGENTS.md` being read. | verified |
| #44 | Native `AGENTS.md` reading needs Claude 2.1.277 or later. | docs-only |
